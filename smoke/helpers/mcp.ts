import { execa } from 'execa';

export type McpChildProcess = ReturnType<typeof execa>;

export type McpMessage = Record<string, any>;

export interface McpFrame {
    readonly headers: string;
    readonly bodyText: string;
    readonly message: any;
}

export const MCP_MAX_MESSAGE_BYTES = 10 * 1024 * 1024;

const DEFAULT_TIMEOUT_MS = 20_000;

class MessageQueue<T> {
    private readonly items: T[] = [];
    private readonly waiters: Array<{
        predicate: (item: T) => boolean;
        resolve: (item: T) => void;
        reject: (error: Error) => void;
        timer: ReturnType<typeof setTimeout>;
    }> = [];
    private failure: Error | null = null;

    push(item: T): void {
        this.items.push(item);
        this.flush();
    }

    tryTake(predicate: (item: T) => boolean): T | undefined {
        const index = this.items.findIndex(predicate);
        if (index === -1) {
            return undefined;
        }
        return this.items.splice(index, 1)[0];
    }

    async wait(predicate: (item: T) => boolean, timeoutMs: number, timeoutMessage: string): Promise<T> {
        const existing = this.tryTake(predicate);
        if (existing !== undefined) {
            return existing;
        }
        if (this.failure) {
            throw this.failure;
        }
        return new Promise<T>((resolve, reject) => {
            const waiter = {
                predicate,
                resolve,
                reject,
                timer: setTimeout(() => {
                    const index = this.waiters.indexOf(waiter);
                    if (index >= 0) {
                        this.waiters.splice(index, 1);
                    }
                    reject(new Error(timeoutMessage));
                }, timeoutMs),
            };
            this.waiters.push(waiter);
        });
    }

    fail(error: Error): void {
        if (this.failure) {
            return;
        }
        this.failure = error;
        while (this.waiters.length) {
            const waiter = this.waiters.shift();
            if (!waiter) {
                continue;
            }
            clearTimeout(waiter.timer);
            waiter.reject(error);
        }
    }

    get pending(): readonly T[] {
        return this.items;
    }

    private flush(): void {
        for (const waiter of [...this.waiters]) {
            const item = this.tryTake(waiter.predicate);
            if (item === undefined) {
                continue;
            }
            const index = this.waiters.indexOf(waiter);
            if (index >= 0) {
                this.waiters.splice(index, 1);
            }
            clearTimeout(waiter.timer);
            waiter.resolve(item);
        }
    }
}

export function mcpIdsMatch(actual: unknown, expected: unknown): boolean {
    return actual === expected;
}

