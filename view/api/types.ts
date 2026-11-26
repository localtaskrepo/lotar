// DTO shapes aligned with crates/lotar-core/src/api_types.rs
export type TaskStatus = string
export type Priority = string
export type TaskType = string

export type RelationshipKind = 'dependency' | 'hierarchy' | 'related' | 'fixes' | 'duplicate'
export type RelationshipField = 'depends_on' | 'blocks' | 'related' | 'parent' | 'children' | 'fixes' | 'duplicate_of'
export interface RelationshipGraphRequest {
    project?: string
    task_id?: string
    depth?: number
    max_nodes?: number
    relationship_kinds?: RelationshipKind[]
    include_isolated?: boolean
    filters?: Record<string, string>
}
export type RelationshipTaskSummary = Pick<
    TaskDTO,
    | 'id'
    | 'title'
    | 'status'
    | 'task_state'
    | 'priority'
    | 'task_type'
    | 'reporter'
    | 'assignee'
    | 'effort'
    | 'tags'
    | 'sprints'
    | 'due_date'
    | 'created'
    | 'modified'
    | 'custom_fields'
>
export interface RelationshipNode {
    id: string
    title: string | null
    status: string | null
    project: string
    state: 'active' | 'external' | 'deleted' | 'missing' | 'ambiguous' | 'unreadable'
    is_done: boolean
    matches_filter: boolean
    task?: RelationshipTaskSummary
}
export interface RelationshipProvenance {
    task_id: string
    field: string
    value: string
}
export interface RelationshipEdge {
    source: string
    target: string
    kind: RelationshipKind
    provenance: RelationshipProvenance[]
}
export interface RelationshipIssue {
    id?: string
    code: string
    severity: string
    message: string
    tasks: string[]
    repairable: boolean
}
export interface RelationshipGraphResponse {
    nodes: RelationshipNode[]
    edges: RelationshipEdge[]
    issues: RelationshipIssue[]
    revision: string
    complete: boolean
    truncated: boolean
}
export interface RelationshipCheckResponse {
    issues: RelationshipIssue[]
    revision: string
    complete: boolean
}
/** A task in another repo's LoTaR (DEV-386): resolved fields, or why it is unresolved. */
export interface CrossRepoTask {
    reference: string
    title?: string
    status?: string
    done?: boolean
    url?: string
    unresolved?: string
}
export interface RelationshipMutationRequest {
    source: string
    target: string
    kind: RelationshipField
    operation: 'add' | 'remove' | 'change_kind'
    from_kind?: RelationshipField
    expected_current?: string
}
export interface RelationshipMutationResponse {
    changed: boolean
    changed_tasks: string[]
    revision: string
}
export interface RelationshipRepairRequest {
    project?: string
    issue_ids?: string[]
    apply?: boolean
    revision?: string
}
export interface RelationshipRepairResponse {
    applied: boolean
    changed_tasks: string[]
    revision: string
    issues: RelationshipIssue[]
}

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

/**
 * A task. `GET /api/tasks/get`, SSE payloads and mutation responses send all
 * of it; `GET /api/tasks/list` rows (DEV-692, `TaskListRow` in
 * crates/lotar-core/src/api_types.rs) leave out `description`, `comments`
 * and `history` (unless `include=history`) and carry the summary fields
 * instead. The task store holds both kinds; see {@link isListRow}.
 */
export interface TaskDTO {
    id: string
    /** The task's web link `<server.public_url>/task/<ID>`; absent while no public URL is set. */
    url?: string
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
    /** DEV-4: the task is in its project's @archive folder and read-only. */
    archived?: boolean
    /** DEV-183: earlier IDs of a task moved between projects, oldest first. */
    moved_from?: string[]
    /** Optional runtime completion metadata; may be omitted in test fixtures. */
    task_state?: TaskRuntimeState
    due_date?: string | null
    effort?: string | null
    subtitle?: string | null
    description?: string | null
    tags: string[]
    relationships: TaskRelationships
    /** Absent in list rows (DEV-692). */
    comments?: TaskComment[]
    references: ReferenceEntry[]
    /** Acceptance criteria entries; omitted when empty. */
    acceptance_criteria?: string[]
    sprints: number[]
    sprint_order?: Record<number, number>
    /** Releases the task belongs to (DEV-393); a task can be in several. */
    releases: number[]
    /** Absent in list rows unless the list asked for `include=history` (DEV-692). */
    history?: TaskHistoryEntry[]
    custom_fields: Record<string, unknown>
    /** List rows only (DEV-692): number of comments. */
    comment_count?: number
    /** List rows only (DEV-692): number of history entries. */
    history_count?: number
    /** List rows only (DEV-692): the newest question no answer points to. */
    open_question?: TaskComment
    /**
     * List rows only (DEV-692): when the task entered its current status —
     * the latest change into it, else `created` when its status never
     * changed, else the latest status change.
     */
    status_since?: string
}

/** A `GET /api/tasks/list` row (DEV-692) rather than a whole task. */
export function isListRow(task: TaskDTO | null | undefined): boolean {
    return !!task && typeof task.comment_count === 'number'
}

