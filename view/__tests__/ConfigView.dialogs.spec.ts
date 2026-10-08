import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'
import type { ConfigInspectResult, GlobalConfigRaw, ResolvedConfigDTO } from '../api/types'
import ConfigView from '../pages/ConfigView.vue'

vi.mock('../api/client', () => ({
    api: {
        inspectConfig: vi.fn(),
        inspectAutomation: vi.fn(),
        setConfig: vi.fn(),
        setAutomation: vi.fn(),
        listProjects: vi.fn(),
        createProject: vi.fn(),
    },
}))

vi.mock('../components/AutomationRulesEditor.vue', () => ({
    default: {
        name: 'AutomationRulesEditor',
        props: ['modelValue', 'effectiveYaml', 'loading', 'error', 'sourceLabel', 'scopeHint'],
        template: '<div class="automation-rules-editor-stub" />',
    },
}))

vi.mock('../components/UiModal.vue', () => ({
    default: {
        name: 'UiModal',
        props: ['open', 'ariaLabel', 'ariaLabelledby', 'ariaDescribedby', 'dismissible', 'initialFocus', 'size'],
        emits: ['close'],
        template: '<div v-if="open" class="ui-modal-stub"><slot /></div>',
    },
}))

import { api } from '../api/client'

const stubs = {
    UiInput: {
        template: '<input :value="modelValue" @input="$emit(\'update:modelValue\', $event.target.value)" />',
        props: ['modelValue'],
        emits: ['update:modelValue'],
    },
    UiSelect: {
        template: '<select :value="modelValue" @change="$emit(\'update:modelValue\', $event.target.value)"><slot /></select>',
        props: ['modelValue'],
        emits: ['update:modelValue'],
    },
    TokenInput: {
        template: '<div class="token-input"><slot /></div>',
        props: ['modelValue'],
        emits: ['update:modelValue'],
    },
    ConfigGroup: {
        template: '<section><slot /></section>',
        props: ['title', 'description', 'source'],
    },
}

function baseResolvedConfig(): ResolvedConfigDTO {
    return {
        server_port: 8080,
        default_project: 'TEST',
        attachments_dir: '@attachments',
        attachments_max_upload_mb: 10,
        sync_reports_dir: '@reports',
        sync_write_reports: true,
        default_assignee: null,
        default_reporter: null,
        default_tags: [],
        default_priority: 'Medium',
        default_status: 'Todo',
        issue_states: ['Todo', 'InProgress', 'Done'],
        issue_types: ['Feature', 'Bug'],
        issue_priorities: ['Low', 'Medium', 'High'],
        issue_done_states: null,
        effective_done_states: ['Done'],
        done_states_mode: 'inferred',
        task_calendar_day: '2026-10-02',
        tags: [],
        custom_fields: [],
        auto_set_reporter: true,
        auto_assign_on_status: true,
        auto_codeowners_assign: true,
        auto_tags_from_path: true,
        auto_branch_infer_type: true,
        auto_branch_infer_status: true,
        auto_branch_infer_priority: true,
        auto_identity: true,
        auto_identity_git: true,
        scan_signal_words: [],
        scan_ticket_patterns: [],
        scan_enable_ticket_words: true,
        scan_enable_mentions: true,
        scan_strip_attributes: true,
        branch_type_aliases: {},
        branch_status_aliases: {},
        branch_priority_aliases: {},
    }
}

function baseGlobalRaw(): GlobalConfigRaw {
    return {
        server_port: 8080,
        default_project: 'TEST',
        attachments_dir: '@attachments',
        attachments_max_upload_mb: 10,
        sync_reports_dir: '@reports',
        sync_write_reports: true,
        issue_states: ['Todo', 'InProgress', 'Done'],
        issue_types: ['Feature', 'Bug'],
        issue_priorities: ['Low', 'Medium', 'High'],
        tags: [],
        default_assignee: null,
        default_reporter: null,
        default_tags: [],
        auto_set_reporter: true,
        auto_assign_on_status: true,
        auto_identity: true,
        auto_identity_git: true,
        auto_codeowners_assign: true,
        auto_tags_from_path: true,
        auto_branch_infer_type: true,
        auto_branch_infer_status: true,
        auto_branch_infer_priority: true,
        default_priority: 'Medium',
        default_status: 'Todo',
        custom_fields: [],
        scan_signal_words: [],
        scan_ticket_patterns: [],
        scan_enable_ticket_words: true,
        scan_enable_mentions: true,
        scan_strip_attributes: true,
        branch_type_aliases: {},
        branch_status_aliases: {},
        branch_priority_aliases: {},
    }
}

function createInspectResult(overrides: Partial<ConfigInspectResult> = {}): ConfigInspectResult {
    return {
        effective: baseResolvedConfig(),
        global_effective: baseResolvedConfig(),
        global_raw: baseGlobalRaw(),
        auth_profiles: {},
        project_raw: null,
        has_global_file: true,
        project_exists: false,
        sources: {},
        ...overrides,
    }
}

async function mountConfigView() {
    const router = createRouter({
        history: createMemoryHistory(),
        routes: [{ path: '/', component: { template: '<div />' } }],
    })
    await router.push('/')
    await router.isReady()
    const wrapper = mount(ConfigView, {
        global: {
            plugins: [router],
            stubs,
        },
    })
    await flushPromises()
    return wrapper
}

