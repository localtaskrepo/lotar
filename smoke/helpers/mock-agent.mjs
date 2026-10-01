#!/usr/bin/env node
// Cross-platform mock agent that emits Copilot CLI stream-json output.
//
// The product spawns `<node> <this script> <runner flags> <prompt>` and
// parses stdout lines as runner events, so this plain Node script replaces
// the former bash mock-agent.sh without any bash/date/sleep dependency.
// Unknown argv entries (the runner's own flags and the trailing prompt)
// are ignored. Recognized flags:
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
    { type: 'system', subtype: 'init', session_id: sessionId },
    { type: 'message', delta: true, content: 'Hello', session_id: sessionId },
    { type: 'message', delta: true, content: ' from mock agent!', session_id: sessionId },
    { type: 'message', content: 'Hello from mock agent!', session_id: sessionId },
    { type: 'result', result: 'Completed successfully', session_id: sessionId },
];

// Write through a single buffered write and exit from its callback so
// piped stdout is flushed before the process exits.
process.stdout.write(`${events.map((event) => JSON.stringify(event)).join('\n')}\n`, () => {
    process.exit(exitCode);
});
