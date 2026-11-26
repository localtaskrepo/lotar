use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::config::types::SyncRemoteConfig;

/// Configuration endpoints serialize this shared resolved contract, including
/// project-only `review_reviewers` (empty for global resolution).
pub type ResolvedConfigDTO = crate::config::types::ResolvedConfig;

#[cfg(feature = "schema")]
use schemars::JsonSchema;

// Single lifecycle-visibility enum shared by the storage filter and the API
// list filter (DEV-92). Defined in the storage layer next to `TaskFilter`.
pub use crate::storage::filter::DeletionFilter;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum RelationshipKind {
    Dependency,
    Hierarchy,
    Related,
    Fixes,
    Duplicate,
}

/// Mutation vocabulary is relative to the addressed source task. Graph edge
/// kinds remain canonical and deliberately do not encode storage direction.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum RelationshipField {
    DependsOn,
    Blocks,
    Related,
    Fixes,
    Parent,
    Children,
    DuplicateOf,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum RelationshipOperation {
    Add,
    Remove,
    ChangeKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct RelationshipGraphRequest {
    pub project: Option<String>,
    pub task_id: Option<String>,
    pub depth: Option<usize>,
    pub max_nodes: Option<usize>,
    pub relationship_kinds: Option<Vec<RelationshipKind>>,
    pub include_isolated: Option<bool>,
    #[serde(default)]
    pub filters: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct RelationshipTaskSummary {
    pub id: String,
    pub title: String,
    pub status: crate::types::TaskStatus,
    pub priority: crate::types::Priority,
    pub task_type: crate::types::TaskType,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub task_state: Option<TaskStateDTO>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reporter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub assignee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub effort: Option<String>,
    pub tags: Vec<String>,
    pub sprints: Vec<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub due_date: Option<String>,
    pub created: String,
    pub modified: String,
    pub custom_fields: crate::types::CustomFields,
}

impl From<&TaskDTO> for RelationshipTaskSummary {
    fn from(task: &TaskDTO) -> Self {
        Self {
            id: task.id.clone(),
            title: task.title.clone(),
            status: task.status.clone(),
            priority: task.priority.clone(),
            task_type: task.task_type.clone(),
            task_state: task.task_state.clone(),
            reporter: task.reporter.clone(),
            assignee: task.assignee.clone(),
            effort: task.effort.clone(),
            tags: task.tags.clone(),
            sprints: task.sprints.clone(),
            due_date: task.due_date.clone(),
            created: task.created.clone(),
            modified: task.modified.clone(),
            custom_fields: task.custom_fields.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct RelationshipNode {
    pub id: String,
    pub title: Option<String>,
    pub status: Option<String>,
    pub project: String,
    /// active, external (read-only sibling workspace), deleted, missing,
    /// ambiguous, or unreadable. No filesystem paths are exposed.
    pub state: String,
    pub is_done: bool,
    pub matches_filter: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub task: Option<RelationshipTaskSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct RelationshipProvenance {
    pub task_id: String,
    pub field: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct RelationshipEdge {
    pub source: String,
    pub target: String,
    pub kind: RelationshipKind,
    pub provenance: Vec<RelationshipProvenance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct RelationshipIssue {
    pub id: String,
    pub code: String,
    pub severity: String,
    pub message: String,
    pub tasks: Vec<String>,
    pub repairable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct RelationshipGraphResponse {
    pub nodes: Vec<RelationshipNode>,
    pub edges: Vec<RelationshipEdge>,
    pub issues: Vec<RelationshipIssue>,
    pub revision: String,
    pub complete: bool,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct RelationshipCheckResponse {
    pub issues: Vec<RelationshipIssue>,
    pub revision: String,
    pub complete: bool,
}

/// A task in another repo's LoTaR (DEV-386), read now: its status, title
/// and link, or why it is unresolved.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct CrossRepoTaskDTO {
    /// `repo:ID` with the canonical ID; the request's value when unresolved.
    pub reference: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub status: Option<String>,
    /// Done by the other repo's done-state policy.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub done: Option<bool>,
    /// The other repo's web link (its `server.public_url`).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub url: Option<String>,
    /// Why the reference does not resolve now.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub unresolved: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct RelationshipMutationRequest {
    pub source: String,
    pub target: String,
    pub kind: RelationshipField,
    pub operation: RelationshipOperation,
    pub from_kind: Option<RelationshipField>,
    pub expected_current: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct RelationshipMutationResponse {
    pub changed: bool,
    pub changed_tasks: Vec<String>,
    pub revision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct RelationshipRepairRequest {
    pub project: Option<String>,
    #[serde(default)]
    pub issue_ids: Vec<String>,
    #[serde(default)]
    pub apply: bool,
    pub revision: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct RelationshipRepairResponse {
    pub applied: bool,
    pub changed_tasks: Vec<String>,
    pub revision: String,
    pub issues: Vec<RelationshipIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskDTO {
    pub id: String,
    /// The task's web link `<server.public_url>/task/<ID>` (DEV-332);
    /// omitted while the workspace sets no public URL.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub url: Option<String>,
    pub title: String,
    pub status: crate::types::TaskStatus,
    pub priority: crate::types::Priority,
    pub task_type: crate::types::TaskType,
    /// Soft-deletion tombstone timestamp (UTC RFC3339, DEV-92). Serialized
    /// only while the task is deleted; omitted for active tasks. Lifecycle
    /// transitions never touch `modified`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub deleted_at: Option<String>,
    /// The task is archived (DEV-4): its file is in the project's
    /// `@archive` folder and it is read-only. Serialized only when true.
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub archived: bool,
    /// Earlier IDs of a task moved between projects, oldest first (DEV-183).
    /// Requests may still use them; they resolve to this task.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub moved_from: Vec<String>,
    /// Server-computed runtime projection (DEV-21): completion flag, due
    /// bucket, and the server-local calendar day at build time. Computed
    /// from the task's own project configuration and the shared due
    /// parser; never persisted in the task YAML.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub task_state: Option<TaskStateDTO>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reporter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub assignee: Option<String>,
    pub created: String,
    pub modified: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub due_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub subtitle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
    // Always serialized, even when empty: the API contract and the web
    // types treat these collections as required (DEV-276).
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub relationships: crate::types::TaskRelationships,
    #[serde(default)]
    pub comments: Vec<crate::types::TaskComment>,
    #[serde(default)]
    pub references: Vec<crate::types::ReferenceEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub acceptance_criteria: Vec<String>,
    #[serde(default)]
    pub sprints: Vec<u32>,
    #[serde(skip_serializing_if = "crate::api_types::btreemap_u32_is_empty", default)]
    pub sprint_order: BTreeMap<u32, u32>,
    /// Releases the task belongs to (DEV-393).
    #[serde(default)]
    pub releases: Vec<u32>,
    #[serde(default)]
    pub history: Vec<crate::types::TaskChangeLogEntry>,
    #[serde(default)]
    pub custom_fields: crate::types::CustomFields,
}

// Helper to allow skipping empty maps for custom_fields
pub fn map_is_empty(map: &crate::types::CustomFields) -> bool {
    map.is_empty()
}

/// Due classification for [`TaskStateDTO`], matching the shared query
/// executor's due-bucket vocabulary. Only the `overdue` bucket excludes
/// terminal tasks; a task due today stays `today` even when done, and a
/// terminal past-due task has no bucket at all.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum TaskDueBucketDTO {
    Today,
    Soon,
    Later,
    Overdue,
}

/// Server-computed runtime task state (DEV-21), attached to every
/// `TaskDTO` produced by the actual task service (get/list/create/update/
/// bulk/reference and SSE payloads). All fields are derived from the
/// task's own project configuration, the shared due parser, and the
/// injected clock at DTO build time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskStateDTO {
    /// Whether the task's status is terminal under its project's
    /// effective `issue.done_states` policy.
    pub is_done: bool,
    /// The ordered EFFECTIVE completion policy actually used for THIS task
    /// (resolved from the task's actual storage root; explicit lists keep
    /// their configured order, inferred lists follow the inference steps).
    /// Lets clients classify optimistic status changes for homonymous or
    /// foreign-root tasks without borrowing a prefix-level config policy
    /// that may belong to a different root. Never persisted in task YAML.
    pub done_states: Vec<String>,
    /// Due bucket from the shared due parser and server-local calendar
    /// day; `null` when the task has no (parseable) due date or when a
    /// terminal task is past due. Always serialized (the declared
    /// contract marks it required-nullable).
    pub due_bucket: Option<TaskDueBucketDTO>,
    /// Server-local calendar day (`YYYY-MM-DD`) when this snapshot was
    /// computed, so clients can detect day rollover.
    pub calendar_day: String,
}

pub fn btreemap_u32_is_empty(map: &BTreeMap<u32, u32>) -> bool {
    map.is_empty()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct TaskOrderRequest {
    pub task: String,
    #[serde(default)]
    pub sprint: Option<u32>,
    #[serde(default)]
    pub before: Option<String>,
    #[serde(default)]
    pub after: Option<String>,
    #[serde(default)]
    pub allow_closed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskOrderResponse {
    pub status: String,
    pub tasks: Vec<String>,
    pub changed: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub sprint_id: Option<u32>,
}

fn deserialize_double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    let inner = Option::<T>::deserialize(deserializer)?;
    Ok(Some(inner))
}

/// Creation payload shared by REST, MCP, and UI clients.
///
/// Enum values (`status`, `priority`, `task_type`) are raw strings validated
/// and canonicalized by the service against the target project's resolved
/// configuration, so callers must not pre-validate against global config.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskCreate {
    pub title: String,
    pub project: Option<String>,
    /// Initial status; validated against the target project's config. When
    /// omitted, branch inference or the project default applies atomically.
    pub status: Option<String>,
    pub priority: Option<String>,
    pub task_type: Option<String>,
    pub reporter: Option<String>,
    pub assignee: Option<String>,
    pub due_date: Option<String>,
    pub effort: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub relationships: Option<crate::types::TaskRelationships>,
    /// Object form wins per key over legacy `fields` key/value payloads.
    pub custom_fields: Option<crate::types::CustomFields>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub sprints: Vec<u32>,
    /// Release ids to add the new task to (DEV-393).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub releases: Vec<u32>,
}

/// A status and/or assignee change made while an agent job runs on the
/// ticket (DEV-320). It is not applied now: the job's end applies it in
/// place of the `set` of the `job_completed`, `job_failed` or
/// `job_cancelled` rules. An empty `assignee` clears the assignee.
/// `cleared` names fields whose recorded change was dropped because they
/// were set back to the ticket's current value (DEV-340); the job's rules
/// decide them again.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct JobOutcome {
    pub job_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cleared: Vec<JobOutcomeField>,
}

/// A field a job's outcome can hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum JobOutcomeField {
    Status,
    Assignee,
}

impl JobOutcomeField {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Assignee => "assignee",
        }
    }
}

impl JobOutcome {
    /// One line per field, for command output: "HelpNeeded will be set when
    /// job <id> ends".
    pub fn describe(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(status) = self.status.as_deref() {
            lines.push(format!("{status} will be set when job {} ends", self.job_id));
        }
        match self.assignee.as_deref() {
            Some("") => lines.push(format!("The assignee will be cleared when job {} ends", self.job_id)),
            Some(assignee) => lines.push(format!("{assignee} will be assigned when job {} ends", self.job_id)),
            None => {}
        }
        for field in &self.cleared {
            lines.push(format!("deferred {} cleared; the job's rules decide when job {} ends", field.as_str(), self.job_id));
        }
        lines
    }

    pub fn is_empty(&self) -> bool {
        self.status.is_none() && self.assignee.is_none() && self.cleared.is_empty()
    }
}

/// How a finding report was handled (DEV-317).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum FindingState {
    /// No ticket had the key; one was created.
    New,
    /// The newest ticket with the key is done; a new one was created.
    Recurred,
    /// An open ticket has the key; a report with a new description was
    /// added to it as a comment, one without only updated `finding_seen`.
    Open,
    /// The newest ticket with the key was closed as won't do; nothing changed.
    Suppressed,
    /// No open ticket had the key; it was added to the open bundle ticket of
    /// the day (`lotar add --bundle`, DEV-800).
    Bundled,
}

/// What a finding report did: the `finding` object of `/api/tasks/add`,
/// MCP `task_create` and `lotar add --finding --format json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct FindingReport {
    pub created: bool,
    pub state: FindingState,
    /// The new ticket, or the existing one that already reports the finding.
    pub id: String,
    /// An open ticket got this report as a comment; `false` when the report
    /// said nothing new and only `finding_seen` was updated (DEV-321). Not
    /// part of the wire format.
    #[serde(skip)]
    pub commented: bool,
}

impl FindingReport {
    /// One line naming the ticket, for text output.
    pub fn describe(&self, key: &str) -> String {
        match self.state {
            FindingState::New => format!("Created {} for finding {key}", self.id),
            FindingState::Recurred => format!("Created {} for finding {key}; it was fixed before and came back", self.id),
            FindingState::Open if self.commented => format!("{} already reports finding {key}; added this report as a comment", self.id),
            FindingState::Open => format!("{} already reports finding {key}; seen again with nothing new, no comment added", self.id),
            FindingState::Suppressed => format!("{} reported finding {key} and was closed as won't do; nothing created", self.id),
            FindingState::Bundled => format!("Added finding {key} to {}, today's bundle", self.id),
        }
    }
}

/// Patch payload shared by REST, MCP, and UI clients.
///
/// Field presence is tri-state on the wire: omitted = no-op, `null` = clear,
/// value = set. Internally `None` means no-op and the empty sentinel (empty
/// string for scalars, empty collection/map for lists) means clear, matching
/// the historical empty-clears behavior. Transport adapters translate `null`
/// to the empty sentinel. `title`, `status`, `priority`, and `task_type`
/// cannot be cleared: `null` is treated as omitted. Enum strings are
/// validated and canonicalized by the service against the task's project
/// configuration. `tags`, `acceptance_criteria`, `relationships`,
/// `custom_fields`, and `sprints` replace the whole value when set.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskUpdate {
    pub title: Option<String>,
    pub status: Option<String>,
    pub priority: Option<String>,
    pub task_type: Option<String>,
    /// Empty string clears.
    pub reporter: Option<String>,
    /// Empty string clears.
    pub assignee: Option<String>,
    /// Empty string clears.
    pub due_date: Option<String>,
    /// Empty string clears.
    pub effort: Option<String>,
    /// Empty string clears.
    pub description: Option<String>,
    /// Replace whole list; empty list clears.
    pub tags: Option<Vec<String>>,
    /// Replace whole list; empty list clears.
    pub acceptance_criteria: Option<Vec<String>>,
    /// Replace whole map; empty map clears.
    pub relationships: Option<crate::types::TaskRelationships>,
    /// Replace whole map; empty map clears all fields.
    pub custom_fields: Option<crate::types::CustomFields>,
    /// Replace sprint memberships; empty list clears them.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub sprints: Option<Vec<u32>>,
    /// Replace release memberships; empty list clears them (DEV-393).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub releases: Option<Vec<u32>>,
}

/// Move a task to another project (DEV-183). The task gets the next free
/// number there; its old ID keeps working through a redirect. Values that the
/// target project does not allow fall back to its defaults unless set here.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskMoveRequest {
    /// The task to move (an old ID of an already moved task works too).
    pub id: String,
    /// Prefix of the project to move it to.
    pub project: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub task_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub priority: Option<String>,
    /// Empty string clears.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reporter: Option<String>,
    /// Empty string clears.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub assignee: Option<String>,
    /// Custom field values to set; `null` or an empty string removes one.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub custom_fields: Option<crate::types::CustomFields>,
    /// Report what would happen without changing anything.
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub dry_run: bool,
}

/// One value the move changed to fit the target project.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskMoveChange {
    pub field: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub old: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub new: Option<String>,
}

/// Result of a task move (DEV-183). For a dry run, `task` shows the task as
/// it would look after the move and nothing was written.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskMoveResponse {
    /// The ID the task had (after following earlier moves).
    pub from: String,
    /// Its new ID.
    pub to: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub changes: Vec<TaskMoveChange>,
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub dry_run: bool,
    pub task: TaskDTO,
}

/// What a job used, as far as its agent reports it over ACP: tokens, and the
/// cost where the agent reports one (Claude). Missing values were not
/// reported. Premium requests and changed lines are not reported over ACP;
/// they come from jobs before DEV-503 (Copilot's CLI).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobUsage {
    /// All input tokens, including cached ones.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    /// Input tokens read from the prompt cache.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    /// Cost in US dollars as computed by the CLI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    /// The job's turns in its session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turns: Option<u64>,
    /// GitHub Copilot premium requests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub premium_requests: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines_added: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines_removed: Option<u64>,
    /// Tokens in the agent's context window, as it last reported (DEV-766): the latest replaces the earlier one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_used: Option<u64>,
    /// The size of the agent's context window, as it last reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_size: Option<u64>,
}

