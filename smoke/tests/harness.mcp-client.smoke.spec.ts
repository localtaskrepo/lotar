import path from 'node:path';
import process from 'node:process';
import { describe, expect, it } from 'vitest';
import { execa } from 'execa';
import { fileURLToPath } from 'node:url';
import { FramedMcpClient, NdjsonMcpClient } from '../helpers/mcp.js';

const FIXTURE = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', 'helpers', 'fake-mcp-child.mjs');

function spawnFakeClient(flags: readonly string[] = [], options: { maxLineBytes?: number } = {}): NdjsonMcpClient {
    const child = execa(process.execPath, [FIXTURE, ...flags], {
        stdin: 'pipe',
        stdout: 'pipe',
        stderr: 'pipe',
    });
    return new NdjsonMcpClient(child, options);
}

function spawnFakeFramedClient(flags: readonly string[] = []): FramedMcpClient {
    const child = execa(process.execPath, [FIXTURE, '--framed', ...flags], {
        stdin: 'pipe',
        stdout: 'pipe',
        stderr: 'pipe',
    });
    return new FramedMcpClient(child);
}

async function settledOutcome(client: NdjsonMcpClient): Promise<{ exitCode: number | null; signal: string | null }> {
    const node = client.childProcess.nodeChildProcess;
    return { exitCode: node.exitCode, signal: node.signalCode };
}

describe('MCP NDJSON smoke client (fake child processes)', () => {
    it('reassembles messages split mid-UTF-8 across chunks', async () => {
        const client = spawnFakeClient();
        try {
            const text = '默认标题 🎉 ünïcödé — ok';
            await client.send({ jsonrpc: '2.0', id: 1, method: 'multibyte', params: { text, chunkBytes: 3 } });
            const response = await client.awaitResponse(1);
            expect(response.error).toBeUndefined();
            expect(response.result.text).toBe(text);
            expect(response.result.repeated).toBe(`${text} ${text}`);
        } finally {
            await client.dispose();
        }
    });

    it('keeps unmatched notifications while matching responses by id', async () => {
        const client = spawnFakeClient();
        try {
            await client.send({ jsonrpc: '2.0', id: 7, method: 'notify-and-echo', params: { marker: 'first' } });
            const response = await client.awaitResponse(7);
            expect(response.result.echo.marker).toBe('first');

            await client.send({ jsonrpc: '2.0', id: 8, method: 'notify-and-echo', params: { marker: 'second' } });
            await client.send({ jsonrpc: '2.0', id: 9, method: 'notify-and-echo', params: { marker: 'third' } });

            const second = await client.awaitResponse(8);
            expect(second.result.echo.marker).toBe('second');

            const third = await client.awaitResponse(9);
            expect(third.result.echo.marker).toBe('third');

            const notifications = await Promise.all([
                client.readUntil((message) => message.method === 'notifications/fake' && message.params?.about === 7),
                client.readUntil((message) => message.method === 'notifications/fake' && message.params?.about === 8),
                client.readUntil((message) => message.method === 'notifications/fake' && message.params?.about === 9),
            ]);
            expect(notifications.map((notification) => notification.params.about)).toEqual([7, 8, 9]);
            expect(notifications.map((notification) => notification.params.seq)).toEqual([1, 2, 3]);
        } finally {
            await client.dispose();
        }
    });

    it('rejects oversized lines without unbounded buffering', async () => {
        const client = spawnFakeClient([], { maxLineBytes: 1024 });
        try {
            await client.send({ jsonrpc: '2.0', id: 1, method: 'big', params: { size: 4096 } });
            await expect(client.awaitResponse(1, 5000)).rejects.toThrow(/exceeded maximum/);
            await expect(client.send({ jsonrpc: '2.0', id: 2, method: 'echo' })).rejects.toThrow();
        } finally {
            await client.dispose();
        }
    });

    it('rejects pending waits when the server dies, with stderr context', async () => {
        const client = spawnFakeClient();
        try {
            await client.send({ jsonrpc: '2.0', id: 1, method: 'die', params: { code: 3 } });
            await expect(client.awaitResponse(1, 10_000)).rejects.toThrow(/stdout closed/);
        } finally {
            await client.dispose();
        }
    });

    it('times out waiting for a hung response', async () => {
        const client = spawnFakeClient();
        try {
            await client.send({ jsonrpc: '2.0', id: 1, method: 'hang' });
            await expect(client.awaitResponse(1, 300)).rejects.toThrow(/Timed out/);
        } finally {
            await client.dispose();
        }
    });

    it('drains large stderr floods while staying responsive', async () => {
        const client = spawnFakeClient();
        try {
            await client.send({ jsonrpc: '2.0', id: 1, method: 'stderr-flood', params: { bytes: 2 * 1024 * 1024 } });
            const response = await client.awaitResponse(1, 20_000);
            expect(response.result.flooded).toBe(2 * 1024 * 1024);
            expect(client.stderrTail.length).toBeLessThanOrEqual(16_000);
        } finally {
            await client.dispose();
        }
    });

    it('reaps a well-behaved child via stdin close alone', async () => {
        const client = spawnFakeClient();
        await client.send({ jsonrpc: '2.0', id: 1, method: 'echo' });
        await client.awaitResponse(1);
        await client.dispose();
        const outcome = await settledOutcome(client);
        expect(outcome.exitCode).toBe(0);
        expect(outcome.signal).toBeNull();
    });

    it('escalates to SIGTERM for a child that ignores stdin close', async () => {
        const client = spawnFakeClient(['--no-exit-on-stdin']);
        await client.send({ jsonrpc: '2.0', id: 1, method: 'echo' });
        await client.awaitResponse(1);
        await client.dispose();
        const outcome = await settledOutcome(client);
        expect(outcome.signal).toBe('SIGTERM');
    });

    it('escalates to SIGKILL for a child that ignores SIGTERM', async () => {
        const client = spawnFakeClient(['--no-exit-on-stdin', '--ignore-sigterm']);
        await client.send({ jsonrpc: '2.0', id: 1, method: 'echo' });
        await client.awaitResponse(1);
        await client.dispose();
        const outcome = await settledOutcome(client);
        expect(outcome.signal).toBe('SIGKILL');
    });
});

