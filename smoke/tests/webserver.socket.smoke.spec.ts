import { existsSync } from 'node:fs';
import net from 'node:net';
import { once } from 'node:events';
import { describe, expect, it } from 'vitest';
import { resolveBinaryPath } from '../helpers/binary.js';
import { startLotarServer } from '../helpers/server.js';
import { SmokeWorkspace } from '../helpers/workspace.js';

/**
 * Raw-socket contract for `lotar serve` connection handling.
 *
 * These tests use ordinary TCP sockets (no browser) so they run ungated on
 * every platform, including sandboxed CI where Git fixtures cannot run. They
 * reproduce the browser-preconnect shape that broke the Linux smoke runs:
 * a browser speculatively opens sockets and sends nothing on them, and a
 * server that services accepted sockets serially lets each idle socket
 * starve every later request until its read timeout fires.
 */

/** Well under the server's 30s socket timeout and the smoke 15s waits. */
const RESPONSE_BOUND_MS = 5_000;

/** Matches a browser's typical speculative preconnect count per origin. */
const IDLE_PRECONNECTS = 3;

interface RawResponse {
    readonly statusLine: string;
    readonly status: number;
    readonly headers: Map<string, string>;
    readonly body: Buffer;
}

function parseRawResponse(buffer: Buffer): RawResponse | undefined {
    const headerEnd = buffer.indexOf('\r\n\r\n');
    if (headerEnd === -1) {
        return undefined;
    }
    const head = buffer.subarray(0, headerEnd).toString('utf8');
    const body = buffer.subarray(headerEnd + 4);
    const lines = head.split('\r\n');
    const statusLine = lines[0] ?? '';
    const status = Number.parseInt(statusLine.split(' ')[1] ?? '', 10);
    const headers = new Map<string, string>();
    for (const line of lines.slice(1)) {
        const separator = line.indexOf(':');
        if (separator === -1) {
            continue;
        }
        headers.set(
            line.slice(0, separator).trim().toLowerCase(),
            line.slice(separator + 1).trim(),
        );
    }
    return { statusLine, status, headers, body };
}

function readHttpResponse(
    host: string,
    port: number,
    request: string,
    boundMs: number,
): Promise<RawResponse> {
    return new Promise((resolve, reject) => {
        const socket = net.connect({ host, port });
        const chunks: Buffer[] = [];
        const timer = setTimeout(() => {
            socket.destroy();
            reject(
                new Error(
                    `no HTTP response within ${boundMs}ms (request: ${request.split('\r\n')[0]})`,
                ),
            );
        }, boundMs);
        const settle = (error?: Error) => {
            clearTimeout(timer);
            socket.destroy();
            if (error) {
                reject(error);
                return;
            }
            const parsed = parseRawResponse(Buffer.concat(chunks));
            if (!parsed) {
                reject(new Error(`incomplete HTTP response to ${request.split('\r\n')[0]}`));
                return;
            }
            resolve(parsed);
        };
        socket.once('error', (error) => settle(error));
        socket.once('connect', () => {
            socket.write(request);
        });
        socket.on('data', (chunk: Buffer) => {
            chunks.push(chunk);
            const parsed = parseRawResponse(Buffer.concat(chunks));
            // One-shot responses are bounded by Content-Length; settle as
            // soon as the full body is in so slow-close sockets cannot
            // inflate the measured latency.
            if (parsed) {
                const length = Number.parseInt(parsed.headers.get('content-length') ?? '', 10);
                if (Number.isNaN(length) || parsed.body.length >= length) {
                    settle();
                }
            }
        });
    });
}

function get(host: string, port: number, target: string, boundMs = RESPONSE_BOUND_MS) {
    return readHttpResponse(
        host,
        port,
        `GET ${target} HTTP/1.1\r\nHost: ${host}:${port}\r\nAccept-Encoding: gzip\r\nConnection: close\r\n\r\n`,
        boundMs,
    );
}

async function openIdleSockets(host: string, port: number, count: number): Promise<net.Socket[]> {
    const sockets: net.Socket[] = [];
    try {
        for (let i = 0; i < count; i += 1) {
            const socket = net.connect({ host, port });
            await once(socket, 'connect');
            sockets.push(socket);
        }
        return sockets;
    } catch (error) {
        for (const socket of sockets) {
            socket.destroy();
        }
        throw error;
    }
}