impl AgentJobUsage {
    /// Add another report (Codex reports usage per turn).
    pub fn add(&mut self, other: &AgentJobUsage) {
        fn sum<T: Copy + std::ops::Add<Output = T>>(a: Option<T>, b: Option<T>) -> Option<T> {
            match (a, b) {
                (Some(a), Some(b)) => Some(a + b),
                (a, b) => a.or(b),
            }
        }
        self.input_tokens = sum(self.input_tokens, other.input_tokens);
        self.cached_input_tokens = sum(self.cached_input_tokens, other.cached_input_tokens);
        self.output_tokens = sum(self.output_tokens, other.output_tokens);
        self.cost_usd = sum(self.cost_usd, other.cost_usd);
        self.turns = sum(self.turns, other.turns);
        self.premium_requests = sum(self.premium_requests, other.premium_requests);
        self.lines_added = sum(self.lines_added, other.lines_added);
        self.lines_removed = sum(self.lines_removed, other.lines_removed);
        self.context_used = other.context_used.or(self.context_used);
        self.context_size = other.context_size.or(self.context_size);
    }

    /// One line for people: "106,028 tokens in (93,837 cached), 588 out · $0.0040 · 4 turns".
    pub fn describe(&self) -> String {
        let number = |n: u64| {
            let digits = n.to_string();
            let mut out = String::new();
            for (i, ch) in digits.chars().enumerate() {
                if i > 0 && (digits.len() - i).is_multiple_of(3) {
                    out.push(',');
                }
                out.push(ch);
            }
            out
        };
        let mut parts = Vec::new();
        match (self.input_tokens, self.output_tokens) {
            (None, None) => {}
            (input, output) => {
                let mut tokens = String::new();
                if let Some(input) = input {
                    tokens.push_str(&format!("{} tokens in", number(input)));
                    if let Some(cached) = self.cached_input_tokens.filter(|c| *c > 0) {
                        tokens.push_str(&format!(" ({} cached)", number(cached)));
                    }
                }
                if let Some(output) = output {
                    if !tokens.is_empty() {
                        tokens.push_str(", ");
                    }
                    tokens.push_str(&format!("{} out", number(output)));
                }
                parts.push(tokens);
            }
        }
        if let Some(cost) = self.cost_usd {
            parts.push(format!("${cost:.4}"));
        }
        if let Some(turns) = self.turns {
            parts.push(format!("{turns} {}", if turns == 1 { "turn" } else { "turns" }));
        }
        if let Some(requests) = self.premium_requests {
            parts.push(format!("{requests} premium {}", if requests == 1.0 { "request" } else { "requests" }));
        }
        if self.lines_added.is_some() || self.lines_removed.is_some() {
            parts.push(format!("+{} −{} lines", self.lines_added.unwrap_or(0), self.lines_removed.unwrap_or(0)));
        }
        parts.join(" · ")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJob {
    pub id: String,
    pub ticket_id: String,
    pub runner: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub agent: Option<String>,
    pub status: String,
    pub created_at: String,
    /// When the job was queued, for a job that waited in the queue (DEV-882).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub queued_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub finished_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub last_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub worktree_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub worktree_branch: Option<String>,
    /// Model the run was asked to use, when one was set.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub model: Option<String>,
    /// Reasoning effort the run was asked to use, when one was set.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub effort: Option<String>,
    /// Tokens, cost and similar as reported by the agent, once known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<AgentJobUsage>,
    /// The agent's permission request that waits for an answer (profile
    /// `on_permission: ask`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<AgentJobQuestion>,
}

/// The T3 Code thread of a `t3` job and its state as T3 Code reports it now (DEV-849). This is
/// T3 Code's state, separate from the job's outcome and the ticket's status, and reading it changes
/// neither.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobT3Thread {
    /// The thread the job ran in, as its log records it; none for a job that recorded no thread.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    /// The thread's web link in T3 Code, as the job recorded it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    /// The T3 Code environment in the recorded link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment_id: Option<String>,
    /// running, waiting (for an answer), finished, failed, stopped, unknown (a status lotar doesn't
    /// know) or unavailable (no thread recorded, T3 Code doesn't answer, or the thread is gone).
    pub state: String,
    /// The thread's status as T3 Code names it (`running`, `completed`, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub t3_status: Option<String>,
    /// Why the state is unavailable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// When T3 Code last changed the thread.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// When lotar asked T3 Code (a cached answer keeps the time it was asked).
    pub checked_at: String,
}

/// One tool call of an agent job (DEV-766): an `agent_job_tool` log entry at its start (`running`) and one at its end
/// (`completed` or `failed`), with the same `id`. The end entry holds everything known about the call.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobTool {
    /// The agent's tool call id.
    pub id: String,
    /// ACP tool kind: execute, read, edit, search, fetch, think, other, …
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub title: String,
    /// running, completed or failed.
    pub status: String,
    /// The input for people: the command, the path, the query.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    /// The tool's arguments as JSON, cut at 1 KB; on the end entry only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_input: Option<String>,
    /// Files the call works on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
    /// The tool's text output, ANSI codes removed, cut at 4 KB.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// The output's full size in bytes, only when `output` was cut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
}

