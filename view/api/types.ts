// DTO shapes aligned with src/api_types.rs
export type TaskStatus = string
export type Priority = string
export type TaskType = string

export interface TaskDTO {
  id: string
  title: string
  status: TaskStatus
  priority: Priority
  task_type: TaskType
  reporter?: string | null
  assignee?: string | null
  created: string
  modified: string
  due_date?: string | null
  effort?: string | null
  subtitle?: string | null
  description?: string | null
  tags: string[]
  relationships: TaskRelationships
  comments: any[]
  references: ReferenceEntry[]
  history: TaskHistoryEntry[]
  custom_fields: Record<string, unknown>
}

export interface ReferenceEntry {
  code?: string | null
  link?: string | null
}

export interface ReferenceSnippetLine {
  number: number
  text: string
}

export interface TaskRelationships {
  depends_on?: string[]
  blocks?: string[]
  related?: string[]
  parent?: string
  children?: string[]
  fixes?: string[]
  duplicate_of?: string
}

export interface ReferenceSnippet {
  path: string
  start_line: number
  end_line: number
  highlight_start: number
  highlight_end: number
  lines: ReferenceSnippetLine[]
  has_more_before: boolean
  has_more_after: boolean
  total_lines: number
}

export interface TaskHistoryEntry {
  at: string
  actor?: string | null
  changes: TaskHistoryChange[]
}

export interface TaskHistoryChange {
  field: string
  old?: string | null
  new?: string | null
}

export interface ActivityFeedChange {
  field: string
  kind: string
  old?: string | null
  new?: string | null
}

export interface ActivityFeedHistoryEntry {
  at: string
  actor?: string | null
  changes: ActivityFeedChange[]
}

export interface ActivityFeedItem {
  commit: string
  author: string
  email: string
  date: string
  message: string
  task_id: string
  task_title?: string | null
  history: ActivityFeedHistoryEntry[]
}

export interface TaskCreate {
  title: string
  project?: string
  priority?: Priority
  task_type?: TaskType
  reporter?: string
  assignee?: string
  due_date?: string
  effort?: string
  description?: string
  tags?: string[]
  relationships?: TaskRelationships
  custom_fields?: Record<string, unknown>
}

export interface TaskUpdate {
  title?: string
  status?: TaskStatus
  priority?: Priority
  task_type?: TaskType
  reporter?: string
  assignee?: string
  due_date?: string
  effort?: string
  description?: string
  tags?: string[]
  relationships?: TaskRelationships
  custom_fields?: Record<string, unknown>
}

export interface TaskListFilter {
  status?: TaskStatus[]
  priority?: Priority[]
  task_type?: TaskType[]
  project?: string
  tags?: string[]
  q?: string
  assignee?: string
  [key: string]: any
}

export interface ProjectDTO { name: string; prefix: string }
export interface ProjectCreateRequest {
  name: string
  prefix?: string
  values?: Record<string, string>
}
export interface ProjectStatsDTO {
  name: string
  open_count: number
  done_count: number
  recent_modified?: string | null
  tags_top: string[]
}

export interface ApiEnvelope<T> { data: T; meta?: any; error?: { code: string; message: string } }

export type ConfigSource = 'project' | 'global' | 'built_in'

export interface ResolvedConfigDTO {
  server_port: number
  default_prefix: string
  default_assignee?: string | null
  default_reporter?: string | null
  default_tags: string[]
  default_priority: string
  default_status?: string | null
  issue_states: string[]
  issue_types: string[]
  issue_priorities: string[]
  tags: string[]
  custom_fields: string[]
  auto_set_reporter: boolean
  auto_assign_on_status: boolean
  auto_codeowners_assign: boolean
  auto_tags_from_path: boolean
  auto_branch_infer_type: boolean
  auto_branch_infer_status: boolean
  auto_branch_infer_priority: boolean
  auto_identity: boolean
  auto_identity_git: boolean
  scan_signal_words: string[]
  scan_ticket_patterns?: string[] | null
  scan_enable_ticket_words: boolean
  scan_enable_mentions: boolean
  scan_strip_attributes: boolean
  branch_type_aliases: Record<string, string>
  branch_status_aliases: Record<string, string>
  branch_priority_aliases: Record<string, string>
}

export interface GlobalConfigRaw {
  server_port: number
  default_prefix: string
  issue_states: string[]
  issue_types: string[]
  issue_priorities: string[]
  tags: string[]
  default_assignee?: string | null
  default_reporter?: string | null
  default_tags: string[]
  auto_set_reporter: boolean
  auto_assign_on_status: boolean
  auto_codeowners_assign: boolean
  auto_tags_from_path: boolean
  auto_branch_infer_type: boolean
  auto_branch_infer_status: boolean
  auto_branch_infer_priority: boolean
  default_priority: string
  default_status?: string | null
  custom_fields: string[]
  auto_identity: boolean
  auto_identity_git: boolean
  scan_signal_words: string[]
  scan_ticket_patterns?: string[] | null
  scan_enable_ticket_words: boolean
  scan_enable_mentions: boolean
  scan_strip_attributes: boolean
  branch_type_aliases: Record<string, string>
  branch_status_aliases: Record<string, string>
  branch_priority_aliases: Record<string, string>
}

export interface ProjectConfigRaw {
  project_name?: string
  issue_states?: string[]
  issue_types?: string[]
  issue_priorities?: string[]
  tags?: string[]
  default_assignee?: string | null
  default_reporter?: string | null
  default_tags?: string[]
  default_priority?: string | null
  default_status?: string | null
  custom_fields?: string[]
  auto_set_reporter?: boolean
  auto_assign_on_status?: boolean
  scan_signal_words?: string[]
  scan_ticket_patterns?: string[]
  scan_enable_ticket_words?: boolean
  scan_enable_mentions?: boolean
  scan_strip_attributes?: boolean
  branch_type_aliases?: Record<string, string>
  branch_status_aliases?: Record<string, string>
  branch_priority_aliases?: Record<string, string>
}

export interface ConfigInspectResult {
  effective: ResolvedConfigDTO
  global_effective: ResolvedConfigDTO
  global_raw: GlobalConfigRaw
  project_raw?: ProjectConfigRaw | null
  has_global_file: boolean
  project_exists: boolean
  sources: Record<string, ConfigSource>
}
