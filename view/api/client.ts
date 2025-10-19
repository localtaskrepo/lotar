import type {
  ActivityFeedItem,
  ApiEnvelope,
  ConfigInspectResult,
  ProjectCreateRequest,
  ProjectDTO,
  ProjectStatsDTO,
  ReferenceSnippet,
  TaskCreate,
  TaskDTO,
  TaskListFilter,
  TaskUpdate,
} from './types'

const BASE = '' // same origin; server serves /api

function qs(params: Record<string, any> = {}): string {
  const usp = new URLSearchParams()
  Object.entries(params).forEach(([k, v]) => {
    if (v === undefined || v === null) return
    if (Array.isArray(v)) {
      if (v.length) usp.set(k, v.join(','))
    } else {
      usp.set(k, String(v))
    }
  })
  const s = usp.toString()
  return s ? `?${s}` : ''
}

async function get<T>(path: string, params?: Record<string, any>): Promise<T> {
  const res = await fetch(`${BASE}${path}${qs(params)}`, { headers: { 'Accept': 'application/json' } })
  if (!res.ok) throw new Error(`GET ${path} failed: ${res.status}`)
  const env = await res.json() as ApiEnvelope<T>
  if ((env as any).error) throw new Error((env as any).error.message)
  return env.data
}

async function post<T>(path: string, body: any): Promise<T> {
  const res = await fetch(`${BASE}${path}`, { method: 'POST', headers: { 'Content-Type': 'application/json', 'Accept': 'application/json' }, body: JSON.stringify(body) })
  if (!res.ok) throw new Error(`POST ${path} failed: ${res.status}`)
  const env = await res.json() as ApiEnvelope<T>
  if ((env as any).error) throw new Error((env as any).error.message)
  return env.data
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
  taskHistory(id: string, limit?: number): Promise<Array<{ commit: string; author: string; email: string; date: string; message: string }>> { return get('/api/tasks/history', { id, limit }) },
  taskCommitDiff(id: string, commit: string): Promise<string> { return get('/api/tasks/commit_diff', { id, commit }) },
  suggestTasks(q: string, project?: string, limit = 20): Promise<Array<{ id: string; title: string }>> { return get('/api/tasks/suggest', { q, project, limit }) },
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
  setConfig(payload: { values: Record<string, string>; project?: string; global?: boolean }): Promise<{ updated: boolean }> {
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
}

export type ApiClient = typeof api