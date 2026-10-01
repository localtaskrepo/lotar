#!/usr/bin/env node
// Gated mock agent fixture: a Copilot-wire mock whose run is held on a
// test-owned gate file so tests can observe and assert mid-run state
// deterministically instead of racing fixed sleeps.
//
// Usage (unknown argv entries such as the runner's own flags and the
// trailing prompt are ignored):
//   --gate-dir <dir>   directory holding the gate and sentinel files (required)
//   --label <name>     sentinel name suffix, unique per concurrent job
//   --mode <kind>      copilot | hold | plain (default copilot)
//   --hold-ms <n>      bounded self-exit while waiting for release (default 20000)
//   --exit <code>      exit code after the gate is released (default 0)
//
// Protocol:
//   1. emit the pre-gate runner lines for the selected mode
//   2. write started-<label>.sentinel
//   3. poll for release.sentinel until it appears or the hold bound elapses
//   4. emit the post-gate runner lines, write completed-<label>.sentinel,
//      and exit with the requested code
//
// The hold bound is a child-owned leak guard, not a test timing knob: the
// owning test releases the gate by writing release.sentinel, so the happy
// path never waits for the bound. Sentinel writes are best-effort so a
// removed gate directory cannot crash an orphaned child.

import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';

function parseArgs(argv) {
    const parsed = {};
    for (let i = 0; i < argv.length; i += 1) {
        const arg = argv[i];
        if (['--gate-dir', '--label', '--mode', '--hold-ms', '--exit'].includes(arg)) {
            const value = argv[i + 1];
            if (value !== undefined) {
                parsed[arg] = value;
                i += 1;
            }
        }
    }
    return parsed;
}

const options = parseArgs(process.argv.slice(2));
const gateDir = options['--gate-dir'];
const label = options['--label'] ?? 'job';
const mode = options['--mode'] ?? 'copilot';
const requestedHold = Number.parseInt(options['--hold-ms'] ?? '20000', 10);
const holdMs = Number.isInteger(requestedHold) ? requestedHold : 20000;
const requestedExit = Number.parseInt(options['--exit'] ?? '0', 10);
const exitCode = Number.isInteger(requestedExit) ? requestedExit : 0;

if (!gateDir) {
    console.error('gate-agent: --gate-dir is required');
    process.exit(2);
}
if (!['copilot', 'hold', 'plain'].includes(mode)) {
    console.error(`gate-agent: unknown mode ${mode}`);
    process.exit(2);
}

const sessionId = `gate-${label}-${process.pid}`;

function writeSentinel(name) {
    try {
        mkdirSync(gateDir, { recursive: true });
        writeFileSync(path.join(gateDir, name), `${process.pid}\n`);
    } catch {
        // Best effort: an orphaned child must not crash on a removed directory.
    }
}

function emit(payload) {
    process.stdout.write(`${JSON.stringify(payload)}\n`);
}

if (mode === 'copilot') {
    emit({ type: 'system', subtype: 'init', session_id: sessionId });
    emit({ type: 'message', delta: true, content: 'phase one', session_id: sessionId });
} else if (mode === 'hold') {
    emit({ type: 'system', subtype: 'init', session_id: sessionId });
}
writeSentinel(`started-${label}.sentinel`);

const releasePath = path.join(gateDir, 'release.sentinel');
const deadline = Date.now() + holdMs;
while (!existsSync(releasePath) && Date.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, 25));
}

const tail = [];
if (mode === 'copilot') {
    tail.push({ type: 'message', delta: true, content: ' / phase two', session_id: sessionId });
    tail.push({ type: 'message', content: 'phase one / phase two done', session_id: sessionId });
} else if (mode === 'hold') {
    tail.push({ type: 'message', content: 'held agent released', session_id: sessionId });
}
tail.push({ type: 'result', result: 'Completed successfully', session_id: sessionId });

// Exit from the write callback so piped stdout is flushed before exit and
// the completion sentinel never precedes the emitted runner lines.
process.stdout.write(`${tail.map((event) => JSON.stringify(event)).join('\n')}\n`, () => {
    writeSentinel(`completed-${label}.sentinel`);
    process.exit(exitCode);
});
