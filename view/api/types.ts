// DTO shapes aligned with src/api_types.rs
export type TaskStatus = string
export type Priority = string
export type TaskType = string

export type TaskDueBucket = 'today' | 'soon' | 'later' | 'overdue'

/**
 * Server-computed runtime completion/due metadata (DEV-21). Populated by all
 * actual get/list/mutation/SSE task payloads using the actual root/project
 * policy and the server's local calendar day; never persisted in task YAML.
 * `due_bucket` is null for tasks without a due date and for terminal tasks
 * that are past due (only Overdue excludes done; a done task due today can
 * still be 'today').
 *
 * `done_states` embeds the ORDERED actual-root terminal policy that produced
 * `is_done`/`due_bucket`. Real payloads always include all four fields; the
 * field stays optional only so legacy test fixtures may omit metadata.
 * Homonymous project prefixes across storage roots can resolve DIFFERENT
 * policies, so the embedded set — not a config/show lookup by id prefix — is
 * authoritative for classifying THIS task.
 */
export interface TaskRuntimeState {
  done_states?: string[]
  is_done: boolean
  due_bucket: TaskDueBucket | null
  /** Server-local calendar day (YYYY-MM-DD) this state was computed for. */
  calendar_day: string
}

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
  /**
   * DEV-92 soft-deletion tombstone timestamp (RFC3339). Absent while the
   * task is active; set when the task is in the trash. Soft delete and
   * restore do NOT change `modified` — presence/absence of this field is
   * the deletion lifecycle signal, never modified ordering.
   */
  deleted_at?: string | null
  /** Optional runtime completion metadata; may be omitted in test fixtures. */
  task_state?: TaskRuntimeState
  due_date?: string | null
  effort?: string | null
  subtitle?: string | null
  description?: string | null
  tags: string[]
  relationships: TaskRelationships
  comments: any[]
  references: ReferenceEntry[]
  /** Acceptance criteria entries; omitted when empty. */
  acceptance_criteria?: string[]
  sprints: number[]
  sprint_order?: Record<number, number>
  history: TaskHistoryEntry[]
  custom_fields: Record<string, unknown>
}

/**
 * Typed external reference entry aligned with `ReferenceEntry` in
 * src/types.rs. Each entry carries exactly one kind: `code` and `file` are
 * repository-relative paths (`code` carries a `#N[-M]` line anchor),
 * `attachment` is a managed attachments-store blob name, and
 * `link`/`jira`/`github` are external values. The file-ish kinds are
 * distinct on purpose (DEV-61): managed blobs are never stored under
 * `file` and repository files never under `attachment`.
 */
