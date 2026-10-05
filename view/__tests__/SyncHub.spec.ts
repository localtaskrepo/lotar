import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'
import type { ConfigInspectResult, GlobalConfigRaw, ProjectConfigRaw, ResolvedConfigDTO, SyncRemoteConfig, SyncReportEntry, SyncReportMeta, SyncResponse } from '../api/types'
import SyncHub from '../pages/SyncHub.vue'

const sseCalls: Array<{
  params: Record<string, string>
  handlers: Map<string, Set<(ev: { data: string }) => void>>
  opts: { onReconnect?: () => void }
  close: ReturnType<typeof vi.fn>
}> = []
const toastMessages: string[] = []

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
    useSse: (_path: string, params: Record<string, string>, opts: { onReconnect?: () => void } = {}) => {
        const handlers = new Map<string, Set<(ev: { data: string }) => void>>()
        const close = vi.fn()
        sseCalls.push({ params, handlers, opts, close })
        return {
            es: {} as EventSource,
            on: vi.fn((event: string, handler: (ev: { data: string }) => void) => {
                if (!handlers.has(event)) handlers.set(event, new Set())
                handlers.get(event)!.add(handler)
            }),
            off: vi.fn((event: string, handler: (ev: { data: string }) => void) => {
                handlers.get(event)?.delete(handler)
            }),
            close,
        }
    },
}))

