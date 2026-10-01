import type { ChildProcess } from 'node:child_process';
import { once } from 'node:events';
import { execa, type ResultPromise } from 'execa';
import getPort from 'get-port';
import { ensureBinaryExists } from './binary.js';
import type { SmokeWorkspace } from './workspace.js';

export interface LotarServerOptions {
    readonly host?: string;
    readonly port?: number;
    readonly open?: boolean;
    readonly env?: NodeJS.ProcessEnv;
    /** Bounded readiness wait in milliseconds (default 15s). */
    readonly readyTimeoutMs?: number;
}

export interface LotarServer {
    readonly url: string;
    readonly host: string;
    readonly port: number;
    readonly raw: ResultPromise;
    stop(): Promise<void>;
}

/** Default bound on how long a spawned server may take to advertise readiness. */
const DEFAULT_READY_TIMEOUT_MS = 15_000;

/**
 * Matches the `   URL: http://host:port` readiness banner.
 *
 * The lookahead requires whitespace after the port so a port number split
 * across stdout chunks is never mistaken for complete; the host capture is
 * greedy so IPv6 literals (`::1`) still split on the last colon.
 */
const READY_LINE_PATTERN = /URL: http:\/\/(\S+):(\d+)(?=\s)/;

/** Cap on buffered startup output before readiness is declared impossible. */
const MAX_READY_BUFFER_CHARS = 64 * 1024;

export interface ReadyMarker {
    readonly host: string;
    readonly port: number;
}

export class ServerReadinessError extends Error {
    readonly reason: 'timeout' | 'eof';

    constructor(
        message: string,
        reason: 'timeout' | 'eof',
        options?: { cause?: unknown },
    ) {
        super(message, options);
        this.name = 'ServerReadinessError';
        this.reason = reason;
    }
}

function chunkToString(chunk: unknown): string {
    if (typeof chunk === 'string') {
        return chunk;
    }
    if (chunk instanceof Uint8Array) {
        return Buffer.from(chunk.buffer, chunk.byteOffset, chunk.byteLength).toString('utf8');
    }
    return String(chunk);
}

function parseReadyMarker(buffer: string): ReadyMarker | undefined {
    const match = READY_LINE_PATTERN.exec(buffer);
    if (!match) {
        return undefined;
    }
    const port = Number.parseInt(match[2] ?? '', 10);
    if (!Number.isInteger(port) || port <= 0 || port > 65535) {
        return undefined;
    }
    return { host: match[1] ?? '', port };
}

/**
 * Scan an async stdout stream for the readiness banner.
 *
 * Chunks are accumulated so a marker split across arbitrary chunk
 * boundaries is still recognized. Rejects with a `ServerReadinessError`
 * when the stream ends before a complete marker (`eof`) or when no marker
 * arrives within `timeoutMs` (`timeout`); the internal timer is always
 * cleared once the scan settles.
 */
export async function readReadyMarker(
    chunks: AsyncIterable<unknown>,
    timeoutMs: number,
): Promise<ReadyMarker> {
    let buffer = '';
    let timer: NodeJS.Timeout | undefined;
    const timeout = new Promise<never>((_, reject) => {
        timer = setTimeout(() => {
            reject(
                new ServerReadinessError(
                    `server did not advertise readiness within ${timeoutMs}ms`,
                    'timeout',
                ),
            );
        }, timeoutMs);
    });

    const scan = (async () => {
        try {
            for await (const chunk of chunks) {
                buffer += chunkToString(chunk);
                const marker = parseReadyMarker(buffer);
                if (marker) {
                    return marker;
                }
                if (buffer.length > MAX_READY_BUFFER_CHARS) {
                    throw new ServerReadinessError(
                        `server produced ${buffer.length} characters of output without a readiness banner`,
                        'eof',
                    );
                }
            }
            throw new ServerReadinessError(
                'server stdout ended before a complete readiness banner was seen (early EOF or split marker never completed)',
                'eof',
            );
        } finally {
            if (timer !== undefined) {
                clearTimeout(timer);
            }
        }
    })();

    return Promise.race([scan, timeout]);
}

export interface LaunchedServer {
    readonly child: ResultPromise;
    readonly node: ChildProcess;
    readonly host: string;
    readonly port: number;
    stop(): Promise<void>;
}

export interface LaunchOptions {
    readonly cwd: string;
    readonly env: NodeJS.ProcessEnv;
    readonly readyTimeoutMs?: number;
    /** When set, the advertised port must match or the launch fails. */
    readonly expectedPort?: number;
}

