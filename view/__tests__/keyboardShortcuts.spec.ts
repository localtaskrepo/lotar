import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { defineComponent, h } from 'vue'

const api = vi.hoisted(() => ({ suggestTasks: vi.fn() }))
vi.mock('../api/client', () => ({ api }))

import CommandPalette, { type PaletteCommand } from '../components/CommandPalette.vue'
import { useGlobalShortcuts, type GlobalShortcutHandlers } from '../composables/useGlobalShortcuts'
import { commandMatchRank, isEditableTarget, isPaletteChord } from '../utils/keyboardShortcuts'

function press(key: string, init: KeyboardEventInit = {}, target: EventTarget = window) {
    const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init })
    target.dispatchEvent(event)
    return event
}

describe('keyboard shortcut helpers', () => {
    it('recognises text fields but not checkboxes or buttons', () => {
        const make = (html: string) => {
            const wrapper = document.createElement('div')
            wrapper.innerHTML = html
            return wrapper.firstElementChild!
        }
        expect(isEditableTarget(make('<input type="text">'))).toBe(true)
        expect(isEditableTarget(make('<textarea></textarea>'))).toBe(true)
        expect(isEditableTarget(make('<select></select>'))).toBe(true)
        expect(isEditableTarget(make('<input type="checkbox">'))).toBe(false)
        expect(isEditableTarget(make('<button></button>'))).toBe(false)
        expect(isEditableTarget(null)).toBe(false)
    })

    it('matches every query word and ranks label prefixes first', () => {
        expect(commandMatchRank('', 'Boards')).toBe(0)
        expect(commandMatchRank('boa', 'Boards')).toBe(0)
        expect(commandMatchRank('dark', 'Use dark theme', 'appearance')).toBe(1)
        expect(commandMatchRank('appearance', 'Use dark theme', 'appearance')).toBe(2)
        expect(commandMatchRank('dark light', 'Use dark theme')).toBeNull()
    })

    it('uses Ctrl+K outside Apple devices', () => {
        expect(isPaletteChord(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true }))).toBe(true)
        expect(isPaletteChord(new KeyboardEvent('keydown', { key: 'K', ctrlKey: true }))).toBe(true)
        expect(isPaletteChord(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, shiftKey: true }))).toBe(false)
        expect(isPaletteChord(new KeyboardEvent('keydown', { key: 'k' }))).toBe(false)
    })
})

describe('useGlobalShortcuts', () => {
    let handlers: GlobalShortcutHandlers & { enabled: boolean; isBusy: boolean }
    let unmount: () => void

    beforeEach(() => {
        vi.useFakeTimers()
        handlers = {
            enabled: true,
            isBusy: false,
            openPalette: vi.fn(),
            openHelp: vi.fn(),
            newTask: vi.fn(),
            navigate: vi.fn(),
            singleKeyEnabled: () => handlers.enabled,
            busy: () => handlers.isBusy,
            paletteBlocked: () => handlers.isBusy,
        }
        const Host = defineComponent({
            setup() {
                useGlobalShortcuts(handlers)
                return () => h('div')
            },
        })
        const wrapper = mount(Host)
        unmount = () => wrapper.unmount()
    })

    afterEach(() => {
        unmount()
        vi.useRealTimers()
    })

    it('runs single-key shortcuts and g sequences outside text fields', () => {
        press('c')
        expect(handlers.newTask).toHaveBeenCalledTimes(1)
        press('?', { shiftKey: true })
        expect(handlers.openHelp).toHaveBeenCalledTimes(1)
        press('g')
        press('b')
        expect(handlers.navigate).toHaveBeenLastCalledWith('/boards')
        // An unknown second key cancels the sequence without navigating.
        press('g')
        press('x')
        press('b')
        expect(handlers.navigate).toHaveBeenCalledTimes(1)
        // The sequence expires.
        press('g')
        vi.advanceTimersByTime(2000)
        press('t')
        expect(handlers.navigate).toHaveBeenCalledTimes(1)
    })

    it('never fires single keys while typing, busy or turned off, but Ctrl+K always works', () => {
        const input = document.createElement('input')
        document.body.appendChild(input)
        press('c', {}, input)
        expect(handlers.newTask).not.toHaveBeenCalled()
        press('k', { ctrlKey: true }, input)
        expect(handlers.openPalette).toHaveBeenCalledTimes(1)
        input.remove()

        handlers.enabled = false
        press('c')
        press('g')
        press('t')
        expect(handlers.newTask).not.toHaveBeenCalled()
        expect(handlers.navigate).not.toHaveBeenCalled()
        press('k', { ctrlKey: true })
        expect(handlers.openPalette).toHaveBeenCalledTimes(2)

        handlers.enabled = true
        handlers.isBusy = true
        press('c')
        press('k', { ctrlKey: true })
        expect(handlers.newTask).not.toHaveBeenCalled()
        expect(handlers.openPalette).toHaveBeenCalledTimes(2)
    })
})

