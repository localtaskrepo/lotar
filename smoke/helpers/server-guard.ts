// Vitest global setup for the smoke suite (DEV-471): a run stops the servers it started, also when it fails or is
// killed. Servers register in a folder that a detached reaper watches; the reaper kills what is left when the run's
// process goes away. A clean run has none left, so the teardown fails the run when tests leaked a server.
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdtempSync, readdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

export const SERVER_REGISTRY_ENV = 'LOTAR_SMOKE_SERVER_REGISTRY';

const REAPER = path.join(path.dirname(fileURLToPath(import.meta.url)), 'server-reaper.mjs');

/** Start a reaper for `registry`; ending its stdin (or this process dying) kills every server registered there. */
export function startServerReaper(registry: string) {
    const reaper = spawn(process.execPath, [REAPER, registry], {
        detached: true,
        stdio: ['pipe', 'ignore', 'ignore'],
    });
    reaper.unref();
    // The pipe must not keep the run alive; it closes when this process exits.
    (reaper.stdin as unknown as { unref(): void }).unref();
    return reaper;
}

export default function setup() {
    const registry = mkdtempSync(path.join(tmpdir(), 'lotar-smoke-servers-'));
    process.env[SERVER_REGISTRY_ENV] = registry;
    const reaper = startServerReaper(registry);

    return async () => {
        const leftovers = readdirSync(registry);
        const exited = once(reaper, 'exit');
        reaper.ref();
        reaper.stdin.end();
        await exited;
        if (leftovers.length > 0) {
            throw new Error(
                `Tests left ${leftovers.length} servers running (pids ${leftovers.join(', ')}); they were killed. Stop each server where it is started.`,
            );
        }
    };
}
