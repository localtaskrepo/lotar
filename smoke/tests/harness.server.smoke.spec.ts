import { spawn, type ChildProcess } from 'node:child_process';
import { once } from 'node:events';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import net from 'node:net';
import { pathToFileURL } from 'node:url';
import getPort from 'get-port';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { resolveBinaryPath } from '../helpers/binary.js';
import { launchAndWaitReady, readReadyMarker, resolveServerBind, startLotarServer } from '../helpers/server.js';
import { SERVER_REGISTRY_ENV } from '../helpers/server-guard.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

vi.mock('get-port', async (importOriginal) => {
    const actual = await importOriginal<typeof import('get-port')>();
    return {
        ...actual,
        default: vi.fn(actual.default),
    };
});

const mockedGetPort = vi.mocked(getPort);

function listen(host: string, port = 0): Promise<net.Server> {
    return new Promise((resolve, reject) => {
        const server = net.createServer();
        server.once('error', reject);
        server.listen({ host, port }, () => resolve(server));
    });
}

function close(server: net.Server): Promise<void> {
    return new Promise((resolve) => {
        server.close(() => resolve());
    });
}

async function* chunksOf(parts: readonly string[]): AsyncIterable<string> {
    for (const part of parts) {
        yield part;
    }
}

async function* charsOf(text: string): AsyncIterable<string> {
    for (const char of text) {
        yield char;
    }
}

// eslint-disable-next-line require-yield -- deliberately never yields, like a mute server
async function* silentStream(): AsyncIterable<string> {
    // Never yields and never ends, like a server that stays mute.
    await new Promise<never>(() => {});
}

async function* partialThenHold(partial: string): AsyncIterable<string> {
    yield partial;
    // Stream stays open: no EOF, so only the readiness timeout can fire.
    await new Promise<never>(() => {});
}

// Bound for tests that wait on a child's exit, EOF or banner rather than on
// the readiness timeout. It only guards against a hang, so it is generous:
// under heavy machine load a Node child can take seconds just to start.
const HANG_GUARD_MS = 60_000;

function tempCwd(): string {
    return mkdtempSync(path.join(tmpdir(), 'dev76-harness-'));
}

function exited(node: ChildProcess | undefined): boolean {
    if (!node) {
        return false;
    }
    return node.exitCode !== null || node.signalCode !== null;
}

describe('smoke harness server bind resolution', () => {
    beforeEach(() => {
        mockedGetPort.mockClear();
    });

    it('returns an explicit port unchanged without availability probing', async () => {
        const holder = await listen('127.0.0.1');
        try {
            const held = (holder.address() as net.AddressInfo).port;
            const bind = await resolveServerBind({ port: held });
            expect(bind.host).toBe('127.0.0.1');
            expect(bind.port).toBe(held);
            expect(mockedGetPort).not.toHaveBeenCalled();
        } finally {
            await close(holder);
        }
    });

    it('probes the loopback host when no host or port is given', async () => {
        const holder = await listen('127.0.0.1');
        try {
            const held = (holder.address() as net.AddressInfo).port;
            const bind = await resolveServerBind();
            expect(mockedGetPort).toHaveBeenCalledTimes(1);
            expect(mockedGetPort).toHaveBeenCalledWith({ host: '127.0.0.1' });
            expect(bind.host).toBe('127.0.0.1');
            expect(bind.port).not.toBe(held);
            // DEV-136: the bind is exactly the port get-port probed on this host.
            // Re-binding it here would race every other process on the machine
            // (a full smoke run opens thousands of ephemeral sockets), because
            // the probe releases the port; availability is get-port's contract.
            expect(bind.port).toBe(await mockedGetPort.mock.results[0]!.value);
            expect(bind.port).toBeGreaterThan(0);
        } finally {
            await close(holder);
        }
    });

    it('probes a custom host on its own address family', async () => {
        const holder = await listen('::1');
        try {
            const held = (holder.address() as net.AddressInfo).port;
            const bind = await resolveServerBind({ host: '::1' });
            expect(mockedGetPort).toHaveBeenCalledTimes(1);
            expect(mockedGetPort).toHaveBeenCalledWith({ host: '::1' });
            expect(bind.host).toBe('::1');
            expect(bind.port).not.toBe(held);
            // DEV-136: the bind is exactly the port get-port probed on this host.
            // Re-binding it here would race every other process on the machine
            // (a full smoke run opens thousands of ephemeral sockets), because
            // the probe releases the port; availability is get-port's contract.
            expect(bind.port).toBe(await mockedGetPort.mock.results[0]!.value);
            expect(bind.port).toBeGreaterThan(0);
        } finally {
            await close(holder);
        }
    });
});

