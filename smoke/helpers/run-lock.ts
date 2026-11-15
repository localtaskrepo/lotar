// Vitest global setup for the smoke suite (DEV-586): one smoke run per machine at a time. Agents in other worktrees
// run the suite too; two runs side by side start dozens of Chromium instances each, and the starved browsers then miss
// their 30 s action timeouts. The lock is a listening port, so the OS frees it when the run ends or is killed. A nested run
// (a smoke test that runs Vitest) inherits LOCK_HELD_ENV and skips it.
import { createServer, type Server } from 'node:net';
import process from 'node:process';
import { setTimeout as sleep } from 'node:timers/promises';

export const SMOKE_LOCK_PORT = 47_586;
const LOCK_HELD_ENV = 'LOTAR_SMOKE_LOCK_HELD';
const MAX_WAIT_MS = 30 * 60_000;

function listen(): Promise<Server | undefined> {
    return new Promise((resolve, reject) => {
        const server = createServer();
        server.once('error', (error: NodeJS.ErrnoException) =>
            error.code === 'EADDRINUSE' ? resolve(undefined) : reject(error),
        );
        server.listen({ port: SMOKE_LOCK_PORT, host: '127.0.0.1', exclusive: true }, () => {
            server.unref();
            resolve(server);
        });
    });
}

export default async function setup() {
    if (process.env[LOCK_HELD_ENV]) {
        return undefined;
    }
    const deadline = Date.now() + MAX_WAIT_MS;
    let lock = await listen();
    if (!lock) {
        console.warn(`Waiting for another smoke run on this machine to finish (it holds port ${SMOKE_LOCK_PORT})…`);
    }
    while (!lock) {
        if (Date.now() > deadline) {
            throw new Error(
                `Another smoke run held port ${SMOKE_LOCK_PORT} for ${MAX_WAIT_MS / 60_000} minutes; find it with \`lsof -i :${SMOKE_LOCK_PORT}\`.`,
            );
        }
        await sleep(2_000);
        lock = await listen();
    }
    process.env[LOCK_HELD_ENV] = '1';
    const held = lock;
    return () => new Promise<void>((resolve) => held.close(() => resolve()));
}
