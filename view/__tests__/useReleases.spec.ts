import { mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { defineComponent, h } from 'vue'

const events = vi.hoisted(() => ({
    handlers: new Map<string, () => void>(),
    opened: 0,
    closed: 0,
}))

vi.mock('../composables/useSse', () => ({
    useSse: () => {
        events.opened += 1
        return {
            on: (kind: string, handler: () => void) => events.handlers.set(kind, handler),
            off: vi.fn(),
            close: () => {
                events.closed += 1
            },
        }
    },
}))

const releaseList = vi.hoisted(() => vi.fn())
vi.mock('../api/client', () => ({ api: { releaseList } }))

import { useReleases } from '../composables/useReleases'

const Consumer = defineComponent({
    setup() {
        const { releases } = useReleases()
        return () => h('span', releases.value.map((release) => release.version).join(','))
    },
})

function release(id: number, version: string) {
    return { id, version, display_name: version, state: 'planned', task_count: 0, done_count: 0, open_count: 0 }
}

describe('useReleases (DEV-464)', () => {
    beforeEach(() => {
        vi.useFakeTimers()
        events.handlers.clear()
        events.opened = 0
        events.closed = 0
        releaseList.mockReset()
    })

    afterEach(() => {
        vi.useRealTimers()
    })

    it('reloads the shared list on a release_changed event while a page shows it', async () => {
        releaseList.mockResolvedValueOnce({ releases: [release(1, '0.1.0')] })
        const first = mount(Consumer)
        const second = mount(Consumer)
        await vi.runAllTimersAsync()
        expect(first.text()).toBe('0.1.0')
        expect(events.opened).toBe(1)

        releaseList.mockResolvedValueOnce({ releases: [release(1, '0.1.0'), release(2, '0.2.0')] })
        events.handlers.get('release_changed')?.()
        await vi.runAllTimersAsync()
        expect(first.text()).toBe('0.1.0,0.2.0')
        expect(second.text()).toBe('0.1.0,0.2.0')

        first.unmount()
        expect(events.closed).toBe(0)
        second.unmount()
        expect(events.closed).toBe(1)
    })
})