describe('MCP framed smoke client (fake child processes)', () => {
    it('parses Content-Length frames split across chunks and keeps unmatched frames', async () => {
        const client = spawnFakeFramedClient();
        try {
            await client.send({ jsonrpc: '2.0', id: 1, method: 'notify-and-echo', params: { marker: 'framed', chunkBytes: 5 } });
            const frame = await client.readUntil((candidate) => candidate.message?.id === 1);
            expect(frame.message.result.echo.marker).toBe('framed');
            expect(frame.headers).toMatch(/^Content-Length:\s*\d+$/i);
            expect(frame.bodyText).toBe(JSON.stringify({ jsonrpc: '2.0', id: 1, result: { echo: { marker: 'framed', chunkBytes: 5 } } }));

            const notification = await client.readUntil(
                (candidate) => candidate.message?.method === 'notifications/fake' && candidate.message?.params?.about === 1,
            );
            expect(notification.message.params.seq).toBe(1);
            expect(notification.headers).toMatch(/^Content-Length:\s*\d+$/i);
        } finally {
            await client.dispose();
        }
    });

    it('fails pending framed reads promptly when the child exits', async () => {
        const client = spawnFakeFramedClient();
        try {
            await client.send({ jsonrpc: '2.0', id: 1, method: 'hang' });
            const pending = client.readFrame(20_000);
            await client.send({ jsonrpc: '2.0', id: 2, method: 'die', params: { code: 3 } });
            await expect(pending).rejects.toThrow(/closed before the awaited frame/);
        } finally {
            await client.dispose();
        }
    });

    it('rejects framed sends after the stream has failed', async () => {
        const client = spawnFakeFramedClient();
        try {
            await client.send({ jsonrpc: '2.0', id: 1, method: 'die', params: { code: 3 } });
            await expect(client.readFrame(20_000)).rejects.toThrow(/closed before the awaited frame/);
            await expect(client.send({ jsonrpc: '2.0', id: 2, method: 'echo' })).rejects.toThrow();
        } finally {
            await client.dispose();
        }
    });
});
