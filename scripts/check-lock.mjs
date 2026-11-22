// The machine's check lock (DEV-784): one full test run at a time across agent jobs, worktrees and processes, so
// several jobs' suites don't load the machine far beyond its cores and time each other's tests out. lotar's own merge
// takes the same lock around its checks (crates/lotar-agent/src/services/agent_job_service/check_lock.rs).
//
// The lock is a listening socket on 127.0.0.1:47586 (the port of DEV-586's smoke lock), so the OS frees it when its
// holder ends or is killed. A holder answers a connection with one line that says who it is; a waiting run prints it.
// A test run's own processes never take the lock: they get LOTAR_CHECK_LOCK_HELD=1, whether the run holds it or is a
// targeted run that doesn't need it. A run under LOTAR_CHECK_LOCK_HELD=1 (inside lotar's merge checks, or a full
// `npm test` holding it already) doesn't wait for it again.
//
// The default export is the Vitest global setup of the UI and smoke suites: full runs take the lock, runs filtered by
// file or test name don't.
import { createConnection, createServer } from 'node:net';
import path from 'node:path';
import process from 'node:process';
import { setTimeout as sleep } from 'node:timers/promises';

export const CHECK_LOCK_PORT = 47_586;
export const CHECK_LOCK_HELD_ENV = 'LOTAR_CHECK_LOCK_HELD';
const MAX_WAIT_MS = 90 * 60_000;
const RETRY_MS = 2_000;
const HOLDER_TIMEOUT_MS = 1_000;

/** @returns {Promise<{ server: import('node:net').Server } | { busy: true } | { error: Error }>} */
function listen(port, line) {
    return new Promise((resolve) => {
        const server = createServer((socket) => socket.end(`${line}\n`));
        server.once('error', (/** @type {NodeJS.ErrnoException} */ error) =>
            resolve(error.code === 'EADDRINUSE' ? { busy: true } : { error }),
        );
        server.listen({ port, host: '127.0.0.1', exclusive: true }, () => {
            server.unref();
            resolve({ server });
        });
    });
}

/** The holder's line, or a generic name when it sends none. */
function holderOf(port) {
    return new Promise((resolve) => {
        let text = '';
        const done = () => resolve(text.trim() || 'another check run');
        const socket = createConnection({ port, host: '127.0.0.1' });
        socket.setEncoding('utf8');
        socket.setTimeout(HOLDER_TIMEOUT_MS, () => socket.destroy());
        socket.on('data', (/** @type {string} */ chunk) => {
            text += chunk;
        });
        socket.on('close', done);
        socket.on('error', () => {});
    });
}

function minutes(ms) {
    return ms < 60_000 ? `${Math.round(ms / 1000)} s` : `${Math.ceil(ms / 60_000)} min`;
}

/**
 * Take the lock, waiting while another run holds it. Sets LOTAR_CHECK_LOCK_HELD=1 in `env` and resolves to the
 * function that frees the lock.
 * @param {{ label: string, port?: number, env?: NodeJS.ProcessEnv, log?: (line: string) => void,
 *   maxWaitMs?: number, retryMs?: number }} options
 * @returns {Promise<() => Promise<void>>}
 */
export async function acquireCheckLock({
    label,
    port = CHECK_LOCK_PORT,
    env = process.env,
    log = (line) => process.stderr.write(`${line}\n`),
    maxWaitMs = MAX_WAIT_MS,
    retryMs = RETRY_MS,
}) {
    const free = async () => {};
    if (env[CHECK_LOCK_HELD_ENV] === '1') {
        return free;
    }
    const since = new Date().toISOString().slice(11, 16);
    const line = `${label} in ${process.cwd()} (pid ${process.pid}), since ${since} UTC`;
    const started = Date.now();
    let holder;
    for (;;) {
        const result = await listen(port, line);
        if ('server' in result) {
            if (holder !== undefined) {
                log(`Took the check lock after ${minutes(Date.now() - started)}`);
            }
            env[CHECK_LOCK_HELD_ENV] = '1';
            const { server } = result;
            return () => new Promise((resolve) => server.close(() => resolve()));
        }
        if ('error' in result) {
            log(`Running without the machine's check lock: port ${port} can't be opened (${result.error.message})`);
            env[CHECK_LOCK_HELD_ENV] = '1';
            return free;
        }
        if (holder === undefined) {
            holder = await holderOf(port);
            log(`Waiting for the machine's check lock (port ${port}), held by ${holder}`);
        }
        if (Date.now() - started > maxWaitMs) {
            throw new Error(
                `The machine's check lock (port ${port}) stayed held for ${minutes(maxWaitMs)} by ${holder}; find it with \`lsof -i :${port}\`.`,
            );
        }
        await sleep(retryMs);
    }
}

/** Whether a Vitest run is filtered by file or test name. `filenamePattern` is internal: without it, a run counts as full. */
export function isFilteredRun(project) {
    return Boolean(project?.vitest?.filenamePattern?.length || project?.config?.testNamePattern);
}

export default async function setup(project) {
    if (isFilteredRun(project)) {
        process.env[CHECK_LOCK_HELD_ENV] = '1';
        return undefined;
    }
    const suite = path.basename(project?.config?.root ?? process.cwd());
    return acquireCheckLock({ label: `the Vitest suite of ${suite}/` });
}
