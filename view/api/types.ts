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
  sprints: number[]
  sprint_order?: Record<number, number>
  history: TaskHistoryEntry[]
  custom_fields: Record<string, unknown>
}

export interface ReferenceEntry {
  code?: string | null
  link?: string | null
  file?: string | null
}

export interface AttachmentUploadRequest {
  id: string
  filename: string
  content_base64: string
}

export interface AttachmentUploadResponse {
  stored_path: string
  attached: boolean
  task: TaskDTO
}

export interface AttachmentRemoveRequest {
  id: string
  stored_path: string
}

export interface AttachmentRemoveResponse {
  task: TaskDTO
  deleted: boolean
  still_referenced: boolean
}

export interface LinkReferenceAddRequest {
  id: string
  url: string
}

export interface LinkReferenceAddResponse {
  task: TaskDTO
  added: boolean
}

export interface LinkReferenceRemoveRequest {
  id: string
  url: string
}

export interface LinkReferenceRemoveResponse {
  task: TaskDTO
  removed: boolean
}

export interface CodeReferenceAddRequest {
  id: string
  code: string
}

export interface CodeReferenceAddResponse {
  task: TaskDTO
  added: boolean
}

export interface CodeReferenceRemoveRequest {
  id: string
  code: string
}

export interface CodeReferenceRemoveResponse {
  task: TaskDTO
  removed: boolean
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
  sprints?: number[]
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
  sprints?: number[]
}

export interface TaskListFilter {
  status?: TaskStatus[]
  priority?: Priority[]
  task_type?: TaskType[]
  project?: string
  tags?: string[]
  q?: string
  assignee?: string
  sprints?: number[]
  custom_fields?: Record<string, string | string[]>
  [key: string]: any
}

export interface TaskSelection {
  filter?: TaskListFilter
  where?: Array<[string, string]>
}

export interface SprintAssignmentRequest {
  sprint?: number | string
  tasks: string[]
  allow_closed?: boolean
  cleanup_missing?: boolean
  force_single?: boolean
  selection?: TaskSelection
}

export interface SprintAssignmentResponse {
  status: string
  action: 'add' | 'remove'
  sprint_id: number
  sprint_label?: string | null
  modified: string[]
  unchanged: string[]
  replaced?: SprintReassignment[]
  messages?: string[]
  integrity?: SprintIntegrityDiagnostics
}

export interface SprintReassignment {
  task_id: string
  previous: number[]
}

export interface SprintDeleteRequest {
  sprint: number
  cleanup_missing?: boolean
}

export interface SprintDeleteResponse {
  status: string
  deleted: boolean
  sprint_id: number
  sprint_label?: string | null
  removed_references: number
  updated_tasks: number
  integrity?: SprintIntegrityDiagnostics
}

export interface SprintCreateRequest {
  label?: string
  goal?: string
  plan_length?: string
  ends_at?: string
  starts_at?: string
  capacity_points?: number
  capacity_hours?: number
  overdue_after?: string
  notes?: string
  skip_defaults?: boolean
}

export interface SprintCreateResponse {
  status: string
  sprint: SprintListItem
  warnings: string[]
  applied_defaults: string[]
}

export interface SprintUpdateRequest {
  sprint: number
  label?: string
  goal?: string
  plan_length?: string
  ends_at?: string
  starts_at?: string
  capacity_points?: number | null
  capacity_hours?: number | null
  overdue_after?: string
  notes?: string
  actual_started_at?: string | null
  actual_closed_at?: string | null
}

export interface SprintUpdateResponse {
  status: string
  sprint: SprintListItem
  warnings: string[]
}

export interface SprintCleanupMetric {
  sprint_id: number
  count: number
}

export interface SprintCleanupSummary {
  removed_references: number
  updated_tasks: number
  removed_by_sprint: SprintCleanupMetric[]
  remaining_missing: number[]
}

export interface SprintIntegrityDiagnostics {
  missing_sprints: number[]
  tasks_with_missing?: number
  auto_cleanup?: SprintCleanupSummary
}

export interface SprintListItem {
  id: number
  label?: string | null
  display_name: string
  created?: string | null
  modified?: string | null
  state: 'pending' | 'active' | 'overdue' | 'complete'
  planned_start?: string | null
  planned_end?: string | null
  actual_start?: string | null
  actual_end?: string | null
  computed_end?: string | null
  goal?: string | null
  plan_length?: string | null
  overdue_after?: string | null
  notes?: string | null
  capacity_points?: number | null
  capacity_hours?: number | null
  warnings: string[]
}

export interface SprintListResponse {
  status: string
  count: number
  sprints: SprintListItem[]
  missing_sprints: number[]
  integrity?: SprintIntegrityDiagnostics
}

