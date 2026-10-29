import type {
    AutomationRunRequest,
    AutomationRunResponse,
    TaskMoveRequest,
    TaskMoveResponse,
    ActivityFeedItem,
    AgentJobCancelAllResponse,
    AgentJobCancelResponse,
    AgentJobCreateRequest,
    AgentJobCreateResponse,
    AgentJobListResponse,
    AgentJobLogsResponse,
    AgentJobMessageRequest,
    AgentJobMessageResponse,
    AgentJobStatusResponse,
    AgentProfileCreateRequest,
    AgentProfilesResponse,
    ApiEnvelope,
    AttachmentRemoveRequest,
    AttachmentRemoveResponse,
    AttachmentUploadRequest,
    AttachmentUploadResponse,
    AttachmentReorderRequest,
    AttachmentInfoRequest,
    AttachmentInfoResponse,
    BrandingResponse,
    BrandingUploadRequest,
    BrandingUploadResponse,
    AutomationInspectResponse,
    AutomationSetRequest,
    AutomationSetResponse,
    AutomationSimulateRequest,
    AutomationSimulateResponse,
    CodeReferenceAddRequest,
    CodeReferenceAddResponse,
    CodeReferenceRemoveRequest,
    CodeReferenceRemoveResponse,
    ConfigInspectResult,
    ConfigSetResponse,
    FileReferenceAddRequest,
    FileReferenceAddResponse,
    FileReferenceRemoveRequest,
    FileReferenceRemoveResponse,
    GenericReferenceAddRequest,
    GenericReferenceAddResponse,
    GenericReferenceRemoveRequest,
    GenericReferenceRemoveResponse,
    LinkReferenceAddRequest,
    LinkReferenceAddResponse,
    LinkReferenceRemoveRequest,
    LinkReferenceRemoveResponse,
    ProjectCreateRequest,
    ProjectDTO,
    ProjectListResponse,
    ProjectStatsDTO,
    ReferenceSnippet,
    RelationshipGraphRequest,
    RelationshipGraphResponse,
    RelationshipCheckResponse,
    RelationshipMutationRequest,
    RelationshipMutationResponse,
    RelationshipRepairRequest,
    RelationshipRepairResponse,
    ScanRequest,
    ScanResponse,
    SprintAssignmentRequest,
    SprintAssignmentResponse,
    SprintBurndownResponse,
    SprintCreateRequest,
    SprintCreateResponse,
    SprintDeleteRequest,
    SprintDeleteResponse,
    SprintListResponse,
    SprintSummaryReportResponse,
    SprintUpdateRequest,
    SprintUpdateResponse,
    SprintVelocityResponse,
    SyncReport,
    SyncReportListResponse,
    SyncRequest,
    SyncResponse,
    SyncValidateRequest,
    SyncValidateResponse,
    TaskCreate,
    TaskDTO,
    TaskOrderRequest,
    TaskOrderResponse,
    TaskDeleteResponse,
    TaskListFilter,
    TaskListResponse,
    TaskUpdate,
} from './types'

const BASE = '' // same origin; server serves /api

function qs(params: Record<string, any> = {}): string {
    const usp = new URLSearchParams()

    const append = (key: string, value: unknown) => {
        if (value === undefined || value === null) return
        if (Array.isArray(value)) {
            if (value.length) usp.set(key, value.join(','))
            return
        }
        usp.set(key, String(value))
    }

    Object.entries(params).forEach(([k, v]) => {
        if (k === 'custom_fields' && v && typeof v === 'object' && !Array.isArray(v)) {
            Object.entries(v as Record<string, unknown>).forEach(([name, value]) => {
                if (value === undefined || value === null) return
                const key = name.startsWith('field:') ? name : `field:${name}`
                append(key, value)
            })
            return
        }
        append(k, v)
    })

    const s = usp.toString()
    return s ? `?${s}` : ''
}