/**
 * Typed external reference entry aligned with `ReferenceEntry` in
 * crates/lotar-core/src/types.rs. Each entry carries exactly one kind: `code` and `file` are
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
    /** Camera, location and other personal metadata was removed (DEV-216). */
    metadata_removed?: boolean
    /** The upload worked, but something looks off (e.g. a fake image). */
    warnings?: string[]
}

/** Repository branding for the web UI (DEV-218). */
export interface BrandingResponse {
    theme?: string
    /** `home` (~/.lotar) or `repository` (.tasks/config.yml); absent = built-in. */
    theme_source?: 'home' | 'repository'
    /** The repository's ui.theme when ~/.lotar overrides it. */
    repository_theme?: string
    /** Path of a usable repository skin (ui.theme_file); theme `custom`. */
    theme_file?: string
    name?: string
    /** The repository's folder name; the tab title without `name`. */
    repository?: string
    logo?: string
    logo_dark?: string
    icon?: string
    /** Without ui.icon: the repository's own icon file that `icon` shows. */
    detected_icon?: string
    problems?: string[]
    /** A commit's web page without the hash, from the git remote (DEV-490). */
    commit_url?: string
}

export interface BrandingUploadResponse {
    /** Repository-relative path for ui.logo / ui.logo_dark / ui.icon. */
    path: string
}

export interface BrandingUploadRequest {
    /** An image slot, or `theme` for a skin (.css). */
    image: 'logo' | 'logo-dark' | 'icon' | 'theme'
    filename: string
    content_base64: string
}

export interface AttachmentReorderRequest {
    id: string
    /** Every attachment of the task (stored names), once, in the new order. */
    attachments: string[]
}

export interface AttachmentInfoRequest {
    project?: string
    attachments: string[]
}

export interface AttachmentInfo {
    attachment: string
    size: number
    width?: number
    height?: number
}

export interface AttachmentInfoResponse {
    items: AttachmentInfo[]
}

export interface FileResolveRequest {
    /** Directory of the repository file the references appear in. */
    base?: string
    references: string[]
}

export interface FileResolveItem {
    reference: string
    /** Repository-relative path with `/` separators. */
    path: string
}

export interface FileResolveResponse {
    items: FileResolveItem[]
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
    /** Release ids to add the new task to (DEV-393). */
    releases?: number[]
    /**
     * Report a problem once (DEV-317): a stable key for it. The response then
     * carries `finding`, and `data` may be an existing ticket.
     */
    finding?: string
}

/**
 * A status/assignee change held back while an agent job runs on the ticket
 * (DEV-320): `/api/tasks/update` and `/api/tasks/status` answer it next to
 * `data` (unchanged), and the job's end applies it in place of the end
 * rules' `set`. An empty `assignee` clears the assignee.
 */
export interface JobOutcome {
    job_id: string
    status?: string
    assignee?: string
    /** Fields set back to the ticket's current value: their recorded change was dropped. */
    cleared?: ('status' | 'assignee')[]
}

/** How `/api/tasks/add` handled a `finding` report. */
export interface FindingReport {
    created: boolean
    /** `bundled` only from `lotar add --bundle` (DEV-800). */
    state: 'new' | 'recurred' | 'open' | 'suppressed' | 'bundled'
    /** The new ticket, or the existing one that already reports the finding. */
    id: string
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
/** DEV-183: move a task to another project. Unset values keep the current
 * one when the target allows it, else the target's default. */
export interface TaskMoveRequest {
    id: string
    project: string
    status?: string
    task_type?: string
    priority?: string
    reporter?: string
    assignee?: string
    /** `null` or an empty string drops a field. */
    custom_fields?: Record<string, unknown>
    dry_run?: boolean
}

export interface TaskMoveChange {
    field: string
    old?: string
    new?: string
}

export interface TaskMoveResponse {
    from: string
    to: string
    changes?: TaskMoveChange[]
    dry_run?: boolean
    task: TaskDTO
}

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
    /** Replaces release memberships; null or [] clears them (DEV-393). */
    releases?: number[] | null
    /**
     * DEV-301: the `modified` value the caller loaded. If the task changed
     * since, the server answers 409 and writes nothing.
     */
    expected_modified?: string | null
}

/**
 * Server-side sort key for task list/export queries: one of the CLI sort
 * fields (`priority`, `status`, `effort`, `due`, `created`, `modified`,
 * `assignee`, `reporter`, `type`, `project`, `id`, `title`, `tags`,
 * `sprints`) or `custom:<name>` for a custom field (the CLI accepts
 * `field:<name>` as an alias). Omitted defaults to `modified`. Ties break by
 * natural task ID ASC (A-2 before A-10).
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
    /** Release selectors CSV (DEV-393): a version or range, `#id`, label, or `none`. */
    releases?: string
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
    /** CSV of kinds tasks must carry: `attachment`, `image`, `link`, `tag` (at least one tag). */
    has?: string
    /** Filters and search text in the web search-box syntax (same as `lotar list`). */
    query?: string
    /** Due from today through today + N days, inclusive. */
    due_within?: number | string
    /** Inclusive effort bounds such as `2h`, `1d` or `3pt`; tasks without an effort are left out. */
    effort_min?: string
    effort_max?: string
    /**
     * DEV-92 deletion visibility: `active` (default, server-side), `deleted`
     * (trash only), or `all`. Part of the canonical query key, so each
     * visibility mode owns its own query entry.
     */
    deletion?: 'active' | 'deleted' | 'all'
    /** DEV-4 archive visibility: `no` (default) leaves archived tasks out, `yes` only them, `all` both. */
    archived?: 'no' | 'yes' | 'all'
    /** DEV-692: `history` keeps each row's history (Insights). */
    include?: 'history'
    /** DEV-692: comma-separated task IDs (at most 100); the answer carries their `positions`. */
    ids?: string
    [key: string]: unknown
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
    /** Page of list rows; omitted (not `[]`) by the server when the page is empty — treat missing as empty. */
    tasks?: TaskDTO[]
    /** With `ids` (DEV-692): each returned task's index in the full filtered order. */
    positions?: Record<string, number>
}

