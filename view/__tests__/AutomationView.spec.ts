import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'
import AutomationView from '../pages/AutomationView.vue'

// DEV-288: unsaved rule edits must survive a scope switch, Reload or leaving
// the page unless the user confirms the discard.

vi.mock('../api/client', () => ({
    api: {
        inspectAutomation: vi.fn(),
        inspectConfig: vi.fn(),
        listAgentProfiles: vi.fn(),
        listProjects: vi.fn(),
        setAutomation: vi.fn(),
        simulateAutomation: vi.fn(),
    },
}))

vi.mock('../components/AutomationRulesEditor.vue', () => ({
    default: {
        name: 'AutomationRulesEditor',
        props: ['modelValue'],
        emits: ['update:modelValue'],
        template:
            '<textarea class="rules-editor-stub" :value="modelValue" @input="$emit(\'update:modelValue\', $event.target.value)" />',
    },
}))

vi.mock('../components/UiModal.vue', () => ({
    default: {
        name: 'UiModal',
        props: ['open', 'ariaLabel', 'size'],
        emits: ['close'],
        template: '<div v-if="open" class="ui-modal-stub"><slot /></div>',
    },
}))

import { api } from '../api/client'

const stubs = {
    UiSelect: {
        template:
            '<select :value="modelValue" @change="$emit(\'update:modelValue\', $event.target.value)"><slot /></select>',
        props: ['modelValue'],
        emits: ['update:modelValue'],
    },
}

function automation(scope: string) {
    return { source: 'project', scope_yaml: `rules: # ${scope || 'global'}\n`, effective_yaml: '' }
}

async function mountView() {
    const router = createRouter({
        history: createMemoryHistory(),
        routes: [
            { path: '/automations', component: AutomationView },
            { path: '/other', component: { template: '<div />' } },
        ],
    })
    await router.push('/automations')
    await router.isReady()
    const wrapper = mount(
        { template: '<RouterView />' },
        { attachTo: document.body, global: { plugins: [router], stubs } },
    )
    await flushPromises()
    return { wrapper, router }
}

const dialog = (wrapper: ReturnType<typeof mount>) => wrapper.find('[data-testid="automation-discard-dialog"]')

async function choose(wrapper: ReturnType<typeof mount>, label: 'Keep editing' | 'Discard changes') {
    await dialog(wrapper)
        .findAll('button')
        .find((button) => button.text() === label)!
        .trigger('click')
    await flushPromises()
}

const editor = (wrapper: ReturnType<typeof mount>) => wrapper.get('.rules-editor-stub').element as HTMLTextAreaElement

describe('AutomationView unsaved rule guards', () => {
    beforeEach(() => {
        vi.clearAllMocks()
        vi.mocked(api.inspectAutomation).mockImplementation(async (scope?: string) => automation(scope ?? '') as any)
        vi.mocked(api.inspectConfig).mockResolvedValue(null as any)
        vi.mocked(api.listAgentProfiles).mockResolvedValue({ profiles: [] } as any)
        vi.mocked(api.listProjects).mockResolvedValue({ projects: [{ prefix: 'APP', name: 'App' }] } as any)
    })

    it('loads without prompting when nothing changed', async () => {
        const { wrapper, router } = await mountView()
        await wrapper.get('select[aria-label="Project scope"]').setValue('APP')
        await flushPromises()
        expect(dialog(wrapper).exists()).toBe(false)
        expect(api.inspectAutomation).toHaveBeenLastCalledWith('APP')

        await router.push('/other')
        expect(router.currentRoute.value.path).toBe('/other')
        wrapper.unmount()
    })

    it('asks before a scope switch, Reload or navigation discards rule edits', async () => {
        const { wrapper, router } = await mountView()
        await wrapper.get('.rules-editor-stub').setValue('rules: [] # edited\n')
        await flushPromises()
        const loads = vi.mocked(api.inspectAutomation).mock.calls.length

        // Scope switch: keep editing restores the select and the edit.
        await wrapper.get('select[aria-label="Project scope"]').setValue('APP')
        await flushPromises()
        expect(dialog(wrapper).exists()).toBe(true)
        await choose(wrapper, 'Keep editing')
        expect((wrapper.get('select[aria-label="Project scope"]').element as HTMLSelectElement).value).toBe('')
        expect(editor(wrapper).value).toBe('rules: [] # edited\n')

        // Reload.
        await wrapper.get('button[aria-label="Refresh automations"]').trigger('click')
        await flushPromises()
        expect(dialog(wrapper).exists()).toBe(true)
        await choose(wrapper, 'Keep editing')
        expect(editor(wrapper).value).toBe('rules: [] # edited\n')

        // Leaving the page.
        const leaving = router.push('/other')
        await flushPromises()
        expect(dialog(wrapper).exists()).toBe(true)
        await choose(wrapper, 'Keep editing')
        await leaving
        expect(router.currentRoute.value.path).toBe('/automations')
        expect(api.inspectAutomation).toHaveBeenCalledTimes(loads)

        // Unloading the tab warns while the edit is pending.
        const unload = new Event('beforeunload', { cancelable: true })
        window.dispatchEvent(unload)
        expect(unload.defaultPrevented).toBe(true)

        // Confirmed discard switches the scope and loads its rules.
        await wrapper.get('select[aria-label="Project scope"]').setValue('APP')
        await flushPromises()
        await choose(wrapper, 'Discard changes')
        expect(api.inspectAutomation).toHaveBeenLastCalledWith('APP')
        expect(editor(wrapper).value).toBe('rules: # APP\n')

        const clean = new Event('beforeunload', { cancelable: true })
        window.dispatchEvent(clean)
        expect(clean.defaultPrevented).toBe(false)
        wrapper.unmount()
    })

    it('saving rules reloads without asking', async () => {
        vi.mocked(api.setAutomation).mockResolvedValue({ errors: [] } as any)
        const { wrapper } = await mountView()
        await wrapper.get('.rules-editor-stub').setValue('rules: [] # edited\n')
        await flushPromises()
        vi.mocked(api.inspectAutomation).mockResolvedValue({
            source: 'global',
            scope_yaml: 'rules: [] # edited\n',
            effective_yaml: '',
        } as any)

        await wrapper
            .findAll('button')
            .find((button) => button.text() === 'Save rules')!
            .trigger('click')
        await flushPromises()
        expect(api.setAutomation).toHaveBeenCalledWith({ yaml: 'rules: [] # edited\n', project: undefined })
        expect(dialog(wrapper).exists()).toBe(false)
        expect(editor(wrapper).value).toBe('rules: [] # edited\n')
        wrapper.unmount()
    })
})