function delay(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitForExit(child: McpChildProcess, timeoutMs: number): Promise<boolean> {
    const exited = Promise.resolve(child).then(
        () => true,
        () => true,
    );
    const timer = delay(timeoutMs).then(() => false);
    return await Promise.race([exited, timer]);
}

export async function disposeMcpChild(
    child: McpChildProcess,
    options: { gracefulMs?: number; terminateMs?: number } = {},
): Promise<void> {
    const gracefulMs = options.gracefulMs ?? 2_000;
    const terminateMs = options.terminateMs ?? 2_000;

    if (child.stdin && !child.stdin.destroyed && !child.stdin.writableEnded) {
        child.stdin.end();
    }

    if (await waitForExit(child, gracefulMs)) {
        return;
    }

    child.kill('SIGTERM');
    if (await waitForExit(child, terminateMs)) {
        return;
    }

    child.kill('SIGKILL');
    await waitForExit(child, terminateMs);
}

interface DrainedStderr {
    tail(): string;
}

function drainStderr(child: McpChildProcess, maxChars = 16_000): DrainedStderr {
    let tail = '';
    if (child.stderr) {
        child.stderr.setEncoding('utf8');
        child.stderr.on('data', (chunk: string) => {
            tail = (tail + chunk).slice(-maxChars);
        });
    }
    return { tail: () => tail };
}

export interface McpClientLike {
    send(message: Record<string, unknown>): Promise<void>;
    awaitResponse(id: unknown, timeoutMs?: number): Promise<any>;
    readUntil(predicate: (message: any) => boolean, timeoutMs?: number): Promise<any>;
    dispose(): Promise<void>;
}

export class NdjsonMcpClient implements McpClientLike {
    private readonly queue = new MessageQueue<McpMessage>();
    private readonly stderrDrain: DrainedStderr;
    private buffer = Buffer.alloc(0);
    private fatal: Error | null = null;
    private ended = false;

    constructor(
        private readonly child: McpChildProcess,
        options: { maxLineBytes?: number } = {},
    ) {
        if (!child.stdin || !child.stdout) {
            throw new Error('NDJSON MCP client requires piped stdio.');
        }
        // Mark execa's promise rejection as handled up front: failures are
        // surfaced through the queue below and dispose(), never as a process
        // level unhandled rejection between abnormal exit and cleanup.
        void child.catch(() => undefined);
        this.maxLineBytes = options.maxLineBytes ?? MCP_MAX_MESSAGE_BYTES;
        this.stderrDrain = drainStderr(child);

        child.stdout.on('data', (chunk: Buffer | string) => {
            if (this.fatal) {
                return;
            }
            const data = typeof chunk === 'string' ? Buffer.from(chunk, 'utf8') : chunk;
            this.buffer = Buffer.concat([this.buffer, data]);
            this.consumeLines();
        });

        child.stdout.on('end', () => {
            this.ended = true;
            if (this.buffer.length > 0 && this.buffer.toString('utf8').trim().length > 0) {
                this.fail(new Error(`MCP server stdout ended with an incomplete line: ${this.buffer.toString('utf8', 0, 200)}`));
                return;
            }
            this.queue.fail(this.eofError());
        });

        child.stdout.on('error', (error: Error) => this.fail(error));

        // execa >= 10 returns a Promise; exit events come from the underlying
        // Node child process.
        child.nodeChildProcess.on('exit', (code, signal) => {
            if (!this.ended) {
                this.fail(this.eofError(code, signal));
            }
        });
    }

    readonly maxLineBytes: number;

    get childProcess(): McpChildProcess {
        return this.child;
    }

    get stderrTail(): string {
        return this.stderrDrain.tail();
    }

    get pendingMessages(): readonly McpMessage[] {
        return this.queue.pending;
    }

    get failure(): Error | null {
        return this.fatal;
    }

    async send(message: Record<string, unknown>): Promise<void> {
        if (this.fatal) {
            throw this.fatal;
        }
        if (!this.child.stdin || this.child.stdin.destroyed) {
            throw new Error('Cannot write to MCP process without stdin pipe.');
        }
        await new Promise<void>((resolve, reject) => {
            this.child.stdin!.write(`${JSON.stringify(message)}\n`, 'utf8', (error) => {
                if (error) {
                    reject(error);
                    return;
                }
                resolve();
            });
        });
    }

    async awaitResponse(id: unknown, timeoutMs = DEFAULT_TIMEOUT_MS): Promise<McpMessage> {
        return await this.queue.wait(
            (message) => mcpIdsMatch(message?.id, id),
            timeoutMs,
            `Timed out waiting for MCP response with id ${JSON.stringify(id)}`,
        );
    }

    async readUntil(predicate: (message: any) => boolean, timeoutMs = DEFAULT_TIMEOUT_MS): Promise<McpMessage> {
        return await this.queue.wait(
            predicate,
            timeoutMs,
            'Timed out waiting for expected MCP message',
        );
    }

    async dispose(): Promise<void> {
        await disposeMcpChild(this.child);
    }

    private consumeLines(): void {
        while (true) {
            const newlineIndex = this.buffer.indexOf(0x0a);
            if (newlineIndex === -1) {
                if (this.buffer.length > this.maxLineBytes) {
                    this.fail(new Error(`MCP NDJSON line exceeded maximum of ${this.maxLineBytes} bytes without a newline`));
                }
                return;
            }
            if (newlineIndex > this.maxLineBytes) {
                this.fail(new Error(`MCP NDJSON line exceeded maximum of ${this.maxLineBytes} bytes`));
                return;
            }
            const lineBuffer = this.buffer.subarray(0, newlineIndex);
            this.buffer = this.buffer.subarray(newlineIndex + 1);
            const line = lineBuffer.toString('utf8').trim();
            if (!line) {
                continue;
            }
            let parsed: any;
            try {
                parsed = JSON.parse(line);
            } catch (error) {
                this.fail(new Error(`MCP server wrote a non-JSON line on stdout: ${line.slice(0, 200)}\n${String(error)}`));
                return;
            }
            if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) {
                this.fail(new Error(`MCP server wrote an unexpected stdout payload: ${line.slice(0, 200)}`));
                return;
            }
            this.queue.push(parsed);
        }
    }

    private fail(error: Error): void {
        if (this.fatal) {
            return;
        }
        this.fatal = error;
        this.queue.fail(error);
    }

    private eofError(code: number | null = null, signal: string | null = null): Error {
        const exit = signal ? `signal ${signal}` : `exit code ${code}`;
        const stderr = this.stderrDrain.tail();
        return new Error(`MCP server stdout closed before the awaited message arrived (process ended: ${exit}).\nstderr tail:\n${stderr}`);
    }
}

export class FramedMcpClient implements McpClientLike {
    private readonly queue = new MessageQueue<McpFrame>();
    private readonly stderrDrain: DrainedStderr;
    private buffer = Buffer.alloc(0);
    private fatal: Error | null = null;
    private ended = false;

