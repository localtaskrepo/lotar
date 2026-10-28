use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

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
    pub title: String,
    pub status: crate::types::TaskStatus,
    pub priority: crate::types::Priority,
    pub task_type: crate::types::TaskType,
    /// Soft-deletion tombstone timestamp (UTC RFC3339, DEV-92). Serialized
    /// only while the task is deleted; omitted for active tasks. Lifecycle
    /// transitions never touch `modified`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub deleted_at: Option<String>,
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
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tags: Vec<String>,
    #[serde(
        skip_serializing_if = "crate::types::TaskRelationships::is_empty",
        default
    )]
    pub relationships: crate::types::TaskRelationships,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub comments: Vec<crate::types::TaskComment>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub references: Vec<crate::types::ReferenceEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub acceptance_criteria: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub sprints: Vec<u32>,
    #[serde(
        skip_serializing_if = "crate::api_types::btreemap_u32_is_empty",
        default
    )]
    pub sprint_order: BTreeMap<u32, u32>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub history: Vec<crate::types::TaskChangeLogEntry>,
    #[serde(skip_serializing_if = "crate::api_types::map_is_empty", default)]
    pub custom_fields: crate::types::CustomFields,
}

// Helper to allow skipping empty maps for custom_fields
pub fn map_is_empty(map: &crate::types::CustomFields) -> bool {
    // CustomFields is a type alias for HashMap<String, Value>
    let as_hash: &HashMap<String, crate::types::CustomFieldValue> = map;
    as_hash.is_empty()
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
}

/// What a job used, as far as its CLI reports it. Claude reports tokens,
/// cost and turns; Codex tokens; Copilot premium requests and changed lines;
/// Gemini tokens. Missing values were not reported.
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
    /// Agent turns (model round trips).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turns: Option<u64>,
    /// GitHub Copilot premium requests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub premium_requests: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines_added: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines_removed: Option<u64>,
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
            parts.push(format!(
                "{turns} {}",
                if turns == 1 { "turn" } else { "turns" }
            ));
        }
        if let Some(requests) = self.premium_requests {
            parts.push(format!(
                "{requests} premium {}",
                if requests == 1.0 {
                    "request"
                } else {
                    "requests"
                }
            ));
        }
        if self.lines_added.is_some() || self.lines_removed.is_some() {
            parts.push(format!(
                "+{} −{} lines",
                self.lines_added.unwrap_or(0),
                self.lines_removed.unwrap_or(0)
            ));
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
    /// Tokens, cost and similar as reported by the CLI, once known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<AgentJobUsage>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

/// Request to simulate automation rules for a given event
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AutomationSimulateRequest {
    /// The task ID to simulate against
    pub ticket_id: String,
    /// The event to simulate. Canonical names: "created", "updated",
    /// "assigned", "commented", "sprint_changed", "job_started",
    /// "job_completed", "job_failed", "job_cancelled". Matching is
    /// case-insensitive and legacy aliases are accepted ("job_start",
    /// "complete", "success", "error", "failure", "cancel").
    pub event: String,
    /// Optional in the payload but ignored: the automation scope always
    /// follows the ticket's project prefix.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub project: Option<String>,
}

/// Response showing what would happen when automation rules are applied
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AutomationSimulateResponse {
    /// Whether any rules matched
    pub matched: bool,
    /// Name of the matched rule, if any
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub rule_name: Option<String>,
    /// Actions that would be performed
    pub actions: Vec<AutomationSimulatedAction>,
    /// Current task state (before automation)
    pub task_before: Option<TaskDTO>,
    /// Simulated task state (after automation)
    pub task_after: Option<TaskDTO>,
    /// Parts of the preview that are only decided when the rule really runs
    /// (event-only template values, runtime assignee tokens, sprint changes).
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// A single action that would be performed by automation
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobLogsResponse {
    pub job: AgentJob,
    pub events: Vec<AgentJobLogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentJobMessageRequest {
    pub id: String,
    pub message: String,
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
    /// URL of the logo, when one is configured and can be served.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo: Option<String>,
    /// URL of the logo for dark mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo_dark: Option<String>,
    /// URL of the app icon (browser tab icon).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Why configured images are not shown (missing file, not an image…).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<String>,
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
}

pub(crate) fn deserialize_string_or_vec<'de, D>(d: D) -> Result<Vec<String>, D::Error>
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
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tasks: Vec<TaskDTO>,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct TaskSelection {
    #[serde(default)]
    pub filter: TaskListFilter,
    #[serde(default)]
    pub r#where: Vec<(String, String)>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection: Option<TaskSelection>,
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
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "crate::api_types::deserialize_double_option"
    )]
    pub capacity_points: Option<Option<u32>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "crate::api_types::deserialize_double_option"
    )]
    pub capacity_hours: Option<Option<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overdue_after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "crate::api_types::deserialize_double_option"
    )]
    pub actual_started_at: Option<Option<String>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "crate::api_types::deserialize_double_option"
    )]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct ProjectListResponse {
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
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