/// One step of an agent's plan (its to-do list), on an `agent_job_plan` log entry (DEV-774).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobPlanEntry {
    pub content: String,
    /// pending, in_progress or completed.
    #[serde(default)]
    pub status: String,
    /// high, medium or low; empty when the agent gives none.
    #[serde(default)]
    pub priority: String,
}

/// One entry of a job's log; `sequence` numbers a job's events from 1.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobEvent {
    pub kind: String,
    pub sequence: u64,
    pub at: String,
    pub message: Option<String>,
    /// The tool call of an `agent_job_tool` entry.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tool: Option<AgentJobTool>,
    /// The whole plan of an `agent_job_plan` entry; empty when the agent cleared it.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub plan: Option<Vec<AgentJobPlanEntry>>,
}

/// Everything that happened on a task, oldest first (DEV-518): field changes, comments, commits of the task
/// file and agent jobs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskTimeline {
    pub task_id: String,
    pub entries: Vec<TaskTimelineEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskTimelineEntry {
    /// One history entry: what an actor changed at once.
    Change {
        at: String,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        actor: Option<String>,
        changes: Vec<crate::types::TaskChange>,
    },
    Comment {
        at: String,
        /// The comment's 0-based index on the task.
        index: usize,
        comment: crate::types::TaskComment,
    },
    /// A commit that changed the task's file.
    Commit { at: String, commit: String, author: String, email: String, message: String },
    /// An agent job on the task, at the time it was asked for.
    Job {
        at: String,
        job: AgentJob,
        /// The job's whole thread, only when asked for.
        #[serde(skip_serializing_if = "Option::is_none", default)]
        thread: Option<Vec<AgentJobEvent>>,
    },
}

impl TaskTimelineEntry {
    pub fn at(&self) -> &str {
        match self {
            Self::Change { at, .. } | Self::Comment { at, .. } | Self::Commit { at, .. } | Self::Job { at, .. } => at,
        }
    }
}

