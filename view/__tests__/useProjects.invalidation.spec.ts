import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { effectScope } from 'vue'
import { createMemoryHistory, createRouter } from 'vue-router'
import type { ApiClient } from '../api/client'
import type { ConfigInspectResult } from '../api/types'
import ConfigView from '../pages/ConfigView.vue'
import { createUseProjects, notifyProjectsChanged } from '../composables/useProjects'

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

import { api } from '../api/client'

function page(prefixes: string[]) {
    return {
        total: prefixes.length,
        limit: 500,
        offset: 0,
        projects: prefixes.map((prefix) => ({ name: prefix, prefix })),
    }
}

function createClient() {
    const listProjects = vi.fn()
    const client = { listProjects, projectStats: vi.fn() } as unknown as ApiClient
    return { client, listProjects }
}

describe('useProjects shared invalidation', () => {
    it('refreshes already-loaded instances so stale mounted snapshots update without reload', async () => {
        const { client, listProjects } = createClient()
        listProjects.mockResolvedValue(page(['APP']))

        const panelScope = effectScope()
        const panel = panelScope.run(() => createUseProjects(client))!
        const configScope = effectScope()
        const config = configScope.run(() => createUseProjects(client))!

        await panel.refresh()
        await config.refresh()
        expect(panel.projects.value.map((project) => project.prefix)).toEqual(['APP'])

        listProjects.mockResolvedValue(page(['APP', 'NEW']))
        await notifyProjectsChanged()

        expect(panel.projects.value.map((project) => project.prefix)).toEqual(['APP', 'NEW'])
        expect(config.projects.value.map((project) => project.prefix)).toEqual(['APP', 'NEW'])

        panelScope.stop()
        configScope.stop()
    })

    it('keeps never-loaded instances lazy across notifications', async () => {
        const lazyClient = createClient()
        const loadedClient = createClient()
        loadedClient.listProjects.mockResolvedValue(page(['APP']))

        const scope = effectScope()
        const lazy = scope.run(() => createUseProjects(lazyClient.client))!
        const loadedScope = effectScope()
        const loaded = loadedScope.run(() => createUseProjects(loadedClient.client))!
        await loaded.refresh()

        loadedClient.listProjects.mockClear()
        await notifyProjectsChanged()
        expect(loadedClient.listProjects).toHaveBeenCalledTimes(1)
        expect(lazyClient.listProjects).not.toHaveBeenCalled()
        expect(lazy.projects.value).toEqual([])

        lazyClient.listProjects.mockResolvedValue(page(['APP']))
        await lazy.refresh()
        expect(lazyClient.listProjects).toHaveBeenCalledTimes(1)
        expect(lazy.projects.value.map((project) => project.prefix)).toEqual(['APP'])

        scope.stop()
        loadedScope.stop()
    })

    it('contains refresh failures per instance while other instances still update', async () => {
        const { client, listProjects } = createClient()
        listProjects.mockResolvedValue(page(['APP']))

        const failingScope = effectScope()
        const failing = failingScope.run(() => createUseProjects(client))!
        const healthyScope = effectScope()
        const healthy = healthyScope.run(() => createUseProjects(client))!
        await failing.refresh()
        await healthy.refresh()

        listProjects
            .mockRejectedValueOnce(new Error('projects fetch failed'))
            .mockResolvedValueOnce(page(['APP', 'NEW']))

        await expect(notifyProjectsChanged()).resolves.toBeUndefined()

        expect(failing.error.value).toBe('projects fetch failed')
        expect(failing.projects.value.map((project) => project.prefix)).toEqual(['APP'])
        expect(healthy.projects.value.map((project) => project.prefix)).toEqual(['APP', 'NEW'])
        expect(healthy.error.value).toBeNull()

        failingScope.stop()
        healthyScope.stop()
    })

    it('stops listening once the owning scope is disposed', async () => {
        const { client, listProjects } = createClient()
        listProjects.mockResolvedValue(page(['APP']))

        const scope = effectScope()
        const instance = scope.run(() => createUseProjects(client))!
        await instance.refresh()
        scope.stop()

        listProjects.mockClear()
        await notifyProjectsChanged()
        expect(listProjects).not.toHaveBeenCalled()
    })
})

describe('ConfigView project creation propagation', () => {
    function baseResolvedConfig() {
        return {
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

    function createInspectResult(): ConfigInspectResult {
        return {
            effective: baseResolvedConfig() as ConfigInspectResult['effective'],
            global_effective: baseResolvedConfig() as ConfigInspectResult['global_effective'],
            global_raw: baseResolvedConfig(),
            auth_profiles: {},
            project_raw: null,
            has_global_file: true,
            project_exists: false,
            sources: {},
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
            attachTo: document.body,
            global: {
                plugins: [router],
                stubs: {
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
                    ConfigGroup: { template: '<section><slot /></section>', props: ['title', 'description', 'source'] },
                },
            },
        })
        return wrapper
    }

    beforeEach(() => {
        vi.clearAllMocks()
        ;(api.inspectConfig as any).mockResolvedValue(createInspectResult())
        ;(api.inspectAutomation as any).mockResolvedValue({
            source: 'global',
            scope_exists: true,
            scope_yaml: '',
            effective_yaml: '',
        })
    })

    afterEach(() => {
        document.body.innerHTML = ''
    })

    it('notifies every live projects snapshot after creating a project', async () => {
        ;(api.listProjects as any).mockResolvedValue(page(['APP']))
        ;(api.createProject as any).mockResolvedValue({ name: 'Fresh Project', prefix: 'FRESH' })

        // Stand-in for an always-mounted consumer (e.g. the task panel host)
        // with its own warmed useProjects instance.
        const panelScope = effectScope()
        const panelProjects = panelScope.run(() => createUseProjects(api as unknown as ApiClient))!
        await panelProjects.refresh()

        const wrapper = await mountConfigView()
        await flushPromises()

        ;(api.listProjects as any).mockResolvedValue(page(['APP', 'FRESH']))

        const vm = wrapper.vm as any
        vm.openCreateDialog()
        vm.createName = 'Fresh Project'
        vm.createPrefix = 'FRESH'
        await vm.submitCreateProject()
        await flushPromises()

        expect(api.createProject).toHaveBeenCalledWith({ name: 'Fresh Project', prefix: 'FRESH' })
        expect(panelProjects.projects.value.map((project) => project.prefix)).toEqual(['APP', 'FRESH'])
        expect(vm.projects.map((project: any) => project.prefix)).toEqual(['APP', 'FRESH'])
        expect(vm.project).toBe('FRESH')

        wrapper.unmount()
        panelScope.stop()
    })
})