describe('ready marker scanning', () => {
    const banner = '   Host: 127.0.0.1\n   Port: 0\n   URL: http://127.0.0.1:41234\n   UI: embedded\n';

    it('recovers the advertised port for every possible split of the banner', async () => {
        for (let split = 1; split < banner.length; split += 1) {
            const marker = await readReadyMarker(chunksOf([banner.slice(0, split), banner.slice(split)]), 1_000);
            expect(marker.host).toBe('127.0.0.1');
            expect(marker.port).toBe(41234);
        }
    });

    it('recovers the advertised port when the banner arrives one character at a time', async () => {
        const marker = await readReadyMarker(charsOf(banner), 1_000);
        expect(marker.port).toBe(41234);
    });

    it('never treats a port number split mid-digits as complete', async () => {
        // The digits are cut short and the stream stays open, so no complete
        // marker exists; the scan must keep waiting until the timeout fires
        // instead of trusting a partial port.
        const partial = banner.slice(0, banner.indexOf(':41234') + 3);
        await expect(readReadyMarker(partialThenHold(partial), 80)).rejects.toThrow(
            /did not advertise readiness within 80ms/i,
        );
    });

    it('rejects on early EOF when the marker never completes', async () => {
        await expect(readReadyMarker(chunksOf(['   URL: http://127.0.0.1:54']), 1_000)).rejects.toThrow(
            /ended before a complete readiness banner/i,
        );
    });

    it('rejects on timeout when the stream stays silent', async () => {
        await expect(readReadyMarker(silentStream(), 80)).rejects.toThrow(/did not advertise readiness within 80ms/i);
    });
});

