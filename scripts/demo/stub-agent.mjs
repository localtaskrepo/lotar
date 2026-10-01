#!/usr/bin/env node
// Deterministic stand-in for a coding agent CLI, used only by the demo workspace
// (screenshots and README recordings). It speaks the Claude Code `stream-json`
// output protocol that LoTaR's `claude` runner parses, so the Agents page renders
// it exactly like a real run, but it never calls a model or edits files.
//
// LoTaR invokes it as `<command> -p --output-format stream-json ... <prompt>`;
// the runner flags are ignored. Optional environment (set in the agent profile):
//   DEMO_AGENT_STEP_MS    delay between log steps (default 700)
//   DEMO_AGENT_HOLD_FILE  while this file exists, pause before the final steps so a
//                         screenshot can catch the job mid-run

import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

const stepMs = Number(process.env.DEMO_AGENT_STEP_MS || 700);
const holdFile = process.env.DEMO_AGENT_HOLD_FILE || '';
const sessionId = 'demo-session-0001';

// The LoTaR agent wrapper exports the ticket id; fall back to the prompt text.
const prompt = process.argv[process.argv.length - 1] || '';
const ticket = process.env.LOTAR_TICKET_ID || (prompt.match(/\b[A-Z][A-Z0-9]{1,9}-\d+\b/) || ['the ticket'])[0];

function ticketTitle() {
    const [project, number] = ticket.split('-');
    const tasksDir = process.env.LOTAR_TASKS_DIR || path.join(process.cwd(), '.tasks');
    try {
        const raw = readFileSync(path.join(tasksDir, project, `${number}.yml`), 'utf8');
        return (raw.match(/^title:\s*(.+)$/m) || [])[1]?.replace(/^['"]|['"]$/g, '') || '';
    } catch {
        return '';
    }
}
const title = ticketTitle();

function emit(event) {
    process.stdout.write(`${JSON.stringify({ session_id: sessionId, ...event })}\n`);
}

function say(text) {
    emit({ type: 'assistant', message: { role: 'assistant', content: [{ type: 'text', text }] } });
}

function stream(text) {
    emit({ type: 'stream_event', event: { type: 'content_block_delta', delta: { type: 'text_delta', text } } });
}

const backoffSteps = [
    `Reading ${ticket} and its linked code references.`,
    'Found the retry loop in web/src/sync/queue.ts: it retries every 5s with no backoff.',
    'Plan: exponential backoff (base 500ms, cap 30s) with full jitter; reset after a successful flush.',
    'Editing web/src/sync/queue.ts …',
    'Added nextDelay(attempt) and replaced the fixed timer.',
    'Adding unit tests in web/src/sync/queue.test.ts …',
    'Running npm test -- sync',
];

const genericSteps = [
    `Reading ${ticket}${title ? ` "${title}"` : ''} and its references.`,
    'Searching the codebase for the affected code paths …',
    'Plan: reproduce with a failing test, apply the smallest fix, then widen coverage.',
    'Wrote a failing test that reproduces the issue.',
    'Applying the fix …',
    'Updating the related docs and changelog entry.',
    'Running npm test',
];

const steps = /backoff|retr(y|ies)/i.test(title) ? backoffSteps : genericSteps;

const finalSteps = [
    '✓ all tests passed',
    `Done. Changes are on branch agent/${ticket}; ready for review.`,
];

emit({ type: 'system', subtype: 'init', model: 'demo-stub', tools: [] });
for (const step of steps) {
    await sleep(stepMs);
    say(step);
}

// Stream a little partial output so the live log visibly ticks.
for (const chunk of ['  test suite ', '········ ', 'ok\n']) {
    await sleep(stepMs / 2);
    stream(chunk);
}

while (holdFile && existsSync(holdFile)) {
    await sleep(200);
}

for (const step of finalSteps) {
    await sleep(stepMs);
    say(step);
}

emit({ type: 'result', subtype: 'success', is_error: false, result: finalSteps[finalSteps.length - 1] });