vi.mock('../components/toast', () => ({
    showToast: vi.fn((message: string) => {
        toastMessages.push(message)
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
        props: ['type'],
        template: '<button :type="type || \'button\'" @click="$emit(\'click\')"><slot /></button>',
    },
    UiCard: { template: '<section><slot /></section>' },
    UiLoader: { template: '<div><slot /></div>' },
    ReloadButton: { template: '<button type="button" @click="$emit(\'click\')">Reload</button>', props: ['loading'] },
    // Render <Teleport> content in place so wrapper.find can reach the dialog.
    teleport: true,
}

function deferred<T>() {
    let resolve!: (value: T) => void
    let reject!: (reason?: unknown) => void
    const promise = new Promise<T>((res, rej) => {
        resolve = res
        reject = rej
    })
    return { promise, resolve, reject }
}

function baseResolvedConfig(overrides: Partial<ResolvedConfigDTO> = {}): ResolvedConfigDTO {
    return {
        server_port: 8080,
        default_project: '',
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
        remotes: {},
        ...overrides,
    }
}

function baseGlobalRaw(remotes: Record<string, SyncRemoteConfig> = {}, defaultProject = ''): GlobalConfigRaw {
    return {
        server_port: 8080,
        default_project: defaultProject,
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
        auto_tags_from_path: true,
        auto_identity: true,
        auto_identity_git: true,
        auto_codeowners_assign: true,
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
        remotes,
    } as GlobalConfigRaw
}

const configState: {
    defaultProject: string
    globalRemotes: Record<string, SyncRemoteConfig>
    projectRemotes: Record<string, Record<string, SyncRemoteConfig>>
} = {
    defaultProject: '',
    globalRemotes: {},
    projectRemotes: {},
}

function resetConfigState() {
    configState.defaultProject = ''
    configState.globalRemotes = {}
    configState.projectRemotes = {}
}

function buildInspect(scope?: string): ConfigInspectResult {
    const target = (scope ?? '').trim()
    const globalEffective = baseResolvedConfig({
        default_project: configState.defaultProject,
        remotes: configState.globalRemotes,
    })
    if (!target) {
        return {
            effective: globalEffective,
            global_effective: globalEffective,
            global_raw: baseGlobalRaw(configState.globalRemotes, configState.defaultProject),
            auth_profiles: {},
            project_raw: null,
            has_global_file: true,
            project_exists: false,
            sources: {},
        }
    }
    const projectRemotes = configState.projectRemotes[target] ?? {}
    const projectRaw: ProjectConfigRaw = { remotes: projectRemotes }
    return {
        effective: baseResolvedConfig({
            default_project: configState.defaultProject,
            remotes: { ...configState.globalRemotes, ...projectRemotes },
        }),
        global_effective: globalEffective,
        global_raw: baseGlobalRaw(configState.globalRemotes, configState.defaultProject),
        auth_profiles: {},
        project_raw: projectRaw,
        has_global_file: true,
        project_exists: true,
        sources: {},
    }
}

function jiraRemote(project = 'DEMO', filter: string | null = null): SyncRemoteConfig {
    return { provider: 'jira', project, repo: null, filter, auth_profile: 'jira.default', mapping: {} }
}

function reportEntry(taskId = 'ALPHA-1'): SyncReportEntry {
    return { status: 'created', at: new Date().toISOString(), task_id: taskId, reference: null, title: null, message: null }
}

function reportMeta(id: string, remote: string, project: string | null, overrides: Partial<SyncReportMeta> = {}): SyncReportMeta {
    return {
        id,
        created_at: new Date().toISOString(),
        status: 'ok',
        direction: 'pull',
        provider: 'jira',
        remote,
        project,
        dry_run: false,
        summary: { created: 1, updated: 0, skipped: 0, failed: 0 },
        warnings: [],
        info: [],
        entries_total: 1,
        stored_path: `${id}.yml`,
        ...overrides,
    }
}

function syncResponse(runId: string, remote: string, project: string | null, overrides: Partial<SyncResponse> = {}): SyncResponse {
    return {
        status: 'ok',
        direction: 'pull',
        provider: 'jira',
        remote,
        project,
        dry_run: false,
        summary: { created: 1, updated: 0, skipped: 0, failed: 0 },
        warnings: [],
        info: [],
        run_id: runId,
        report: reportMeta(runId, remote, project),
        report_entries: [reportEntry()],
        ...overrides,
    }
}

function emptyList() {
    return { total: 0, limit: 20, offset: 0, reports: [] }
}

let listHandler: (params: { project?: string; limit?: number }) => Promise<any>

function currentSse() {
    return sseCalls[sseCalls.length - 1]!
}

function emitSse(kind: string, payload: unknown) {
    const ev = { data: JSON.stringify(payload) }
    currentSse().handlers.get(kind)?.forEach((handler) => handler(ev))
}

function triggerReconnect() {
    currentSse().opts.onReconnect?.()
}

async function mountSyncHub(route = '/sync') {
    const router = createRouter({
        history: createMemoryHistory(),
        routes: [{ path: '/sync', component: SyncHub }],
    })
    router.push(route)
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

function remoteRow(wrapper: ReturnType<typeof mount>, name: string) {
    const row = wrapper.findAll('.remote-row').find((candidate) => candidate.text().includes(name))
    expect(row, `remote row ${name}`).toBeTruthy()
    return row!
}

function rowButton(row: ReturnType<typeof remoteRow>, label: string) {
    const button = row.findAll('button').find((candidate) => candidate.text() === label)
    expect(button, `button ${label}`).toBeTruthy()
    return button!
}

async function submitDialog(wrapper: ReturnType<typeof mount>) {
    // jsdom does not perform implicit form submission on button clicks.
    await wrapper.find('form').trigger('submit')
    await flushPromises()
}

function dialogButton(wrapper: ReturnType<typeof mount>, label: string) {
    const button = wrapper.findAll('button').find((candidate) => candidate.text().includes(label))
    expect(button, `dialog button ${label}`).toBeTruthy()
    return button!
}

async function switchScope(wrapper: ReturnType<typeof mount>, value: string) {
    await wrapper.find('#sync-scope').setValue(value)
    await flushPromises()
}

beforeEach(() => {
    sseCalls.splice(0)
    toastMessages.splice(0)
    resetConfigState()
    vi.clearAllMocks()
    listHandler = async () => emptyList()
    ;(api.inspectConfig as any).mockImplementation(async (scope?: string) => buildInspect(scope))
    ;(api.listProjects as any).mockResolvedValue({
        total: 2,
        limit: 50,
        offset: 0,
        projects: [
            { name: 'Alpha', prefix: 'ALPHA' },
            { name: 'Beta', prefix: 'BETA' },
        ],
    })
    ;(api.syncReportsList as any).mockImplementation((params: any) => listHandler(params ?? {}))
    ;(api.syncReportGet as any).mockImplementation(async (path: string) => ({
        ...reportMeta(path.replace(/\.yml$/, ''), 'loaded', null),
        entries: [reportEntry()],
    }))
    ;(api.setConfig as any).mockResolvedValue({ updated: true, warnings: [], info: [], errors: [] })
})

afterEach(() => {
    vi.restoreAllMocks()
})

describe('SyncHub scope isolation', () => {
    it('renders empty states', async () => {
        const wrapper = await mountSyncHub()
        expect(wrapper.text()).toContain('No remotes configured for this scope')
        expect(wrapper.text()).toContain('No reports yet')
    })

    it('opens one persistent unfiltered SSE connection that survives project switches', async () => {
        const wrapper = await mountSyncHub()
        expect(sseCalls).toHaveLength(1)
        expect(sseCalls[0]!.params.kinds).toBe('sync_started,sync_progress,sync_completed,sync_failed')
        expect(sseCalls[0]!.params.project).toBeUndefined()

        await switchScope(wrapper, 'ALPHA')
        expect(sseCalls).toHaveLength(1)
        expect(sseCalls[0]!.close).not.toHaveBeenCalled()
        wrapper.unmount()
        expect(sseCalls[0]!.close).toHaveBeenCalled()
    })

    it('attributes runs by captured origin scope and keeps remote homonyms independent', async () => {
        configState.defaultProject = 'ALPHA'
        configState.globalRemotes = { shared: jiraRemote('ALPHA') }
        configState.projectRemotes = { ALPHA: { shared: jiraRemote('ALPHA') } }
        const pull = deferred<SyncResponse>()
        ;(api.syncPull as any).mockReturnValue(pull.promise)

        const wrapper = await mountSyncHub()
        const globalRow = remoteRow(wrapper, 'shared')
        await rowButton(globalRow, 'Pull').trigger('click')
        await flushPromises()

        // Global-origin run is busy in Global only; the ALPHA homonym stays free.
        expect(rowButton(remoteRow(wrapper, 'shared'), 'Pull').attributes('disabled')).toBeDefined()
        await switchScope(wrapper, 'ALPHA')
        const alphaRow = remoteRow(wrapper, 'shared')
        expect(rowButton(alphaRow, 'Pull').attributes('disabled')).toBeUndefined()

        const runId = (api.syncPull as any).mock.calls[0][0].client_run_id as string
        emitSse('sync_completed', {
            run_id: runId,
            report: reportMeta(runId, 'shared', 'ALPHA'),
            finished_at: new Date().toISOString(),
        })
        await flushPromises()

        // Back in Global the run is a success and the report keeps the server task project.
        await switchScope(wrapper, '')
        expect(remoteRow(wrapper, 'shared').text()).toContain('Success')
        expect(wrapper.text()).toContain('ALPHA')

        // The global-origin run never appears in the ALPHA homonym scope.
        await switchScope(wrapper, 'ALPHA')
        expect(wrapper.text()).not.toContain('Success')
        pull.resolve(syncResponse(runId, 'shared', 'ALPHA'))
        await flushPromises()
        expect(rowButton(remoteRow(wrapper, 'shared'), 'Pull').attributes('disabled')).toBeUndefined()
        wrapper.unmount()
    })

    it('external SSE events never control known homonym remote state and surface as external reports', async () => {
        configState.globalRemotes = { shared: jiraRemote() }
        const wrapper = await mountSyncHub()

        emitSse('sync_started', {
            run_id: 'ext-1',
            remote: 'shared',
            direction: 'pull',
            project: 'ALPHA',
            dry_run: false,
            started_at: new Date().toISOString(),
        })
        await flushPromises()
        // An external run must not mark the local remote busy.
        expect(rowButton(remoteRow(wrapper, 'shared'), 'Pull').attributes('disabled')).toBeUndefined()

        emitSse('sync_completed', {
            run_id: 'ext-1',
            report: reportMeta('ext-1', 'shared', 'ALPHA'),
            finished_at: new Date().toISOString(),
        })
        await flushPromises()
        // The global aggregator shows the external report, tagged External, and never a busy pill.
        expect(wrapper.text()).toContain('External')
        expect(remoteRow(wrapper, 'shared').text()).not.toContain('Running')

        // Project scope sees external activity for its own project only.
        await switchScope(wrapper, 'ALPHA')
        expect(wrapper.text()).toContain('External')
        await switchScope(wrapper, 'BETA')
        expect(wrapper.text()).not.toContain('External')
        wrapper.unmount()
    })

    it('SSE completion before REST finalizes the run and the late REST response does not regress it', async () => {
        configState.globalRemotes = { 'jira-home': jiraRemote() }
        const pull = deferred<SyncResponse>()
        ;(api.syncPull as any).mockReturnValue(pull.promise)
        const wrapper = await mountSyncHub()

        await rowButton(remoteRow(wrapper, 'jira-home'), 'Pull').trigger('click')
        await flushPromises()
        const runId = (api.syncPull as any).mock.calls[0][0].client_run_id as string

        emitSse('sync_completed', {
            run_id: runId,
            report: reportMeta(runId, 'jira-home', null),
            finished_at: new Date().toISOString(),
        })
        await flushPromises()
        expect(remoteRow(wrapper, 'jira-home').text()).toContain('Success')

        // Late REST resolution arrives after SSE; the finalized run must not regress.
        pull.resolve(syncResponse(runId, 'jira-home', null))
        await flushPromises()
        expect(remoteRow(wrapper, 'jira-home').text()).toContain('Success')
        expect(rowButton(remoteRow(wrapper, 'jira-home'), 'Pull').attributes('disabled')).toBeUndefined()
        wrapper.unmount()
    })

    it('completed runs do not steal a report selection the user made meanwhile', async () => {
        configState.globalRemotes = { 'jira-home': jiraRemote() }
        const stored = reportMeta('rep-old', 'oldremote', null)
        listHandler = async () => ({ total: 1, limit: 20, offset: 0, reports: [stored] })
        const pull = deferred<SyncResponse>()
        ;(api.syncPull as any).mockReturnValue(pull.promise)

        const wrapper = await mountSyncHub()
        await rowButton(remoteRow(wrapper, 'jira-home'), 'Pull').trigger('click')
        await flushPromises()
        const runId = (api.syncPull as any).mock.calls[0][0].client_run_id as string

        // User opens a different stored report while the run is in flight.
        const item = wrapper.findAll('.report-item').find((el) => el.text().includes('oldremote'))!
        await item.trigger('click')
        await flushPromises()

        emitSse('sync_completed', {
            run_id: runId,
            report: reportMeta(runId, 'jira-home', null),
            finished_at: new Date().toISOString(),
        })
        await flushPromises()

        expect(wrapper.find('.report-item.active').text()).toContain('oldremote')
        pull.resolve(syncResponse(runId, 'jira-home', null))
        await flushPromises()
        expect(wrapper.find('.report-item.active').text()).toContain('oldremote')
        wrapper.unmount()
    })

    it('distinguishes transport request failures from server sync failures', async () => {
        configState.globalRemotes = { 'jira-home': jiraRemote(), other: jiraRemote() }
        const wrapper = await mountSyncHub()

        const failed = deferred<SyncResponse>()
        ;(api.syncPull as any).mockReturnValueOnce(failed.promise)
        await rowButton(remoteRow(wrapper, 'jira-home'), 'Pull').trigger('click')
        await flushPromises()
        failed.reject(new Error('network down'))
        await flushPromises()
        expect(remoteRow(wrapper, 'jira-home').text()).toContain('Failed')
        expect(toastMessages.some((m) => m.includes('Request failed: network down'))).toBe(true)

        const serverFailed = deferred<SyncResponse>()
        ;(api.syncPull as any).mockReturnValueOnce(serverFailed.promise)
        await rowButton(remoteRow(wrapper, 'other'), 'Pull').trigger('click')
        await flushPromises()
        const serverRunId = (api.syncPull as any).mock.calls[1][0].client_run_id as string
        emitSse('sync_failed', {
            run_id: serverRunId,
            direction: 'pull',
            remote: 'other',
            project: null,
            error: 'Auth profile is required for sync operations',
            finished_at: new Date().toISOString(),
        })
        await flushPromises()
        expect(remoteRow(wrapper, 'other').text()).toContain('Failed')
        expect(toastMessages.some((m) => m.includes('Auth profile'))).toBe(false)
        serverFailed.resolve(syncResponse(serverRunId, 'other', null))
        await flushPromises()
        expect(remoteRow(wrapper, 'other').text()).toContain('Failed')
        wrapper.unmount()
    })

    it('drops stale report responses across scope switches (A-B-A)', async () => {
        const calls: Array<ReturnType<typeof deferred<any>>> = []
        listHandler = () => {
            const d = deferred<any>()
            calls.push(d)
            return d.promise
        }
        const wrapper = await mountSyncHub()
        calls[0]!.resolve({ total: 1, limit: 20, offset: 0, reports: [reportMeta('g-1', 'gvremote', null)] })
        await flushPromises()
        expect(wrapper.text()).toContain('gvremote')

        await wrapper.find('#sync-scope').setValue('ALPHA')
        await wrapper.find('#sync-scope').setValue('')
        await flushPromises()
        // Old scope data is hidden immediately on switch.
        expect(wrapper.text()).not.toContain('gvremote')

        calls[1]!.resolve({ total: 1, limit: 20, offset: 0, reports: [reportMeta('a-1', 'avremote', 'ALPHA')] })
        await flushPromises()
        // Stale ALPHA response after switching back to Global is dropped.
        expect(wrapper.text()).not.toContain('avremote')

        calls[2]!.resolve({ total: 1, limit: 20, offset: 0, reports: [reportMeta('g-2', 'gwremote', null)] })
        await flushPromises()
        expect(wrapper.text()).toContain('gwremote')
        wrapper.unmount()
    })

    it('keeps valid same-scope reports on refresh failure with an error banner', async () => {
        let failNext = false
        listHandler = async () => {
            if (failNext) throw new Error('boom')
            return { total: 1, limit: 20, offset: 0, reports: [reportMeta('g-1', 'gvremote', null)] }
        }
        const wrapper = await mountSyncHub()
        expect(wrapper.text()).toContain('gvremote')

        failNext = true
        const reload = wrapper.findAll('button').find((b) => b.text() === 'Reload' && b.element.closest('.sync-card--reports') !== null)!
        await reload.trigger('click')
        await flushPromises()
        expect(wrapper.text()).toContain('gvremote')
        expect(wrapper.text()).toContain('boom')
        wrapper.unmount()
    })

    it('resolves stored reports through the listing query scope root, not report.project', async () => {
        configState.projectRemotes = { ALPHA: {} }
        const globalItem = reportMeta('glob-1', 'gvremote', 'ALPHA')
        listHandler = async (params) => {
            if (params?.project === 'ALPHA') {
                return { total: 1, limit: 20, offset: 0, reports: [reportMeta('alpha-1', 'avremote', 'ALPHA')] }
            }
            return { total: 1, limit: 20, offset: 0, reports: [globalItem] }
        }
        const wrapper = await mountSyncHub()
        const item = wrapper.findAll('.report-item').find((el) => el.text().includes('gvremote'))!
        await item.trigger('click')
        await flushPromises()
        // Global aggregator item resolves via the global root even though report.project is ALPHA.
        expect(api.syncReportGet).toHaveBeenCalledWith('glob-1.yml', undefined)

        await switchScope(wrapper, 'ALPHA')
        const alphaItem = wrapper.findAll('.report-item').find((el) => el.text().includes('avremote'))!
        await alphaItem.trigger('click')
        await flushPromises()
        expect(api.syncReportGet).toHaveBeenCalledWith('alpha-1.yml', 'ALPHA')
        wrapper.unmount()
    })

    it('falls back across candidate report roots when the primary lookup misses', async () => {
        const meta = reportMeta('ext-9', 'extremote', 'ALPHA', { stored_path: 'ext-9.yml' })
        listHandler = async () => emptyList()
        const wrapper = await mountSyncHub()

        emitSse('sync_completed', {
            run_id: 'ext-9',
            report: meta,
            finished_at: new Date().toISOString(),
        })
        await flushPromises()
        ;(api.syncReportGet as any).mockImplementation(async (_path: string, project?: string) => {
            if (project === 'ALPHA') {
                const err = new Error('GET /api/sync/reports/get failed: 404') as Error & { status?: number }
                err.status = 404
                throw err
            }
            return { ...meta, entries: [] }
        })

        const item = wrapper.findAll('.report-item').find((el) => el.text().includes('extremote'))!
        await item.trigger('click')
        await flushPromises()
        expect(api.syncReportGet).toHaveBeenCalledWith('ext-9.yml', 'ALPHA')
        expect(api.syncReportGet).toHaveBeenCalledWith('ext-9.yml', undefined)
        expect(wrapper.text()).not.toContain('Failed to load report')
        wrapper.unmount()
    })

    it('reconciles a missed completion after reconnect and reports unknown runs honestly', async () => {
        configState.globalRemotes = { 'jira-home': jiraRemote(), other: jiraRemote() }
        const wrapper = await mountSyncHub()

        // Run A: report exists on the server; the completion event was missed.
        const pullA = deferred<SyncResponse>()
        ;(api.syncPull as any).mockReturnValueOnce(pullA.promise)
        await rowButton(remoteRow(wrapper, 'jira-home'), 'Pull').trigger('click')
        await flushPromises()
        const runA = (api.syncPull as any).mock.calls[0][0].client_run_id as string

        // Run B: no report anywhere; the outcome is genuinely unknown.
        const pullB = deferred<SyncResponse>()
        ;(api.syncPull as any).mockReturnValueOnce(pullB.promise)
        await rowButton(remoteRow(wrapper, 'other'), 'Pull').trigger('click')
        await flushPromises()
        const runB = (api.syncPull as any).mock.calls[1][0].client_run_id as string

        listHandler = async (params) => {
            if (params?.limit === 200) {
                return { total: 1, limit: 200, offset: 0, reports: [reportMeta(runA, 'jira-home', null)] }
            }
            return emptyList()
        }
        triggerReconnect()
        await flushPromises()
        await flushPromises()

        expect(remoteRow(wrapper, 'jira-home').text()).toContain('Success')
        expect(remoteRow(wrapper, 'other').text()).toContain('Unknown')
        // Unknown is not busy: buttons stay usable, and re-running warns about duplicates.
        expect(rowButton(remoteRow(wrapper, 'other'), 'Pull').attributes('disabled')).toBeUndefined()
        await rowButton(remoteRow(wrapper, 'other'), 'Pull').trigger('click')
        expect(toastMessages.some((m) => m.includes('unknown status'))).toBe(true)
        wrapper.unmount()
    })

    it('generates unique client run ids for rapid starts', async () => {
        configState.globalRemotes = { a: jiraRemote(), b: jiraRemote() }
        const pull = deferred<SyncResponse>()
        ;(api.syncPull as any).mockReturnValue(pull.promise)
        const wrapper = await mountSyncHub()

        await rowButton(remoteRow(wrapper, 'a'), 'Pull').trigger('click')
        await rowButton(remoteRow(wrapper, 'b'), 'Pull').trigger('click')
        await flushPromises()
        const ids = (api.syncPull as any).mock.calls.map((call: any[]) => call[0].client_run_id)
        expect(ids).toHaveLength(2)
        expect(ids[0]).not.toBe(ids[1])
        wrapper.unmount()
    })

    it('filters report items by the selected date range', async () => {
        const old = reportMeta('old-1', 'oldremote', null, { created_at: '2024-01-01T00:00:00.000Z' })
        const recent = reportMeta('new-1', 'newremote', null, { created_at: new Date().toISOString() })
        listHandler = async () => ({ total: 2, limit: 20, offset: 0, reports: [old, recent] })
        const wrapper = await mountSyncHub()
        expect(wrapper.text()).toContain('oldremote')
        expect(wrapper.text()).toContain('newremote')

        const start = wrapper.find('input[type="datetime-local"]')
        await start.setValue('2026-01-01T00:00')
        expect(wrapper.text()).not.toContain('oldremote')
        expect(wrapper.text()).toContain('newremote')
        wrapper.unmount()
    })
})

describe('SyncHub remote editor', () => {
    it('requires an explicit target for inherited remotes and honors override-in-project', async () => {
        configState.globalRemotes = { g1: jiraRemote('DEMO') }
        configState.projectRemotes = { ALPHA: { p1: jiraRemote('ALPHA') } }
        const wrapper = await mountSyncHub('/sync?project=ALPHA')

        const row = remoteRow(wrapper, 'g1')
        await rowButton(row, 'Edit').trigger('click')
        await flushPromises()
        expect(wrapper.find('[data-testid="remote-dialog-choice"]').exists()).toBe(true)
        expect(wrapper.find('[data-testid="remote-dialog-target"]').text()).toContain('Inherited from Global')

        await dialogButton(wrapper, 'Override in project ALPHA').trigger('click')
        await flushPromises()
        expect(wrapper.find('[data-testid="remote-dialog-choice"]').exists()).toBe(false)
        expect(wrapper.find('[data-testid="remote-dialog-target"]').text()).toContain('Project ALPHA')

        await submitDialog(wrapper)
        expect(api.setConfig).toHaveBeenCalledTimes(1)
        const payload = (api.setConfig as any).mock.calls[0][0]
        expect(payload.project).toBe('ALPHA')
        expect(payload.global).toBeUndefined()
        // The project map keeps the project-owned remote and gains the override.
        const yaml = payload.values.remotes as string
        expect(yaml).toContain('p1')
        expect(yaml).toContain('g1')
        wrapper.unmount()
    })

    it('edit-in-global writes the global map from the project view', async () => {
        configState.globalRemotes = { g1: jiraRemote('DEMO') }
        configState.projectRemotes = { ALPHA: {} }
        const wrapper = await mountSyncHub('/sync?project=ALPHA')

        await rowButton(remoteRow(wrapper, 'g1'), 'Edit').trigger('click')
        await flushPromises()
        await dialogButton(wrapper, 'Edit in Global scope').trigger('click')
        await flushPromises()
        await submitDialog(wrapper)

        expect(api.setConfig).toHaveBeenCalledTimes(1)
        const payload = (api.setConfig as any).mock.calls[0][0]
        expect(payload.global).toBe(true)
        expect(payload.project).toBeUndefined()
        wrapper.unmount()
    })

    it('keeps the captured submit target across a project switch', async () => {
        configState.globalRemotes = { g1: jiraRemote('DEMO') }
        const wrapper = await mountSyncHub()

        await rowButton(remoteRow(wrapper, 'g1'), 'Edit').trigger('click')
        await flushPromises()
        expect(wrapper.find('[data-testid="remote-dialog-target"]').text()).toContain('Global')

        await switchScope(wrapper, 'ALPHA')
        await flushPromises()
        // Dialog stays open with the captured global target; no dynamic project read at save.
        expect(wrapper.find('[data-testid="remote-dialog-target"]').text()).toContain('Global')
        await submitDialog(wrapper)

        expect(api.setConfig).toHaveBeenCalledTimes(1)
        const payload = (api.setConfig as any).mock.calls[0][0]
        expect(payload.global).toBe(true)
        expect(payload.project).toBeUndefined()
        wrapper.unmount()
    })

    it('merges into fresh server state and preserves concurrent unrelated remotes', async () => {
        configState.globalRemotes = { g1: jiraRemote('DEMO') }
        const wrapper = await mountSyncHub()

        await rowButton(remoteRow(wrapper, 'g1'), 'Edit').trigger('click')
        await flushPromises()
        await wrapper.find('input[placeholder="Optional filter"]').setValue('status != Done')
        // A concurrent client added an unrelated remote after this dialog opened.
        configState.globalRemotes = { g1: jiraRemote('DEMO'), concurrent: jiraRemote('OTHER') }

        await submitDialog(wrapper)
        expect(api.setConfig).toHaveBeenCalledTimes(1)
        const payload = (api.setConfig as any).mock.calls[0][0]
        const yaml = payload.values.remotes as string
        expect(yaml).toContain('concurrent')
        expect(yaml).toContain('status != Done')
        wrapper.unmount()
    })

    it('detects concurrent same-remote edits and refuses to overwrite them', async () => {
        configState.globalRemotes = { g1: jiraRemote('DEMO') }
        const wrapper = await mountSyncHub()

        await rowButton(remoteRow(wrapper, 'g1'), 'Edit').trigger('click')
        await flushPromises()
        configState.globalRemotes = { g1: jiraRemote('CHANGED') }

        await submitDialog(wrapper)
        expect(api.setConfig).not.toHaveBeenCalled()
        expect(wrapper.find('[data-testid="remote-dialog-error"]').text()).toContain('changed in Global config')
        wrapper.unmount()
    })

    it('rejects renames and additions that collide with existing remote names', async () => {
        configState.globalRemotes = { g1: jiraRemote('DEMO'), taken: jiraRemote('OTHER') }
        const wrapper = await mountSyncHub()

        await rowButton(remoteRow(wrapper, 'g1'), 'Edit').trigger('click')
        await flushPromises()
        await wrapper.find('input[placeholder="jira-home"]').setValue('taken')
        await submitDialog(wrapper)
        expect(api.setConfig).not.toHaveBeenCalled()
        expect(wrapper.find('[data-testid="remote-dialog-error"]').text()).toContain("already exists in Global config")
        wrapper.unmount()
    })

    it('surfaces YAML mapping errors without calling the API', async () => {
        configState.globalRemotes = { g1: jiraRemote('DEMO') }
        const wrapper = await mountSyncHub()

        await rowButton(remoteRow(wrapper, 'g1'), 'Edit').trigger('click')
        await flushPromises()
        const textarea = wrapper.find('textarea')
        await textarea.setValue('title: [broken')
        await submitDialog(wrapper)
        expect(api.setConfig).not.toHaveBeenCalled()
        expect(wrapper.find('[data-testid="remote-dialog-error"]').text()).toContain('Mapping must be valid YAML')
        wrapper.unmount()
    })

    it('validates against the captured target scope and drops late validation results', async () => {
        configState.globalRemotes = { g1: jiraRemote('DEMO') }
        const validate = deferred<any>()
        ;(api.syncValidate as any).mockReturnValue(validate.promise)
        const wrapper = await mountSyncHub()

        await rowButton(remoteRow(wrapper, 'g1'), 'Edit').trigger('click')
        await flushPromises()
        await dialogButton(wrapper, 'Validate').trigger('click')
        await flushPromises()
        expect(api.syncValidate).toHaveBeenCalledWith(expect.objectContaining({ project: undefined }))

        // Close and reopen (add mode) before the late response resolves.
        await dialogButton(wrapper, 'Cancel').trigger('click')
        await dialogButton(wrapper, 'Add remote').trigger('click')
        await flushPromises()
        validate.resolve({ status: 'ok', provider: 'jira', remote: 'g1', checked_at: new Date().toISOString(), warnings: [] })
        await flushPromises()
        expect(wrapper.text()).not.toContain('Validated')
        wrapper.unmount()
    })

    it('deletes from the captured target and reports concurrent removal honestly', async () => {
        configState.projectRemotes = { ALPHA: { p1: jiraRemote('ALPHA') } }
        const wrapper = await mountSyncHub('/sync?project=ALPHA')

        await rowButton(remoteRow(wrapper, 'p1'), 'Edit').trigger('click')
        await flushPromises()
        await dialogButton(wrapper, 'Remove from Project ALPHA').trigger('click')
        await flushPromises()
        expect(api.setConfig).toHaveBeenCalledTimes(1)
        const payload = (api.setConfig as any).mock.calls[0][0]
        expect(payload.project).toBe('ALPHA')
        expect(payload.values.remotes).toBe('')

        // Concurrent removal: fresh state no longer has the remote.
        await rowButton(remoteRow(wrapper, 'p1'), 'Edit').trigger('click')
        await flushPromises()
        configState.projectRemotes = { ALPHA: {} }
        await dialogButton(wrapper, 'Remove from Project ALPHA').trigger('click')
        await flushPromises()
        expect(api.setConfig).toHaveBeenCalledTimes(1)
        expect(wrapper.find('[data-testid="remote-dialog-error"]').text()).toContain('was already removed')
        wrapper.unmount()
    })
})

describe('SyncHub scope validity (B1)', () => {
    it('hides stale rows during a pending scope switch and shows them once the new scope loads', async () => {
        configState.globalRemotes = { gr: jiraRemote() }
        configState.projectRemotes = { ALPHA: { ar: jiraRemote() } }
        const wrapper = await mountSyncHub()
        expect(wrapper.findAll('.remote-row').length).toBeGreaterThan(0)

        const inspectQueue: Array<ReturnType<typeof deferred<any>>> = []
        ;(api.inspectConfig as any).mockImplementation(() => {
            const d = deferred<any>()
            inspectQueue.push(d)
            return d.promise
        })
        await wrapper.find('#sync-scope').setValue('ALPHA')
        await flushPromises()

        // Old (global) rows must not display as clickable ALPHA rows.
        expect(wrapper.text()).not.toContain('gr')
        expect(wrapper.find('[data-testid="scope-unavailable"]').exists()).toBe(true)
        expect(wrapper.text()).toContain('Loading sync settings for this scope')
        const addButton = wrapper.findAll('button').find((b) => b.text() === 'Add remote')!
        expect(addButton.attributes('disabled')).toBeDefined()
        expect(api.syncPull).not.toHaveBeenCalled()

        inspectQueue.forEach((d) => d.resolve(buildInspect('ALPHA')))
        await flushPromises()
        expect(wrapper.text()).toContain('ar')
        expect(wrapper.find('[data-testid="scope-unavailable"]').exists()).toBe(false)
        wrapper.unmount()
    })

    it('keeps rows hidden after a failed scope switch until a valid load restores them', async () => {
        configState.globalRemotes = { gr: jiraRemote() }
        const wrapper = await mountSyncHub()
        expect(wrapper.text()).toContain('gr')

        const inspectQueue: Array<ReturnType<typeof deferred<any>>> = []
        ;(api.inspectConfig as any).mockImplementation(() => {
            const d = deferred<any>()
            inspectQueue.push(d)
            return d.promise
        })
        await wrapper.find('#sync-scope').setValue('ALPHA')
        await flushPromises()
        inspectQueue.forEach((d) => d.reject(new Error('config unreachable')))
        await flushPromises()

        expect(wrapper.text()).not.toContain('gr')
        expect(wrapper.find('[data-testid="scope-unavailable"]').text()).toContain('unavailable')
        expect(wrapper.find('[data-testid="scope-load-error"]').text()).toContain('config unreachable')
        expect(api.syncPull).not.toHaveBeenCalled()

        await wrapper.find('#sync-scope').setValue('')
        await flushPromises()
        inspectQueue.forEach((d) => d.resolve(buildInspect('')))
        await flushPromises()
        expect(wrapper.text()).toContain('gr')
        wrapper.unmount()
    })

    it('never re-stamps an older scope from a late inspect response (A-B-A)', async () => {
        configState.globalRemotes = { gr: jiraRemote() }
        configState.projectRemotes = { ALPHA: { alpharemote: jiraRemote() } }
        const wrapper = await mountSyncHub()
        expect(wrapper.text()).toContain('gr')

        const inspectQueue: Array<ReturnType<typeof deferred<any>>> = []
        ;(api.inspectConfig as any).mockImplementation(() => {
            const d = deferred<any>()
            inspectQueue.push(d)
            return d.promise
        })
        await wrapper.find('#sync-scope').setValue('ALPHA')
        await wrapper.find('#sync-scope').setValue('')
        await flushPromises()

        // Resolve the CURRENT (global) loads first, then the stale ALPHA loads.
        inspectQueue[2]!.resolve(buildInspect(''))
        inspectQueue[3]!.resolve(buildInspect(''))
        await flushPromises()
        expect(wrapper.text()).toContain('gr')

        inspectQueue[0]!.resolve(buildInspect('ALPHA'))
        inspectQueue[1]!.resolve(buildInspect('ALPHA'))
        await flushPromises()
        // Late ALPHA data must not leak into the validated global view.
        expect(wrapper.text()).toContain('gr')
        expect(wrapper.text()).not.toContain('alpharemote')

        const pull = deferred<SyncResponse>()
        ;(api.syncPull as any).mockReturnValue(pull.promise)
        await rowButton(remoteRow(wrapper, 'gr'), 'Pull').trigger('click')
        await flushPromises()
        expect((api.syncPull as any).mock.calls[0][0].project).toBeUndefined()
        wrapper.unmount()
    })

    it('keeps valid rows with an error banner when a same-scope reload fails', async () => {
        configState.globalRemotes = { gr: jiraRemote() }
        const wrapper = await mountSyncHub()
        expect(wrapper.text()).toContain('gr')

        const failed = deferred<any>()
        ;(api.inspectConfig as any).mockReturnValue(failed.promise)
        const reloadButton = wrapper.findAll('button').find((b) => b.text() === 'Reload' && b.element.closest('.page-header') !== null)!
        await reloadButton.trigger('click')
        await flushPromises()
        failed.reject(new Error('transient failure'))
        await flushPromises()

        expect(wrapper.text()).toContain('gr')
        expect(wrapper.find('[data-testid="scope-load-error"]').text()).toContain('transient failure')
        wrapper.unmount()
    })
})

describe('SyncHub default-project preflight (B2)', () => {
    it('blocks global runs when the default project overrides the remote definition', async () => {
        configState.defaultProject = 'ALPHA'
        configState.globalRemotes = { shared: jiraRemote('DEMO') }
        configState.projectRemotes = { ALPHA: { shared: jiraRemote('ALPHA') } }
        const wrapper = await mountSyncHub()

        await rowButton(remoteRow(wrapper, 'shared'), 'Pull').trigger('click')
        await flushPromises()

        expect(api.syncPull).not.toHaveBeenCalled()
        expect(remoteRow(wrapper, 'shared').text()).not.toContain('Running')
        expect(toastMessages.some((m) => m.includes('overridden in default project ALPHA') && m.includes('Switch to ALPHA'))).toBe(true)
        wrapper.unmount()
    })

    it('runs global pull and push with an execution-project hint when definitions match', async () => {
        configState.defaultProject = 'ALPHA'
        configState.globalRemotes = { shared: jiraRemote('ALPHA') }
        configState.projectRemotes = { ALPHA: { shared: jiraRemote('ALPHA') } }
        const pull = deferred<SyncResponse>()
        ;(api.syncPull as any).mockReturnValue(pull.promise)
        const wrapper = await mountSyncHub()

        await rowButton(remoteRow(wrapper, 'shared'), 'Pull').trigger('click')
        await flushPromises()
        expect(api.syncPull).toHaveBeenCalledTimes(1)
        expect(toastMessages.some((m) => m.includes('runs through default project ALPHA'))).toBe(true)
        pull.resolve(syncResponse('run-1', 'shared', 'ALPHA'))
        await flushPromises()

        const push = deferred<SyncResponse>()
        ;(api.syncPush as any).mockReturnValue(push.promise)
        await rowButton(remoteRow(wrapper, 'shared'), 'Push').trigger('click')
        await flushPromises()
        expect(api.syncPush).toHaveBeenCalledTimes(1)
        push.resolve(syncResponse('run-2', 'shared', 'ALPHA'))
        await flushPromises()
        wrapper.unmount()
    })

    it('fails closed when the default-project verification cannot complete', async () => {
        configState.defaultProject = 'ALPHA'
        configState.globalRemotes = { shared: jiraRemote('ALPHA') }
        ;(api.inspectConfig as any).mockImplementation(async (scopeArg?: string) => {
            if (scopeArg === 'ALPHA') throw new Error('inspect exploded')
            return buildInspect(scopeArg)
        })
        const wrapper = await mountSyncHub()

        await rowButton(remoteRow(wrapper, 'shared'), 'Pull').trigger('click')
        await flushPromises()
        expect(api.syncPull).not.toHaveBeenCalled()
        expect(toastMessages.some((m) => m.includes('Cannot verify remote') && m.includes('blocked'))).toBe(true)
        wrapper.unmount()
    })

    it('aborts a pending preflight when the scope navigates away before verification finishes', async () => {
        configState.defaultProject = 'ALPHA'
        configState.globalRemotes = { shared: jiraRemote('ALPHA') }
        configState.projectRemotes = { ALPHA: { shared: jiraRemote('ALPHA') } }
        const alphaDeferreds: Array<ReturnType<typeof deferred<any>>> = []
        ;(api.inspectConfig as any).mockImplementation((scopeArg?: string) => {
            if (scopeArg === 'ALPHA') {
                const d = deferred<any>()
                alphaDeferreds.push(d)
                return d.promise
            }
            return Promise.resolve(buildInspect(scopeArg))
        })
        const wrapper = await mountSyncHub()

        await rowButton(remoteRow(wrapper, 'shared'), 'Pull').trigger('click')
        await flushPromises()
        expect(alphaDeferreds.length).toBe(1)

        await switchScope(wrapper, 'ALPHA')
        // The scope loads (further ALPHA inspects) settle; the preflight resolves last.
        alphaDeferreds.slice(1).forEach((d) => d.resolve(buildInspect('ALPHA')))
        await flushPromises()
        alphaDeferreds[0]!.resolve(buildInspect('ALPHA'))
        await flushPromises()

        expect(api.syncPull).not.toHaveBeenCalled()
        expect(wrapper.text()).not.toContain('Running')
        wrapper.unmount()
    })

    it('preflight busy prevents double starts while verification is pending', async () => {
        configState.defaultProject = 'ALPHA'
        configState.globalRemotes = { shared: jiraRemote('ALPHA') }
        const alphaDeferreds: Array<ReturnType<typeof deferred<any>>> = []
        ;(api.inspectConfig as any).mockImplementation((scopeArg?: string) => {
            if (scopeArg === 'ALPHA') {
                const d = deferred<any>()
                alphaDeferreds.push(d)
                return d.promise
            }
            return Promise.resolve(buildInspect(scopeArg))
        })
        const wrapper = await mountSyncHub()

        await rowButton(remoteRow(wrapper, 'shared'), 'Pull').trigger('click')
        await rowButton(remoteRow(wrapper, 'shared'), 'Pull').trigger('click')
        await flushPromises()
        expect(alphaDeferreds.length).toBe(1)
        expect(api.syncPull).not.toHaveBeenCalled()

        alphaDeferreds[0]!.resolve(buildInspect('ALPHA'))
        await flushPromises()
        expect(api.syncPull).toHaveBeenCalledTimes(1)
        wrapper.unmount()
    })
})

describe('SyncHub external runs and field attribution (B3)', () => {
    it('keeps external progress entries through completion with correct root attribution', async () => {
        configState.globalRemotes = { shared: jiraRemote() }
        const wrapper = await mountSyncHub()

        // A stored-path run proves root attribution via the fetch candidates.
        emitSse('sync_started', {
            run_id: 'ext-7',
            remote: 'shared',
            direction: 'pull',
            project: 'ALPHA',
            dry_run: false,
            started_at: new Date().toISOString(),
        })
        emitSse('sync_completed', {
            run_id: 'ext-7',
            report: reportMeta('ext-7', 'shared', 'ALPHA', { stored_path: 'ext-7.yml' }),
            finished_at: new Date().toISOString(),
        })
        await flushPromises()

        const storedItem = wrapper.findAll('.report-item').find((el) => el.text().includes('shared'))!
        await storedItem.trigger('click')
        await flushPromises()
        expect(api.syncReportGet).toHaveBeenCalledWith('ext-7.yml', 'ALPHA')

        // A pathless run keeps its accumulated progress entries after completion.
        emitSse('sync_started', {
            run_id: 'ext-8',
            remote: 'shared',
            direction: 'pull',
            project: 'ALPHA',
            dry_run: false,
            started_at: new Date().toISOString(),
        })
        emitSse('sync_progress', {
            run_id: 'ext-8',
            direction: 'pull',
            remote: 'shared',
            project: 'ALPHA',
            entry: { ...reportEntry('ALPHA-2'), status: 'updated' },
            summary: { created: 0, updated: 1, skipped: 0, failed: 0 },
        })
        await flushPromises()
        emitSse('sync_completed', {
            run_id: 'ext-8',
            report: reportMeta('ext-8', 'shared', 'ALPHA', { stored_path: null }),
            finished_at: new Date().toISOString(),
        })
        await flushPromises()

        const liveItem = wrapper.findAll('.report-item').find((el) => el.text().includes('shared') && el.text().includes('External'))!
        await liveItem.trigger('click')
        await flushPromises()
        // Entries accumulated before completion remain inspectable.
        expect(wrapper.text()).toContain('ALPHA-2')
        expect(api.syncReportGet).not.toHaveBeenCalledWith('ext-8.yml', expect.anything())
        wrapper.unmount()
    })

    it('hides Fields synced when the remote definition cannot be attributed to the report scope', async () => {
        configState.globalRemotes = { shared: { ...jiraRemote(), mapping: { title: 'summary' } } }
        const crossProject = reportMeta('agg-1', 'shared', 'ALPHA')
        listHandler = async () => ({ total: 1, limit: 20, offset: 0, reports: [crossProject] })
        ;(api.syncReportGet as any).mockResolvedValue({ ...crossProject, entries: [] })
        const wrapper = await mountSyncHub()

        const item = wrapper.findAll('.report-item').find((el) => el.text().includes('shared'))!
        await item.trigger('click')
        await flushPromises()
        expect(wrapper.text()).not.toContain('Fields synced')
        wrapper.unmount()

        // Same-scope project reports keep their field chips.
        configState.projectRemotes = { ALPHA: { p1: { ...jiraRemote(), mapping: { title: 'summary' } } } }
        const sameScope = reportMeta('agg-2', 'p1', 'ALPHA')
        listHandler = async () => ({ total: 1, limit: 20, offset: 0, reports: [sameScope] })
        ;(api.syncReportGet as any).mockResolvedValue({ ...sameScope, entries: [] })
        const wrapper2 = await mountSyncHub('/sync?project=ALPHA')
        const item2 = wrapper2.findAll('.report-item').find((el) => el.text().includes('p1'))!
        await item2.trigger('click')
        await flushPromises()
        expect(wrapper2.text()).toContain('Fields synced')
        wrapper2.unmount()
    })
})

describe('SyncHub teardown', () => {
    it('drops late async writes after unmount', async () => {
        configState.globalRemotes = { 'jira-home': jiraRemote() }
        const pull = deferred<SyncResponse>()
        ;(api.syncPull as any).mockReturnValue(pull.promise)
        const wrapper = await mountSyncHub()

        await rowButton(remoteRow(wrapper, 'jira-home'), 'Pull').trigger('click')
        await rowButton(remoteRow(wrapper, 'jira-home'), 'Edit').trigger('click')
        await flushPromises()
        wrapper.unmount()

        pull.resolve(syncResponse('late-run', 'jira-home', null))
        await flushPromises()
        expect(sseCalls[0]!.close).toHaveBeenCalled()
    })
})
