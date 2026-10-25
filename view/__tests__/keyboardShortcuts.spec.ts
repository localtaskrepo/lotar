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
