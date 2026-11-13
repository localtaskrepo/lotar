import { afterEach, beforeAll, vi } from 'vitest'

class MemoryStorage {
    private store = new Map<string, string>()

    clear() {
        this.store.clear()
    }

    getItem(key: string) {
        return this.store.get(key) ?? null
    }

    setItem(key: string, value: string) {
        this.store.set(key, value)
    }

    removeItem(key: string) {
        this.store.delete(key)
    }

    key(index: number) {
        return Array.from(this.store.keys())[index] ?? null
    }

    get length() {
        return this.store.size
    }
}

const jsonResponse = (data: unknown) =>
    new Response(JSON.stringify({ data }), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
    })

beforeAll(() => {
    vi.stubGlobal('localStorage', new MemoryStorage())
    // jsdom has no layout, so it leaves out scrollIntoView; every browser has it.
    Element.prototype.scrollIntoView = () => {}
    // jsdom has no EventSource; pages subscribe to /api/events on mount. This
    // one never connects. Tests that drive events stub their own.
    vi.stubGlobal(
        'EventSource',
        class extends EventTarget {
            static CONNECTING = 0
            static OPEN = 1
            static CLOSED = 2
            readyState = 0
            constructor(public url: string) {
                super()
            }
            close() {
                this.readyState = 2
            }
        },
    )

    vi.stubGlobal(
        'fetch',
        vi.fn(async (input: RequestInfo | URL) => {
            const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url

            if (url.includes('/api/tasks/export')) {
                return new Response('', {
                    status: 200,
                    headers: { 'Content-Type': 'text/csv' },
                })
            }

            return jsonResponse(null)
        }),
    )
})

afterEach(() => {
    globalThis.localStorage.clear()
    vi.clearAllMocks()
})