    constructor(private readonly child: McpChildProcess) {
        if (!child.stdin || !child.stdout) {
            throw new Error('Framed MCP client requires piped stdio.');
        }
        // See NdjsonMcpClient: keep execa's promise rejection handled while
        // real failures surface through the queue and dispose().
        void child.catch(() => undefined);
        this.stderrDrain = drainStderr(child);

        child.stdout.on('data', (chunk: Buffer | string) => {
            if (this.fatal) {
                return;
            }
            const data = typeof chunk === 'string' ? Buffer.from(chunk, 'utf8') : chunk;
            this.buffer = Buffer.concat([this.buffer, data]);
            this.tryParseFrames();
        });

        child.stdout.on('end', () => {
            this.ended = true;
            if (this.buffer.length > 0) {
                this.fail(new Error(`MCP framed stream ended with ${this.buffer.length} unparsed bytes`));
                return;
            }
            this.fail(this.eofError());
        });

        child.stdout.on('error', (error: Error) => this.fail(error));

        // execa >= 10 returns a Promise; exit events come from the underlying
        // Node child process. Pending reads must fail promptly instead of
        // waiting out their timeout when the process is gone.
        child.nodeChildProcess.on('exit', (code, signal) => {
            if (!this.ended) {
                this.fail(this.eofError(code, signal));
            }
        });
    }

    get childProcess(): McpChildProcess {
        return this.child;
    }

    get stderrTail(): string {
        return this.stderrDrain.tail();
    }

    get pendingFrames(): readonly McpFrame[] {
        return this.queue.pending;
    }

    get failure(): Error | null {
        return this.fatal;
    }

    async send(message: Record<string, unknown>): Promise<void> {
        if (this.fatal) {
            throw this.fatal;
        }
        if (!this.child.stdin || this.child.stdin.destroyed) {
            throw new Error('Cannot write to MCP process without stdin pipe.');
        }
        const payload = Buffer.from(JSON.stringify(message), 'utf8');
        const header = Buffer.from(`Content-Length: ${payload.length}\r\n\r\n`, 'utf8');
        await this.writeChunk(header);
        await this.writeChunk(payload);
    }

    private writeChunk(buffer: Buffer): Promise<void> {
        return new Promise<void>((resolve, reject) => {
            this.child.stdin!.write(buffer, (error) => {
                if (error) {
                    reject(error);
                    return;
                }
                resolve();
            });
        });
    }

    async readFrame(timeoutMs = DEFAULT_TIMEOUT_MS): Promise<McpFrame> {
        return await this.queue.wait(
            () => true,
            timeoutMs,
            'Timed out waiting for MCP frame',
        );
    }

    async readUntil(predicate: (frame: McpFrame) => boolean, timeoutMs = DEFAULT_TIMEOUT_MS): Promise<McpFrame> {
        return await this.queue.wait(
            predicate,
            timeoutMs,
            'Timed out waiting for expected MCP frame',
        );
    }

    async awaitResponse(id: unknown, timeoutMs = DEFAULT_TIMEOUT_MS): Promise<any> {
        const frame = await this.queue.wait(
            (candidate) => mcpIdsMatch(candidate.message?.id, id),
            timeoutMs,
            `Timed out waiting for MCP response with id ${JSON.stringify(id)}`,
        );
        return frame.message;
    }

    async dispose(): Promise<void> {
        await disposeMcpChild(this.child);
    }

    private tryParseFrames(): void {
        const separator = Buffer.from('\r\n\r\n', 'utf8');

        while (true) {
            const headerIndex = this.buffer.indexOf(separator);
            if (headerIndex === -1) {
                if (this.buffer.length > MCP_MAX_MESSAGE_BYTES) {
                    this.fail(new Error('MCP framed header search exceeded maximum buffer size'));
                }
                return;
            }

            const headerBuffer = this.buffer.subarray(0, headerIndex);
            const headers = headerBuffer.toString('utf8');
            const contentLengthMatch = headers.match(/Content-Length:\s*(\d+)/i);
            const bodyStart = headerIndex + separator.length;

            if (!contentLengthMatch) {
                this.buffer = this.buffer.subarray(bodyStart);
                continue;
            }

            const length = Number(contentLengthMatch[1]);
            if (!Number.isFinite(length) || length < 0) {
                this.buffer = this.buffer.subarray(bodyStart);
                continue;
            }

            if (length > MCP_MAX_MESSAGE_BYTES) {
                this.fail(new Error(`MCP frame announced ${length} bytes, exceeding the ${MCP_MAX_MESSAGE_BYTES} byte maximum`));
                return;
            }

            const bytesRemaining = this.buffer.length - bodyStart;
            if (bytesRemaining < length) {
                return;
            }

            const bodyBuffer = this.buffer.subarray(bodyStart, bodyStart + length);
            this.buffer = this.buffer.subarray(bodyStart + length);
            const bodyText = bodyBuffer.toString('utf8');
            let message: any = null;
            try {
                message = JSON.parse(bodyText);
            } catch {
                message = null;
            }
            this.queue.push({ headers, bodyText, message });
        }
    }

    private fail(error: Error): void {
        if (this.fatal) {
            return;
        }
        this.fatal = error;
        this.queue.fail(error);
    }

    private eofError(code: number | null = null, signal: string | null = null): Error {
        const exit = signal ? `signal ${signal}` : `exit code ${code}`;
        const stderr = this.stderrDrain.tail();
        return new Error(`MCP framed stream closed before the awaited frame arrived (process ended: ${exit}).\nstderr tail:\n${stderr}`);
    }
}
