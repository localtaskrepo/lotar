import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

// DEV-419: the compact Board and Tasks layout applies on narrow screens unless Preferences chose Classic.
type Listener = (event: { matches: boolean }) => void

let listeners: Listener[] = []
let matches = false

function setNarrow(value: boolean) {
    matches = value
    for (const listener of listeners) listener({ matches: value })
}

async function load() {
    vi.resetModules()
    const layout = await import('../composables/useCompactLayout')
    const preferences = await import('../utils/preferences')
    return { compact: layout.useCompactLayout(), preferences }
}

describe('useCompactLayout', () => {
    beforeEach(() => {
        listeners = []
        matches = false
        localStorage.clear()
        vi.stubGlobal(
            'matchMedia',
            vi.fn((query: string) => ({
                media: query,
                get matches() {
                    return matches
                },
                addEventListener: (_type: string, listener: Listener) => listeners.push(listener),
            })),
        )
    })

    afterEach(() => {
        vi.unstubAllGlobals()
    })

    it('is compact on a narrow screen by default and follows width changes', async () => {
        matches = true
        const { compact } = await load()
        expect(compact.value).toBe(true)
        setNarrow(false)
        expect(compact.value).toBe(false)
        setNarrow(true)
        expect(compact.value).toBe(true)
    })

    it('stays classic when Preferences chose it, and follows a change of the preference', async () => {
        matches = true
        localStorage.setItem('lotar.preferences.layout.narrow', 'classic')
        const { compact, preferences } = await load()
        expect(compact.value).toBe(false)
        preferences.storeNarrowLayoutPreference('compact')
        expect(compact.value).toBe(true)
    })

    it('never applies on a wide screen', async () => {
        const { compact } = await load()
        expect(compact.value).toBe(false)
    })
})