export interface SprintStatusWarningPayload {
  code: string
  message: string
}

export interface SprintReviewLifecyclePayload {
  status: string
  state: string
  planned_start?: string | null
  planned_end?: string | null
  actual_start?: string | null
  actual_end?: string | null
  computed_end?: string | null
}

export interface SprintSummary {
  id: number
  label?: string | null
  status: string
  goal?: string | null
  starts_at?: string | null
  ends_at?: string | null
  computed_end?: string | null
  has_warnings?: boolean
}

export interface SprintDetail {
  id: number
  status: string
  label?: string | null
  goal?: string | null
  starts_at?: string | null
  ends_at?: string | null
  computed_end?: string | null
  has_warnings?: boolean
  status_warnings: SprintStatusWarningPayload[]
  sprint: Record<string, unknown>
}

export interface SprintReviewTask {
  id: string
  title: string
  status: string
  assignee?: string | null
}

export interface SprintReviewStatusMetric {
  status: string
  count: number
  done?: boolean
}

export interface SprintStatsCountsPayload {
  committed: number
  done: number
  remaining: number
  completion_ratio: number
}

export interface SprintStatsEffortPayload {
  committed: number
  done: number
  remaining: number
  completion_ratio: number
  capacity?: number | null
  capacity_commitment_ratio?: number | null
  capacity_consumed_ratio?: number | null
}

export interface SprintStatsMetricsPayload {
  tasks: SprintStatsCountsPayload
  hours?: SprintStatsEffortPayload | null
  points?: SprintStatsEffortPayload | null
  status_breakdown: SprintReviewStatusMetric[]
}

export interface SprintStatsTimelinePayload {
  planned_start?: string | null
  actual_start?: string | null
  planned_end?: string | null
  computed_end?: string | null
  actual_end?: string | null
  planned_duration_days?: number | null
  actual_duration_days?: number | null
  elapsed_days?: number | null
  remaining_days?: number | null
  overdue_days?: number | null
}

export interface SprintSummaryReportMetrics {
  tasks: SprintStatsCountsPayload
  hours?: SprintStatsEffortPayload | null
  points?: SprintStatsEffortPayload | null
  blocked?: number
}

export interface SprintSummaryReportResponse {
  status: string
  sprint: SprintDetail
  lifecycle: SprintReviewLifecyclePayload
  metrics: SprintSummaryReportMetrics
  timeline: SprintStatsTimelinePayload
  blocked_tasks: SprintReviewTask[]
}

export interface SprintBurndownTotalsPayload {
  tasks: number
  points?: number | null
  hours?: number | null
}

export interface SprintBurndownPointPayload {
  date: string
  remaining_tasks: number
  ideal_tasks: number
  remaining_points?: number | null
  ideal_points?: number | null
  remaining_hours?: number | null
  ideal_hours?: number | null
}

export interface SprintBurndownResponse {
  status: string
  sprint: SprintDetail
  lifecycle: SprintReviewLifecyclePayload
  totals: SprintBurndownTotalsPayload
  series: SprintBurndownPointPayload[]
}

export interface SprintVelocityEntryPayload {
  summary: SprintSummary
  lifecycle: SprintReviewLifecyclePayload
  start?: string | null
  end?: string | null
  actual_start?: string | null
  actual_end?: string | null
  duration_days?: number | null
  window?: string | null
  committed: number
  completed: number
  completion_ratio?: number | null
  capacity?: number | null
  capacity_commitment_ratio?: number | null
  capacity_consumed_ratio?: number | null
  relative: string
  status_warnings: SprintStatusWarningPayload[]
}

export interface SprintVelocityResponse {
  status: string
  metric: string
  count: number
  truncated: boolean
  include_active?: boolean
  skipped_incomplete?: boolean
  average_velocity?: number | null
  average_completion_ratio?: number | null
  entries: SprintVelocityEntryPayload[]
}

export interface SprintBacklogTask {
  id: string
  title: string
  status: string
  priority: string
  assignee?: string | null
  due_date?: string | null
  tags: string[]
}

export interface SprintBacklogResponse {
  status: string
  count: number
  truncated: boolean
  tasks: SprintBacklogTask[]
  missing_sprints: number[]
  integrity?: SprintIntegrityDiagnostics
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
  default_project: string
  attachments_dir: string
  attachments_max_upload_mb: number
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
  default_project: string
  attachments_dir: string
  attachments_max_upload_mb: number
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
  attachments_dir?: string
  attachments_max_upload_mb?: number
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

export interface ConfigSetResponse {
  updated: boolean
  warnings: string[]
  info: string[]
  errors: string[]
}
