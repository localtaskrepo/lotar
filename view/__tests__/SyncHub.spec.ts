import { flushPromises, mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'
import type { ConfigInspectResult, GlobalConfigRaw, ResolvedConfigDTO } from '../api/types'
import SyncHub from '../pages/SyncHub.vue'

vi.mock('../api/client', () => ({
    api: {
        inspectConfig: vi.fn(),
        listProjects: vi.fn(),
        setConfig: vi.fn(),
        syncPull: vi.fn(),
        syncPush: vi.fn(),
        syncValidate: vi.fn(),
        syncReportsList: vi.fn(),
        syncReportGet: vi.fn(),
    },
}))

vi.mock('../composables/useSse', () => ({
    useSse: () => ({
        es: {} as EventSource,
        on: vi.fn(),
        off: vi.fn(),
        close: vi.fn(),
    }),
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
    UiButton: {
        template: '<button type="button" @click="$emit(\'click\')"><slot /></button>',
    },
    UiCard: { template: '<section><slot /></section>' },
    UiLoader: { template: '<div><slot /></div>' },
    ReloadButton: { template: '<button type="button" @click="$emit(\'click\')">Reload</button>', props: ['loading'] },
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
        remotes: {},
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
        remotes: {},
    }
}

function createInspectResult(): ConfigInspectResult {
    return {
        effective: baseResolvedConfig(),
        global_effective: baseResolvedConfig(),
        global_raw: baseGlobalRaw(),
        auth_profiles: {},
        project_raw: null,
        has_global_file: false,
        project_exists: false,
        sources: {},
    }
}

async function mountSyncHub() {
    const router = createRouter({
        history: createMemoryHistory(),
        routes: [{ path: '/sync', component: SyncHub }],
    })
    router.push('/sync')
    await router.isReady()

    const wrapper = mount(SyncHub, {
        global: {
            plugins: [router],
            stubs,
        },
    })
    await flushPromises()
    return wrapper
}

describe('SyncHub', () => {
    it('renders empty states', async () => {
        ; (api.inspectConfig as any).mockResolvedValue(createInspectResult())
            ; (api.listProjects as any).mockResolvedValue({ total: 0, limit: 50, offset: 0, projects: [] })
            ; (api.syncReportsList as any).mockResolvedValue({ total: 0, limit: 50, offset: 0, reports: [] })

        const wrapper = await mountSyncHub()

        expect(wrapper.text()).toContain('No remotes configured for this scope')
        expect(wrapper.text()).toContain('No reports yet')
    })
})
