import { afterEach, beforeAll, vi } from 'vitest';

class MemoryStorage {
    private store = new Map<string, string>();

    clear() {
        this.store.clear();
    }

    getItem(key: string) {
        return this.store.get(key) ?? null;
    }

    setItem(key: string, value: string) {
        this.store.set(key, String(value));
    }

    removeItem(key: string) {
        this.store.delete(key);
    }

    key(index: number) {
        return Array.from(this.store.keys())[index] ?? null;
    }

    get length() {
        return this.store.size;
    }
}

const jsonResponse = (data: unknown) =>
    new Response(JSON.stringify({ data }), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
    });

beforeAll(() => {
    if (typeof globalThis.localStorage !== 'object' || typeof globalThis.localStorage?.clear !== 'function') {
        vi.stubGlobal('localStorage', new MemoryStorage());
    }

    vi.stubGlobal(
        'fetch',
        vi.fn(async (input: RequestInfo | URL) => {
            const url = typeof input === 'string' ? input : input.toString();

            if (url.includes('/api/tasks/export')) {
                return new Response('', {
                    status: 200,
                    headers: { 'Content-Type': 'text/csv' },
                });
            }

            return jsonResponse(null);
        }),
    );
});

afterEach(() => {
    if (typeof globalThis.localStorage?.clear === 'function') {
        globalThis.localStorage.clear();
    }
    vi.clearAllMocks();
});
