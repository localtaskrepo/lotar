import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from 'vitest'

const api = vi.hoisted(() => ({
    activityFeed: vi.fn(),
}))

vi.mock('../api/client', () => ({ api }))
vi.mock('../composables/useTaskPanelController', () => ({
    useTaskPanelController: () => ({ openTaskPanel: vi.fn() }),
}))

function feedItem(id: string, task: string, message: string): any {
    return {
        commit: `sha-${id}`,
        author: 'Ada Lovelace',
        email: 'ada@example.com',
        date: '2026-09-13T10:00:00.000Z',
        message,
        task_id: task,
        task_title: `Task ${task}`,
        history: [],
    }
}

describe('ActivityDrawer', () => {
    let ActivityDrawer: (typeof import('../components/ActivityDrawer.vue'))['default']
    let feedMock: Mock
    let _resetActivityStore: (typeof import('../composables/useActivity'))['_resetActivityStore']

    beforeEach(async () => {
        const client = await import('../api/client')
        const mod = await import('../composables/useActivity')
        ActivityDrawer = (await import('../components/ActivityDrawer.vue')).default
        feedMock = client.api.activityFeed as unknown as Mock
        _resetActivityStore = mod._resetActivityStore
        _resetActivityStore()
    })

    afterEach(() => {
        _resetActivityStore()
        vi.clearAllMocks()
    })

    // The drawer renders through UiModal's Teleport; stubbing it keeps the
    // content inside the detached wrapper for the feed assertions.
    function mountDrawer(open = true) {
        return mount(ActivityDrawer, { props: { open }, global: { stubs: { teleport: true } } })
    }

    it('requests its own global 30-day feed on open', async () => {
        feedMock.mockResolvedValue([feedItem('a', 'DEV-1', 'first load')])
        const wrapper = mountDrawer()
        await flushPromises()

        expect(feedMock).toHaveBeenCalledTimes(1)
        const params = feedMock.mock.calls[0]![0] as Record<string, unknown>
        expect(params.limit).toBe(200)
        expect(params).not.toHaveProperty('project')
        expect(String(params.since)).toMatch(/^\d{4}-\d{2}-\d{2}T/)

        expect(wrapper.text()).toContain('DEV-1')
        expect(wrapper.text()).toContain('first load')
        wrapper.unmount()
    })

    it('does not fetch while closed', async () => {
        feedMock.mockResolvedValue([])
        const wrapper = mountDrawer(false)
        await flushPromises()
        expect(feedMock).not.toHaveBeenCalled()
        wrapper.unmount()
    })

    it('genuinely refetches on manual refresh and renders the new snapshot', async () => {
        feedMock.mockResolvedValueOnce([feedItem('a', 'DEV-1', 'before')])
        const wrapper = mountDrawer()
        await flushPromises()
        expect(wrapper.text()).toContain('before')

        feedMock.mockResolvedValueOnce([feedItem('b', 'DEV-2', 'after')])
        await wrapper.find('button[title="Refresh activity feed"]').trigger('click')
        await flushPromises()

        expect(feedMock).toHaveBeenCalledTimes(2)
        expect(wrapper.text()).toContain('after')
        expect(wrapper.text()).not.toContain('before')
        wrapper.unmount()
    })

    it('renders the error while keeping the previous snapshot', async () => {
        feedMock.mockResolvedValueOnce([feedItem('a', 'DEV-1', 'snapshot')])
        const wrapper = mountDrawer()
        await flushPromises()

        feedMock.mockRejectedValueOnce(new Error('refresh exploded'))
        await wrapper.find('button[title="Refresh activity feed"]').trigger('click')
        await flushPromises()

        expect(wrapper.find('.error').text()).toBe('refresh exploded')
        expect(wrapper.text()).toContain('snapshot')
        wrapper.unmount()
    })

    // DEV-288: it is announced as a modal, so it must behave like one.
    describe('modal behaviour', () => {
        function press(key: string, init: KeyboardEventInit = {}) {
            const target = (document.activeElement as HTMLElement | null) ?? document.body
            target.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init }))
        }

        it('focuses the close button, traps Tab, closes on Escape and returns focus', async () => {
            feedMock.mockResolvedValue([feedItem('a', 'DEV-1', 'first load')])
            const opener = document.createElement('button')
            opener.textContent = 'Activity'
            document.body.appendChild(opener)
            opener.focus()

            const wrapper = mount(ActivityDrawer, { props: { open: false }, attachTo: document.body })
            await wrapper.setProps({ open: true })
            await flushPromises()

            const dialog = document.querySelector<HTMLElement>('[role="dialog"][aria-label="Recent activity"]')
            expect(dialog?.getAttribute('aria-modal')).toBe('true')
            const close = dialog!.querySelector<HTMLElement>('button[aria-label="Close activity drawer"]')!
            expect(document.activeElement).toBe(close)
            // Content behind the modal is inert, so the opener is out of reach.
            expect(opener.closest('[inert]')).not.toBeNull()

            const controls = Array.from(dialog!.querySelectorAll<HTMLElement>('button'))
            const first = controls[0]!
            const last = controls[controls.length - 1]!
            first.focus()
            press('Tab', { shiftKey: true })
            expect(document.activeElement).toBe(last)
            press('Tab')
            expect(document.activeElement).toBe(first)

            press('Escape')
            expect(wrapper.emitted('close')).toHaveLength(1)

            await wrapper.setProps({ open: false })
            await flushPromises()
            expect(document.querySelector('[aria-label="Recent activity"]')).toBeNull()
            expect(opener.closest('[inert]')).toBeNull()
            expect(document.activeElement).toBe(opener)

            wrapper.unmount()
            opener.remove()
        })
    })
})
