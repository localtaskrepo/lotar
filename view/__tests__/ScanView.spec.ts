import { flushPromises, mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'
import type { ConfigInspectResult, GlobalConfigRaw, ResolvedConfigDTO } from '../api/types'
import ScanView from '../pages/ScanView.vue'

vi.mock('../api/client', () => ({
  api: {
    scanRun: vi.fn(),
    referenceSnippet: vi.fn(),
    listProjects: vi.fn(),
    inspectConfig: vi.fn(),
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
  UiButton: {
    template: '<button type="button" @click="$emit(\'click\')"><slot /></button>',
  },
  UiCard: { template: '<section><slot /></section>' },
  UiLoader: { template: '<div><slot /></div>' },
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
    scan_signal_words: ['TODO'],
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
    scan_signal_words: ['TODO'],
    scan_ticket_patterns: [],
    scan_enable_ticket_words: true,
    scan_enable_mentions: true,
    scan_strip_attributes: true,
    branch_type_aliases: {},
    branch_status_aliases: {},
    branch_priority_aliases: {},
  }
}

function baseConfigInspect(): ConfigInspectResult {
  return {
    effective: baseResolvedConfig(),
    global_effective: baseResolvedConfig(),
    global_raw: baseGlobalRaw(),
    auth_profiles: {},
    project_raw: null,
    has_global_file: true,
    project_exists: true,
    sources: {},
  }
}

describe('ScanView', () => {
  it('runs a dry scan and renders results', async () => {
    ; (api.listProjects as any).mockResolvedValue({ projects: [{ prefix: 'TEST', name: 'Test' }], limit: 1, offset: 0, total: 1 })
      ; (api.inspectConfig as any).mockResolvedValue(baseConfigInspect())
      ; (api.scanRun as any).mockResolvedValue({
        status: 'ok',
        dry_run: true,
        project: 'TEST',
        summary: { created: 1, updated: 0, skipped: 0, failed: 0 },
        warnings: [],
        info: [],
        entries: [
          {
            status: 'created',
            action: 'create',
            file: 'src/lib.rs',
            line: 12,
            title: 'Add feature',
            annotation: 'TODO: Add feature',
            code_reference: 'src/lib.rs#12',
          },
        ],
      })

    const router = createRouter({
      history: createMemoryHistory(),
      routes: [{ path: '/scan', component: ScanView }],
    })
    await router.push('/scan')
    await router.isReady()

    const wrapper = mount(ScanView, { global: { plugins: [router], stubs } })
    await flushPromises()

    const dryRunButton = wrapper.findAll('button').find((btn) => btn.text() === 'Dry run')
    await dryRunButton?.trigger('click')
    await flushPromises()

    expect(api.scanRun).toHaveBeenCalled()
    expect(wrapper.text()).toContain('Add feature')
    expect(wrapper.text()).toContain('src/lib.rs:12')
  })
})