/// A permission request of a job's agent, waiting for an answer (DEV-503).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobQuestion {
    pub request: String,
    pub title: String,
    pub options: Vec<AgentJobQuestionOption>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobQuestionOption {
    pub id: String,
    pub name: String,
    /// allow_once, allow_always, reject_once or reject_always.
    pub kind: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobCreateRequest {
    pub ticket_id: String,
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub runner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub agent: Option<String>,
    /// Overrides the profile's model for this run.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub model: Option<String>,
    /// Overrides the profile's reasoning effort for this run.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub effort: Option<String>,
    /// Overrides the profile's provider for this run (t3 runner).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub provider: Option<String>,
    /// Overrides the profile's session for this run: `continue` runs it in
    /// the ticket's last T3 Code thread, `fresh` in a new thread also when
    /// that thread has unmerged work.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub session: Option<crate::config::types::AgentSession>,
    /// Overrides the profile's `thread_name` for this run (t3 runner): the
    /// thread's title, with the ticket's `${{...}}` templates filled in when
    /// the job starts.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub thread_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobCreateResponse {
    pub job: AgentJob,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobStatusResponse {
    pub job: AgentJob,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobListResponse {
    pub jobs: Vec<AgentJob>,
    /// Queue statistics
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub queue_stats: Option<AgentQueueStats>,
    /// The tickets waiting in the agent queue of every process (DEV-771)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub queue: Option<AgentQueueOverview>,
}

/// The agent queue as `lotar agent queue` shows it (DEV-771).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentQueueOverview {
    /// `active` (a worker process runs), `stopped` (tickets wait, no worker runs) or `idle`
    pub worker_state: String,
    /// What the waiting tickets wait for, with tickets queued
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reason: Option<String>,
    /// The waiting tickets, in queue order
    pub pending: Vec<AgentQueuedTicket>,
    /// The jobs waiting for their tickets' prerequisites
    pub waiting_for_prerequisites: Vec<AgentParkedTicket>,
}

/// A ticket waiting in the agent queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentQueuedTicket {
    pub ticket_id: String,
    /// Agent profile; empty for a bare runner
    pub agent: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub runner: Option<String>,
    pub queued_at: String,
    /// Failed starts so far
    pub attempts: u32,
    /// The entry starts no earlier than this
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub not_before: Option<String>,
    /// What the ticket waits for: a free slot, its ticket's running job, memory, a paused queue, a worker
    pub waits_for: String,
}

/// A job waiting for its ticket's prerequisites.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentParkedTicket {
    pub ticket_id: String,
    /// Agent profile; empty for a bare runner
    pub agent: String,
    pub parked_at: String,
    /// The ticket's status before it was set to Blocked
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobCancelRequest {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobCancelResponse {
    pub cancelled: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub job: Option<AgentJob>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobCancelAllResponse {
    pub cancelled: usize,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub jobs: Vec<AgentJob>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AutomationInspectResponse {
    pub scope: String,
    pub source: String,
    pub scope_exists: bool,
    pub scope_yaml: String,
    pub effective_yaml: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AutomationSetRequest {
    pub yaml: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub project: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AutomationSetResponse {
    pub updated: bool,
    pub warnings: Vec<String>,
    pub info: Vec<String>,
    pub errors: Vec<String>,
}

/// Run rules' manual actions (`on.manual`) on the tasks they match now
/// (DEV-241). This is how automations run on a schedule: from cron.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AutomationRunRequest {
    /// Names of the rules to run; empty runs every rule with a manual action.
    #[serde(default)]
    pub rules: Vec<String>,
    /// Only consider tasks matching this query (search syntax of the task list).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    /// Report what would change without changing anything.
    #[serde(default)]
    pub dry_run: bool,
}

/// One rule acting on one task in a manual run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AutomationRunEntry {
    pub rule: String,
    pub task: String,
    pub actions: Vec<AutomationSimulatedAction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AutomationRunResponse {
    pub dry_run: bool,
    /// Enabled rules with a manual action that were considered.
    pub rules: Vec<String>,
    /// How many tasks were checked.
    pub checked: usize,
    pub entries: Vec<AutomationRunEntry>,
}

/// Request to simulate automation rules for a given event
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AutomationSimulateRequest {
    /// The task ID to simulate against
    pub ticket_id: String,
    /// The event to simulate. Canonical names: "created", "updated",
    /// "assigned", "commented", "sprint_changed", "release_changed", "job_started",
    /// "job_completed", "job_failed", "job_cancelled", "job_paused", "manual",
    /// "unblocked". Matching
    /// is case-insensitive and legacy aliases are accepted ("job_start",
    /// "complete", "success", "error", "failure", "cancel", "pause").
    pub event: String,
    /// Optional in the payload but ignored: the automation scope always
    /// follows the ticket's project prefix.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub project: Option<String>,
    /// Simulate this rule (an automation rule object, as in automation.yml)
    /// instead of the saved ones, e.g. while it is still being edited
    /// (DEV-241). Its conditions are checked against the task as it is.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub rule: Option<serde_json::Value>,
}

/// Response showing what would happen when automation rules are applied
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AutomationSimulateResponse {
    /// Whether any rules matched
    pub matched: bool,
    /// Every rule the event fires, in file order (DEV-477)
    pub rules: Vec<AutomationSimulatedRule>,
    /// Actions of all those rules, in the same order
    pub actions: Vec<AutomationSimulatedAction>,
    /// Current task state (before automation)
    pub task_before: Option<TaskDTO>,
    /// Simulated task state (after automation)
    pub task_after: Option<TaskDTO>,
    /// Parts of the preview that are only decided when the rule really runs
    /// (event-only template values, runtime assignee tokens, sprint changes),
    /// the fields a change event has to change for the rule to fire, and,
    /// when nothing matched, rules that only fire when several fields change
    /// in the same update.
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// A single action that would be performed by automation
/// One rule a simulated event fires
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AutomationSimulatedRule {
    /// The rule's name, or `Rule <position>` when it has none
    pub name: String,
    /// What this rule would do, on top of the rules before it
    pub actions: Vec<AutomationSimulatedAction>,
    /// This rule's part of the response's warnings
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AutomationSimulatedAction {
    /// Action type: "set_status", "set_assignee", "add_tag", etc.
    pub action: String,
    /// Description of what would change
    pub description: String,
}

/// Queue statistics for agent jobs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentQueueStats {
    /// Number of jobs currently running
    pub running: usize,
    /// Number of jobs waiting in queue
    pub queued: usize,
    /// Maximum parallel jobs (None = unlimited)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub max_parallel: Option<usize>,
}

/// `GET /api/status` (DEV-881): this instance's running jobs, its agent queue
/// and the tickets that wait for someone, for a tool that shows several
/// instances side by side. Every link is absolute.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct StatusFeed {
    /// Format version; grows when fields are added, changes meaning never.
    pub version: u32,
    pub instance: StatusInstance,
    /// When this document was made (RFC 3339).
    pub generated_at: String,
    /// The jobs running now, oldest first.
    pub running: Vec<StatusRunningJob>,
    pub queue: StatusQueue,
    /// Tickets HelpNeeded or Blocked, longest waiting first.
    pub waiting: Vec<StatusWaitingTicket>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct StatusInstance {
    /// `ui.name`, else the repository's folder name.
    pub name: String,
    /// The web UI's address: `server.public_url`, else the address the request was sent to.
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct StatusRunningJob {
    pub job_id: String,
    pub ticket_id: String,
    /// The ticket's title; empty when the ticket can't be read.
    pub title: String,
    /// The agent profile (fix, review, merge, ...); empty for a bare runner.
    pub profile: String,
    pub runner: String,
    pub started_at: String,
    /// The job's log in this instance's UI.
    pub job_url: String,
    pub ticket_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct StatusQueue {
    /// Number of entries.
    pub count: usize,
    /// The queue starts no new job: memory is short, or the queue is paused.
    pub held: bool,
    /// Why it is held.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub held_reason: Option<String>,
    /// The waiting entries in start order, as `lotar agent queue` lists them.
    pub entries: Vec<StatusQueueEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct StatusQueueEntry {
    pub ticket_id: String,
    /// The agent profile, the kind of job (fix, review, merge, ...); empty for a bare runner.
    pub profile: String,
    pub queued_at: String,
    /// What the entry waits for.
    pub waits_for: String,
    pub ticket_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct StatusWaitingTicket {
    pub ticket_id: String,
    pub title: String,
    /// `HelpNeeded` or `Blocked`.
    pub status: String,
    /// Who it waits on.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub assignee: Option<String>,
    /// When it got that status: the date of its question (RFC 3339).
    pub since: String,
    pub ticket_url: String,
}

/// List of available agent profiles
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentProfilesResponse {
    pub profiles: Vec<AgentProfileInfo>,
}

/// Basic info about an agent profile
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentProfileInfo {
    pub name: String,
    pub runner: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
    /// Model the profile asks its CLI for, when set.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub model: Option<String>,
    /// Reasoning effort the profile asks for, when set.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub effort: Option<String>,
    /// Provider the profile runs its model on (t3 runner), when set.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub provider: Option<String>,
    /// `read-only`, `workspace` or `full`.
    pub permissions: String,
}

/// Body of `POST /api/agents/profiles`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentProfileCreateRequest {
    /// Profile name; tickets assigned to `@name` start a job with it.
    pub name: String,
    /// A runner name such as `claude`, `codex` or `copilot` (see the agent docs).
    pub runner: String,
    /// Project prefix to add it to; defaults to the default project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// Add it to the global config (all projects) instead.
    #[serde(default)]
    pub global: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    /// T3 Code provider instance for the t3 runner, such as `claudeAgent`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// `read-only`, `workspace` (default) or `full`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobLogEntry {
    pub kind: String,
    /// Stable per-job event identity: positive, starts at 1, strictly
    /// monotonic for the job's lifetime, and never reused after in-memory
    /// log truncation. Matches the `sequence` sent on SSE `agent_job_*`
    /// events for lossless REST/SSE cache joins.
    pub sequence: u64,
    pub at: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub message: Option<String>,
    /// The tool call of an `agent_job_tool` entry (DEV-766).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tool: Option<AgentJobTool>,
    /// The whole plan of an `agent_job_plan` entry (DEV-774); empty when the agent cleared it.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub plan: Option<Vec<AgentJobPlanEntry>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobLogsResponse {
    pub job: AgentJob,
    pub events: Vec<AgentJobLogEntry>,
}

/// `GET /api/jobs/inputs`: what the job's run went in with, `null` when its log has no record (jobs from before
/// DEV-767, logging off, a commit retry that didn't run the agent).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobInputsResponse {
    pub inputs: Option<AgentJobRunInputs>,
}

/// `GET /api/jobs/t3-thread` (DEV-849).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobT3ThreadResponse {
    pub thread: AgentJobT3Thread,
}

/// The prompt and settings a job's run started with (DEV-767), written to its log as the first turn goes out.
/// Secrets stay out: environment variables, MCP server env and HTTP headers are listed by name only.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobRunInputs {
    pub at: String,
    /// The first turn's prompt, exactly as the agent got it.
    pub prompt: String,
    /// The prompt's parts in order, covering all of it.
    pub sections: Vec<AgentJobPromptSection>,
    pub instructions_source: AgentJobInstructionsSource,
    pub settings: AgentJobRunSettings,
    /// The environment variables the agent's process got on top of the inherited ones: names only.
    pub env: Vec<String>,
}

/// One part of a job's prompt, from its heading line.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobPromptSection {
    pub heading: String,
    /// Offset in the prompt, in Unicode code points.
    pub start: usize,
    /// Length in Unicode code points.
    pub length: usize,
}

/// Where the prompt's agent instructions came from; `None` when there were none.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobInstructionsSource {
    /// `agent.instructions`, or lotar's built-in text when it is unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<AgentJobInstructionsOrigin>,
    /// The profile's own `instructions`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<AgentJobInstructionsOrigin>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobInstructionsOrigin {
    /// `builtin` (lotar's docs/help/agent-instructions.md), `inline` (text in the config) or `file`.
    pub kind: String,
    /// The file `builtin` and `file` instructions were read from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// The settings a job's session (or `command` runner) started with.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobRunSettings {
    pub runner: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_profile: Option<String>,
    /// The harnessmux agent the session runs; `None` for the `command` runner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// `read-only`, `workspace` or `full`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<String>,
    /// `deny`, `allow-reads`, `allow` or `ask`: what happens when the agent asks for a permission.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_permission: Option<String>,
    #[serde(default)]
    pub rules: AgentJobRunRules,
    /// The agent's own ACP config options.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, String>,
    /// Extra directories the agent may write to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub add_dirs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<AgentJobMcpServer>,
    /// The directory the agent runs in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// The T3 Code thread's title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The profile's `command` and `args`, as configured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<AgentJobRunSession>,
}

/// The permission rules a session ran with, decided before `on_permission`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobRunRules {
    /// Commands refused, such as `git push`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deny_commands: Vec<String>,
    /// Tools allowed without asking.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow_tools: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deny_tools: Vec<String>,
    /// Edits outside the working directory and `add_dirs` are refused.
    #[serde(default)]
    pub confine_writes: bool,
}

/// An MCP server the session got; `env` and `headers` are names only.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobMcpServer {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<String>,
    /// The tools the server offers the job, when lotar limits them (its own `lotar` server).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
}

/// The harnessmux session a job ran in.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobRunSession {
    pub name: String,
    /// `new` or `resumed` (the next turn of an existing session).
    pub mode: String,
    /// The profile's `session`: `new`, `continue` or `fresh`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configured: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobMessageRequest {
    pub id: String,
    pub message: String,
}

/// Answers the job's open question (`AgentJob.question`) with one of its options.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobAnswerRequest {
    pub id: String,
    /// `AgentJob.question.request`.
    pub request: String,
    /// One of `AgentJob.question.options[].id`.
    pub option: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobMessageResponse {
    pub accepted: bool,
    pub job: AgentJob,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AttachmentUploadRequest {
    pub id: String,
    pub filename: String,
    pub content_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AttachmentUploadResponse {
    pub stored_path: String,
    pub attached: bool,
    pub task: TaskDTO,
    /// Camera, location and other personal metadata was removed from the
    /// image before it was stored (DEV-216).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub metadata_removed: bool,
    /// Problems worth telling the uploader about, e.g. an image extension on
    /// a file that is not an image. The upload still succeeded.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// Repository branding for the web UI (DEV-218).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct BrandingResponse {
    /// Default theme id for everyone (`ui.theme`); absent = built-in default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Where `theme` comes from: `home` (the person's ~/.lotar, which wins)
    /// or `repository` (.tasks/config.yml). Absent with the built-in default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_source: Option<String>,
    /// The repository's own `ui.theme` when ~/.lotar overrides it, so the
    /// Config page can say so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_theme: Option<String>,
    /// The repository skin's path (`ui.theme_file`) when it is usable; the
    /// theme `custom` then shows it (CSS at /api/branding/theme.css).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_file: Option<String>,
    /// Name shown next to the logo and in the browser tab (`ui.name`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The repository's folder name; the browser tab shows it without `name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    /// URL of the logo, when one is configured and can be served.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo: Option<String>,
    /// URL of the logo for dark mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo_dark: Option<String>,
    /// URL of the app icon (browser tab icon).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Without `ui.icon`: the repository's own icon file that `icon` shows
    /// (e.g. `public/favicon.svg`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detected_icon: Option<String>,
    /// Why configured images are not shown (missing file, not an image…).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<String>,
    /// A commit's web page without the hash, from the first git remote on
    /// a known host; report comments link their commits with it (DEV-490).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct BrandingUploadResponse {
    /// Path of the stored image, relative to the repository (for `ui.logo` etc.).
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct BrandingUploadRequest {
    /// `logo`, `logo-dark` or `icon`.
    pub image: String,
    pub filename: String,
    pub content_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AttachmentReorderRequest {
    pub id: String,
    /// The task's attachments (stored names) in their new order. Must list
    /// every managed attachment of the task exactly once.
    pub attachments: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AttachmentInfoRequest {
    #[serde(default)]
    pub project: Option<String>,
    pub attachments: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AttachmentInfo {
    pub attachment: String,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AttachmentInfoResponse {
    pub items: Vec<AttachmentInfo>,
}

/// File names found in ticket text, to look up in the repository (DEV-549).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct FileResolveRequest {
    /// Directory of the repository file the references appear in, for
    /// relative links inside repository markdown.
    #[serde(default)]
    pub base: Option<String>,
    pub references: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct FileResolveItem {
    pub reference: String,
    /// Repository-relative path with `/` separators.
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct FileResolveResponse {
    pub items: Vec<FileResolveItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AttachmentRemoveRequest {
    pub id: String,
    pub stored_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AttachmentRemoveResponse {
    pub task: TaskDTO,
    pub deleted: bool,
    pub still_referenced: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct LinkReferenceAddRequest {
    pub id: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct LinkReferenceAddResponse {
    pub task: TaskDTO,
    pub added: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct LinkReferenceRemoveRequest {
    pub id: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct LinkReferenceRemoveResponse {
    pub task: TaskDTO,
    pub removed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct CodeReferenceAddRequest {
    pub id: String,
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct CodeReferenceAddResponse {
    pub task: TaskDTO,
    pub added: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct CodeReferenceRemoveRequest {
    pub id: String,
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct CodeReferenceRemoveResponse {
    pub task: TaskDTO,
    pub removed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct FileReferenceAddRequest {
    pub id: String,
    /// Repository-relative file path (never a managed attachments-store
    /// blob; store content is referenced through attachment endpoints).
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct FileReferenceAddResponse {
    pub task: TaskDTO,
    pub added: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct FileReferenceRemoveRequest {
    pub id: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct FileReferenceRemoveResponse {
    pub task: TaskDTO,
    pub removed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct GenericReferenceAddRequest {
    pub id: String,
    pub kind: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct GenericReferenceAddResponse {
    pub task: TaskDTO,
    pub added: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct GenericReferenceRemoveRequest {
    pub id: String,
    pub kind: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct GenericReferenceRemoveResponse {
    pub task: TaskDTO,
    pub removed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskListFilter {
    pub status: Vec<crate::types::TaskStatus>,
    pub priority: Vec<crate::types::Priority>,
    pub task_type: Vec<crate::types::TaskType>,
    pub project: Option<String>,
    pub tags: Vec<String>,
    pub text_query: Option<String>,
    pub sprints: Vec<u32>,
    /// Assignee equality filter (any-of, `member_for_comparison` semantics).
    /// `@me` is expected to be resolved by the caller. Deserializes from
    /// either a single string (legacy wire shape) or an array.
    #[serde(default, deserialize_with = "deserialize_string_or_vec")]
    pub assignee: Vec<String>,
    /// Keep only tasks with no assignee (`__none__`).
    #[serde(default)]
    pub assignee_none: bool,
    #[serde(default)]
    pub custom_fields: BTreeMap<String, Vec<String>>,
    /// Lifecycle visibility (DEV-92): `active` (default) hides tombstones,
    /// `deleted` selects only tombstones, `all` returns everything.
    #[serde(default)]
    pub deletion: DeletionFilter,
    /// Archive visibility (DEV-4): `no` (default) skips `@archive` folders,
    /// `yes` lists only archived tasks, `all` lists both.
    #[serde(default)]
    pub archived: crate::storage::layout::ArchiveFilter,
}

pub fn deserialize_string_or_vec<'de, D>(d: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum StringOrVec {
        Str(String),
        Vec(Vec<String>),
    }
    match StringOrVec::deserialize(d)? {
        StringOrVec::Str(s) if s.is_empty() => Ok(Vec::new()),
        StringOrVec::Str(s) => Ok(vec![s]),
        StringOrVec::Vec(v) => Ok(v),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskListResponse {
    /// Tasks matching the query (all of them, also when `ids` narrows `tasks`).
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tasks: Vec<TaskListRow>,
    /// With `ids` (DEV-692): each returned task's index in the full
    /// filtered order, so a client can place it without the whole list.
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub positions: BTreeMap<String, usize>,
}

/// A task as `GET /api/tasks/list` returns it (DEV-692): the [`TaskDTO`]
/// without its description, comments and history, which make up most of a
/// ticket and which no list view shows, plus what list views show of them.
/// `GET /api/tasks/get` returns the whole ticket.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskListRow {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub url: Option<String>,
    pub title: String,
    pub status: crate::types::TaskStatus,
    pub priority: crate::types::Priority,
    pub task_type: crate::types::TaskType,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub deleted_at: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub archived: bool,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub moved_from: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub task_state: Option<TaskStateDTO>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reporter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub assignee: Option<String>,
    pub created: String,
    pub modified: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub due_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub subtitle: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub relationships: crate::types::TaskRelationships,
    #[serde(default)]
    pub references: Vec<crate::types::ReferenceEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub acceptance_criteria: Vec<String>,
    #[serde(default)]
    pub sprints: Vec<u32>,
    #[serde(skip_serializing_if = "crate::api_types::btreemap_u32_is_empty", default)]
    pub sprint_order: BTreeMap<u32, u32>,
    #[serde(default)]
    pub releases: Vec<u32>,
    #[serde(default)]
    pub custom_fields: crate::types::CustomFields,
    /// Number of comments.
    pub comment_count: usize,
    /// Number of history entries.
    pub history_count: usize,
    /// The newest question comment no answer points to (DEV-488), which
    /// list rows and board cards let the reader answer.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub open_question: Option<crate::types::TaskComment>,
    /// When the task entered its current status (board age rules, DEV-133):
    /// the latest recorded change into it; the creation time when its
    /// status never changed; else the latest recorded status change.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub status_since: Option<String>,
    /// The whole history, only with `include=history`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub history: Option<Vec<crate::types::TaskChangeLogEntry>>,
}

impl TaskListRow {
    /// The list row of `task`; `with_history` keeps its history.
    pub fn from_task(task: TaskDTO, with_history: bool) -> Self {
        let open_question = open_question(&task.comments).cloned();
        let status_since = status_since(&task);
        Self {
            comment_count: crate::types::live_comments(&task.comments).count(),
            history_count: task.history.len(),
            open_question,
            status_since,
            history: with_history.then_some(task.history),
            id: task.id,
            url: task.url,
            title: task.title,
            status: task.status,
            priority: task.priority,
            task_type: task.task_type,
            deleted_at: task.deleted_at,
            archived: task.archived,
            moved_from: task.moved_from,
            task_state: task.task_state,
            reporter: task.reporter,
            assignee: task.assignee,
            created: task.created,
            modified: task.modified,
            due_date: task.due_date,
            effort: task.effort,
            subtitle: task.subtitle,
            tags: task.tags,
            relationships: task.relationships,
            references: task.references,
            acceptance_criteria: task.acceptance_criteria,
            sprints: task.sprints,
            sprint_order: task.sprint_order,
            releases: task.releases,
            custom_fields: task.custom_fields,
        }
    }
}

/// The newest question comment no answer points to (DEV-488), as the web
/// view's `lastOpenQuestion` picks it.
fn open_question(comments: &[crate::types::TaskComment]) -> Option<&crate::types::TaskComment> {
    crate::types::open_questions(comments).last().map(|&index| &comments[index])
}

/// Status labels compared like board columns: case, spaces, `_` and `-`
/// do not count.
fn status_key(value: &str) -> String {
    value.chars().filter(|c| !c.is_whitespace() && *c != '_' && *c != '-').flat_map(char::to_lowercase).collect()
}

/// When `task` entered its current status; see [`TaskListRow::status_since`].
fn status_since(task: &TaskDTO) -> Option<String> {
    let wanted = status_key(task.status.as_str());
    let mut latest_into: Option<(jiff::Timestamp, &str)> = None;
    let mut latest_any: Option<(jiff::Timestamp, &str)> = None;
    let mut saw_status_change = false;
    for entry in &task.history {
        let changes: Vec<_> = entry.changes.iter().filter(|change| change.field == "status").collect();
        if changes.is_empty() {
            continue;
        }
        saw_status_change = true;
        let Ok(at) = entry.at.parse::<jiff::Timestamp>() else { continue };
        if latest_any.is_none_or(|(seen, _)| at > seen) {
            latest_any = Some((at, entry.at.as_str()));
        }
        if changes.iter().any(|change| change.new.as_deref().is_some_and(|new| status_key(new) == wanted)) && latest_into.is_none_or(|(seen, _)| at > seen) {
            latest_into = Some((at, entry.at.as_str()));
        }
    }
    if let Some((_, at)) = latest_into {
        return Some(at.to_string());
    }
    if !saw_status_change {
        return task.created.parse::<jiff::Timestamp>().ok().map(|_| task.created.clone());
    }
    latest_any.map(|(_, at)| at.to_string())
}

/// Deletion request shared by REST, MCP, and UI clients (DEV-92).
///
/// `hard` defaults to `false`: ordinary deletion is a soft tombstone that
/// keeps the task file, ID, sprints, and content. Hard deletion physically
/// removes the task file while preserving attachment blobs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskDeleteRequest {
    pub id: String,
    #[serde(default)]
    pub hard: bool,
}

/// Result of a task deletion (or a read-only deletion preview, where
/// `deleted` is `false`). All fields are always serialized. `deleted` is
/// true whenever the task is in the deleted state after the call, so a
/// repeated soft deletion of an already-deleted task stays `true`
/// (idempotent). `warnings` is non-empty only for hard deletions, where it
/// lists retained attachment blobs (with stored paths) and incoming
/// relationships from other stored tasks; nothing is cleaned up.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskDeleteResponse {
    pub deleted: bool,
    pub hard: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(untagged)]
pub enum SprintSelector {
    Id(u32),
    Keyword(String),
}

impl SprintSelector {
    pub fn as_reference(&self) -> String {
        match self {
            SprintSelector::Id(id) => id.to_string(),
            SprintSelector::Keyword(value) => value.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintAssignmentRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sprint: Option<SprintSelector>,
    #[serde(default)]
    pub tasks: Vec<String>,
    #[serde(default)]
    pub allow_closed: bool,
    #[serde(default)]
    pub cleanup_missing: bool,
    #[serde(default)]
    pub force_single: bool,
    /// Also act on the tasks this search selects, in the web search syntax
    /// (DEV-236), e.g. `status:todo tag:ui`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub query: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintCreateRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_length: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ends_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub starts_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capacity_points: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capacity_hours: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overdue_after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default)]
    pub skip_defaults: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintCreateResponse {
    pub status: String,
    pub sprint: SprintListItem,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub applied_defaults: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintUpdateRequest {
    pub sprint: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_length: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ends_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub starts_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default, deserialize_with = "crate::api_types::deserialize_double_option")]
    pub capacity_points: Option<Option<u32>>,
    #[serde(skip_serializing_if = "Option::is_none", default, deserialize_with = "crate::api_types::deserialize_double_option")]
    pub capacity_hours: Option<Option<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overdue_after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default, deserialize_with = "crate::api_types::deserialize_double_option")]
    pub actual_started_at: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none", default, deserialize_with = "crate::api_types::deserialize_double_option")]
    pub actual_closed_at: Option<Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintUpdateResponse {
    pub status: String,
    pub sprint: SprintListItem,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintReassignment {
    pub task_id: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub previous: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintAssignmentResponse {
    pub status: String,
    pub action: String,
    pub sprint_id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sprint_label: Option<String>,
    pub modified: Vec<String>,
    pub unchanged: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub replaced: Vec<SprintReassignment>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub messages: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrity: Option<SprintIntegrityDiagnostics>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintDeleteRequest {
    pub sprint: u32,
    #[serde(default)]
    pub cleanup_missing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintDeleteResponse {
    pub status: String,
    pub deleted: bool,
    pub sprint_id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sprint_label: Option<String>,
    pub removed_references: usize,
    pub updated_tasks: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrity: Option<SprintIntegrityDiagnostics>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintIntegrityDiagnostics {
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub missing_sprints: Vec<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tasks_with_missing: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_cleanup: Option<SprintCleanupSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintCleanupSummary {
    pub removed_references: usize,
    pub updated_tasks: usize,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub removed_by_sprint: Vec<SprintCleanupMetric>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub remaining_missing: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintCleanupMetric {
    pub sprint_id: u32,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintBacklogItem {
    pub id: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub assignee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub due_date: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintBacklogResponse {
    pub status: String,
    pub count: usize,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tasks: Vec<SprintBacklogItem>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub missing_sprints: Vec<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrity: Option<SprintIntegrityDiagnostics>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintListItem {
    pub id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub created: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub modified: Option<String>,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub planned_start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub planned_end: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub actual_start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub actual_end: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub computed_end: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub goal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub plan_length: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub overdue_after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub capacity_points: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub capacity_hours: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SprintListResponse {
    pub status: String,
    pub total: usize,
    pub count: usize,
    pub limit: usize,
    pub offset: usize,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub sprints: Vec<SprintListItem>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub missing_sprints: Vec<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrity: Option<SprintIntegrityDiagnostics>,
}

/// A release (DEV-393) with counts over its tickets.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct ReleaseListItem {
    pub id: u32,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub label: Option<String>,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub notes: Option<String>,
    /// `YYYY-MM-DD`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub planned_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub released_at: Option<String>,
    /// `planned` or `released`.
    pub state: String,
    /// Planned for a day before today and not released.
    pub overdue: bool,
    pub task_count: usize,
    pub done_count: usize,
    pub open_count: usize,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub created: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub modified: Option<String>,
}

/// A release named by ID (`3`) or by `#3`, version or label.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(untagged)]
pub enum ReleaseSelector {
    Id(u32),
    Reference(String),
}

impl ReleaseSelector {
    pub fn as_reference(&self) -> String {
        match self {
            ReleaseSelector::Id(id) => id.to_string(),
            ReleaseSelector::Reference(value) => value.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct ReleaseListResponse {
    pub status: String,
    pub total: usize,
    /// In version order.
    #[serde(default)]
    pub releases: Vec<ReleaseListItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct ReleaseGetResponse {
    pub status: String,
    pub release: ReleaseListItem,
    /// The release's tickets.
    #[serde(default)]
    pub tasks: Vec<TaskDTO>,
    #[serde(default)]
    pub history: Vec<crate::types::TaskChangeLogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ReleaseCreateRequest {
    /// `0.15.0` or `v0.15.0` (stored without the `v`).
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub notes: Option<String>,
    /// `YYYY-MM-DD`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub planned_date: Option<String>,
    /// RFC 3339 or `YYYY-MM-DD`; set for a release that is already out.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub released_at: Option<String>,
}

/// Fields left out stay as they are; an empty string clears one.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ReleaseUpdateRequest {
    pub release: ReleaseSelector,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub planned_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub released_at: Option<String>,
}

/// Mark a release released (at `at`, default now), reopen it, or delete it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ReleaseActionRequest {
    pub release: ReleaseSelector,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct ReleaseMutationResponse {
    pub status: String,
    pub release: ReleaseListItem,
    /// False when the request changed nothing.
    pub changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct ReleaseDeleteResponse {
    pub status: String,
    pub deleted: bool,
    pub release_id: u32,
    /// Tickets the release was taken off.
    #[serde(default)]
    pub removed_from: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ReleaseAssignmentRequest {
    pub release: ReleaseSelector,
    #[serde(default)]
    pub tasks: Vec<String>,
    /// Also act on the tasks this search selects, in the web search syntax.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub query: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct ReleaseAssignmentResponse {
    pub status: String,
    /// `add`, `move` or `remove`.
    pub action: String,
    pub release_id: u32,
    pub release_version: String,
    #[serde(default)]
    pub modified: Vec<String>,
    #[serde(default)]
    pub unchanged: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct ReleaseBacklogResponse {
    pub status: String,
    pub total: usize,
    pub count: usize,
    pub limit: usize,
    pub offset: usize,
    /// Open tickets in no release.
    #[serde(default)]
    pub tasks: Vec<TaskDTO>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct ProjectListResponse {
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    #[serde(default)]
    pub projects: Vec<ProjectDTO>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct ProjectDTO {
    pub name: String,
    pub prefix: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct ProjectStatsDTO {
    pub name: String,
    pub open_count: u64,
    pub done_count: u64,
    pub recent_modified: Option<String>,
    pub tags_top: Vec<String>,
}

/// The distinct people, tags and custom field values a project's tasks hold,
/// for the task panel's suggestions without whole tasks (DEV-545).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskValuesDTO {
    /// Assignees and reporters as stored.
    pub people: Vec<String>,
    pub tags: Vec<String>,
    /// String and number values by field name; list fields item by item.
    pub custom_fields: std::collections::BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SyncRequest {
    pub remote: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub project: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auth_profile: Option<String>,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub include_report: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub write_report: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub client_run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SyncValidateRequest {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub remote: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub project: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auth_profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub remote_config: Option<SyncRemoteConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SyncValidateResponse {
    pub status: String,
    pub provider: String,
    pub remote: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub project: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub repo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub filter: Option<String>,
    pub checked_at: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub info: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct ScanTarget {
    pub file: String,
    pub line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct ScanRequest {
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub paths: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub include: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub exclude: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub project: Option<String>,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub strip_attributes: Option<bool>,
    #[serde(default)]
    pub reanchor: bool,
    #[serde(default)]
    pub modified_only: bool,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub targets: Vec<ScanTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct ScanSummary {
    pub created: usize,
    pub updated: usize,
    pub skipped: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct ScanEntry {
    pub status: String,
    pub action: String,
    pub file: String,
    pub line: usize,
    pub title: String,
    pub annotation: String,
    pub code_reference: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub existing_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub original_line: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub updated_line: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct ScanResponse {
    pub status: String,
    pub dry_run: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub project: Option<String>,
    pub summary: ScanSummary,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub info: Vec<String>,
    pub entries: Vec<ScanEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SyncSummary {
    pub created: usize,
    pub updated: usize,
    pub skipped: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SyncReportEntry {
    pub status: String,
    pub at: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reference: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SyncReportMeta {
    pub id: String,
    pub created_at: String,
    pub status: String,
    pub direction: String,
    pub provider: String,
    pub remote: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub project: Option<String>,
    pub dry_run: bool,
    pub summary: SyncSummary,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub info: Vec<String>,
    pub entries_total: usize,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub stored_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SyncReport {
    pub id: String,
    pub created_at: String,
    pub status: String,
    pub direction: String,
    pub provider: String,
    pub remote: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub project: Option<String>,
    pub dry_run: bool,
    pub summary: SyncSummary,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub info: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub entries: Vec<SyncReportEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SyncReportListResponse {
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    /// Always present, including `[]` for scopes without reports.
    pub reports: Vec<SyncReportMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SyncResponse {
    /// `ok`, or `partial` when some items failed (DEV-300).
    pub status: String,
    pub direction: String,
    pub provider: String,
    pub remote: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub project: Option<String>,
    pub dry_run: bool,
    pub summary: SyncSummary,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub info: Vec<String>,
    pub run_id: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub report: Option<SyncReportMeta>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub report_entries: Vec<SyncReportEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn dto(extra: &serde_json::Value) -> TaskDTO {
        let mut base = json!({
            "id": "DEMO-1", "title": "One", "status": "Doing", "priority": "Low", "task_type": "Feature",
            "created": "2026-01-01T00:00:00Z", "modified": "2026-01-05T00:00:00Z",
            "description": "long text", "tags": ["a"], "relationships": {}, "comments": [], "references": [],
            "sprints": [], "releases": [], "history": [], "custom_fields": {}
        });
        for (key, value) in extra.as_object().into_iter().flatten() {
            base[key] = value.clone();
        }
        serde_json::from_value(base).unwrap()
    }

    #[test]
    fn list_row_keys_match_task_dto_but_heavy_fields() {
        let task = dto(&json!({"assignee": "me", "due_date": "2026-02-01", "effort": "1d", "subtitle": "s",
            "acceptance_criteria": ["x"], "sprints": [1], "sprint_order": {"1": 2}, "releases": [3],
            "archived": true, "moved_from": ["OLD-1"], "deleted_at": "2026-01-06T00:00:00Z", "reporter": "r",
            "url": "u", "references": [{"link": "https://x"}],
            "comments": [{"date": "2026-01-02T00:00:00Z", "text": "c"}],
            "history": [{"at": "2026-01-03T00:00:00Z", "changes": [{"field": "status", "old": "Todo", "new": "Doing"}]}]}));
        let full = serde_json::to_value(&task).unwrap();
        let row = serde_json::to_value(TaskListRow::from_task(task, false)).unwrap();
        let mut full_keys: Vec<_> = full.as_object().unwrap().keys().cloned().collect();
        full_keys.retain(|key| !["description", "comments", "history"].contains(&key.as_str()));
        let mut row_keys: Vec<_> = row.as_object().unwrap().keys().cloned().collect();
        row_keys.retain(|key| !["comment_count", "history_count", "open_question", "status_since"].contains(&key.as_str()));
        full_keys.sort();
        row_keys.sort();
        assert_eq!(row_keys, full_keys, "TaskListRow must carry every TaskDTO field but description, comments and history");
        assert_eq!(row["comment_count"], 1);
        assert_eq!(row["history_count"], 1);
        assert_eq!(row["status_since"], "2026-01-03T00:00:00Z");
    }

    #[test]
    fn list_row_keeps_history_only_when_asked() {
        let history = json!([{"at": "2026-01-03T00:00:00Z", "changes": [{"field": "title", "new": "One"}]}]);
        let row = serde_json::to_value(TaskListRow::from_task(dto(&json!({"history": history.clone()})), true)).unwrap();
        assert_eq!(row["history"], history);
        let row = serde_json::to_value(TaskListRow::from_task(dto(&json!({"history": history})), false)).unwrap();
        assert!(row.get("history").is_none(), "{row}");
    }

    #[test]
    fn open_question_is_the_newest_unanswered_question() {
        let question = |date: &str| json!({"date": date, "text": "q", "question": {"options": [{"key": "a", "label": "A"}]}});
        let answer = json!({"date": "2026-01-04T00:00:00Z", "text": "A", "answer": {"to": "2026-01-03T00:00:00Z", "choice": ["A"]}});
        let comments = json!([question("2026-01-02T00:00:00Z"), question("2026-01-03T00:00:00Z"), answer]);
        let row = TaskListRow::from_task(dto(&json!({"comments": comments})), false);
        assert_eq!(row.open_question.map(|c| c.date).as_deref(), Some("2026-01-02T00:00:00Z"));
        let answered = json!([question("2026-01-03T00:00:00Z"), {"date": "2026-01-04T00:00:00Z", "text": "A", "answer": {"to": "2026-01-03T00:00:00Z"}}]);
        assert!(TaskListRow::from_task(dto(&json!({"comments": answered})), false).open_question.is_none());
    }

    #[test]
    fn status_since_follows_the_board_age_rules() {
        let since = |history: serde_json::Value| TaskListRow::from_task(dto(&json!({"history": history})), false).status_since;
        assert_eq!(since(json!([])).as_deref(), Some("2026-01-01T00:00:00Z"), "no status change: created");
        let into = json!([
            {"at": "2026-01-02T00:00:00Z", "changes": [{"field": "status", "new": "doing"}]},
            {"at": "2026-01-03T00:00:00Z", "changes": [{"field": "status", "new": "Review"}]},
            {"at": "2026-01-04T00:00:00Z", "changes": [{"field": "status", "new": "DOING"}]},
            {"at": "2026-01-05T00:00:00Z", "changes": [{"field": "title", "new": "x"}]}
        ]);
        assert_eq!(since(into).as_deref(), Some("2026-01-04T00:00:00Z"), "latest change into the status, any case");
        let elsewhere = json!([
            {"at": "2026-01-02T00:00:00Z", "changes": [{"field": "status", "new": "Todo"}]},
            {"at": "2026-01-03T00:00:00Z", "changes": [{"field": "status", "new": "Review"}]}
        ]);
        assert_eq!(since(elsewhere).as_deref(), Some("2026-01-03T00:00:00Z"), "no change into it: latest status change");
    }

    #[test]
    fn sprint_update_request_handles_absent_closed_at() {
        let payload: SprintUpdateRequest = serde_json::from_value(json!({
            "sprint": 1
        }))
        .unwrap();
        assert!(payload.actual_closed_at.is_none());
    }

    #[test]
    fn sprint_update_request_handles_null_closed_at() {
        let payload: SprintUpdateRequest = serde_json::from_value(json!({
            "sprint": 1,
            "actual_closed_at": null
        }))
        .unwrap();
        assert!(matches!(payload.actual_closed_at, Some(None)));
    }

    #[test]
    fn sprint_update_request_handles_value_closed_at() {
        let iso = "2025-01-01T00:00:00Z";
        let payload: SprintUpdateRequest = serde_json::from_value(json!({
            "sprint": 1,
            "actual_closed_at": iso
        }))
        .unwrap();
        assert!(matches!(payload.actual_closed_at, Some(Some(ref value)) if value == iso));
    }

    #[test]
    fn sprint_update_request_handles_null_capacity_points() {
        let payload: SprintUpdateRequest = serde_json::from_value(json!({
            "sprint": 1,
            "capacity_points": null
        }))
        .unwrap();
        assert!(matches!(payload.capacity_points, Some(None)));
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct ProjectCreateRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub values: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct ReferenceSnippetLineDTO {
    pub number: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct ReferenceSnippetDTO {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub highlight_start: usize,
    pub highlight_end: usize,
    pub lines: Vec<ReferenceSnippetLineDTO>,
    pub has_more_before: bool,
    pub has_more_after: bool,
    pub total_lines: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct ApiErrorPayload {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}
