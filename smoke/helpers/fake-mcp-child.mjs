#!/usr/bin/env node
import readline from 'node:readline';

const args = new Set(process.argv.slice(2));
const ignoreSigterm = args.has('--ignore-sigterm');
const noExitOnStdinEnd = args.has('--no-exit-on-stdin');
const framedOutput = args.has('--framed');

if (ignoreSigterm) {
    process.on('SIGTERM', () => {
        process.stderr.write('fake-mcp-child: ignoring SIGTERM\n');
    });
}

let notificationSeq = 0;
let writeQueue = Promise.resolve();

function enqueueWrite(producer) {
    writeQueue = writeQueue.then(() => producer());
}

function writeChunked(text, chunkBytes) {
    return new Promise((resolve) => {
        const body = Buffer.from(text, 'utf8');
        const buffer = framedOutput
            ? Buffer.concat([Buffer.from(`Content-Length: ${body.length}\r\n\r\n`, 'utf8'), body])
            : Buffer.from(`${text}\n`, 'utf8');
        let offset = 0;
        const step = () => {
            if (offset >= buffer.length) {
                resolve();
                return;
            }
            const start = offset;
            const end = Math.min(start + chunkBytes, buffer.length);
            offset = end;
            const canContinue = process.stdout.write(buffer.subarray(start, end));
            if (canContinue) {
                setImmediate(step);
            } else {
                process.stdout.once('drain', () => setImmediate(step));
            }
        };
        step();
    });
}

function respond(request, result) {
    const line = JSON.stringify({ jsonrpc: '2.0', id: request.id, result });
    const chunkBytes = Number(request?.params?.chunkBytes ?? 7);
    enqueueWrite(() => writeChunked(line, chunkBytes));
}

function notify(method, params) {
    notificationSeq += 1;
    const line = JSON.stringify({
        jsonrpc: '2.0',
        method,
        params: { seq: notificationSeq, ...params },
    });
    enqueueWrite(() => writeChunked(line, 4096));
}

async function handle(request) {
    if (request?.id === undefined || request?.id === null) {
        return;
    }
    switch (request.method) {
        case 'echo': {
            respond(request, { echo: request.params ?? {} });
            break;
        }
        case 'notify-and-echo': {
            notify('notifications/fake', { about: request.id });
            respond(request, { echo: request.params ?? {} });
            break;
        }
        case 'multibyte': {
            const text = request?.params?.text ?? '默认 🎉 ünïcödé ok';
            respond(request, { text, repeated: `${text} ${text}` });
            break;
        }
        case 'big': {
            const size = Number(request?.params?.size ?? 8192);
            const filler = 'x'.repeat(Math.max(0, size));
            respond(request, { filler });
            break;
        }
        case 'hang': {
            break;
        }
        case 'die': {
            process.exit(Number(request?.params?.code ?? 1));
        }
        case 'exit-after': {
            respond(request, { bye: true });
            setImmediate(() => process.exit(Number(request?.params?.code ?? 0)));
            break;
        }
        case 'stderr-flood': {
            const bytes = Number(request?.params?.bytes ?? 1 << 21);
            const chunk = Buffer.alloc(64 * 1024, 0x65);
            let remaining = bytes;
            const pump = () => {
                if (remaining <= 0) {
                    respond(request, { flooded: bytes });
                    return;
                }
                const n = Math.min(chunk.length, remaining);
                remaining -= n;
                if (process.stderr.write(chunk.subarray(0, n))) {
                    setImmediate(pump);
                } else {
                    process.stderr.once('drain', () => setImmediate(pump));
                }            };
            pump();
            break;
        }
        default: {
            const line = JSON.stringify({
                jsonrpc: '2.0',
                id: request.id,
                error: { code: -32601, message: `Method not found: ${String(request.method)}` },
            });
            enqueueWrite(() => writeChunked(line, 4096));
        }
    }
}

function handleStdinLine(trimmed) {
    if (!trimmed) {
        return;
    }
    let parsed;
    try {
        parsed = JSON.parse(trimmed);
    } catch (error) {
        process.stderr.write(`fake-mcp-child: bad line: ${trimmed.slice(0, 120)}: ${String(error)}\n`);
        return;
    }
    void handle(parsed);
}

function readFramedStdin() {
    let buffered = Buffer.alloc(0);
    process.stdin.on('data', (chunk) => {
        buffered = Buffer.concat([buffered, chunk]);
        while (true) {
            const separator = buffered.indexOf('\r\n\r\n');
            if (separator === -1) {
                return;
            }
            const headers = buffered.subarray(0, separator).toString('utf8');
            const match = headers.match(/content-length:\s*(\d+)/i);
            const bodyStart = separator + 4;
            if (!match) {
                buffered = buffered.subarray(bodyStart);
                continue;
            }
            const length = Number(match[1]);
            if (buffered.length - bodyStart < length) {
                return;
            }
            const body = buffered.subarray(bodyStart, bodyStart + length).toString('utf8');
            buffered = buffered.subarray(bodyStart + length);
            handleStdinLine(body.trim());
        }
    });
    process.stdin.on('end', () => {
        if (!noExitOnStdinEnd) {
            process.exit(0);
        }
    });
}

if (framedOutput) {
    readFramedStdin();
} else {
    const rl = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
    rl.on('line', (line) => handleStdinLine(line.trim()));
    rl.on('close', () => {
        if (!noExitOnStdinEnd) {
            process.exit(0);
        }
    });
}

if (noExitOnStdinEnd) {
    // Keep the event loop alive so only signals (or an explicit die/exit-after
    // request) can end this child; used to exercise the dispose escalation
    // ladder.
    setInterval(() => {}, 1 << 30);
}