export interface ReferenceEntry {
  code?: string | null
  link?: string | null
  file?: string | null
  attachment?: string | null
  jira?: string | null
  github?: string | null
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

export type TaskReferenceKind = 'jira' | 'github'

export interface GenericReferenceAddRequest {
  id: string
  kind: TaskReferenceKind
  value: string
}

export interface GenericReferenceAddResponse {
  task: TaskDTO
  added: boolean
}

export interface GenericReferenceRemoveRequest {
  id: string
  kind: TaskReferenceKind
  value: string
}

export interface GenericReferenceRemoveResponse {
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

export interface FileReferenceAddRequest {
  id: string
  /** Repository-relative file path (never a managed attachments-store blob). */
  path: string
}

export interface FileReferenceAddResponse {
  task: TaskDTO
  added: boolean
}

export interface FileReferenceRemoveRequest {
  id: string
  path: string
}

export interface FileReferenceRemoveResponse {
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
  /**
   * Initial status; validated against the target project's issue_states and
   * stored atomically with creation. When omitted, branch inference or the
   * project default applies. No add-then-status follow-up is needed.
   */
  status?: TaskStatus
  priority?: Priority
  task_type?: TaskType
  reporter?: string
  assignee?: string
  due_date?: string
  effort?: string
  description?: string
  tags?: string[]
  acceptance_criteria?: string[]
  relationships?: TaskRelationships
  custom_fields?: Record<string, unknown>
  sprints?: number[]
}

/**
 * Patch payload with tri-state field semantics: omitted = no-op, null = clear
 * (where clearing is allowed), value = set. Empty strings clear clearable
 * scalars and empty arrays/maps clear collections. `title`, `status`,
 * `priority`, and `task_type` cannot be cleared; null is treated as omitted.
 * Enum strings are validated server-side against the task's project config.
 * `tags`, `acceptance_criteria`, `relationships`, `custom_fields`, and
 * `sprints` replace the whole value when set.
 */
export interface TaskUpdate {
  title?: string
  status?: TaskStatus
  priority?: Priority
  task_type?: TaskType
  /** Supports @me; null or empty string clears. */
  reporter?: string | null
  /** Supports @me; null or empty string clears. */
  assignee?: string | null
  /** Null or empty string clears. */
  due_date?: string | null
  /** Null or empty string clears. */
  effort?: string | null
  /** Null or empty string clears. */
  description?: string | null
  /** Replaces the list; null or [] clears. */
  tags?: string[] | null
  /** Replaces the list; null or [] clears. */
  acceptance_criteria?: string[] | null
  /** Replaces the map; null or {} clears. */
  relationships?: TaskRelationships | null
  /** Replaces the whole map; null or {} clears all fields. */
  custom_fields?: Record<string, unknown> | null
  /** Replaces sprint memberships; null or [] clears them. */
  sprints?: number[] | null
}

/**
 * Server-side sort key for task list/export queries: one of the CLI sort
 * fields (`priority`, `status`, `effort`, `due`, `created`, `modified`,
 * `assignee`, `reporter`, `type`, `project`, `id`, `title`, `tags`,
 * `sprints`) or `custom:<name>` for a custom field (the CLI accepts
 * `field:<name>` as an alias). Omitted defaults to `modified`. Ties break by
 * canonical task ID lexical ASC.
 */
export type TaskSortBy = string

/** Sort direction for task queries; `desc` is the default. */
export type TaskSortOrder = 'asc' | 'desc'

export interface TaskListFilter {
  status?: TaskStatus[]
  priority?: Priority[]
  task_type?: TaskType[]
  project?: string
  tags?: string[]
  q?: string
  /** Any-of assignee equality filter; `__none__` requests unassigned tasks. */
  assignee?: string | string[]
  assignee_none?: boolean
  /** Sprint id CSV; a raw string is forwarded when tokens are invalid so the strict server parser rejects the query. */
  sprints?: number[] | string
  custom_fields?: Record<string, string | string[]>
  /** Server-side sort key; see {@link TaskSortBy}. */
  sort_by?: TaskSortBy
  /** Sort direction applied to `sort_by`; invalid raw values are forwarded so the strict server parser rejects the query. */
  order?: TaskSortOrder | (string & {})
  /** Smart due bucket: `today` | `soon` | `later` | `overdue`. */
  due?: string
  /** Smart recency window: `7d`. */
  recent?: string
  /** CSV of missing-field filters: `effort`, `due`. */
  needs?: string
  /**
   * DEV-92 deletion visibility: `active` (default, server-side), `deleted`
   * (trash only), or `all`. Part of the canonical query key, so each
   * visibility mode owns its own query entry.
   */
  deletion?: 'active' | 'deleted' | 'all'
  [key: string]: any
}

/** Response of POST /api/tasks/delete (soft by default, `hard` on request). */
export interface TaskDeleteResponse {
  deleted: boolean
  hard: boolean
  /** Retained attachment blobs / incoming relationships the server detected. */
  warnings: string[]
}

export interface TaskListResponse {
  total: number
  limit: number
  offset: number
  /** Page of tasks; omitted (not `[]`) by the server when the page is empty — treat missing as empty. */
  tasks?: TaskDTO[]
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
  sprint_label?: string
  removed_references: number
  updated_tasks: number
  integrity?: SprintIntegrityDiagnostics
}

/**
 * Optional fields accept `null`, treated as omitted (no clear); the server
 * skips unset values when serializing sprint responses.
 */
export interface SprintCreateRequest {
  label?: string | null
  goal?: string | null
  plan_length?: string | null
  ends_at?: string | null
  starts_at?: string | null
  capacity_points?: number | null
  capacity_hours?: number | null
  overdue_after?: string | null
  notes?: string | null
  skip_defaults?: boolean
}

export interface SprintCreateResponse {
  status: string
  sprint: SprintListItem
  /** Omitted when empty. */
  warnings?: string[]
  /** Defaults applied from configuration; omitted when empty. */
  applied_defaults?: string[]
}

/**
 * Single-value optional fields accept `null` treated as omitted (no clear);
 * `capacity_points`/`capacity_hours`/`actual_started_at`/`actual_closed_at`
 * are double-options where `null` explicitly clears the stored value.
 */
export interface SprintUpdateRequest {
  sprint: number
  label?: string | null
  goal?: string | null
  plan_length?: string | null
  ends_at?: string | null
  starts_at?: string | null
  capacity_points?: number | null
  capacity_hours?: number | null
  overdue_after?: string | null
  notes?: string | null
  actual_started_at?: string | null
  actual_closed_at?: string | null
}

export interface SprintUpdateResponse {
  status: string
  sprint: SprintListItem
  /** Omitted when empty. */
  warnings?: string[]
}

export interface SprintCleanupMetric {
  sprint_id: number
  count: number
}

export interface SprintCleanupSummary {
  removed_references: number
  updated_tasks: number
  /** Omitted when empty. */
  removed_by_sprint?: SprintCleanupMetric[]
  /** Omitted when empty. */
  remaining_missing?: number[]
}

export interface SprintIntegrityDiagnostics {
  /** Omitted when empty. */
  missing_sprints?: number[]
  tasks_with_missing?: number
  auto_cleanup?: SprintCleanupSummary
}

/**
 * Optional members are omitted (not `null`) by the server when unset and
 * omitted when empty for `warnings`.
 */
export interface SprintListItem {
  id: number
  label?: string
  display_name: string
  created?: string
  modified?: string
  state: 'pending' | 'active' | 'overdue' | 'complete'
  planned_start?: string
  planned_end?: string
  actual_start?: string
  actual_end?: string
  computed_end?: string
  goal?: string
  plan_length?: string
  overdue_after?: string
  notes?: string
  capacity_points?: number
  capacity_hours?: number
  warnings?: string[]
}

export interface SprintListResponse {
  status: string
  total: number
  count: number
  limit: number
  offset: number
  sprints: SprintListItem[]
  missing_sprints: number[]
  integrity?: SprintIntegrityDiagnostics
}

export interface ProjectListResponse {
  total: number
  limit: number
  offset: number
  projects: ProjectDTO[]
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

export type DoneStatesMode = 'explicit' | 'inferred'

export type SyncProvider = 'jira' | 'github'
export type SyncWhenEmpty = 'skip' | 'clear'

export interface SyncFieldMappingDetail {
  field?: string | null
  values?: Record<string, string>
  set?: string | null
  default?: string | null
  add?: string[]
  when_empty?: SyncWhenEmpty | null
}

export type SyncFieldMapping = string | SyncFieldMappingDetail

export interface SyncRemoteConfig {
  provider: SyncProvider
  project?: string | null
  repo?: string | null
  filter?: string | null
  auth_profile?: string | null
  mapping?: Record<string, SyncFieldMapping>
}

export interface SyncAuthProfile {
  provider?: SyncProvider | null
  method?: string | null
  token_env?: string | null
  email_env?: string | null
  base_url?: string | null
  api_url?: string | null
}


export interface ResolvedConfigDTO {
  server_port: number
  default_project: string
  attachments_dir: string
  attachments_max_upload_mb: number
  sync_reports_dir: string
  sync_write_reports: boolean
  default_assignee?: string | null
  default_reporter?: string | null
  default_tags: string[]
  default_priority: string
  default_status?: string | null
  issue_states: string[]
  issue_types: string[]
  issue_priorities: string[]
  /** Raw inherited explicit done-state list; null/absent when unset. */
  issue_done_states?: string[] | null
  /** Server-resolved terminal statuses; always present. */
  effective_done_states: string[]
  /** Whether the effective list came from an explicit setting or inference. */
  done_states_mode: DoneStatesMode
  /** Server-local calendar day (YYYY-MM-DD) at resolution time. */
  task_calendar_day: string
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
  remotes?: Record<string, SyncRemoteConfig>
}

export interface GlobalConfigRaw {
  server_port: number
  default_project: string
  attachments_dir: string
  attachments_max_upload_mb: number
  sync_reports_dir: string
  sync_write_reports: boolean
  issue_states: string[]
  issue_types: string[]
  issue_priorities: string[]
  /** Explicit done states; null/absent = automatic legacy inference. */
  issue_done_states?: string[] | null
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
  remotes?: Record<string, SyncRemoteConfig>
  auth_profiles?: Record<string, SyncAuthProfile>
}

export interface ProjectConfigRaw {
  project_name?: string
  attachments_dir?: string
  attachments_max_upload_mb?: number
  sync_reports_dir?: string
  sync_write_reports?: boolean
  issue_states?: string[]
  issue_types?: string[]
  issue_priorities?: string[]
  /** Explicit done states; null/absent = inherit the global setting. */
  issue_done_states?: string[] | null
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
  remotes?: Record<string, SyncRemoteConfig>
  auth_profiles?: Record<string, SyncAuthProfile>
}

export interface ConfigInspectResult {
  effective: ResolvedConfigDTO
  global_effective: ResolvedConfigDTO
  global_raw: GlobalConfigRaw
  auth_profiles: Record<string, SyncAuthProfile>
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

export interface AutomationInspectResponse {
  scope: string
  source: string
  scope_exists: boolean
  scope_yaml: string
  effective_yaml: string
}

export interface AutomationSetRequest {
  yaml: string
  project?: string
}

export interface AutomationSetResponse {
  updated: boolean
  warnings: string[]
  info: string[]
  errors: string[]
}

export interface ScanTarget {
  file: string
  line: number
}

export interface ScanRequest {
  paths?: string[]
  include?: string[]
  exclude?: string[]
  project?: string
  dry_run?: boolean
  strip_attributes?: boolean
  reanchor?: boolean
  modified_only?: boolean
  targets?: ScanTarget[]
}

export interface ScanSummary {
  created: number
  updated: number
  skipped: number
  failed: number
}

export interface ScanEntry {
  status: 'created' | 'updated' | 'skipped' | 'failed' | string
  action: 'create' | 'refresh' | 'skip' | string
  file: string
  line: number
  title: string
  annotation: string
  code_reference: string
  existing_key?: string | null
  task_id?: string | null
  original_line?: string | null
  updated_line?: string | null
  message?: string | null
}

export interface ScanResponse {
  status: string
  dry_run: boolean
  project?: string | null
  summary: ScanSummary
  warnings: string[]
  info: string[]
  entries: ScanEntry[]
}

export interface SyncRequest {
  remote: string
  project?: string
  task_id?: string
  auth_profile?: string
  dry_run?: boolean
  include_report?: boolean
  write_report?: boolean
  client_run_id?: string
}

export interface SyncValidateRequest {
  remote?: string
  project?: string
  auth_profile?: string
  remote_config?: SyncRemoteConfig
}

export interface SyncSummary {
  created: number
  updated: number
  skipped: number
  failed: number
}

export type SyncReportStatus = 'created' | 'updated' | 'skipped' | 'failed'

export interface SyncReportEntry {
  status: SyncReportStatus
  at: string
  task_id?: string | null
  reference?: string | null
  title?: string | null
  message?: string | null
  fields?: string[]
}

export interface SyncReportMeta {
  id: string
  created_at: string
  status: string
  direction: 'push' | 'pull'
  provider: SyncProvider
  remote: string
  project?: string | null
  dry_run: boolean
  summary: SyncSummary
  warnings?: string[]
  info?: string[]
  entries_total: number
  stored_path?: string | null
}

export interface SyncReport {
  id: string
  created_at: string
  status: string
  direction: 'push' | 'pull'
  provider: SyncProvider
  remote: string
  project?: string | null
  dry_run: boolean
  summary: SyncSummary
  warnings?: string[]
  info?: string[]
  entries: SyncReportEntry[]
}

export interface SyncReportListResponse {
  total: number
  limit: number
  offset: number
  reports: SyncReportMeta[]
}

export interface SyncResponse {
  status: string
  direction: 'push' | 'pull'
  provider: SyncProvider
  remote: string
  project?: string | null
  dry_run: boolean
  summary: SyncSummary
  warnings?: string[]
  info?: string[]
  run_id: string
  report?: SyncReportMeta | null
  report_entries?: SyncReportEntry[]
}

export interface SyncValidateResponse {
  status: string
  provider: SyncProvider
  remote: string
  project?: string | null
  repo?: string | null
  filter?: string | null
  checked_at: string
  warnings?: string[]
  info?: string[]
}

export interface AgentJob {
  id: string
  ticket_id: string
  runner: string
  agent?: string | null
  status: string
  created_at: string
  started_at?: string | null
  finished_at?: string | null
  exit_code?: number | null
  last_message?: string | null
  summary?: string | null
  session_id?: string | null
  worktree_path?: string | null
  worktree_branch?: string | null
}

export interface AgentJobCreateRequest {
  ticket_id: string
  prompt: string
  runner?: string
  agent?: string
}

export interface AgentJobCreateResponse {
  job: AgentJob
}

export interface AgentJobStatusResponse {
  job: AgentJob
}

export interface AgentQueueStats {
  running: number
  queued: number
  max_parallel?: number | null
}

export interface AgentJobListResponse {
  jobs: AgentJob[]
  queue_stats?: AgentQueueStats | null
}

export interface AgentJobCancelRequest {
  id: string
}

export interface AgentJobCancelResponse {
  cancelled: boolean
  job?: AgentJob | null
}

export interface AgentJobCancelAllResponse {
  cancelled: number
  jobs: AgentJob[]
}

export interface AgentJobLogEntry {
  kind: string
  at: string
  message?: string | null
}

export interface AgentJobLogsResponse {
  job: AgentJob
  events: AgentJobLogEntry[]
}

export interface AgentJobMessageRequest {
  id: string
  message: string
}

export interface AgentJobMessageResponse {
  accepted: boolean
  job: AgentJob
}

// Agent profiles
export interface AgentProfileInfo {
  name: string
  runner: string
  description?: string | null
}

export interface AgentProfilesResponse {
  profiles: AgentProfileInfo[]
}

// Automation simulation
export interface AutomationSimulateRequest {
  ticket_id: string
  event: string
  /** Accepted but ignored: scope follows the ticket's project prefix. */
  project?: string | null
}

export interface AutomationSimulatedAction {
  action: string
  description: string
}

/**
 * The server always serializes all five keys (hand-built JSON):
 * `rule_name` is `null` when no rule matched and `task_after` is `null`
 * when no rule matched; `task_before` is always a task object.
 */
export interface AutomationSimulateResponse {
  matched: boolean
  rule_name: string | null
  actions: AutomationSimulatedAction[]
  task_before: TaskDTO
  task_after: TaskDTO | null
}