export interface SprintBacklogItem {
    id: string
    title: string
    status: string
    priority: string
    assignee?: string
    due_date?: string
    tags?: string[]
}

export interface TaskOrderRequest {
    task: string
    /** Omitted/null selects backlog; otherwise a positive sprint ID. */
    sprint?: number | null
    before?: string | null
    after?: string | null
    allow_closed?: boolean
}

export interface TaskOrderResponse {
    status: string
    tasks: string[]
    changed: boolean
    sprint_id?: number
}

export interface SprintBacklogResponse {
    status: string
    count: number
    truncated: boolean
    tasks?: SprintBacklogItem[]
    missing_sprints?: number[]
    integrity?: SprintIntegrityDiagnostics
}

export interface SprintAssignmentRequest {
    sprint?: number | string
    tasks: string[]
    allow_closed?: boolean
    cleanup_missing?: boolean
    force_single?: boolean
    /** Also act on the tasks this search selects, in the web search syntax (e.g. `status:todo tag:ui`). */
    query?: string
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

/** A release (DEV-393) with counts over its tickets. */
export interface ReleaseListItem {
    id: number
    /** Stored without a leading `v` (`0.15.0`). */
    version: string
    label?: string
    display_name: string
    notes?: string
    /** `YYYY-MM-DD`. */
    planned_date?: string
    /** RFC 3339 or `YYYY-MM-DD`; absent while planned. */
    released_at?: string
    state: 'planned' | 'released'
    /** Planned for a day before today and not released. */
    overdue: boolean
    task_count: number
    done_count: number
    open_count: number
    created?: string
    modified?: string
}

/** A release named by id (`3`) or by `#3`, version or label. */
export type ReleaseSelector = number | string

export interface ReleaseListResponse {
    status: string
    total: number
    /** In version order. */
    releases: ReleaseListItem[]
}

export interface ReleaseGetResponse {
    status: string
    release: ReleaseListItem
    tasks: TaskDTO[]
    history: TaskHistoryEntry[]
}

export interface ReleaseCreateRequest {
    /** `0.15.0` or `v0.15.0` (stored without the `v`). */
    version: string
    label?: string
    notes?: string
    /** `YYYY-MM-DD`. */
    planned_date?: string
    /** RFC 3339 or `YYYY-MM-DD`; set for a release that is already out. */
    released_at?: string
}

/** Fields left out stay as they are; an empty string clears one. */
export interface ReleaseUpdateRequest {
    release: ReleaseSelector
    version?: string
    label?: string
    notes?: string
    planned_date?: string
    released_at?: string
}

/** Mark a release released (at `at`, default now), reopen it, or delete it. */
export interface ReleaseActionRequest {
    release: ReleaseSelector
    at?: string
}

export interface ReleaseMutationResponse {
    status: string
    release: ReleaseListItem
    /** False when the request changed nothing. */
    changed: boolean
}

export interface ReleaseDeleteResponse {
    status: string
    deleted: boolean
    release_id: number
    /** Tickets the release was taken off. */
    removed_from: string[]
}

export interface ReleaseAssignmentRequest {
    release: ReleaseSelector
    tasks: string[]
    /** Also act on the tasks this search selects, in the web search syntax. */
    query?: string
}

export interface ReleaseAssignmentResponse {
    status: string
    action: 'add' | 'move' | 'remove'
    release_id: number
    release_version: string
    modified: string[]
    unchanged: string[]
}

export interface ReleaseBacklogResponse {
    status: string
    total: number
    count: number
    limit: number
    offset: number
    /** Open tickets in no release. */
    tasks: TaskDTO[]
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

export interface ProjectDTO {
    name: string
    prefix: string
}
export interface ProjectCreateRequest {
    name: string
    prefix?: string
    values?: Record<string, string>
}
// The distinct values a project's tasks hold, for suggestions (DEV-545).
export interface TaskValuesDTO {
    people: string[]
    tags: string[]
    custom_fields: Record<string, string[]>
}
export interface ProjectStatsDTO {
    name: string
    open_count: number
    done_count: number
    recent_modified?: string | null
    tags_top: string[]
}

// Task/sprint domain errors: invalid input/identity/safety = 400 INVALID_ARGUMENT,
// missing resource = 404 NOT_FOUND, storage/serialization/index = 500 INTERNAL.
/** A task comment; keys a newer server adds pass through unchanged. */
export interface TaskComment {
    date: string
    /** Who wrote it (the history's actor); absent on comments written before DEV-327. */
    author?: string
    /** The comment; question, answer and review comments keep their options, choice or verdict and findings here in prose too. */
    text: string
    /** DEV-488: the comment asks a question answered by choosing an option. */
    question?: CommentQuestion
    /** DEV-488: the comment answers the question asked at `answer.to`. */
    answer?: CommentAnswer
    /** DEV-489: the comment reports a review's verdict and findings. */
    review?: CommentReview
    /** A hand-off report (DEV-490); `text` stays the prose summary. */
    report?: CommentReport
    /** DEV-809: when the comment was deleted. It stays in the list so indexes don't shift; views leave it out and a deleted question is withdrawn. */
    deleted_at?: string
    [key: string]: unknown
}

/** DEV-488: a question answered by choosing an option. */
export interface CommentQuestion {
    options: QuestionOption[]
    /** Key of the recommended option. */
    recommended?: string
    /** Several options may be chosen; absent means false. */
    multiple?: boolean
    /** A free-text answer is allowed; absent means true. */
    other?: boolean
}

export interface QuestionOption {
    key: string
    label: string
    /** Fields choosing this option sets, as an automation `set:`. */
    set?: QuestionOptionSet
}

export interface QuestionOptionSet {
    status?: string
    assignee?: string
    reporter?: string
    priority?: string
    type?: string
    due_date?: string
    effort?: string
    /** Replaces the task's tags. */
    tags?: string[]
}

export interface CommentAnswer {
    /** The question comment's `date`. */
    to: string
    /** Chosen option keys; absent for a free-text answer. */
    choice?: string[]
    note?: string
}

/** DEV-489: a review's result; `pass` lets the work go on, `block` sends it back. */
export interface CommentReview {
    verdict: 'pass' | 'block'
    /** The commit reviewed. */
    commit?: string
    findings?: ReviewFinding[]
}

export interface ReviewFinding {
    summary: string
    file?: string
    /** Line in `file`, starting at 1. */
    line?: number
    /** Must be fixed before the work goes on; absent means false. */
    blocking?: boolean
}

/** `POST /api/tasks/comment` with `answer`: `to` defaults to the last open question. */
export interface CommentAnswerInput {
    to?: string
    choice?: string[]
    note?: string
}

/** A hand-off report: every part is optional. */
export interface CommentReport {
    commits?: ReportCommit[]
    checks?: ReportCheck[]
    /** The next step. */
    next?: string
}

export interface ReportCommit {
    hash: string
    /** What the commit is. */
    what?: string
}

export type ReportCheckResult = 'passed' | 'failed' | 'not_run'

export interface ReportCheck {
    command: string
    result: ReportCheckResult
    /** Why it failed or did not run. */
    reason?: string
}

/** `GET /api/config/show`: the scope's configuration as stored (its shape depends on the scope) plus computed completion fields. */
export type ConfigShowResponse = Record<string, unknown>

export interface ApiEnvelope<T> {
    data: T
    meta?: unknown
    error?: { code: string; message: string }
}

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

/** Inline instructions text, or a file reference; null/absent = built-in. */
export type AgentInstructionsValue = string | { file: string } | null | undefined

export interface AgentWorktreeConfig {
    enabled: boolean
    dir?: string | null
    /** Folder for the worktrees' build output: `target/` links to `<build_root>/<repo>/<ticket>` (DEV-762). */
    build_root?: string | null
    branch_prefix: string
    max_parallel_jobs?: number | null
    cleanup_on_done?: boolean
    cleanup_on_failure?: boolean
    cleanup_on_cancel?: boolean
    cleanup_delete_branches?: boolean
    /** Git-ignored files don't block cleanup (default true). */
    cleanup_ignored_files?: boolean
    /** Paths in the worktree deleted when a job ends, if git ignores them (build output). */
    cleanup_paths?: string[]
    /** Command, program first, run in a ticket's worktrees when a job leaves it waiting (DEV-712). */
    waiting_run?: string[]
}

/** `agent.memory_gate` (DEV-623, global only): new jobs wait while the machine is short of memory. */
export interface AgentMemoryGateConfig {
    /** Unset: on on macOS, off elsewhere. */
    enabled?: boolean
    max_swap_percent: number
    settle_seconds: number
}

/** `agent.disk_gate` (DEV-837, global only): new worktree jobs wait while free disk is under the floor. */
export interface AgentDiskGateConfig {
    /** 0: off. */
    min_free_gb: number
    idle_hours: number
}

/** `agent.job_history` (DEV-693, global only): how long job logs are kept. */
export interface AgentJobHistoryConfig {
    keep_days: number
    /** 0: no size limit. */
    max_mb: number
}

/** `agent.merge` (DEV-605): lotar merges a merge job's branch itself; absent = the merge agent merges. */
export interface AgentMergeConfig {
    checks?: string[]
    install?: string | null
    push?: boolean
    finding_check?: string | null
}

export interface SprintDefaultsConfig {
    capacity_points?: number | null
    capacity_hours?: number | null
    length?: string | null
    overdue_after?: string | null
}

/** Properties of one tag (DEV-20); unknown keys from newer servers are kept. */
export interface TagProperties {
    /** `#rgb`, `#rrggbb` or a named colour. */
    color?: string | null
    [key: string]: unknown
}

/** Properties of one custom field (DEV-373); unknown keys from newer servers are kept. */
/** Which custom fields the web UI offers (DEV-485). */
export type CustomFieldsFrom = 'config' | 'scanned' | 'both'

export interface CustomFieldProperties {
    /** `text` (the default) or `version`. */
    type?: CustomFieldTypeName | null
    [key: string]: unknown
}

export type CustomFieldTypeName = 'text' | 'version'

export interface ResolvedConfigDTO {
    /** Project-only named reviewers; empty keeps reporter-only review authorization. */
    review_reviewers: string[]
    server_port: number
    /** The web UI's public address (`server.public_url`), when set. */
    server_public_url?: string | null
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
    /** DEV-16: write the other side of relationships (default true). */
    auto_reciprocal_relationships?: boolean
    /** DEV-7: ticket IDs mentioned in descriptions and comments add related links (default true). */
    auto_link_mentions?: boolean
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
    /** Per-tag properties such as colours (DEV-20). */
    tag_properties?: Record<string, TagProperties>
    custom_field_properties?: Record<string, CustomFieldProperties>
    ui_custom_fields_from?: CustomFieldsFrom
    /** `config show` only: the field names the pickers offer, per `ui_custom_fields_from`. */
    custom_fields_offered?: string[]
    // Always sent by the server; optional so partial fixtures stay valid.
    members?: string[]
    strict_members?: boolean
    auto_populate_members?: boolean
    agent_context_enabled?: boolean
    agent_context_extension?: string
    agent_logs_dir?: string | null
    agent_instructions?: AgentInstructionsValue
    agent_worktree?: AgentWorktreeConfig
    agent_merge?: AgentMergeConfig | null
    agent_memory_gate?: AgentMemoryGateConfig
    agent_disk_gate?: AgentDiskGateConfig
    agent_job_history?: AgentJobHistoryConfig
    sprint_defaults?: SprintDefaultsConfig
    sprint_notifications?: { enabled: boolean }
    remotes?: Record<string, SyncRemoteConfig>
}

export interface GlobalConfigRaw {
    server_port: number
    /** The web UI's public address (`server.public_url`), when set. */
    server_public_url?: string | null
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
    /** DEV-16: write the other side of relationships (default true). */
    auto_reciprocal_relationships?: boolean
    /** DEV-7: ticket IDs mentioned in descriptions and comments add related links (default true). */
    auto_link_mentions?: boolean
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
    /** Per-tag properties such as colours (DEV-20). */
    tag_properties?: Record<string, TagProperties>
    custom_field_properties?: Record<string, CustomFieldProperties>
    ui_custom_fields_from?: CustomFieldsFrom
    /** Global-only external web bundle path; absent when unset. */
    web_ui_path?: string | null
    ui_theme?: string | null
    ui_name?: string | null
    ui_logo?: string | null
    ui_logo_dark?: string | null
    ui_icon?: string | null
    ui_theme_file?: string | null
    remotes?: Record<string, SyncRemoteConfig>
    auth_profiles?: Record<string, SyncAuthProfile>
}

export interface ProjectConfigRaw {
    /** Project-only reviewer literals; absent/null means reporter-only. */
    review_reviewers?: string[] | null
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
    members?: string[]
    strict_members?: boolean
    auto_populate_members?: boolean
    auto_set_reporter?: boolean
    /** DEV-16: write the other side of relationships (default true). */
    auto_reciprocal_relationships?: boolean
    /** DEV-7: ticket IDs mentioned in descriptions and comments add related links (default true). */
    auto_link_mentions?: boolean
    auto_assign_on_status?: boolean
    auto_codeowners_assign?: boolean
    auto_tags_from_path?: boolean
    auto_branch_infer_type?: boolean
    auto_branch_infer_status?: boolean
    auto_branch_infer_priority?: boolean
    auto_identity?: boolean
    auto_identity_git?: boolean
    agent_context_enabled?: boolean
    agent_context_extension?: string
    agent_logs_dir?: string
    agent_instructions?: AgentInstructionsValue
    agent_worktree?: Partial<AgentWorktreeConfig>
    agent_merge?: AgentMergeConfig | null
    scan_signal_words?: string[]
    scan_ticket_patterns?: string[]
    scan_enable_ticket_words?: boolean
    scan_enable_mentions?: boolean
    scan_strip_attributes?: boolean
    branch_type_aliases?: Record<string, string>
    branch_status_aliases?: Record<string, string>
    branch_priority_aliases?: Record<string, string>
    tag_properties?: Record<string, TagProperties> | null
    custom_field_properties?: Record<string, CustomFieldProperties> | null
    ui_custom_fields_from?: CustomFieldsFrom | null
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
    /** Relocate drifted anchors and remove dead ones within `paths`; reported in `info`. */
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
    status: 'created' | 'updated' | 'skipped' | 'failed' | (string & {})
    action: 'create' | 'refresh' | 'skip' | (string & {})
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
    reports: SyncReportMeta[] // Always present, including [] for an empty scope.
}

export interface SyncResponse {
    /** `ok`, or `partial` when some items failed. */
    status: 'ok' | 'partial' | (string & {})
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
    /** When the job was queued, for a job that waited in the queue (DEV-882). */
    queued_at?: string | null
    started_at?: string | null
    finished_at?: string | null
    exit_code?: number | null
    last_message?: string | null
    summary?: string | null
    session_id?: string | null
    worktree_path?: string | null
    worktree_branch?: string | null
    /** Model the run was asked to use, when one was set. */
    model?: string | null
    /** Reasoning effort the run was asked to use, when one was set. */
    effort?: string | null
    /** What the job used, as its agent reports it. */
    usage?: AgentJobUsage | null
    /** The agent's permission request that waits for an answer (profile `on_permission: ask`). */
    question?: AgentJobQuestion | null
}

/** A permission request of a job's agent, waiting for an answer. */
export interface AgentJobQuestion {
    request: string
    title: string
    options: AgentJobQuestionOption[]
}

export interface AgentJobQuestionOption {
    id: string
    name: string
    /** allow_once, allow_always, reject_once or reject_always. */
    kind: string
}

/** Usage an agent reports for a job; missing values were not reported. */
export interface AgentJobUsage {
    /** All input tokens, including cached ones. */
    input_tokens?: number
    cached_input_tokens?: number
    output_tokens?: number
    cost_usd?: number
    turns?: number
    premium_requests?: number
    lines_added?: number
    lines_removed?: number
    /** Tokens in the agent's context window, as it last reported. */
    context_used?: number
    /** The context window's size, as the agent last reported. */
    context_size?: number
}

export interface AgentJobCreateRequest {
    ticket_id: string
    prompt: string
    runner?: string
    agent?: string
    /** Overrides the profile's model for this run. */
    model?: string
    /** Overrides the profile's reasoning effort for this run. */
    effort?: string
    /** Overrides the profile's provider for this run (t3 runner). */
    provider?: string
    /** `continue` runs the job as the next turn of the ticket's last T3 Code thread; `fresh` starts a new thread on its branch when that holds unmerged work. */
    session?: 'new' | 'continue' | 'fresh'
    /** Overrides the profile's thread_name for this run (t3 runner): the thread's title, with the ticket's `${{...}}` templates filled in when the job starts. */
    thread_name?: string
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

export interface AgentQueuedTicket {
    ticket_id: string
    /** Agent profile; empty for a bare runner */
    agent: string
    runner?: string | null
    queued_at: string
    attempts: number
    not_before?: string | null
    /** release, merge, review, fix, another profile's name, or run for a bare runner (DEV-786) */
    kind: string
    /** The ticket's priority when the entry was queued (DEV-786) */
    priority?: string | null
    /** Moved by hand, or a paused job's retry: it keeps its place (DEV-786) */
    pinned: boolean
    /** A worker is starting its job: it can't be moved (DEV-786) */
    starting: boolean
    /** What the ticket waits for: a free slot, its ticket's running job, memory, a paused queue, a worker */
    waits_for: string
}

export interface AgentParkedTicket {
    ticket_id: string
    agent: string
    parked_at: string
    status?: string | null
}

export interface AgentQueueOverview {
    worker_state: 'active' | 'stopped' | 'idle'
    reason?: string | null
    /** In start order: priority, kind, arrival; moved entries keep their place (DEV-786) */
    pending: AgentQueuedTicket[]
    waiting_for_prerequisites: AgentParkedTicket[]
}

/** GET /api/status (DEV-881): this instance's jobs, queue and waiting tickets; every link absolute */
export interface StatusFeed {
    version: number
    instance: { name: string; url: string }
    generated_at: string
    running: StatusRunningJob[]
    queue: StatusQueue
    waiting: StatusWaitingTicket[]
}

export interface StatusRunningJob {
    job_id: string
    ticket_id: string
    title: string
    profile: string
    runner: string
    started_at: string
    job_url: string
    ticket_url: string
}

export interface StatusQueue {
    count: number
    held: boolean
    held_reason?: string | null
    entries: StatusQueueEntry[]
}

export interface StatusQueueEntry {
    ticket_id: string
    profile: string
    queued_at: string
    waits_for: string
    ticket_url: string
}

export interface StatusWaitingTicket {
    ticket_id: string
    title: string
    status: 'HelpNeeded' | 'Blocked'
    assignee?: string | null
    since: string
    ticket_url: string
}

export type AgentQueueMove = 'first' | 'up' | 'down' | 'last'

export interface AgentQueueMoveRequest {
    ticket_id: string
    /** The entry's profile, needed when the ticket is queued for several */
    agent?: string | null
    to: AgentQueueMove
}

export interface AgentQueueMoveResponse {
    entry: AgentQueuedTicket
    /** The entry's place in the start order, from 1 */
    position: number
    total: number
    /** The queue after the move, as GET /api/jobs lists it */
    queue: AgentQueueOverview
}

export interface AgentJobListResponse {
    jobs: AgentJob[]
    queue_stats?: AgentQueueStats | null
    /** The agent queue of every process (DEV-771) */
    queue?: AgentQueueOverview | null
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

/** One tool call of a job: an `agent_job_tool` entry at its start (`running`) and one at its end, same `id`. */
export interface AgentJobTool {
    id: string
    /** ACP tool kind: execute, read, edit, search, fetch, think, other, … */
    kind?: string
    title: string
    status: 'running' | 'completed' | 'failed'
    /** The input for people: the command, the path, the query. */
    input?: string
    /** The tool's arguments as JSON, cut at 1 KB; end entry only. */
    raw_input?: string
    paths?: string[]
    /** Text output, ANSI codes removed, cut at 4 KB. */
    output?: string
    /** The output's full size in bytes, only when `output` was cut. */
    output_bytes?: number
    started_at?: string
    finished_at?: string
}

/** One step of an agent's plan (its to-do list), on an `agent_job_plan` entry (DEV-774). */
export interface AgentJobPlanEntry {
    content: string
    /** pending, in_progress or completed. */
    status: string
    /** high, medium or low; empty when the agent gives none. */
    priority: string
}

export interface AgentJobLogEntry {
    kind: string
    /** Stable per-job identity: positive, starts at 1, strictly monotonic for the
     * job's lifetime, never reused after in-memory truncation. Identical to the
     * `sequence` on the corresponding SSE `agent_job_*` event. */
    sequence: number
    at: string
    message?: string | null
    /** The tool call of an `agent_job_tool` entry. */
    tool?: AgentJobTool | null
    /** The whole plan of an `agent_job_plan` entry; empty when the agent cleared it. */
    plan?: AgentJobPlanEntry[] | null
    /** The turn of an `agent_job_transcript` entry (DEV-909). */
    transcript?: AgentJobTranscriptRef | null
}

/** The turn a job sent to its harnessmux session, on an `agent_job_transcript` log entry (DEV-909). */
export interface AgentJobTranscriptRef {
    /** `harnessmux`. */
    source: string
    /** The harnessmux session name. */
    session: string
    /** The harnessmux agent the session runs. */
    agent: string
    /** The agent's own session id (ACP session id, T3 Code thread id). */
    agent_session?: string
    /** The turn number harnessmux gave the job's prompt. */
    turn: number
    /** The reference's format: 1. */
    format: number
    /** A T3 Code thread's link. */
    link?: string
}

/** `GET /api/jobs/transcript` (DEV-909). */
export interface AgentJobTranscriptResponse {
    transcript: AgentJobTranscript
}

/** The conversation of a job's turns, read from its harnessmux event log, a page at a time (DEV-909). */
export interface AgentJobTranscript {
    /** `expired`: the session's event log is gone; `unsupported`: no session named, another format, or no turn in the job. */
    state: 'ok' | 'expired' | 'unsupported'
    /** `reference`: the job log names the turns; `time_window`: an older log, the turns that started while the job ran. */
    source?: 'reference' | 'time_window'
    /** The harnessmux session name. */
    session?: string
    agent?: string
    /** The job's turns in the session. */
    turns?: number[]
    /** The session's turn count. */
    session_turns?: number
    /** The T3 Code thread's link of a t3 job. */
    link?: string
    /** Whether the turns hold tool calls: a t3 session written by harnessmux 0.2.0 has none. */
    tool_events: boolean
    /** The page, shaped like job log entries; `sequence` is the harnessmux `seq`. */
    events: AgentJobLogEntry[]
    /** The `after` for the next page; none on the last page. */
    next_after?: number
    /** Why the state isn't `ok`. */
    reason?: string
}

/** Everything that happened on a task, oldest first (DEV-518). */
export interface TaskTimeline {
    task_id: string
    entries: TaskTimelineEntry[]
}

export type TaskTimelineEntry =
    | { kind: 'change'; at: string; actor?: string | null; changes: TaskHistoryChange[] }
    | { kind: 'comment'; at: string; index: number; comment: TaskComment }
    | { kind: 'commit'; at: string; commit: string; author: string; email: string; message: string }
    | { kind: 'job'; at: string; job: AgentJob; thread?: AgentJobLogEntry[] | null }

export interface AgentJobLogsResponse {
    job: AgentJob
    events: AgentJobLogEntry[]
}

/** `GET /api/jobs/t3-thread` (DEV-849). */
export interface AgentJobT3ThreadResponse {
    thread: AgentJobT3Thread
}

/** A t3 job's T3 Code thread and its state now: T3 Code's state, not the job's or the ticket's. */
export interface AgentJobT3Thread {
    thread_id?: string
    /** The thread's web link in T3 Code, as the job recorded it. */
    link?: string
    environment_id?: string
    state: 'running' | 'waiting' | 'finished' | 'failed' | 'stopped' | 'unknown' | 'unavailable'
    /** The thread's status as T3 Code names it; when a milestone gives the state, the settled run's outcome, or none for a question. */
    t3_status?: string
    /** Why the state is unavailable. */
    reason?: string
    /** When T3 Code last changed the thread; when a milestone gives the state, when lotar logged it. */
    updated_at?: string
    /** When lotar asked T3 Code. */
    checked_at: string
}

/** `GET /api/jobs/inputs` (DEV-767): `null` when the job's log has no record. */
export interface AgentJobInputsResponse {
    inputs: AgentJobRunInputs | null
}

/** The prompt and settings a job's run started with; env, MCP env and headers are names only. */
export interface AgentJobRunInputs {
    at: string
    /** The first turn's prompt, exactly as the agent got it. */
    prompt: string
    /** The prompt's parts in order, covering all of it. */
    sections: AgentJobPromptSection[]
    instructions_source: AgentJobInstructionsSource
    settings: AgentJobRunSettings
    /** Environment variable names the agent's process got. */
    env: string[]
}

export interface AgentJobPromptSection {
    heading: string
    /** Offset in Unicode code points. */
    start: number
    /** Length in Unicode code points. */
    length: number
}

export interface AgentJobInstructionsSource {
    base?: AgentJobInstructionsOrigin
    profile?: AgentJobInstructionsOrigin
}

export interface AgentJobInstructionsOrigin {
    /** `builtin`, `inline` or `file`. */
    kind: string
    path?: string
}

export interface AgentJobRunSettings {
    runner: string
    agent_profile?: string
    harness_agent?: string
    model?: string
    effort?: string
    provider?: string
    permissions?: string
    on_permission?: string
    rules: AgentJobRunRules
    options?: Record<string, string>
    add_dirs?: string[]
    mcp_servers?: AgentJobMcpServer[]
    cwd?: string
    title?: string
    command?: string
    args?: string[]
    session?: AgentJobRunSession
}

export interface AgentJobRunRules {
    deny_commands?: string[]
    allow_tools?: string[]
    deny_tools?: string[]
    confine_writes: boolean
}

export interface AgentJobMcpServer {
    name: string
    command?: string
    args?: string[]
    url?: string
    /** Names only. */
    env?: string[]
    /** Names only. */
    headers?: string[]
    tools?: string[]
}

export interface AgentJobRunSession {
    name: string
    /** `new` or `resumed`. */
    mode: string
    /** The profile's `session`: `new`, `continue` or `fresh`. */
    configured?: string
}

export interface AgentJobMessageRequest {
    id: string
    message: string
}

export interface AgentJobAnswerRequest {
    id: string
    /** `AgentJob.question.request`. */
    request: string
    /** One of `AgentJob.question.options[].id`. */
    option: string
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
    /** Model the profile asks its CLI for, when set. */
    model?: string | null
    /** Reasoning effort the profile asks for, when set. */
    effort?: string | null
    /** Provider the profile runs its model on (t3 runner), when set. */
    provider?: string | null
    /** `read-only`, `workspace` or `full`. */
    permissions: string
}

export interface AgentProfilesResponse {
    profiles: AgentProfileInfo[]
    /** Agent CLIs found on the server's PATH (claude, codex, copilot, gemini). */
    installed_runners?: string[]
    /** Project the profiles were resolved for (the default project when none was asked for). */
    project?: string | null
}

/** Body of `POST /api/agents/profiles`. */
export interface AgentProfileCreateRequest {
    name: string
    runner: string
    project?: string
    global?: boolean
    model?: string
    effort?: string
    /** T3 Code provider instance for the t3 runner, such as `claudeAgent`. */
    provider?: string
    permissions?: string
}

// Automation simulation
export interface AutomationSimulateRequest {
    ticket_id: string
    event: string
    /** Accepted but ignored: scope follows the ticket's project prefix. */
    project?: string | null
    /** DEV-241: simulate this rule (as in automation.yml) instead of the saved ones. */
    rule?: Record<string, unknown>
}

/** DEV-241: run rules' manual actions (`on.manual`) on the tasks they match now. */
export interface AutomationRunRequest {
    /** Rule names; empty runs every rule with a manual action. */
    rules?: string[]
    /** Only tasks matching this query (task list search syntax). */
    query?: string
    dry_run?: boolean
}

export interface AutomationRunEntry {
    rule: string
    task: string
    actions: AutomationSimulatedAction[]
    warnings?: string[]
    error?: string
}

export interface AutomationRunResponse {
    dry_run: boolean
    rules: string[]
    checked: number
    entries: AutomationRunEntry[]
}

export interface AutomationSimulatedAction {
    action: string
    description: string
}

export interface AutomationSimulatedRule {
    /** The rule's name, or `Rule <position>` when it has none. */
    name: string
    /** What this rule would do, on top of the rules before it. */
    actions: AutomationSimulatedAction[]
    /** This rule's part of the response's warnings. */
    warnings: string[]
}

/**
 * The server always serializes all six keys (hand-built JSON):
 * `rules` lists every rule the event fires, in file order (empty when no
 * rule matched); `actions` holds all their actions in the same order;
 * `task_after` is `null` when no rule matched and otherwise shows the
 * ticket after all of them; `task_before` is always a task object; `warnings`
 * lists preview parts only decided when the rule runs, the fields a change
 * event has to change, and (when nothing matched) rules that need several
 * fields to change in the same update (may be empty).
 */
export interface AutomationSimulateResponse {
    matched: boolean
    rules: AutomationSimulatedRule[]
    actions: AutomationSimulatedAction[]
    task_before: TaskDTO
    task_after: TaskDTO | null
    warnings: string[]
}
