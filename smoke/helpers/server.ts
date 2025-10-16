import { execa, type ResultPromise } from 'execa';
import getPort from 'get-port';
import { once } from 'node:events';
import { ensureBinaryExists } from './binary.js';
import type { SmokeWorkspace } from './workspace.js';

export interface LotarServerOptions {
    readonly host?: string;
    readonly port?: number;
    readonly open?: boolean;
    readonly env?: NodeJS.ProcessEnv;
}

export interface LotarServer {
    readonly url: string;
    readonly host: string;
    readonly port: number;
    readonly raw: ResultPromise;
    stop(): Promise<void>;
}

async function waitForServerReady(child: ResultPromise): Promise<void> {
    if (!child.stdout) {
        return;
    }

    for await (const chunk of child.stdout) {
        const text = chunk.toString();
        if (process.env.SMOKE_DEBUG === '1') {
            console.debug('[smoke][server][stdout]', text.trimEnd());
        }
        if (text.includes('URL: http://')) {
            return;
        }
    }
}

export async function startLotarServer(
    workspace: SmokeWorkspace,
    options: LotarServerOptions = {},
): Promise<LotarServer> {
    const binary = await ensureBinaryExists();
    const host = options.host ?? '127.0.0.1';
    const port = options.port ?? (await getPort());
    const env = {
        ...workspace.env,
        ...options.env,
    };

    const child = execa(binary, ['serve', '--port', String(port), '--host', host], {
        cwd: workspace.root,
        env,
        stdio: ['ignore', 'pipe', 'pipe'],
    });

    child.stdout?.setEncoding('utf8');
    child.stderr?.setEncoding('utf8');

    const stderrChunks: string[] = [];
    child.stderr?.on('data', (chunk: unknown) => {
        if (typeof chunk === 'string') {
            stderrChunks.push(chunk);
            if (process.env.SMOKE_DEBUG === '1') {
                console.debug('[smoke][server][stderr]', chunk.trimEnd());
            }
        } else if (Buffer.isBuffer(chunk)) {
            const text = chunk.toString('utf8');
            stderrChunks.push(text);
            if (process.env.SMOKE_DEBUG === '1') {
                console.debug('[smoke][server][stderr]', text.trimEnd());
            }
        }
    });

    const ready = waitForServerReady(child);
    const exit = once(child, 'exit') as Promise<[number | null, NodeJS.Signals | null]>;

    type ReadyResult = 'ready';
    type ExitResult = { code: number | null; signal: NodeJS.Signals | null };

    const winner: ReadyResult | ExitResult = await Promise.race([
        ready.then<ReadyResult>(() => 'ready'),
        exit.then<ExitResult>(([code, signal]) => ({ code, signal })),
    ]);

    if (winner !== 'ready') {
        const message = `lotar serve exited before becoming ready: code=${winner.code} signal=${winner.signal}`;
        const stderr = stderrChunks.join('');
        throw new Error(message + (stderr ? `\nstderr:\n${stderr}` : ''));
    }

    return {
        url: `http://${host}:${port}`,
        host,
        port,
        raw: child,
        async stop() {
            if (child.exitCode !== null || child.killed) {
                return;
            }

            const exited = once(child, 'exit');
            const graceful = child.kill('SIGINT');

            const forceTimer = setTimeout(() => {
                if (child.exitCode === null) {
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
            }

            child.stdout?.removeAllListeners();
            child.stderr?.removeAllListeners();
            child.stdout?.destroy();
            child.stderr?.destroy();

            if (!graceful && child.exitCode === null && !child.killed) {
                child.kill('SIGKILL');
            }
        },
    };
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