describe('CommandPalette', () => {
    beforeEach(() => {
        vi.useFakeTimers()
        api.suggestTasks.mockReset()
        api.suggestTasks.mockResolvedValue([{ id: 'ATLS-26', title: 'Offline edits are lost' }])
    })

    afterEach(() => {
        vi.useRealTimers()
        document.body.innerHTML = ''
    })

    function mountPalette(commands: PaletteCommand[]) {
        return mount(CommandPalette, { props: { open: true, commands }, attachTo: document.body })
    }

    const commands = (run = vi.fn()): PaletteCommand[] => [
        { id: 'go-tasks', group: 'Go to', label: 'Tasks', run },
        { id: 'go-boards', group: 'Go to', label: 'Boards', shortcut: 'G B', run },
        { id: 'new-task', group: 'Actions', label: 'New task', keywords: 'create', shortcut: 'C', run },
    ]

    it('filters commands as you type and runs the highlighted one with Enter', async () => {
        const run = vi.fn()
        const wrapper = mountPalette(commands(run))
        await flushPromises()
        const input = document.querySelector<HTMLInputElement>('[data-testid="command-palette-input"]')!
        input.value = 'boa'
        input.dispatchEvent(new Event('input'))
        await flushPromises()
        const options = [...document.querySelectorAll('[role="option"]')].map((el) => el.textContent?.trim())
        expect(options[0]).toContain('Boards')
        input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
        await flushPromises()
        expect(run).toHaveBeenCalledTimes(1)
        expect(wrapper.emitted('close')).toBeTruthy()
        wrapper.unmount()
    })

    it('finds tasks by ID first and opens the chosen task', async () => {
        const wrapper = mountPalette(commands())
        await flushPromises()
        const input = document.querySelector<HTMLInputElement>('[data-testid="command-palette-input"]')!
        input.value = 'ATLS-26'
        input.dispatchEvent(new Event('input'))
        await vi.advanceTimersByTimeAsync(200)
        await flushPromises()
        expect(api.suggestTasks).toHaveBeenCalledWith('ATLS-26', undefined, 8)
        const first = document.querySelector('[role="option"]')!
        expect(first.textContent).toContain('ATLS-26')
        expect(first.getAttribute('aria-selected')).toBe('true')
        input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
        await flushPromises()
        expect(wrapper.emitted('open-task')?.[0]).toEqual(['ATLS-26'])
        wrapper.unmount()
    })

    it('moves the highlight with the arrow keys and wraps around', async () => {
        const wrapper = mountPalette(commands())
        await flushPromises()
        const input = document.querySelector<HTMLInputElement>('[data-testid="command-palette-input"]')!
        const selected = () => document.querySelector('[role="option"][aria-selected="true"]')?.textContent
        expect(selected()).toContain('Tasks')
        input.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }))
        await flushPromises()
        expect(selected()).toContain('Boards')
        input.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true }))
        input.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true }))
        await flushPromises()
        expect(selected()).toContain('New task')
        expect(input.getAttribute('aria-activedescendant')).toBeTruthy()
        wrapper.unmount()
    })
})

