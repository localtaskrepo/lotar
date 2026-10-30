import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ref } from 'vue'
import { createMemoryHistory, createRouter } from 'vue-router'
import type { ConfigInspectResult, ProjectDTO } from '../api/types'
import { useConfigForm } from '../composables/useConfigForm'
import ConfigView from '../pages/ConfigView.vue'
import { CONFIG_SECTIONS, resolveSection } from '../utils/configSections'

// DEV-19: the Config page shows one section at a time, routed through
// ?section=, without losing unsaved edits.

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
    default: { name: 'AutomationRulesEditor', props: ['modelValue'], template: '<div />' },
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

function resolved() {
    return {
        review_reviewers: [],
        server_port: 8080,
        default_project: 'APP',
        attachments_dir: '@attachments',
        attachments_max_upload_mb: 10,
        sync_reports_dir: '@reports',
        sync_write_reports: true,
        default_assignee: null,
        default_reporter: null,
        default_tags: [],
        default_priority: 'Medium',
        default_status: 'Todo',
        issue_states: ['Todo', 'Done'],
        issue_types: ['Feature'],
        issue_priorities: ['Low', 'Medium'],
        issue_done_states: null,
        effective_done_states: ['Done'],
        done_states_mode: 'inferred',
        task_calendar_day: '2026-10-24',
        tags: ['*'],
        custom_fields: ['*'],
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
        members: [],
        strict_members: false,
        auto_populate_members: true,
        agent_context_enabled: true,
        agent_context_extension: '.context',
        agent_worktree: { enabled: false, branch_prefix: 'agent/' },
        sprint_defaults: {},
        sprint_notifications: { enabled: true },
    } as ConfigInspectResult['effective']
}

function inspect(projectRaw: ConfigInspectResult['project_raw'] = null): ConfigInspectResult {
    return {
        effective: resolved(),
        global_effective: resolved(),
        global_raw: {} as ConfigInspectResult['global_raw'],
        auth_profiles: {},
        project_raw: projectRaw,
        has_global_file: true,
        project_exists: !!projectRaw,
        sources: {},
    }
}

async function mountAt(path: string) {
    const router = createRouter({
        history: createMemoryHistory(),
        routes: [
            { path: '/config', component: ConfigView },
            { path: '/other', component: { template: '<div />' } },
        ],
    })
    await router.push(path)
    await router.isReady()
    // Rendered through RouterView like the app, so route-leave guards apply.
    const wrapper = mount(
        { template: '<RouterView />' },
        {
            attachTo: document.body,
            global: { plugins: [router] },
        },
    )
    await flushPromises()
    const vm = () => wrapper.findComponent(ConfigView).vm as any
    return { wrapper, router, vm }
}

const visibleSection = (wrapper: ReturnType<typeof mount>) =>
    wrapper
        .findAll('section.config-section')
        .filter((section) => (section.element as HTMLElement).style.display !== 'none')
        .map((section) => section.attributes('data-section'))

describe('Config section map (DEV-19)', () => {
    it('assigns every editable setting to exactly one section', () => {
        const form = useConfigForm({
            project: ref(''),
            projects: ref<ProjectDTO[]>([]),
            inspectData: ref(inspect()),
        })
        const keys = Object.keys(form.snapshotForm()).sort()
        const mapped = CONFIG_SECTIONS.flatMap((section) => section.fields as string[])
        expect([...mapped].sort()).toEqual(keys)
        expect(new Set(mapped).size).toBe(mapped.length)
    })

    it('falls back to the first section for unknown or scope-unavailable sections', () => {
        expect(resolveSection('workflow', true)).toBe('workflow')
        expect(resolveSection('nope', true)).toBe('general')
        expect(resolveSection('sprints', true)).toBe('sprints')
        expect(resolveSection('sprints', false)).toBe('general')
    })
})

