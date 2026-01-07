import type {
  ActivityFeedItem,
  ApiEnvelope,
  AttachmentRemoveRequest,
  AttachmentRemoveResponse,
  AttachmentUploadRequest,
  AttachmentUploadResponse,
  CodeReferenceAddRequest,
  CodeReferenceAddResponse,
  CodeReferenceRemoveRequest,
  CodeReferenceRemoveResponse,
  ConfigInspectResult,
  ConfigSetResponse,
  LinkReferenceAddRequest,
  LinkReferenceAddResponse,
  LinkReferenceRemoveRequest,
  LinkReferenceRemoveResponse,
  ProjectCreateRequest,
  ProjectDTO,
  ProjectStatsDTO,
  ReferenceSnippet,
  SprintAssignmentRequest,
  SprintAssignmentResponse,
  SprintBacklogResponse,
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
  TaskCreate,
  TaskDTO,
  TaskListFilter,
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
    const message =
      (payload as any)?.error?.message ||
      (payload as any)?.message ||
      raw?.trim() ||
      `${res.status}`
    throw new Error(`${method} ${path} failed: ${message}`)
  }

  if (!payload) {
    throw new Error(`${method} ${path} failed: Empty response`)
  }

  if ((payload as any).error) {
    throw new Error((payload as any).error.message || `${method} ${path} failed`)
  }

  if (!('data' in (payload as ApiEnvelope<T>))) {
    return (payload as any) as T
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
  // Tasks
  listTasks(filter: TaskListFilter = {}): Promise<TaskDTO[]> { return get('/api/tasks/list', filter as any) },
  getTask(id: string, project?: string): Promise<TaskDTO> { return get('/api/tasks/get', { id, project }) },
  addTask(payload: TaskCreate): Promise<TaskDTO> { return post('/api/tasks/add', payload) },
  updateTask(id: string, patch: TaskUpdate): Promise<TaskDTO> { return post('/api/tasks/update', { id, ...patch }) },
  addComment(id: string, text: string): Promise<TaskDTO> { return post('/api/tasks/comment', { id, text }) },
  updateComment(id: string, index: number, text: string): Promise<TaskDTO> { return post('/api/tasks/comment/update', { id, index, text }) },
  setStatus(id: string, status: string): Promise<TaskDTO> { return post('/api/tasks/status', { id, status }) },
  deleteTask(id: string, project?: string): Promise<{ deleted: boolean }> { return post('/api/tasks/delete' + qs({ project }), { id }) },
  uploadTaskAttachment(payload: AttachmentUploadRequest): Promise<AttachmentUploadResponse> { return post('/api/tasks/attachments/upload', payload) },
  removeTaskAttachment(payload: AttachmentRemoveRequest): Promise<AttachmentRemoveResponse> { return post('/api/tasks/attachments/remove', payload) },
  addTaskLinkReference(payload: LinkReferenceAddRequest): Promise<LinkReferenceAddResponse> { return post('/api/tasks/references/link/add', payload) },
  removeTaskLinkReference(payload: LinkReferenceRemoveRequest): Promise<LinkReferenceRemoveResponse> { return post('/api/tasks/references/link/remove', payload) },
  addTaskCodeReference(payload: CodeReferenceAddRequest): Promise<CodeReferenceAddResponse> { return post('/api/tasks/references/code/add', payload) },
  removeTaskCodeReference(payload: CodeReferenceRemoveRequest): Promise<CodeReferenceRemoveResponse> { return post('/api/tasks/references/code/remove', payload) },
  taskHistory(id: string, limit?: number): Promise<Array<{ commit: string; author: string; email: string; date: string; message: string }>> { return get('/api/tasks/history', { id, limit }) },
  taskCommitDiff(id: string, commit: string): Promise<string> { return get('/api/tasks/commit_diff', { id, commit }) },
  suggestTasks(q: string, project?: string, limit = 20): Promise<Array<{ id: string; title: string }>> { return get('/api/tasks/suggest', { q, project, limit }) },
  suggestReferenceFiles(q: string, limit = 20): Promise<string[]> { return get('/api/references/files', { q, limit }) },
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
  listProjects(): Promise<ProjectDTO[]> { return get('/api/projects/list') },
  createProject(payload: ProjectCreateRequest): Promise<ProjectDTO> { return post('/api/projects/create', payload) },
  projectStats(project: string): Promise<ProjectStatsDTO> { return get('/api/projects/stats', { project }) },

  // Config
  showConfig(project?: string): Promise<any> { return get('/api/config/show', { project }) },
  inspectConfig(project?: string): Promise<ConfigInspectResult> { return get('/api/config/inspect', { project }) },
  setConfig(payload: { values: Record<string, string>; project?: string; global?: boolean }): Promise<ConfigSetResponse> {
    const body: Record<string, unknown> = { values: payload.values }
    if (payload.project) body.project = payload.project
    if (payload.global) body.global = true
    return post('/api/config/set', body)
  },

  // Activity
  activitySeries(group: 'author' | 'day' | 'week' | 'project', params: { since?: string; until?: string; project?: string } = {}): Promise<Array<{ key: string; count: number; last_date: string }>> {
    return get('/api/activity/series', { group, ...params })
  },
  activityAuthors(params: { since?: string; until?: string; project?: string } = {}): Promise<Array<{ author: string; email: string; commits: number; last_date: string }>> {
    return get('/api/activity/authors', params)
  },
  activityChangedTasks(params: { since?: string; until?: string; author?: string; project?: string } = {}): Promise<Array<{ id: string; project: string; file: string; last_commit: string; last_author: string; last_date: string; commits: number }>> {
    return get('/api/activity/changed_tasks', params)
  },
  activityFeed(params: { since?: string; until?: string; project?: string; limit?: number } = {}): Promise<ActivityFeedItem[]> {
    return get('/api/activity/feed', params)
  },
  whoami(): Promise<string> { return get('/api/whoami') },
  exportTasks(filter: TaskListFilter & { q?: string } = {} as any): Promise<Response> {
    const params: any = {
      status: filter.status,
      priority: filter.priority,
      type: (filter as any).task_type || (filter as any).type,
      project: filter.project,
      tags: filter.tags,
      q: (filter as any).text_query || (filter as any).q,
    }
    return fetch(`/api/tasks/export${qs(params)}`, { headers: { 'Accept': 'text/csv' } })
  },

  // Sprints
  sprintList(): Promise<SprintListResponse> { return get('/api/sprints/list') },
  sprintCreate(payload: SprintCreateRequest): Promise<SprintCreateResponse> { return post('/api/sprints/create', payload) },
  sprintUpdate(payload: SprintUpdateRequest): Promise<SprintUpdateResponse> { return post('/api/sprints/update', payload) },
  sprintDelete(payload: SprintDeleteRequest): Promise<SprintDeleteResponse> { return post('/api/sprints/delete', payload) },
  sprintAdd(payload: SprintAssignmentRequest): Promise<SprintAssignmentResponse> { return post('/api/sprints/add', payload) },
  sprintRemove(payload: SprintAssignmentRequest): Promise<SprintAssignmentResponse> { return post('/api/sprints/remove', payload) },
  sprintSummary(sprint: number): Promise<SprintSummaryReportResponse> { return get('/api/sprints/summary', { sprint }) },
  sprintBurndown(sprint: number): Promise<SprintBurndownResponse> { return get('/api/sprints/burndown', { sprint }) },
  sprintVelocity(params: { limit?: number; include_active?: boolean; metric?: 'tasks' | 'points' | 'hours' } = {}): Promise<SprintVelocityResponse> {
    return get('/api/sprints/velocity', params)
  },
  sprintBacklog(params: { project?: string; tags?: string[]; status?: string[]; assignee?: string; limit?: number; cleanup_missing?: boolean } = {}): Promise<SprintBacklogResponse> {
    const query: Record<string, unknown> = {}
    if (params.project) query.project = params.project
    if (params.tags?.length) query.tags = params.tags
    if (params.status?.length) query.status = params.status
    if (params.assignee) query.assignee = params.assignee
    if (typeof params.limit === 'number') query.limit = params.limit
    if (typeof params.cleanup_missing === 'boolean' && params.cleanup_missing) query.cleanup_missing = 'true'
    return get('/api/sprints/backlog', query)
  },
}

export type ApiClient = typeof api