async function parseEnvelope<T>(method: 'GET' | 'POST', path: string, res: Response): Promise<T> {
    const raw = await res.text()
    let payload: ApiEnvelope<T> | { error?: { message?: string }; data?: T } | null = null
    if (raw) {
        try {
            payload = JSON.parse(raw)
        } catch {
            payload = null
        }
    }

    if (!res.ok) {
        const message = (payload as any)?.error?.message || (payload as any)?.message || raw?.trim() || `${res.status}`
        // Carry the HTTP status so callers can distinguish not-found from
        // transient failures without parsing messages.
        const error = new Error(`${method} ${path} failed: ${message}`) as Error & { status?: number }
        error.status = res.status
        throw error
    }

    if (!payload) {
        throw new Error(`${method} ${path} failed: Empty response`)
    }

    if ((payload as any).error) {
        throw new Error((payload as any).error.message || `${method} ${path} failed`)
    }

    if (!('data' in (payload as ApiEnvelope<T>))) {
        return payload as any as T
    }

    return (payload as ApiEnvelope<T>).data
}

async function get<T>(path: string, params?: Record<string, any>): Promise<T> {
    const res = await fetch(`${BASE}${path}${qs(params)}`, { headers: { Accept: 'application/json' } })
    return parseEnvelope('GET', path, res)
}

