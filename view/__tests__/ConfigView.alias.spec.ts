import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'
import type { ConfigInspectResult, GlobalConfigRaw, ResolvedConfigDTO } from '../api/types'
import ConfigAliasSectionSource from '../components/ConfigAliasSection.vue?raw'
import ConfigView from '../pages/ConfigView.vue'

vi.mock('../api/client', () => ({
    api: {
        inspectConfig: vi.fn(),
        setConfig: vi.fn(),
        listProjects: vi.fn(),
    },
}))

import { api } from '../api/client'

const stubs = {
    UiInput: {
        template: '<input />',
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

function extractStyle(source: string): string {
    const match = source.match(/<style[^>]*>([\s\S]*?)<\/style>/)
    return match ? match[1] : ''
}

function baseResolvedConfig(): ResolvedConfigDTO {
    return {
        server_port: 8080,
        default_project: 'TEST',
        attachments_dir: '@attachments',
        attachments_max_upload_mb: 10,
        default_assignee: null,
        default_reporter: null,
        default_tags: [],
        default_priority: 'Medium',
        default_status: 'Todo',
        issue_states: ['Todo', 'InProgress', 'Done'],
        issue_types: ['Feature', 'Bug'],
        issue_priorities: ['Low', 'Medium', 'High'],
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
        branch_type_aliases: { feat: 'Feature' },
        branch_status_aliases: { wip: 'InProgress' },
        branch_priority_aliases: { hotfix: 'High' },
    }
}

function baseGlobalRaw(): GlobalConfigRaw {
    return {
        server_port: 8080,
        default_project: 'TEST',
        attachments_dir: '@attachments',
        attachments_max_upload_mb: 10,
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
        branch_type_aliases: { feat: 'Feature' },
        branch_status_aliases: { wip: 'InProgress' },
        branch_priority_aliases: { hotfix: 'High' },
    }
}

function createInspectResult(overrides: Partial<ConfigInspectResult> = {}): ConfigInspectResult {
    return {
        effective: baseResolvedConfig(),
        global_effective: baseResolvedConfig(),
        global_raw: baseGlobalRaw(),
        project_raw: null,
        has_global_file: true,
        project_exists: false,
        sources: {},
        ...overrides,
    }
}

async function mountConfigView(initialRoute: string) {
    const router = createRouter({
        history: createMemoryHistory(),
        routes: [{ path: '/', component: { template: '<div />' } }],
    })
    await router.push(initialRoute)
    await router.isReady()
    const wrapper = mount(ConfigView, {
        attachTo: document.body,
        global: {
            plugins: [router],
            stubs,
        },
    })
    return { wrapper, router }
}
describe('ConfigView branch alias handling', () => {
    beforeEach(() => {
        vi.clearAllMocks()
            ; (api.listProjects as any).mockResolvedValue({
                total: 1,
                limit: 50,
                offset: 0,
                projects: [{ name: 'App', prefix: 'APP' }],
            })
            ; (api.setConfig as any).mockResolvedValue({ updated: true, warnings: [], info: [], errors: [] })
    })

    afterEach(() => {
        document.body.innerHTML = ''
    })

    it('serializes alias payloads for global scope', async () => {
        ; (api.inspectConfig as any).mockResolvedValue(createInspectResult())

        const { wrapper } = await mountConfigView('/')
        await flushPromises()

        const vm = wrapper.vm as any
        vm.form.branchTypeAliases[0].value = 'Story'
        vm.form.branchTypeAliases.push({ key: 'fix', value: 'Bug' })

        const payload = vm.buildPayload()
        expect(JSON.parse(payload.branch_type_aliases)).toEqual({ feat: 'Story', fix: 'Bug' })
        expect(payload.branch_status_aliases).toBeUndefined()

        wrapper.unmount()
    })

    it('produces empty payload to clear project alias map', async () => {
        const projectInspect = createInspectResult({
            effective: {
                ...baseResolvedConfig(),
                project_name: 'APP',
                branch_priority_aliases: { hotfix: 'High', sev1: 'Critical' },
            } as any,
            project_raw: {
                project_name: 'APP',
                branch_priority_aliases: { hotfix: 'High', sev1: 'Critical' },
            } as any,
            project_exists: true,
            sources: { branch_priority_aliases: 'project' },
        })
            ; (api.inspectConfig as any).mockResolvedValue(projectInspect)

        const { wrapper } = await mountConfigView('/?project=APP')
        await flushPromises()

        const vm = wrapper.vm as any
        vm.form.branchPriorityAliases = []

        const payload = vm.buildPayload()
        expect(payload.branch_priority_aliases).toBe('')

        wrapper.unmount()
    })

    it('flags duplicate alias entries during validation', async () => {
        ; (api.inspectConfig as any).mockResolvedValue(createInspectResult())

        const { wrapper } = await mountConfigView('/')
        await flushPromises()

        const vm = wrapper.vm as any
        vm.form.branchStatusAliases = [
            { key: 'WIP', value: 'InProgress' },
            { key: 'wip', value: 'Todo' },
        ]
        vm.validateField('branch_status_aliases')
        expect(vm.errors.branch_status_aliases).toContain('duplicated')

        wrapper.unmount()
    })

    it('keeps alias sections styled with expected layout values', async () => {
        ; (api.inspectConfig as any).mockResolvedValue(createInspectResult())

        const { wrapper } = await mountConfigView('/')
        await flushPromises()

        const sections = wrapper.findAll('.alias-section')
        expect(sections.length).toBeGreaterThan(1)

        const aliasStyle = extractStyle(ConfigAliasSectionSource)
        expect(aliasStyle).toMatch(/\.alias-section[^{}]*\{[^}]*display:\s*flex/)
        expect(aliasStyle).toMatch(/\.alias-section[^{}]*\{[^}]*flex-direction:\s*column/)
        expect(aliasStyle).toMatch(/\.alias-section[^{}]*\{[^}]*gap:\s*8px/)
        expect(aliasStyle).toMatch(/\.alias-row[^{}]*\{[^}]*grid-template-columns:\s*1fr\s+1fr\s+auto/)
        expect(aliasStyle).toMatch(/\.alias-row[^{}]*\{[^}]*align-items:\s*center/)
        expect(aliasStyle).toMatch(/\.alias-actions[^{}]*\{[^}]*display:\s*flex/)
        expect(aliasStyle).toMatch(/\.alias-actions[^{}]*\{[^}]*gap:\s*8px/)

        wrapper.unmount()
    })

    it('shows an error banner and keeps existing state when reload fails', async () => {
        ; (api.inspectConfig as any).mockResolvedValue(createInspectResult())

        const { wrapper } = await mountConfigView('/')
        await flushPromises()

        const vm = wrapper.vm as any
        expect(vm.form.serverPort).toBe('8080')
        expect(wrapper.find('.alert-error').exists()).toBe(false)

            ; (api.inspectConfig as any).mockRejectedValueOnce(new Error('server exploded'))

        const reloadButton = wrapper.find('button[aria-label="Reload configuration"]')
        expect(reloadButton.exists()).toBe(true)
        await reloadButton.trigger('click')
        await flushPromises()

        const errorBanner = wrapper.find('.alert-error')
        expect(errorBanner.exists()).toBe(true)
        expect(errorBanner.text()).toContain('server exploded')
        expect(vm.form.serverPort).toBe('8080')

        wrapper.unmount()
    })

    it('surfaces errors when save fails without reloading data', async () => {
        ; (api.inspectConfig as any).mockResolvedValue(createInspectResult())

        const { wrapper } = await mountConfigView('/')
        await flushPromises()

        const vm = wrapper.vm as any
        vm.form.defaultProject = 'ALPHA'
        await wrapper.vm.$nextTick()

            ; (api.setConfig as any).mockRejectedValueOnce(new Error('save denied'))

        const saveButton = wrapper.find('.floating-actions__buttons .btn')
        expect(saveButton.exists()).toBe(true)
        expect(saveButton.text()).toContain('Save changes')
        expect(saveButton.attributes('disabled')).toBeUndefined()
        await saveButton.trigger('click')
        await flushPromises()
        await wrapper.vm.$nextTick()

        expect((api.setConfig as any).mock.calls.length).toBe(1)
        expect((wrapper.vm as any).error).toBe('save denied')
        expect(wrapper.find('.alert-error').exists()).toBe(true)
        expect(wrapper.find('.alert-error').text()).toContain('save denied')
        expect((api.inspectConfig as any).mock.calls.length).toBe(1)

        wrapper.unmount()
    })

    it('syncs project selection with the route and reloads config', async () => {
        ; (api.inspectConfig as any)
            .mockResolvedValueOnce(createInspectResult())
            .mockResolvedValueOnce(
                createInspectResult({
                    effective: {
                        ...baseResolvedConfig(),
                        project_name: 'APP',
                        default_project: 'APP',
                    } as any,
                    project_raw: {
                        project_name: 'APP',
                        default_project: 'APP',
                    } as any,
                    project_exists: true,
                    sources: { project_name: 'project' },
                }),
            )

        const { wrapper, router } = await mountConfigView('/')
        await flushPromises()

        expect(router.currentRoute.value.query.project).toBeUndefined()

        const scopeSelect = wrapper.find('select.scope-select')
        expect(scopeSelect.exists()).toBe(true)
        await scopeSelect.setValue('APP')
        await flushPromises()

        expect(router.currentRoute.value.query.project).toBe('APP')
        expect((api.inspectConfig as any).mock.calls[1][0]).toBe('APP')

        const vm = wrapper.vm as any
        expect(vm.form.projectName).toBe('APP')

        wrapper.unmount()
    })

    it('builds payloads with correct tri-state toggle behaviour', async () => {
        const inspectMock = api.inspectConfig as any

        inspectMock.mockResolvedValueOnce(createInspectResult())

        const { wrapper: globalWrapper } = await mountConfigView('/')
        await flushPromises()

        const globalVm = globalWrapper.vm as any
        globalVm.form.autoSetReporter = 'false'
        const globalPayload = globalVm.buildPayload()
        expect(globalPayload.auto_set_reporter).toBe('false')

        globalWrapper.unmount()

        const projectInspect = createInspectResult({
            effective: {
                ...baseResolvedConfig(),
                project_name: 'APP',
                auto_assign_on_status: true,
            } as any,
            project_raw: {
                project_name: 'APP',
                auto_assign_on_status: true,
            } as any,
            project_exists: true,
            sources: { auto_assign_on_status: 'project' },
        })
        inspectMock.mockResolvedValueOnce(projectInspect)
        inspectMock.mockResolvedValueOnce(projectInspect) // watcher-triggered reload will request the same scope again

        const callsBeforeProject = inspectMock.mock.calls.length
        const { wrapper: projectWrapper } = await mountConfigView('/?project=APP')
        await flushPromises()

        const projectCalls = inspectMock.mock.calls.slice(callsBeforeProject)
        expect(projectCalls.length).toBeGreaterThanOrEqual(2)
        expect(projectCalls[0][0]).toBe('APP')
        expect(projectCalls[1][0]).toBe('APP')

        const projectVm = projectWrapper.vm as any
        const baselineRef = (projectVm as any).$?.setupState?.baseline as { value: any }
        expect(baselineRef?.value).not.toBeNull()
        expect(projectVm.isGlobal).toBe(false)
        projectVm.form.autoAssignOnStatus = 'true'
        await projectWrapper.vm.$nextTick()
        baselineRef.value = projectVm.snapshotForm()
        projectVm.form.autoAssignOnStatus = 'inherit'
        await projectWrapper.vm.$nextTick()
        const projectPayload = projectVm.buildPayload()
        expect(projectPayload.auto_assign_on_status).toBe('')
        expect(Object.keys(projectPayload)).toEqual(['auto_assign_on_status'])

        projectWrapper.unmount()
    })

    it('hydrates default project from inspect payloads', async () => {
        const inspect = createInspectResult()
            ; (api.inspectConfig as any).mockResolvedValue(inspect)

        const { wrapper } = await mountConfigView('/')
        await flushPromises()

        const vm = wrapper.vm as any
        expect(vm.form.defaultProject).toBe('TEST')

        vm.openCreateDialog()
        expect(vm.createPrefix).toBe('TEST')

        wrapper.unmount()
    })
})