function destroyAll(sockets: net.Socket[]): void {
    for (const socket of sockets) {
        socket.destroy();
    }
}

const lotarBinaryAvailable = existsSync(resolveBinaryPath());

describe.skipIf(!lotarBinaryAvailable)('lotar serve socket concurrency (real binary)', () => {
    it('an idle preconnect socket does not starve later API requests', async () => {
        const workspace = await SmokeWorkspace.create({ name: 'dev101-socket-api-' });
        try {
            const server = await startLotarServer(workspace);
            try {
                const idle = await openIdleSockets(server.host, server.port, IDLE_PRECONNECTS);
                try {
                    const response = await get(server.host, server.port, '/api/openapi.json');
                    expect(response.status).toBe(200);
                    expect(response.headers.get('content-type')).toBe('application/json');
                } finally {
                    destroyAll(idle);
                }
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('an idle preconnect socket does not starve later static asset requests', async () => {
        const workspace = await SmokeWorkspace.create({ name: 'dev101-socket-static-' });
        try {
            const server = await startLotarServer(workspace);
            try {
                const idle = await openIdleSockets(server.host, server.port, IDLE_PRECONNECTS);
                try {
                    const response = await get(server.host, server.port, '/');
                    expect(response.status).toBe(200);
                    expect(response.headers.get('content-type')).toBe('text/html');
                } finally {
                    destroyAll(idle);
                }
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('an idle preconnect socket does not starve the SSE stream handshake', async () => {
        const workspace = await SmokeWorkspace.create({ name: 'dev101-socket-sse-' });
        try {
            const server = await startLotarServer(workspace);
            try {
                const idle = await openIdleSockets(server.host, server.port, IDLE_PRECONNECTS);
                try {
                    const response = await get(server.host, server.port, '/api/events?kinds=agent_job_started');
                    expect(response.status).toBe(200);
                    expect(response.headers.get('content-type')).toBe('text/event-stream');
                } finally {
                    destroyAll(idle);
                }
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});

describe.skipIf(!lotarBinaryAvailable)('lotar serve one-shot response protocol (real binary)', () => {
    it('API responses advertise Connection: close and the server closes the socket', async () => {
        const workspace = await SmokeWorkspace.create({ name: 'dev101-close-api-' });
        try {
            const server = await startLotarServer(workspace);
            try {
                const response = await get(server.host, server.port, '/api/openapi.json');
                expect(response.status).toBe(200);
                expect(response.headers.get('connection')).toBe('close');
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('static 200 and 404 responses advertise Connection: close', async () => {
        const workspace = await SmokeWorkspace.create({ name: 'dev101-close-static-' });
        try {
            const server = await startLotarServer(workspace);
            try {
                const ok = await get(server.host, server.port, '/');
                expect(ok.status).toBe(200);
                expect(ok.headers.get('connection')).toBe('close');

                // A dotted path skips the SPA index fallback and is a real 404.
                const missing = await get(
                    server.host,
                    server.port,
                    `/dev101-missing-${Date.now()}.txt`,
                );
                expect(missing.status).toBe(404);
                expect(missing.headers.get('connection')).toBe('close');
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });

    it('origin-rejected mutations advertise Connection: close', async () => {
        const workspace = await SmokeWorkspace.create({ name: 'dev101-close-origin-' });
        try {
            const server = await startLotarServer(workspace);
            try {
                const body = '{"title":"blocked"}';
                const response = await readHttpResponse(
                    server.host,
                    server.port,
                    [
                        'POST /api/tasks/add?project=DEV101X HTTP/1.1',
                        `Host: ${server.host}:${server.port}`,
                        'Origin: https://evil.example',
                        'Content-Type: application/json',
                        `Content-Length: ${body.length}`,
                        'Connection: close',
                        '',
                        body,
                    ].join('\r\n') + '\r\n',
                    RESPONSE_BOUND_MS,
                );
                expect(response.status).toBe(403);
                expect(response.headers.get('connection')).toBe('close');
                expect(response.body.toString('utf8')).toContain('FORBIDDEN');
            } finally {
                await server.stop();
            }
        } finally {
            await workspace.dispose();
        }
    });
});
