#!/usr/bin/env node
// Gated mock agent fixture: a Copilot CLI (`--output-format json`) mock whose run is held on a
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
//   0. write boot-<label>.sentinel (earliest possible diagnostics marker:
//      pid, platform, full argv — written before validation can fail so a
//      missing started marker is provably either "process never reached the
//      script" (no boot marker anywhere) or "booted but held/failed")
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

function writeMarker(name, payload) {
    try {
        mkdirSync(gateDir, { recursive: true });
        writeFileSync(path.join(gateDir, name), `${JSON.stringify(payload)}\n`);
    } catch {
        // Best effort: diagnostics must never crash the fixture.
    }
}

// Step 0: prove the process booted and record how it was invoked, before
// the gate-dir/mode validation below can reject a mangled argv.
writeMarker(`boot-${label}.sentinel`, {
    pid: process.pid,
    platform: process.platform,
    argv: process.argv,
    gateDir: gateDir ?? null,
    mode,
    holdMs,
});

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
    emit({ type: 'user.message', data: { content: 'gate prompt' } });
    emit({ type: 'tool.execution_start', data: { toolName: 'bash', arguments: { command: 'phase one' } } });
} else if (mode === 'hold') {
    emit({ type: 'user.message', data: { content: 'gate prompt' } });
}
writeSentinel(`started-${label}.sentinel`);

const releasePath = path.join(gateDir, 'release.sentinel');
const deadline = Date.now() + holdMs;
while (!existsSync(releasePath) && Date.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, 25));
}

const tail = [];
if (mode === 'copilot') {
    tail.push({ type: 'tool.execution_start', data: { toolName: 'bash', arguments: { command: 'phase two' } } });
    tail.push({ type: 'assistant.message', data: { content: 'phase one / phase two done' } });
} else if (mode === 'hold') {
    tail.push({ type: 'assistant.message', data: { content: 'held agent released' } });
}
tail.push({ type: 'result', sessionId, exitCode, usage: { premiumRequests: 0 } });

// Exit from the write callback so piped stdout is flushed before exit and
// the completion sentinel never precedes the emitted runner lines.
process.stdout.write(`${tail.map((event) => JSON.stringify(event)).join('\n')}\n`, () => {
    writeSentinel(`completed-${label}.sentinel`);
    process.exit(exitCode);
});
