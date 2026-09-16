import net from 'node:net';
import getPort from 'get-port';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { resolveServerBind } from '../helpers/server.js';

vi.mock('get-port', async (importOriginal) => {
    const actual = await importOriginal<typeof import('get-port')>();
    return {
        ...actual,
        default: vi.fn(actual.default),
    };
});

const mockedGetPort = vi.mocked(getPort);

function listen(host: string, port = 0): Promise<net.Server> {
    return new Promise((resolve, reject) => {
        const server = net.createServer();
        server.once('error', reject);
        server.listen({ host, port }, () => resolve(server));
    });
}

function close(server: net.Server): Promise<void> {
    return new Promise((resolve) => {
        server.close(() => resolve());
    });
}

describe('smoke harness server bind resolution', () => {
    beforeEach(() => {
        mockedGetPort.mockClear();
    });

    it('returns an explicit port unchanged without availability probing', async () => {
        const holder = await listen('127.0.0.1');
        try {
            const held = (holder.address() as net.AddressInfo).port;
            const bind = await resolveServerBind({ port: held });
            expect(bind.host).toBe('127.0.0.1');
            expect(bind.port).toBe(held);
            expect(mockedGetPort).not.toHaveBeenCalled();
        } finally {
            await close(holder);
        }
    });

    it('probes the loopback host when no host or port is given', async () => {
        const holder = await listen('127.0.0.1');
        try {
            const held = (holder.address() as net.AddressInfo).port;
            const bind = await resolveServerBind();
            expect(mockedGetPort).toHaveBeenCalledTimes(1);
            expect(mockedGetPort).toHaveBeenCalledWith({ host: '127.0.0.1' });
            expect(bind.host).toBe('127.0.0.1');
            expect(bind.port).not.toBe(held);
            const verification = await listen('127.0.0.1', bind.port);
            await close(verification);
        } finally {
            await close(holder);
        }
    });

    it('probes a custom host on its own address family', async () => {
        const holder = await listen('::1');
        try {
            const held = (holder.address() as net.AddressInfo).port;
            const bind = await resolveServerBind({ host: '::1' });
            expect(mockedGetPort).toHaveBeenCalledTimes(1);
            expect(mockedGetPort).toHaveBeenCalledWith({ host: '::1' });
            expect(bind.host).toBe('::1');
            expect(bind.port).not.toBe(held);
            const verification = await listen('::1', bind.port);
            await close(verification);
        } finally {
            await close(holder);
        }
    });
});