describe('server launch readiness against real child processes', () => {
    it('fails deterministically when the child exits before readiness', async () => {
        const cwd = tempCwd();
        try {
            const error = await launchAndWaitReady(process.execPath, ['-e', 'process.exit(3)'], {
                cwd,
                env: { ...process.env },
                readyTimeoutMs: HANG_GUARD_MS,
            }).then(
                () => undefined,
                (failure: unknown) => failure as Error & { node?: ChildProcess },
            );
            expect(error).toBeInstanceOf(Error);
            expect(error?.message).toMatch(/exited before becoming ready[\s\S]*code=3/);
            // Failure paths always reap the child.
            expect(exited(error?.node)).toBe(true);
        } finally {
            rmSync(cwd, { recursive: true, force: true });
        }
    });

    it('fails when stdout ends after only a partial marker', async () => {
        const cwd = tempCwd();
        try {
            const error = await launchAndWaitReady(
                process.execPath,
                ['-e', "process.stdout.write('   URL: http://127.0.0.1:54'); process.exit(0);"],
                { cwd, env: { ...process.env }, readyTimeoutMs: HANG_GUARD_MS },
            ).then(
                () => undefined,
                (failure: unknown) => failure as Error & { node?: ChildProcess },
            );
            expect(error).toBeInstanceOf(Error);
            expect(error?.message).toMatch(/exited before becoming ready|ended before a complete readiness banner/i);
        } finally {
            rmSync(cwd, { recursive: true, force: true });
        }
    });

    it('times out and kills a child that never becomes ready', async () => {
        const cwd = tempCwd();
        try {
            const error = await launchAndWaitReady(process.execPath, ['-e', 'setInterval(() => {}, 1000);'], {
                cwd,
                env: { ...process.env },
                readyTimeoutMs: 400,
            }).then(
                () => undefined,
                (failure: unknown) => failure as Error & { node?: ChildProcess },
            );
            expect(error?.message).toMatch(/did not advertise readiness within 400ms/i);
            // The timed-out child was killed and reaped before the rejection.
            expect(exited(error?.node)).toBe(true);
        } finally {
            rmSync(cwd, { recursive: true, force: true });
        }
    });

    it('resolves a marker split across delayed stdout chunks and stops cleanly', async () => {
        const cwd = tempCwd();
        const script = [
            "process.stdout.write('   Host: 127.0.0.1\\n   Port: 0\\n   URL: http://');",
            "setTimeout(() => process.stdout.write('127.0.0.1:45678\\n'), 30);",
            'setInterval(() => {}, 1000);',
        ].join('\n');
        try {
            const server = await launchAndWaitReady(process.execPath, ['-e', script], {
                cwd,
                env: { ...process.env },
                readyTimeoutMs: HANG_GUARD_MS,
            });
            expect(server.host).toBe('127.0.0.1');
            expect(server.port).toBe(45678);
            await server.stop();
            expect(exited(server.node)).toBe(true);
        } finally {
            rmSync(cwd, { recursive: true, force: true });
        }
    });

    it('stop() returns promptly when the server dies after readiness', async () => {
        const cwd = tempCwd();
        const script = [
            "process.stdout.write('   URL: http://127.0.0.1:45678\\n');",
            'setTimeout(() => process.exit(0), 50);',
        ].join('\n');
        try {
            const server = await launchAndWaitReady(process.execPath, ['-e', script], {
                cwd,
                env: { ...process.env },
                readyTimeoutMs: HANG_GUARD_MS,
            });
            await new Promise((resolve) => setTimeout(resolve, 300));
            await expect(server.stop()).resolves.toBeUndefined();
        } finally {
            rmSync(cwd, { recursive: true, force: true });
        }
    });

    it('rejects when the advertised port does not match the requested one', async () => {
        const cwd = tempCwd();
        const script = [
            "process.stdout.write('   URL: http://127.0.0.1:45678\\n');",
            'setInterval(() => {}, 1000);',
        ].join('\n');
        try {
            const error = await launchAndWaitReady(process.execPath, ['-e', script], {
                cwd,
                env: { ...process.env },
                readyTimeoutMs: HANG_GUARD_MS,
                expectedPort: 59_999,
            }).then(
                () => undefined,
                (failure: unknown) => failure as Error & { node?: ChildProcess },
            );
            expect(error?.message).toMatch(/advertised port 45678 but 59999 was requested/i);
            expect(exited(error?.node)).toBe(true);
        } finally {
            rmSync(cwd, { recursive: true, force: true });
        }
    });
});

const lotarBinaryAvailable = existsSync(resolveBinaryPath());

