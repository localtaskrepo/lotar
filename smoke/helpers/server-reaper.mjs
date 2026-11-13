#!/usr/bin/env node
// Stops the servers a smoke run left behind (DEV-471). The run starts this
// process detached with a pipe on stdin and registers every server it spawns
// as a file named after its pid in the registry folder (argv[2]). When the
// pipe closes (the run ended, failed or was killed, even with SIGKILL), every
// server still registered is killed and the folder is removed.
import { readdirSync, rmSync } from 'node:fs';
import process from 'node:process';

const registry = process.argv[2];
if (!registry) {
    process.stderr.write('usage: server-reaper.mjs <registry-dir>\n');
    process.exit(2);
}

function reap() {
    let names = [];
    try {
        names = readdirSync(registry);
    } catch {
        // Nothing was registered.
    }
    for (const name of names) {
        const pid = Number.parseInt(name, 10);
        if (Number.isInteger(pid) && pid > 0) {
            try {
                process.kill(pid, 'SIGKILL');
            } catch {
                // Already gone.
            }
        }
    }
    rmSync(registry, { recursive: true, force: true });
    process.exit(0);
}

process.stdin.on('error', reap);
process.stdin.on('end', reap);
process.stdin.on('close', reap);
process.stdin.resume();
