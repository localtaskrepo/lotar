#!/usr/bin/env node
// Cross-platform mock agent: a `command` runner program, whose stdout lines
// LoTaR logs as the job's progress. Unknown argv entries are ignored.
// Recognized flags:
//   --exit <code>   exit with the given code after printing its lines

function parseArgs(argv) {
    const parsed = {};
    for (let i = 0; i < argv.length; i += 1) {
        const arg = argv[i];
        if (arg === '--exit') {
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

const lines = ['bash: echo Hello', 'Hello from mock agent!'];

// Write through a single buffered write and exit from its callback so
// piped stdout is flushed before the process exits.
process.stdout.write(`${lines.join('\n')}\n`, () => {
    process.exit(exitCode);
});