describe.skipIf(!lotarBinaryAvailable)('lotar serve readiness contract (real binary)', () => {
    it('rejects on an occupied port without mistaking it for readiness, and leaves no child', async () => {
        const holder = await listen('127.0.0.1');
        const workspace = await SmokeWorkspace.create({ name: 'dev76-occupied-' });
        try {
            const held = (holder.address() as net.AddressInfo).port;
            const error = await startLotarServer(workspace, { port: held }).then(
                () => undefined,
                (failure: unknown) => failure as Error & { node?: ChildProcess },
            );
            expect(error).toBeInstanceOf(Error);
            // The banner is only printed after a successful bind, so a bind
            // failure surfaces as a pre-readiness exit with the bind error.
            expect(error?.message).toMatch(/exited before becoming ready/);
            expect(error?.message).toMatch(/Failed to bind/);
            expect(exited(error?.node)).toBe(true);
        } finally {
            await workspace.dispose();
            await close(holder);
        }
    });

    it('binds a genuine ephemeral port and advertises the actual bound port', async () => {
        const workspace = await SmokeWorkspace.create({ name: 'dev76-ephemeral-' });
        try {
            const server = await startLotarServer(workspace);
            expect(server.port).toBeGreaterThan(0);
            expect(server.url).toBe(`http://127.0.0.1:${server.port}`);
            const response = await fetch(`${server.url}/api/openapi.json`);
            expect(response.status).toBe(200);
            await server.stop();
            expect(exited(server.raw.nodeChildProcess)).toBe(true);
        } finally {
            await workspace.dispose();
        }
    });

    it('serves the UI from LOTAR_WEB_UI_PATH when the environment sets it (DEV-334)', async () => {
        const workspace = await SmokeWorkspace.create({ name: 'dev334-web-ui-' });
        const webUi = path.join(workspace.root, 'fresh-web');
        mkdirSync(webUi);
        writeFileSync(path.join(webUi, 'index.html'), '<!doctype html><title>dev334 fresh build</title>');
        vi.stubEnv('LOTAR_WEB_UI_EMBEDDED', '0');
        vi.stubEnv('LOTAR_WEB_UI_PATH', webUi);
        try {
            const server = await startLotarServer(workspace);
            try {
                const response = await fetch(server.url);
                expect(await response.text()).toContain('dev334 fresh build');
            } finally {
                await server.stop();
            }
        } finally {
            vi.unstubAllEnvs();
            await workspace.dispose();
        }
    });

    it('accepts an explicit free port and reports it back', async () => {
        const workspace = await SmokeWorkspace.create({ name: 'dev76-explicit-' });
        try {
            // DEV-136: a probed port can be taken by another process before the
            // server binds it; that is a genuine bind failure, not a harness
            // bug. Probe a fresh port in that case (bounded, bind failures only).
            for (let attempt = 1; ; attempt += 1) {
                const { port } = await resolveServerBind();
                const started = await startLotarServer(workspace, { port }).then(
                    (server) => ({ server }),
                    (error: unknown) => ({ error: error as Error }),
                );
                if ('error' in started) {
                    if (attempt < 3 && /Failed to bind/.test(started.error.message)) continue;
                    throw started.error;
                }
                expect(started.server.port).toBe(port);
                await started.server.stop();
                break;
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('stops the servers of a run that is killed (DEV-471)', async () => {
        const workspace = await SmokeWorkspace.create({ name: 'dev471-reaper-' });
        const registry = mkdtempSync(path.join(tmpdir(), 'dev471-registry-'));
        vi.stubEnv(SERVER_REGISTRY_ENV, registry);
        // Stands in for the smoke run: it starts the reaper the way the global setup does.
        const guard = pathToFileURL(path.resolve(import.meta.dirname, '../helpers/server-guard.ts')).href;
        const run = spawn(
            process.execPath,
            [
                '--input-type=module',
                '-e',
                `import { startServerReaper } from '${guard}'; startServerReaper(process.argv[1]); console.log('ready'); setInterval(() => {}, 1000);`,
                registry,
            ],
            { stdio: ['ignore', 'pipe', 'inherit'] },
        );
        try {
            const [ready] = (await once(run.stdout, 'data')) as [Buffer];
            expect(ready.toString()).toContain('ready');
            const server = await startLotarServer(workspace);
            try {
                const node = server.raw.nodeChildProcess;
                expect(readdirSync(registry)).toEqual([String(node.pid)]);

                run.kill('SIGKILL');
                await expect.poll(() => exited(node), { timeout: HANG_GUARD_MS }).toBe(true);
                expect(existsSync(registry)).toBe(false);
            } finally {
                await server.stop();
            }
        } finally {
            run.kill('SIGKILL');
            vi.unstubAllEnvs();
            rmSync(registry, { recursive: true, force: true });
            await workspace.dispose();
        }
    });
});
