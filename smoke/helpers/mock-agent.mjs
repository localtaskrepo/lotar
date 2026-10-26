#!/usr/bin/env node
// Cross-platform mock agent that emits GitHub Copilot CLI output
// (`copilot -p <prompt> --output-format json`), in the envelope a real run
// prints: `{ type, data }` lines ending with a `result` line.
//
// The product spawns `<node> <this script> <runner flags> <prompt>` and
// parses stdout lines as runner events. Unknown argv entries (the runner's
// own flags and the prompt) are ignored. Recognized flags:
//   --exit <code>   exit with the given code after emitting all events
//   --session <id>  use a fixed session id

function parseArgs(argv) {
    const parsed = {};
    for (let i = 0; i < argv.length; i += 1) {
        const arg = argv[i];
        if (arg === '--exit' || arg === '--session') {
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
const requestedExit = Number.parseInt(options['--exit'] ?? '0', 10);
const exitCode = Number.isInteger(requestedExit) ? requestedExit : 0;
const sessionId = options['--session'] ?? `test-session-${process.pid}-${Date.now()}`;

const events = [
    { type: 'user.message', data: { content: 'mock prompt' } },
    { type: 'tool.execution_start', data: { toolName: 'bash', arguments: { command: 'echo Hello' } } },
    { type: 'assistant.message', data: { content: 'Hello from mock agent!' } },
    { type: 'assistant.turn_end', data: { turnId: '0' } },
    { type: 'result', sessionId, exitCode, usage: { premiumRequests: 0 } },
];

// Write through a single buffered write and exit from its callback so
// piped stdout is flushed before the process exits.
process.stdout.write(`${events.map((event) => JSON.stringify(event)).join('\n')}\n`, () => {
    process.exit(exitCode);
});