async function post<T>(path: string, body: any): Promise<T> {
    const res = await fetch(`${BASE}${path}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
        body: JSON.stringify(body),
    })
    return parseEnvelope('POST', path, res)
}

export const api = {
    relationshipGraph(params: RelationshipGraphRequest = {}): Promise<RelationshipGraphResponse> {
        const { relationship_kinds, ...query } = params
        return get('/api/relationships/graph', {
            ...query,
            kinds: relationship_kinds?.join(','),
            filters: params.filters === undefined ? undefined : JSON.stringify(params.filters),
        })
    },
    relationshipCheck(project?: string): Promise<RelationshipCheckResponse> {
        return get('/api/relationships/check', { project })
    },
    relationshipMutate(payload: RelationshipMutationRequest): Promise<RelationshipMutationResponse> {
        return post('/api/relationships/mutate', payload)
    },
    relationshipRepair(payload: RelationshipRepairRequest): Promise<RelationshipRepairResponse> {
        return post('/api/relationships/repair', payload)
    },
    // Tasks
    listTasks(filter: TaskListFilter = {}): Promise<TaskListResponse> {
        return get('/api/tasks/list', filter as any)
    },
    getTask(id: string, project?: string, opts?: { includeDeleted?: boolean }): Promise<TaskDTO> {
        return get('/api/tasks/get', {
            id,
            project,
            include_deleted: opts?.includeDeleted === true ? 'true' : undefined,
        })
    },
    addTask(payload: TaskCreate): Promise<TaskDTO> {
        return post('/api/tasks/add', payload)
    },
    updateTask(id: string, patch: TaskUpdate): Promise<TaskDTO> {
        return post('/api/tasks/update', { id, ...patch })
    },
    addComment(id: string, text: string): Promise<TaskDTO> {
        return post('/api/tasks/comment', { id, text })
    },
    updateComment(id: string, index: number, text: string): Promise<TaskDTO> {
        return post('/api/tasks/comment/update', { id, index, text })
    },
    setStatus(id: string, status: string): Promise<TaskDTO> {
        return post('/api/tasks/status', { id, status })
    },
    /**
     * DEV-92: deletion is soft by default (recoverable trash tombstone);
     * `hard: true` physically removes the task. The response reports what the
     * server retained (attachment blobs, incoming relationships) as warnings.
     */
    deleteTask(id: string, opts?: { project?: string; hard?: boolean }): Promise<TaskDeleteResponse> {
        return post('/api/tasks/delete' + qs({ project: opts?.project }), { id, ...(opts?.hard ? { hard: true } : {}) })
    },
    /** DEV-92: restore a soft-deleted task; returns the active TaskDTO. */
    restoreTask(id: string, project?: string): Promise<TaskDTO> {
        return post('/api/tasks/restore' + qs({ project }), { id })
    },
    /** DEV-4: move a task into its project's @archive folder (read-only there). */
    archiveTask(id: string, project?: string): Promise<TaskDTO> {
        return post('/api/tasks/archive' + qs({ project }), { id })
    },
    /** DEV-4: bring an archived task back. */
    unarchiveTask(id: string, project?: string): Promise<TaskDTO> {
        return post('/api/tasks/unarchive' + qs({ project }), { id })
    },
    /** DEV-183: move a task to another project (`dry_run` previews). */
    moveTask(request: TaskMoveRequest): Promise<TaskMoveResponse> {
        return post('/api/tasks/move', request)
    },
    /**
     * Upload an attachment. With `onProgress`, the request goes through
     * XMLHttpRequest so the panel can show per-file progress (fetch has no
     * upload progress events).
     */
    uploadTaskAttachment(
        payload: AttachmentUploadRequest,
        onProgress?: (fraction: number) => void,
    ): Promise<AttachmentUploadResponse> {
        const path = '/api/tasks/attachments/upload'
        if (!onProgress || typeof XMLHttpRequest === 'undefined') return post(path, payload)
        return new Promise((resolve, reject) => {
            const xhr = new XMLHttpRequest()
            xhr.open('POST', `${BASE}${path}`)
            xhr.setRequestHeader('Content-Type', 'application/json')
            xhr.setRequestHeader('Accept', 'application/json')
            xhr.upload.onprogress = (event) => {
                if (event.lengthComputable && event.total > 0) onProgress(event.loaded / event.total)
            }
            xhr.onload = () => {
                onProgress(1)
                parseEnvelope<AttachmentUploadResponse>(
                    'POST',
                    path,
                    new Response(xhr.responseText, { status: xhr.status }),
                ).then(resolve, reject)
            }
            xhr.onerror = () => reject(new Error(`POST ${path} failed: network error`))
            xhr.send(JSON.stringify(payload))
        })
    },
    branding(): Promise<BrandingResponse> {
        return get('/api/branding')
    },
    uploadBranding(payload: BrandingUploadRequest): Promise<BrandingUploadResponse> {
        return post('/api/branding/upload', payload)
    },
    reorderTaskAttachments(payload: AttachmentReorderRequest): Promise<TaskDTO> {
        return post('/api/tasks/attachments/reorder', payload)
    },
    attachmentInfo(payload: AttachmentInfoRequest): Promise<AttachmentInfoResponse> {
        return post('/api/attachments/info', payload)
    },
    removeTaskAttachment(payload: AttachmentRemoveRequest): Promise<AttachmentRemoveResponse> {
        return post('/api/tasks/attachments/remove', payload)
    },
    addTaskLinkReference(payload: LinkReferenceAddRequest): Promise<LinkReferenceAddResponse> {
        return post('/api/tasks/references/link/add', payload)
    },
    removeTaskLinkReference(payload: LinkReferenceRemoveRequest): Promise<LinkReferenceRemoveResponse> {
        return post('/api/tasks/references/link/remove', payload)
    },
    addTaskCodeReference(payload: CodeReferenceAddRequest): Promise<CodeReferenceAddResponse> {
        return post('/api/tasks/references/code/add', payload)
    },
    removeTaskCodeReference(payload: CodeReferenceRemoveRequest): Promise<CodeReferenceRemoveResponse> {
        return post('/api/tasks/references/code/remove', payload)
    },
    addTaskFileReference(payload: FileReferenceAddRequest): Promise<FileReferenceAddResponse> {
        return post('/api/tasks/references/file/add', payload)
    },
    removeTaskFileReference(payload: FileReferenceRemoveRequest): Promise<FileReferenceRemoveResponse> {
        return post('/api/tasks/references/file/remove', payload)
    },
    addTaskReference(payload: GenericReferenceAddRequest): Promise<GenericReferenceAddResponse> {
        return post('/api/tasks/references/add', payload)
    },
    removeTaskReference(payload: GenericReferenceRemoveRequest): Promise<GenericReferenceRemoveResponse> {
        return post('/api/tasks/references/remove', payload)
    },
    taskHistory(
        id: string,
        limit?: number,
    ): Promise<Array<{ commit: string; author: string; email: string; date: string; message: string }>> {
        return get('/api/tasks/history', { id, limit })
    },
    suggestTasks(q: string, project?: string, limit = 20): Promise<Array<{ id: string; title: string }>> {
        return get('/api/tasks/suggest', { q, project, limit })
    },
    suggestReferenceFiles(q: string, limit = 20): Promise<string[]> {
        return get('/api/references/files', { q, limit })
    },
    referenceSnippet(code: string, context?: number | { before?: number; after?: number }): Promise<ReferenceSnippet> {
        const params: Record<string, unknown> = { code }
        if (typeof context === 'number') {
            params.context = context
        } else if (context) {
            if (typeof context.before === 'number') params.before = context.before
            if (typeof context.after === 'number') params.after = context.after
        }
        return get('/api/references/snippet', params)
    },

    // Projects
    listProjects(params: { limit?: number; offset?: number } = {}): Promise<ProjectListResponse> {
        return get('/api/projects/list', params as any)
    },
    createProject(payload: ProjectCreateRequest): Promise<ProjectDTO> {
        return post('/api/projects/create', payload)
    },
    projectStats(project: string): Promise<ProjectStatsDTO> {
        return get('/api/projects/stats', { project })
    },

    // Config
    showConfig(project?: string): Promise<any> {
        return get('/api/config/show', { project })
    },
    inspectConfig(project?: string): Promise<ConfigInspectResult> {
        return get('/api/config/inspect', { project })
    },
    setConfig(payload: {
        values: Record<string, string>
        project?: string
        global?: boolean
    }): Promise<ConfigSetResponse> {
        const body: Record<string, unknown> = { values: payload.values }
        if (payload.project) body.project = payload.project
        if (payload.global) body.global = true
        return post('/api/config/set', body)
    },

    // Sync
    syncPull(payload: SyncRequest): Promise<SyncResponse> {
        return post('/api/sync/pull', payload)
    },
    syncPush(payload: SyncRequest): Promise<SyncResponse> {
        return post('/api/sync/push', payload)
    },
    syncValidate(payload: SyncValidateRequest): Promise<SyncValidateResponse> {
        return post('/api/sync/validate', payload)
    },
    syncReportsList(
        params: { project?: string; limit?: number; offset?: number } = {},
    ): Promise<SyncReportListResponse> {
        return get('/api/sync/reports/list', params as any)
    },
    syncReportGet(path: string, project?: string): Promise<SyncReport> {
        return get('/api/sync/reports/get', { path, project })
    },

    // Scan
    scanRun(payload: ScanRequest): Promise<ScanResponse> {
        return post('/api/scan/run', payload)
    },

    // Agent jobs
    listAgentJobs(params: { status?: string; ticket_id?: string } = {}): Promise<AgentJobListResponse> {
        return get('/api/jobs', params as any)
    },
    getAgentJob(id: string): Promise<AgentJobStatusResponse> {
        return get('/api/jobs/get', { id })
    },
    runAgentJob(payload: AgentJobCreateRequest): Promise<AgentJobCreateResponse> {
        return post('/api/jobs', payload)
    },
    cancelAgentJob(id: string): Promise<AgentJobCancelResponse> {
        return post('/api/jobs/cancel', { id })
    },
    cancelAllAgentJobs(): Promise<AgentJobCancelAllResponse> {
        return post('/api/jobs/cancel-all', {})
    },

    // Automations
    inspectAutomation(project?: string): Promise<AutomationInspectResponse> {
        return get('/api/automation/show', project ? { project } : undefined)
    },
    setAutomation(payload: AutomationSetRequest): Promise<AutomationSetResponse> {
        return post('/api/automation/set', payload)
    },
    simulateAutomation(payload: AutomationSimulateRequest): Promise<AutomationSimulateResponse> {
        return post('/api/automation/simulate', payload)
    },
    /** DEV-241: run manual (cron-style) rules now, or preview them with `dry_run`. */
    runAutomation(payload: AutomationRunRequest): Promise<AutomationRunResponse> {
        return post('/api/automation/run', payload)
    },
    getAgentJobLogs(id: string): Promise<AgentJobLogsResponse> {
        return get('/api/jobs/logs', { id })
    },
    sendAgentJobMessage(payload: AgentJobMessageRequest): Promise<AgentJobMessageResponse> {
        return post('/api/jobs/message', payload)
    },

    // Agent profiles
    listAgentProfiles(project?: string): Promise<AgentProfilesResponse> {
        return get('/api/agents/profiles', project ? { project } : undefined)
    },
    addAgentProfile(payload: AgentProfileCreateRequest): Promise<ConfigSetResponse> {
        return post('/api/agents/profiles', payload)
    },

    // Activity
    activitySeries(
        group: 'author' | 'day' | 'week' | 'project',
        params: { since?: string; until?: string; project?: string } = {},
    ): Promise<Array<{ key: string; count: number; last_date: string }>> {
        return get('/api/activity/series', { group, ...params })
    },
    activityFeed(
        params: { since?: string; until?: string; project?: string; limit?: number } = {},
    ): Promise<ActivityFeedItem[]> {
        return get('/api/activity/feed', params)
    },
    whoami(): Promise<string> {
        return get('/api/whoami')
    },
    /**
     * CSV export of the CURRENT filtered set in the CURRENT global order.
     * Forwards every supported list filter verbatim (status, priority, type,
     * project, tags, q, assignee incl. `__none__`, sprints, custom fields,
     * unknown extra keys, due/recent/needs smart filters, order, sort_by) so the
     * export matches what the list view displays.
     */
    exportTasks(filter: TaskListFilter & { q?: string } = {} as any): Promise<Response> {
        const source = filter as Record<string, any>
        const params: Record<string, unknown> = { ...source }
        if (params.type === undefined && source.task_type !== undefined) params.type = source.task_type
        if (params.q === undefined && source.text_query !== undefined) params.q = source.text_query
        delete params.task_type
        delete params.text_query
        // DTO-only flag; the REST grammar spells it assignee=__none__.
        delete params.assignee_none
        return fetch(`/api/tasks/export${qs(params as Record<string, any>)}`, { headers: { Accept: 'text/csv' } })
    },

    // Sprints
    sprintList(): Promise<SprintListResponse> {
        return get('/api/sprints/list')
    },
    sprintCreate(payload: SprintCreateRequest): Promise<SprintCreateResponse> {
        return post('/api/sprints/create', payload)
    },
    sprintUpdate(payload: SprintUpdateRequest): Promise<SprintUpdateResponse> {
        return post('/api/sprints/update', payload)
    },
    sprintDelete(payload: SprintDeleteRequest): Promise<SprintDeleteResponse> {
        return post('/api/sprints/delete', payload)
    },
    sprintAdd(payload: SprintAssignmentRequest): Promise<SprintAssignmentResponse> {
        return post('/api/sprints/add', payload)
    },
    sprintRemove(payload: SprintAssignmentRequest): Promise<SprintAssignmentResponse> {
        return post('/api/sprints/remove', payload)
    },
    getBacklogOrder(): Promise<TaskOrderResponse> {
        return get('/api/sprints/backlog/order')
    },
    reorderTasks(payload: TaskOrderRequest): Promise<TaskOrderResponse> {
        return post('/api/sprints/order', payload)
    },
    sprintSummary(sprint: number): Promise<SprintSummaryReportResponse> {
        return get('/api/sprints/summary', { sprint })
    },
    sprintBurndown(sprint: number): Promise<SprintBurndownResponse> {
        return get('/api/sprints/burndown', { sprint })
    },
    sprintVelocity(
        params: { limit?: number; include_active?: boolean; metric?: 'tasks' | 'points' | 'hours' } = {},
    ): Promise<SprintVelocityResponse> {
        return get('/api/sprints/velocity', params)
    },
}

export type ApiClient = typeof api