/**
 * Spawn a server command and wait until it advertises readiness on stdout.
 *
 * Every failure path (exit before ready, early EOF, timeout, port mismatch)
 * kills and reaps the child before rejecting, so a failed launch never
 * leaks a process.
 */
export async function launchAndWaitReady(
    command: string,
    args: readonly string[],
    options: LaunchOptions,
): Promise<LaunchedServer> {
    const child = execa(command, args, {
        cwd: options.cwd,
        env: options.env,
        stdio: ['ignore', 'pipe', 'pipe'],
    });

    // Mark execa's own promise rejection as handled from the start: every
    // failure is reported through the annotated errors below, and a bare
    // unhandled rejection would otherwise surface at process level.
    void child.catch(() => undefined);

    child.stdout?.setEncoding('utf8');
    child.stderr?.setEncoding('utf8');
    // Destroying a still-readable pipe (e.g. when the readiness scan stops
    // iterating) raises an AbortError; keep a no-op handler so it can never
    // become an unhandled stream error.
    child.stdout?.once('error', () => undefined);
    child.stderr?.once('error', () => undefined);

    const stderrChunks: string[] = [];
    child.stderr?.on('data', (chunk: unknown) => {
        const text = chunkToString(chunk);
        stderrChunks.push(text);
        if (process.env.SMOKE_DEBUG === '1') {
            console.debug('[smoke][server][stderr]', text.trimEnd());
        }
    });

    // execa >= 10 returns a Promise, not an EventEmitter; event/state access
    // goes through the underlying Node child process.
    const node = child.nodeChildProcess;

    const describeStderr = () =>
        stderrChunks.length > 0 ? `\nstderr:\n${stderrChunks.join('')}` : '';

    const annotate = (error: Error & { node?: unknown }): Error => {
        error.node = node;
        return error;
    };

    const readyTimeoutMs = options.readyTimeoutMs ?? DEFAULT_READY_TIMEOUT_MS;

    type ReadyResult = { kind: 'ready'; marker: ReadyMarker };
    type ExitResult = { kind: 'exit'; code: number | null; signal: NodeJS.Signals | null };
    type Outcome = ReadyResult | ExitResult | { kind: 'error'; error: Error };

    const waitForReady = async (): Promise<Outcome> => {
        if (!child.stdout) {
            return {
                kind: 'error',
                error: new Error('server was spawned without a piped stdout'),
            };
        }
        try {
            return {
                kind: 'ready',
                marker: await readReadyMarker(child.stdout, readyTimeoutMs),
            };
        } catch (error) {
            return { kind: 'error', error: error as Error };
        }
    };

    const waitForExit = async (): Promise<Outcome> => {
        const [code, signal] = await once(node, 'exit');
        return { kind: 'exit', code: code as number | null, signal: signal as NodeJS.Signals | null };
    };

    const outcome: Outcome =
        node.exitCode !== null || node.signalCode !== null
            ? {
                  kind: 'exit',
                  code: node.exitCode,
                  signal: node.signalCode as NodeJS.Signals | null,
              }
            : await Promise.race([waitForReady(), waitForExit()]);

    let failure: ExitResult | { kind: 'error'; error: Error } | undefined =
        outcome.kind === 'ready' ? undefined : outcome;

    if (
        failure?.kind === 'error' &&
        failure.error instanceof ServerReadinessError &&
        failure.error.reason === 'eof'
    ) {
        // stdout closed: if the process exits right after (the common
        // failure shape), report the exit with its code and stderr, which is
        // the precise diagnosis. Bound the wait so a process that closes
        // stdout but keeps running still reports the EOF failure.
        let code: number | null = node.exitCode;
        let signal: NodeJS.Signals | null = node.signalCode;
        if (code === null && signal === null) {
            const exit = await Promise.race([
                once(node, 'exit'),
                new Promise<undefined>((resolve) => {
                    setTimeout(() => resolve(undefined), 250);
                }),
            ]);
            if (Array.isArray(exit)) {
                code = exit[0] as number | null;
                signal = exit[1] as NodeJS.Signals | null;
            }
        }
        if (code !== null || signal !== null) {
            failure = { kind: 'exit', code, signal };
        }
    }

    if (failure) {
        // Always clean the child up before reporting failure. Cleanup must
        // never mask the readiness failure itself (e.g. execa surfaces the
        // child's non-zero exit as a promise rejection while we reap it).
        await stopChildProcess(child, node).catch(() => undefined);
        if (failure.kind === 'exit') {
            throw annotate(
                new Error(
                    `server exited before becoming ready: code=${failure.code} signal=${failure.signal}${describeStderr()}`,
                ),
            );
        }
        throw annotate(failure.error);
    }

    if (outcome.kind !== 'ready') {
        throw new Error('unreachable: failure paths threw above');
    }
    const marker = outcome.marker;
    if (
        options.expectedPort !== undefined &&
        marker.port !== options.expectedPort
    ) {
        await stopChildProcess(child, node);
        throw annotate(
            new Error(
                `server advertised port ${marker.port} but ${options.expectedPort} was requested${describeStderr()}`,
            ),
        );
    }

    return {
        child,
        node,
        host: marker.host,
        port: marker.port,
        stop: () => stopChildProcess(child, node),
    };
}