describe('ConfigView sections (DEV-19)', () => {
    beforeEach(() => {
        vi.clearAllMocks()
        ;(api.listProjects as any).mockResolvedValue({
            total: 1,
            limit: 50,
            offset: 0,
            projects: [{ name: 'App', prefix: 'APP' }],
        })
        ;(api.inspectConfig as any).mockImplementation(async (project?: string) =>
            inspect(project ? { project_name: 'App' } : null),
        )
        ;(api.inspectAutomation as any).mockResolvedValue({
            source: 'global',
            scope_exists: true,
            scope_yaml: '',
            effective_yaml: '',
        })
    })

    it('finds settings by name or description and opens their section (DEV-166)', async () => {
        const { wrapper, router } = await mountAt('/config')
        const search = wrapper.get('[data-testid="config-search"]')
        await search.setValue('jobs at once')
        await search.trigger('focus')
        const options = wrapper.findAll('.config-search__result')
        expect(options.length).toBeGreaterThan(0)
        expect(options[0]!.text()).toContain('Jobs at once')
        expect(options[0]!.text()).toContain('Agents')

        // Description words match too, and Enter picks the highlighted result.
        await search.setValue('build output')
        expect(wrapper.findAll('.config-search__result')[0]!.text()).toContain('Ignore build output')
        await search.setValue('jobs at once')
        await search.trigger('keydown', { key: 'Enter' })
        await flushPromises()
        expect(router.currentRoute.value.query.section).toBe('agents')
        const field = wrapper.get('[data-testid="config-agent-max-jobs"]').element
        expect(field.closest('.config-search-hit')).not.toBeNull()
        expect(document.activeElement).toBe(field)

        // Back in the box right after a pick (blur pending): new results stay open.
        await search.trigger('blur')
        await search.trigger('focus')
        await search.setValue('build output')
        await new Promise((resolve) => setTimeout(resolve, 200))
        expect(wrapper.findAll('.config-search__result').length).toBeGreaterThan(0)

        await search.setValue('nothing like this')
        expect(wrapper.text()).toContain('No settings match.')
        wrapper.unmount()
    })

    it('routes the visible section through ?section= and keeps edits across sections', async () => {
        const { wrapper, router, vm: view } = await mountAt('/config')
        expect(visibleSection(wrapper)).toEqual(['general'])

        await wrapper.get('[data-section-link="defaults"]').trigger('click')
        await flushPromises()
        expect(router.currentRoute.value.query.section).toBe('defaults')
        expect(visibleSection(wrapper)).toEqual(['defaults'])
        expect(wrapper.get('[data-section-link="defaults"]').attributes('aria-current')).toBe('page')

        const vm = view()
        vm.form.defaultReporter = 'alice'
        await flushPromises()
        expect(wrapper.get('[data-section-link="defaults"]').find('.config-nav__badge--dirty').exists()).toBe(true)

        await wrapper.get('[data-section-link="people"]').trigger('click')
        await flushPromises()
        expect(visibleSection(wrapper)).toEqual(['people'])
        expect(vm.form.defaultReporter).toBe('alice')
        expect(wrapper.get('[data-section-link="defaults"]').find('.config-nav__badge--dirty').exists()).toBe(true)
        expect(wrapper.get('[data-section-link="people"]').find('.config-nav__badge').exists()).toBe(false)
        wrapper.unmount()
    })

    it('hides global-only sections in project scope and keeps the section across scope switches', async () => {
        const { wrapper, router } = await mountAt('/config?section=sprints')
        expect(visibleSection(wrapper)).toEqual(['sprints'])

        const select = wrapper.get('select[aria-label="Project scope"]')
        await select.setValue('APP')
        await flushPromises()
        expect(router.currentRoute.value.query.project).toBe('APP')
        expect(wrapper.find('[data-section-link="sprints"]').exists()).toBe(false)
        expect(visibleSection(wrapper)).toEqual(['general'])

        await wrapper.get('[data-section-link="workflow"]').trigger('click')
        await flushPromises()
        expect(router.currentRoute.value.query).toMatchObject({ project: 'APP', section: 'workflow' })
        wrapper.unmount()
    })

    it('asks before a scope switch or navigation discards unsaved changes', async () => {
        const { wrapper, router, vm: view } = await mountAt('/config?section=defaults')
        const vm = view()
        vm.form.defaultAssignee = 'bob'
        await flushPromises()

        await wrapper.get('select[aria-label="Project scope"]').setValue('APP')
        await flushPromises()
        const dialog = () => wrapper.find('[data-testid="config-discard-dialog"]')
        expect(dialog().exists()).toBe(true)

        await dialog()
            .findAll('button')
            .find((b) => b.text() === 'Keep editing')!
            .trigger('click')
        await flushPromises()
        expect(router.currentRoute.value.query.project).toBeUndefined()
        expect(vm.project).toBe('')
        expect(vm.form.defaultAssignee).toBe('bob')
        expect((wrapper.get('select[aria-label="Project scope"]').element as HTMLSelectElement).value).toBe('')

        const leaving = router.push('/other')
        await flushPromises()
        expect(dialog().exists()).toBe(true)
        await dialog()
            .findAll('button')
            .find((b) => b.text() === 'Keep editing')!
            .trigger('click')
        await leaving
        expect(router.currentRoute.value.path).toBe('/config')

        await wrapper.get('select[aria-label="Project scope"]').setValue('APP')
        await flushPromises()
        await dialog()
            .findAll('button')
            .find((b) => b.text() === 'Discard changes')!
            .trigger('click')
        await flushPromises()
        expect(router.currentRoute.value.query).toMatchObject({ project: 'APP', section: 'defaults' })
        expect(vm.form.defaultAssignee).toBe('')
        wrapper.unmount()
    })

    it('jumps to the first section with validation errors when saving', async () => {
        const { wrapper, router, vm: view } = await mountAt('/config?section=general')
        const vm = view()
        vm.form.sprintLength = 'soon'
        await flushPromises()

        await wrapper.get('.floating-actions__buttons .btn').trigger('click')
        await flushPromises()
        expect(api.setConfig).not.toHaveBeenCalled()
        expect(router.currentRoute.value.query.section).toBe('sprints')
        expect(wrapper.get('[data-section-link="sprints"]').find('.config-nav__badge--error').exists()).toBe(true)
        wrapper.unmount()
    })
})