describe('recent tasks', () => {
    it('keeps the newest first without duplicates, up to the limit', async () => {
        const { readRecentTasks, recordRecentTask, RECENT_TASKS_LIMIT } = await import('../utils/recentTasks')
        for (let n = 1; n <= RECENT_TASKS_LIMIT + 2; n++) recordRecentTask(`A-${n}`, `Task ${n}`)
        recordRecentTask('A-5', 'Renamed five')
        const recent = readRecentTasks()
        expect(recent).toHaveLength(RECENT_TASKS_LIMIT)
        expect(recent[0]).toEqual({ id: 'A-5', title: 'Renamed five' })
        expect(recent.filter((task) => task.id === 'A-5')).toHaveLength(1)
        expect(recent.map((task) => task.id)).not.toContain('A-1')
    })
})

describe('useListKeyNavigation', () => {
    it('moves focus through the items with j and k, but not while typing or with the panel open', async () => {
        const { useListKeyNavigation } = await import('../composables/useListKeyNavigation')
        const { useTaskPanelController } = await import('../composables/useTaskPanelController')
        const Host = defineComponent({
            setup() {
                useListKeyNavigation('[data-row]')
                return () =>
                    h('div', [
                        h('input', { id: 'typing' }),
                        ...['one', 'two', 'three'].map((id) => h('div', { 'data-row': '', tabindex: -1, id })),
                    ])
            },
        })
        const wrapper = mount(Host, { attachTo: document.body })
        // jsdom has no layout; make the rows count as visible.
        for (const row of document.querySelectorAll<HTMLElement>('[data-row]')) {
            row.getClientRects = () => [{}] as unknown as DOMRectList
        }
        const focused = () => document.activeElement?.id
        press('j')
        expect(focused()).toBe('one')
        press('j')
        press('j')
        press('j')
        expect(focused()).toBe('three')
        press('k')
        expect(focused()).toBe('two')

        const input = document.getElementById('typing') as HTMLInputElement
        input.focus()
        press('j', {}, input)
        expect(focused()).toBe('typing')

        const panel = useTaskPanelController()
        panel.openTaskPanel({ taskId: 'A-1' })
        document.body.focus()
        press('j')
        expect(focused()).not.toBe('one')
        panel.closeTaskPanel()
        wrapper.unmount()
    })
})

describe('CommandPalette groups', () => {
    it('lists this task first and shows recent tasks only while the search is empty', async () => {
        const run = vi.fn()
        const wrapper = mount(CommandPalette, {
            props: {
                open: true,
                commands: [
                    { id: 'go-tasks', group: 'Go to', label: 'Tasks', run },
                    {
                        id: 'recent-A-2',
                        group: 'Recent tasks',
                        label: 'Fix login',
                        detail: 'A-2',
                        emptyOnly: true,
                        run,
                    },
                    { id: 'task-status-Done', group: 'This task', label: 'Set status to Done', run },
                    { id: 'filter-overdue', group: 'Filters', label: 'Show overdue tasks', run },
                ],
            },
            attachTo: document.body,
        })
        await flushPromises()
        const groups = () => [...document.querySelectorAll('.palette__group')].map((el) => el.textContent?.trim())
        expect(groups()).toEqual(['This task', 'Recent tasks', 'Go to', 'Filters'])
        const input = document.querySelector<HTMLInputElement>('[data-testid="command-palette-input"]')!
        input.value = 'fix'
        input.dispatchEvent(new Event('input'))
        await flushPromises()
        expect(document.querySelector('[data-testid="command-palette-option-recent-A-2"]')).toBeNull()
        wrapper.unmount()
    })
})