/**
 * Resolve where a smoke server should bind.
 *
 * Note: `startLotarServer` no longer probes for a free port by default — it
 * passes `--port 0` and reads the actual bound port back from the readiness
 * banner — but this helper remains for callers that need to pre-resolve a
 * bind (and documents the host-matched probing contract).
 */
export async function resolveServerBind(options: LotarServerOptions = {}): Promise<{
    host: string;
    port: number;
}> {
    const host = options.host ?? '127.0.0.1';
    const port = options.port ?? (await getPort({ host }));
    return { host, port };
}

export async function startLotarServer(
    workspace: SmokeWorkspace,
    options: LotarServerOptions = {},
): Promise<LotarServer> {
    const binary = await ensureBinaryExists();
    const host = options.host ?? '127.0.0.1';
    // Genuine ephemeral bind: unless the caller pinned a port, pass --port 0
    // so the OS assigns a free port at bind time (no probe-then-bind race);
    // the actual port is advertised by the post-bind readiness banner.
    const requestedPort = options.port ?? 0;
    const env = {
        ...workspace.env,
        // Default to embedded UI in smoke tests to ensure we test the bundled assets.
        // Tests that specifically need to test custom UI paths can override this.
        LOTAR_WEB_UI_EMBEDDED: '1',
        ...options.env,
    };

    const launched = await launchAndWaitReady(
        binary,
        ['serve', '--port', String(requestedPort), '--host', host],
        {
            cwd: workspace.root,
            env,
            readyTimeoutMs: options.readyTimeoutMs,
            expectedPort: options.port,
        },
    );

    const url = `http://${host}:${launched.port}`;
    return {
        url,
        host,
        port: launched.port,
        raw: launched.child,
        stop: () => launched.stop(),
    };
}

/**
 * Graceful teardown: SIGINT, bounded wait, SIGKILL fallback, then await the
 * exit so the child is reaped before returning (joined teardown).
 */
async function stopChildProcess(child: ResultPromise, node: ChildProcess): Promise<void> {
    if (node.exitCode !== null || node.signalCode !== null) {
        // Already exited (by code or signal): attach a rejection handler so
        // execa's promise is never left unhandled, then we are done.
        void child.catch(() => undefined);
        return;
    }

    const exited = once(node, 'exit');
    const graceful = child.kill('SIGINT');

    const forceTimer = setTimeout(() => {
        if (node.exitCode === null) {
            child.kill('SIGKILL');
        }
    }, 2_000);

    try {
        await exited;
        try {
            await child;
        } catch (error) {
            if (!shouldIgnoreTermination(error)) {
                throw error;
            }
        }
    } finally {
        clearTimeout(forceTimer);
        // Tear the pipes down. Keep one no-op error listener per stream so
        // destroying a still-readable stream cannot raise an unhandled
        // 'error' event (Node emits AbortError on destroy).
        child.stdout?.removeAllListeners();
        child.stderr?.removeAllListeners();
        child.stdout?.once('error', () => undefined);
        child.stderr?.once('error', () => undefined);
        child.stdout?.destroy();
        child.stderr?.destroy();

        if (!graceful && node.exitCode === null && !node.killed) {
            child.kill('SIGKILL');
        }
    }
}

function shouldIgnoreTermination(error: unknown): boolean {
    if (!error || typeof error !== 'object') {
        return false;
    }

    const err = error as {
        code?: string;
        isTerminated?: boolean;
        isCanceled?: boolean;
        signal?: string | null;
        isGracefullyCanceled?: boolean;
    };

    return (
        err.code === 'ABORT_ERR' ||
        err.isTerminated === true ||
        err.isGracefullyCanceled === true ||
        err.isCanceled === true ||
        err.signal === 'SIGINT'
    );
}