function openModal(wrapper: ReturnType<typeof mount>) {
    return wrapper.findAllComponents({ name: 'UiModal' }).find((modal) => modal.props('open'))
}

describe('ConfigView dialog migration', () => {
    beforeEach(() => {
        vi.clearAllMocks()
            ; (api.listProjects as any).mockResolvedValue({
                total: 1,
                limit: 50,
                offset: 0,
                projects: [{ name: 'App', prefix: 'APP' }],
            })
            ; (api.inspectConfig as any).mockResolvedValue(createInspectResult())
            ; (api.inspectAutomation as any).mockResolvedValue({
                source: 'global',
                scope_exists: true,
                scope_yaml: '',
                effective_yaml: '',
            })
    })

    it('renders the create dialog through UiModal with bound title and description ids', async () => {
        const wrapper = await mountConfigView()

        expect(openModal(wrapper)).toBeUndefined()

        const newProjectButton = wrapper.findAll('button').find((button) => button.text().includes('New project'))
        await newProjectButton?.trigger('click')
        await flushPromises()

        const modal = openModal(wrapper)
        expect(modal).toBeTruthy()

        const heading = wrapper.find('.dialog-header h2')
        expect(heading.text()).toBe('Create a project')
        expect(heading.attributes('id')).toBeTruthy()
        expect(modal!.props('ariaLabelledby')).toBe(heading.attributes('id'))

        const description = wrapper.find('.dialog-header p')
        expect(modal!.props('ariaDescribedby')).toBe(description.attributes('id'))

        // migrated shell: UiModal owns the overlay; the form hook class remains
        expect(wrapper.find('.dialog-backdrop').exists()).toBe(false)
        expect(wrapper.find('.dialog-form').exists()).toBe(true)

        // dialog-scoped label associations
        expect(wrapper.find('label[for="create-project-name"]').exists()).toBe(true)
        expect(wrapper.find('input#create-project-name').exists()).toBe(true)
        expect(wrapper.find('label[for="create-project-prefix"]').exists()).toBe(true)
        expect(wrapper.find('input#create-project-prefix').exists()).toBe(true)

        wrapper.unmount()
    })

    it('keeps the create dialog open, non-dismissible, and single-submit while creating', async () => {
        ; (api.createProject as any).mockImplementation(() => new Promise(() => {}))
        const wrapper = await mountConfigView()

        const newProjectButton = wrapper.findAll('button').find((button) => button.text().includes('New project'))
        await newProjectButton?.trigger('click')
        await flushPromises()

        await wrapper.find('input[placeholder="Marketing website"]').setValue('Fresh App')
        await wrapper.find('input[placeholder="AUTO"]').setValue('FRESH')

        // jsdom does not perform implicit form submission on button clicks,
        // so drive the submit handler the way the form does.
        await wrapper.find('form.dialog-form').trigger('submit')
        await flushPromises()

        const modal = openModal(wrapper)
        expect(modal).toBeTruthy()
        expect(modal!.props('dismissible')).toBe(false)

        modal!.vm.$emit('close')
        await flushPromises()
        expect(openModal(wrapper)).toBeTruthy()
        expect(wrapper.find('.dialog-form').exists()).toBe(true)

        await wrapper.find('form.dialog-form').trigger('submit')
        await wrapper.find('form.dialog-form').trigger('submit')
        await flushPromises()
        expect(api.createProject).toHaveBeenCalledTimes(1)

        wrapper.unmount()
    })

    it('cancels the create dialog safely and resets its state', async () => {
        const wrapper = await mountConfigView()

        const newProjectButton = wrapper.findAll('button').find((button) => button.text().includes('New project'))
        await newProjectButton?.trigger('click')
        await flushPromises()

        await wrapper.find('input[placeholder="Marketing website"]').setValue('Discarded')
        const cancelButton = wrapper.findAll('button').find((button) => button.text() === 'Cancel')
        await cancelButton?.trigger('click')
        await flushPromises()

        expect(openModal(wrapper)).toBeUndefined()
        expect((wrapper.vm as any).createName).toBe('')
        expect((wrapper.vm as any).createPrefix).toBe('')
        expect(api.createProject).not.toHaveBeenCalled()

        wrapper.unmount()
    })

    it('renders the help dialog through UiModal with a bound title and closes on close', async () => {
        const wrapper = await mountConfigView()

        const helpButton = wrapper.find('button[aria-label="Open help"]')
        await helpButton.trigger('click')
        await flushPromises()

        const modal = openModal(wrapper)
        expect(modal).toBeTruthy()

        const heading = wrapper.find('.help-header h2')
        expect(heading.text()).toBe('Configuration help')
        expect(heading.attributes('id')).toBeTruthy()
        expect(modal!.props('ariaLabelledby')).toBe(heading.attributes('id'))
        expect(wrapper.find('.help-backdrop').exists()).toBe(false)
        expect(wrapper.find('.help-content').exists()).toBe(true)

        modal!.vm.$emit('close')
        await flushPromises()
        expect(openModal(wrapper)).toBeUndefined()

        wrapper.unmount()
    })
})
