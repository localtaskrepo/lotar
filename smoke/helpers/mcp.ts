import type { ExecaChildProcess } from 'execa';

export interface McpFrame {
    readonly headers: string;
    readonly bodyText: string;
    readonly message: any;
}

export class FramedMcpClient {
    private buffer = Buffer.alloc(0);
    private readonly waiters: Array<() => void> = [];
    private dataVersion = 0;

    constructor(private readonly child: ExecaChildProcess) {
        if (!child.stdin || !child.stdout) {
            throw new Error('Framed MCP client requires piped stdio.');
        }

        child.stdout.on('data', (chunk: Buffer | string) => {
            const data = typeof chunk === 'string' ? Buffer.from(chunk, 'utf8') : chunk;
            this.buffer = Buffer.concat([this.buffer, data]);
            this.dataVersion += 1;
            this.flushWaiters();
        });
    }

    async send(message: Record<string, unknown>): Promise<void> {
        if (!this.child.stdin) {
            throw new Error('Cannot write to MCP process without stdin pipe.');
        }
        const payload = Buffer.from(JSON.stringify(message), 'utf8');
        const header = Buffer.from(`Content-Length: ${payload.length}\r\n\r\n`, 'utf8');
        this.child.stdin.write(header);
        this.child.stdin.write(payload);
    }

    async readFrame(timeoutMs = 20000): Promise<McpFrame> {
        const deadline = Date.now() + timeoutMs;
        while (true) {
            const frame = this.tryParseFrame();
            if (frame) {
                return frame;
            }
            const remaining = deadline - Date.now();
            if (remaining <= 0) {
                throw new Error('Timed out waiting for MCP frame');
            }
            const hasData = await this.waitForData(Math.min(remaining, 500), true);
            if (!hasData) {
                continue;
            }
        }
    }

    async readUntil(predicate: (frame: McpFrame) => boolean, timeoutMs = 20000): Promise<McpFrame> {
        const deadline = Date.now() + timeoutMs;
        while (true) {
            const frame = this.tryParseFrame();
            if (frame) {
                if (predicate(frame)) {
                    return frame;
                }
                continue;
            }

            const remaining = deadline - Date.now();
            if (remaining <= 0) {
                throw new Error('Timed out waiting for expected MCP frame');
            }

            await this.waitForData(Math.min(remaining, 500), true);
        }
    }

    async dispose(): Promise<void> {
        if (this.child.stdin && !this.child.stdin.destroyed) {
            this.child.stdin.end();
        }
        this.child.kill('SIGTERM');
        const timeout = setTimeout(() => {
            this.child.kill('SIGKILL');
        }, 2000);
        try {
            await this.child;
        } catch {
            // Ignore errors from terminating the child process during cleanup.
        } finally {
            clearTimeout(timeout);
        }
    }

    private tryParseFrame(): McpFrame | null {
        const separator = Buffer.from('\r\n\r\n', 'utf8');

        while (true) {
            const headerIndex = this.buffer.indexOf(separator);
            if (headerIndex === -1) {
                return null;
            }

            const headerBuffer = this.buffer.slice(0, headerIndex);
            const headers = headerBuffer.toString('utf8');
            const contentLengthMatch = headers.match(/Content-Length:\s*(\d+)/i);
            const bodyStart = headerIndex + separator.length;

            if (!contentLengthMatch) {
                // If stdout includes any non-framed output, discard up to the separator
                // so we can search for the next valid MCP frame.
                this.buffer = this.buffer.slice(bodyStart);
                continue;
            }

            const length = Number(contentLengthMatch[1]);
            if (!Number.isFinite(length) || length < 0) {
                this.buffer = this.buffer.slice(bodyStart);
                continue;
            }

            const bytesRemaining = this.buffer.length - bodyStart;
            if (bytesRemaining < length) {
                return null;
            }

            const bodyBuffer = this.buffer.slice(bodyStart, bodyStart + length);
            this.buffer = this.buffer.slice(bodyStart + length);
            const bodyText = bodyBuffer.toString('utf8');
            let message: any = null;
            try {
                message = JSON.parse(bodyText);
            } catch {
                // Leave message as null to help debugging when invalid JSON is encountered.
            }
            return {
                headers,
                bodyText,
                message,
            };
        }
    }

    private async waitForData(timeoutMs: number, requireNewData = false): Promise<boolean> {
        const startVersion = this.dataVersion;
        if (!requireNewData && this.buffer.length > 0) {
            return true;
        }
        return new Promise<boolean>((resolve) => {
            let settled = false;
            const timeoutHandle = setTimeout(() => {
                if (settled) {
                    return;
                }
                settled = true;
                const index = this.waiters.indexOf(notify);
                if (index >= 0) {
                    this.waiters.splice(index, 1);
                }
                resolve(false);
            }, timeoutMs);
            const notify = () => {
                if (settled) {
                    return;
                }
                settled = true;
                clearTimeout(timeoutHandle);
                resolve(true);
            };
            this.waiters.push(notify);

            // Avoid a race where data arrives between the initial checks and registering
            // the waiter.
            if (requireNewData) {
                if (this.dataVersion !== startVersion) {
                    this.flushWaiters();
                }
            } else if (this.buffer.length > 0) {
                this.flushWaiters();
            }
        });
    }

    private flushWaiters(): void {
        while (this.waiters.length) {
            const waiter = this.waiters.shift();
            waiter?.();
        }
    }
}
