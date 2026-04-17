use crate::types::{Priority, TaskStatus, TaskType};
#[cfg(feature = "schema")]
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectTemplate {
    pub name: String,
    pub description: String,
    pub config: ProjectConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SprintDefaultsConfig {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub capacity_points: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub capacity_hours: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub length: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub overdue_after: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SprintNotificationsConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for SprintNotificationsConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SprintConfig {
    #[serde(default)]
    pub defaults: SprintDefaultsConfig,
    #[serde(default)]
    pub notifications: SprintNotificationsConfig,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum SyncProvider {
    #[default]
    Jira,
    Github,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum SyncWhenEmpty {
    Skip,
    Clear,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SyncFieldMappingDetail {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub field: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub values: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub set: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub default: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub add: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub when_empty: Option<SyncWhenEmpty>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(untagged)]
pub enum SyncFieldMapping {
    Simple(String),
    Detailed(SyncFieldMappingDetail),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SyncRemoteConfig {
    pub provider: SyncProvider,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub project: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub repo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auth_profile: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub mapping: HashMap<String, SyncFieldMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SyncAuthProfile {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub provider: Option<SyncProvider>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub token_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub email_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub api_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct SyncConfig {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub remotes: HashMap<String, SyncRemoteConfig>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub auth_profiles: HashMap<String, SyncAuthProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentToolsConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disallowed: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentMcpConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentProfileDetail {
    pub runner: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tools: Option<AgentToolsConfig>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub mcp: Option<AgentMcpConfig>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub instructions: Option<AgentInstructionsConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(untagged)]
pub enum AgentInstructionsConfig {
    Inline(String),
    File { file: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[serde(untagged)]
pub enum AgentProfileConfig {
    Runner(String),
    Detailed(AgentProfileDetail),
}

impl AgentProfileConfig {
    pub fn to_detail(&self) -> AgentProfileDetail {
        match self {
            AgentProfileConfig::Runner(runner) => AgentProfileDetail {
                runner: runner.clone(),
                command: None,
                args: Vec::new(),
                env: HashMap::new(),
                tools: None,
                mcp: None,
                instructions: None,
            },
            AgentProfileConfig::Detailed(detail) => detail.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentAutomationAction {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub set_status: Option<TaskStatus>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reassign_to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentAutomationActionOverride {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub set_status: Option<TaskStatus>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reassign_to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentAutomationConfig {
    #[serde(default = "default_agent_on_start_action")]
    pub on_start: AgentAutomationAction,
    #[serde(default = "default_agent_on_success_action")]
    pub on_success: AgentAutomationAction,
    #[serde(default)]
    pub on_failure: AgentAutomationAction,
    #[serde(default)]
    pub on_cancel: AgentAutomationAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentAutomationConfigOverride {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub on_start: Option<AgentAutomationActionOverride>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub on_success: Option<AgentAutomationActionOverride>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub on_failure: Option<AgentAutomationActionOverride>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub on_cancel: Option<AgentAutomationActionOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentWorktreeConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub dir: Option<String>,
    #[serde(default = "default_agent_worktree_branch_prefix")]
    pub branch_prefix: String,
    /// Maximum number of agent jobs that can run in parallel.
    /// When set, new jobs are queued if the limit is reached.
    /// None means unlimited.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub max_parallel_jobs: Option<usize>,
    /// Automatically remove the worktree when the ticket is in a done state
    /// after a successful job.
    #[serde(default)]
    pub cleanup_on_done: bool,
    /// Remove the worktree after a failed job.
    #[serde(default)]
    pub cleanup_on_failure: bool,
    /// Remove the worktree after a cancelled job.
    #[serde(default)]
    pub cleanup_on_cancel: bool,
    /// Delete the worktree branch when cleanup runs.
    #[serde(default)]
    pub cleanup_delete_branches: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "schema", derive(JsonSchema))]
pub struct AgentWorktreeConfigOverride {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub branch_prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub max_parallel_jobs: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cleanup_on_done: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cleanup_on_failure: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cleanup_on_cancel: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cleanup_delete_branches: Option<bool>,
}

impl Default for AgentWorktreeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            dir: None,
            branch_prefix: default_agent_worktree_branch_prefix(),
            max_parallel_jobs: None,
            cleanup_on_done: false,
            cleanup_on_failure: false,
            cleanup_on_cancel: false,
            cleanup_delete_branches: false,
        }
    }
}

impl Default for AgentAutomationConfig {
    fn default() -> Self {
        Self {
            on_start: default_agent_on_start_action(),
            on_success: default_agent_on_success_action(),
            on_failure: AgentAutomationAction::default(),
            on_cancel: AgentAutomationAction::default(),
        }
    }
}

impl SyncConfig {
    pub fn is_empty(&self) -> bool {
        self.remotes.is_empty() && self.auth_profiles.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct ConfigurableField<T> {
    pub values: Vec<T>,
}

impl<T> Serialize for ConfigurableField<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.values.serialize(serializer)
    }
}

impl<'de, T> Deserialize<'de> for ConfigurableField<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let values = Vec::<T>::deserialize(deserializer)?;
        Ok(ConfigurableField { values })
    }
}

impl<T> ConfigurableField<T> where
    T: Clone + PartialEq + std::fmt::Debug + for<'de> serde::Deserialize<'de>
{
}

// Specialized implementation for String fields that support wildcard
// This will serialize as a direct array without the "values" wrapper
#[derive(Debug, Clone)]
pub struct StringConfigField {
    pub values: Vec<String>,
}

impl Serialize for StringConfigField {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.values.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for StringConfigField {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let values = Vec::<String>::deserialize(deserializer)?;
        Ok(StringConfigField { values })
    }
}

impl StringConfigField {
    pub fn new_wildcard() -> Self {
        Self {
            values: vec!["*".to_string()],
        }
    }

    #[allow(dead_code)]
    pub fn new_strict(values: Vec<String>) -> Self {
        Self { values }
    }

    pub fn has_wildcard(&self) -> bool {
        self.values.contains(&"*".to_string())
    }

    #[allow(dead_code)]
    pub fn get_suggestions(&self) -> Vec<String> {
        self.values.iter().filter(|v| *v != "*").cloned().collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub project_name: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub issue_states: Option<ConfigurableField<TaskStatus>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub issue_types: Option<ConfigurableField<TaskType>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub issue_priorities: Option<ConfigurableField<Priority>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tags: Option<StringConfigField>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub default_assignee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub default_reporter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub members: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub strict_members: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auto_populate_members: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub default_tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub default_priority: Option<Priority>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub default_status: Option<TaskStatus>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub custom_fields: Option<StringConfigField>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auto_set_reporter: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auto_assign_on_status: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auto_codeowners_assign: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auto_tags_from_path: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auto_branch_infer_type: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auto_branch_infer_status: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auto_branch_infer_priority: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auto_identity: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auto_identity_git: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub scan_signal_words: Option<Vec<String>>, // case-insensitive signal words for scanner
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub scan_ticket_patterns: Option<Vec<String>>, // regex patterns to detect ticket keys
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub scan_enable_ticket_words: Option<bool>, // treat ticket keys as signal words when enabled
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub scan_enable_mentions: Option<bool>, // when true, add code references for existing keys found in source

    // Scan mutation policy (project-level override)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub scan_strip_attributes: Option<bool>,

    // Optional per-project branch alias maps
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub branch_type_aliases: Option<HashMap<String, TaskType>>, // token -> TaskType
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub branch_status_aliases: Option<HashMap<String, TaskStatus>>, // token -> TaskStatus
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub branch_priority_aliases: Option<HashMap<String, Priority>>, // token -> Priority

    // Attachments (project-level override)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub attachments_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub attachments_max_upload_mb: Option<i64>,

    // Sync reports (project-level override)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub sync_reports_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub sync_write_reports: Option<bool>,

    // Agent context + profiles (project-level override)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub agent_context_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub agent_context_extension: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub agent_logs_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub agent_instructions: Option<AgentInstructionsConfig>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub agents: Option<HashMap<String, AgentProfileConfig>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub agent_automation: Option<AgentAutomationConfigOverride>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub agent_worktree: Option<AgentWorktreeConfigOverride>,

    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub remotes: HashMap<String, SyncRemoteConfig>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub auth_profiles: HashMap<String, SyncAuthProfile>,
}

impl ProjectConfig {
    pub fn new(project_name: String) -> Self {
        Self {
            project_name,
            issue_states: None,
            issue_types: None,
            issue_priorities: None,
            tags: None,
            default_assignee: None,
            default_reporter: None,
            members: None,
            strict_members: None,
            auto_populate_members: None,
            default_tags: None,
            default_priority: None,
            default_status: None,
            custom_fields: None,
            auto_set_reporter: None,
            auto_assign_on_status: None,
            auto_codeowners_assign: None,
            auto_tags_from_path: None,
            auto_branch_infer_type: None,
            auto_branch_infer_status: None,
            auto_branch_infer_priority: None,
            auto_identity: None,
            auto_identity_git: None,
            scan_signal_words: None,
            scan_ticket_patterns: None,
            scan_enable_ticket_words: None,
            scan_enable_mentions: None,
            scan_strip_attributes: None,
            branch_type_aliases: None,
            branch_status_aliases: None,
            branch_priority_aliases: None,

            attachments_dir: None,
            attachments_max_upload_mb: None,
            sync_reports_dir: None,
            sync_write_reports: None,
            agent_context_enabled: None,
            agent_context_extension: None,
            agent_logs_dir: None,
            agent_instructions: None,
            agents: None,
            agent_automation: None,
            agent_worktree: None,
            remotes: HashMap::new(),
            auth_profiles: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    #[serde(default = "default_port")]
    pub server_port: u16,
    #[serde(default = "default_project_name", rename = "default_project")]
    pub default_project: String,

    // Default configurations for all projects
    #[serde(default = "default_issue_states")]
    pub issue_states: ConfigurableField<TaskStatus>,
    #[serde(default = "default_issue_types")]
    pub issue_types: ConfigurableField<TaskType>,
    #[serde(default = "default_issue_priorities")]
    pub issue_priorities: ConfigurableField<Priority>,
    #[serde(default = "default_tags")]
    pub tags: StringConfigField,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub default_assignee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub default_reporter: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub default_tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<String>,
    #[serde(default)]
    pub strict_members: bool,
    #[serde(default = "default_true")]
    pub auto_populate_members: bool,
    #[serde(default = "default_true")]
    pub auto_set_reporter: bool,
    #[serde(default = "default_true")]
    pub auto_assign_on_status: bool,
    #[serde(default = "default_true")]
    pub auto_codeowners_assign: bool,
    #[serde(default = "default_true")]
    pub auto_tags_from_path: bool,
    #[serde(default = "default_true")]
    pub auto_branch_infer_type: bool,
    #[serde(default = "default_true")]
    pub auto_branch_infer_status: bool,
    #[serde(default = "default_true")]
    pub auto_branch_infer_priority: bool,
    #[serde(default = "default_priority")]
    pub default_priority: Priority,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub default_status: Option<TaskStatus>,
    #[serde(default = "default_custom_fields")]
    pub custom_fields: StringConfigField,
    #[serde(default = "default_scan_signal_words")]
    pub scan_signal_words: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub scan_ticket_patterns: Option<Vec<String>>,
    #[serde(default)]
    pub scan_enable_ticket_words: bool,
    #[serde(default = "default_true")]
    pub scan_enable_mentions: bool,

    #[serde(default)]
    pub sprints: SprintConfig,

    // Scan mutation policy
    #[serde(default = "default_true")]
    pub scan_strip_attributes: bool,

    // Branch alias maps (global-level)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub branch_type_aliases: HashMap<String, TaskType>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub branch_status_aliases: HashMap<String, TaskStatus>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub branch_priority_aliases: HashMap<String, Priority>,

    // Automation toggles
    #[serde(default = "default_true")]
    pub auto_identity: bool,
    #[serde(default = "default_true")]
    pub auto_identity_git: bool,

    // Attachments
    #[serde(default = "default_attachments_dir")]
    pub attachments_dir: String,

    /// Maximum upload size in megabytes.
    /// - `10` (default): allow up to 10 MiB
    /// - `0`: disable uploads
    /// - `-1`: unlimited
    #[serde(default = "default_attachments_max_upload_mb")]
    pub attachments_max_upload_mb: i64,

    // Sync reports
    #[serde(default = "default_sync_reports_dir")]
    pub sync_reports_dir: String,
    #[serde(default = "default_true")]
    pub sync_write_reports: bool,

    // Agent context + profiles
    #[serde(default = "default_true")]
    pub agent_context_enabled: bool,
    /// Extension for context files stored alongside tickets (e.g. ".context").
    #[serde(default = "default_agent_context_extension")]
    pub agent_context_extension: String,
    /// Optional directory for verbose agent debug logs. When set, logs are written
    /// to this directory. Supports relative (to workspace) or absolute paths.
    /// If unset, no debug logs are written.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub agent_logs_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub agent_instructions: Option<AgentInstructionsConfig>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub agents: HashMap<String, AgentProfileConfig>,
    #[serde(default)]
    pub agent_automation: AgentAutomationConfig,
    #[serde(default)]
    pub agent_worktree: AgentWorktreeConfig,

    /// Optional path to a directory containing custom web UI assets.
    /// When set and the path exists, the server will serve files from this directory
    /// before falling back to the bundled UI. This allows using a custom or development UI.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub web_ui_path: Option<String>,

    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub remotes: HashMap<String, SyncRemoteConfig>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub auth_profiles: HashMap<String, SyncAuthProfile>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolvedConfig {
    pub server_port: u16,
    pub default_project: String,
    pub issue_states: ConfigurableField<TaskStatus>,
    pub issue_types: ConfigurableField<TaskType>,
    pub issue_priorities: ConfigurableField<Priority>,
    pub tags: StringConfigField,
    pub default_assignee: Option<String>,
    pub default_reporter: Option<String>,
    pub default_tags: Vec<String>,
    pub members: Vec<String>,
    pub strict_members: bool,
    pub auto_populate_members: bool,
    pub auto_set_reporter: bool,
    pub auto_assign_on_status: bool,
    pub auto_codeowners_assign: bool,
    pub default_priority: Priority,
    pub default_status: Option<TaskStatus>,
    pub custom_fields: StringConfigField,
    pub scan_signal_words: Vec<String>,
    pub scan_strip_attributes: bool,
    // Effective scanner options
    pub scan_ticket_patterns: Option<Vec<String>>, // effective patterns if configured
    pub scan_enable_ticket_words: bool,
    pub scan_enable_mentions: bool,

    pub sprint_defaults: SprintDefaultsConfig,
    pub sprint_notifications: SprintNotificationsConfig,

    // Automation toggles (effective)
    pub auto_identity: bool,
    pub auto_identity_git: bool,
    pub auto_tags_from_path: bool,
    pub auto_branch_infer_type: bool,
    pub auto_branch_infer_status: bool,
    pub auto_branch_infer_priority: bool,

    // Effective alias maps
    pub branch_type_aliases: HashMap<String, TaskType>,
    pub branch_status_aliases: HashMap<String, TaskStatus>,
    pub branch_priority_aliases: HashMap<String, Priority>,

    // Attachments
    pub attachments_dir: String,
    pub attachments_max_upload_mb: i64,

    // Sync reports
    pub sync_reports_dir: String,
    pub sync_write_reports: bool,

    pub agent_context_enabled: bool,
    pub agent_context_extension: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub agent_logs_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub agent_instructions: Option<AgentInstructionsConfig>,
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub agent_profiles: HashMap<String, AgentProfileDetail>,
    pub agent_automation: AgentAutomationConfig,
    pub agent_worktree: AgentWorktreeConfig,

    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub remotes: HashMap<String, SyncRemoteConfig>,
}

impl ResolvedConfig {
    pub fn effective_default_status(&self) -> Option<TaskStatus> {
        if self.issue_states.values.is_empty() {
            return None;
        }

        if let Some(explicit) = &self.default_status
            && self.issue_states.values.contains(explicit)
        {
            return Some(explicit.clone());
        }

        Some(self.issue_states.values[0].clone())
    }

    pub fn effective_default_priority(&self) -> Option<Priority> {
        if self.issue_priorities.values.is_empty() {
            return None;
        }

        let configured = self.default_priority.clone();
        if self
            .issue_priorities
            .values
            .iter()
            .any(|value| value.eq_ignore_case(configured.as_str()))
        {
            return Some(configured);
        }

        Some(self.issue_priorities.values[0].clone())
    }

    pub fn effective_default_task_type(&self) -> Option<TaskType> {
        if self.issue_types.values.is_empty() {
            return None;
        }

        Some(self.issue_types.values[0].clone())
    }
}

#[derive(Debug)]
pub enum ConfigError {
    IoError(String),
    ParseError(String),
    FileNotFound(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::IoError(msg) => write!(f, "IO Error: {}", msg),
            ConfigError::ParseError(msg) => write!(f, "Parse Error: {}", msg),
            ConfigError::FileNotFound(msg) => write!(f, "Config file not found: {}", msg),
        }
    }
}

impl std::error::Error for ConfigError {}

// Default value functions
fn default_port() -> u16 {
    8080
}
fn default_project_name() -> String {
    // Don't default to "auto" - this should only be set during initial config creation
    // with actual auto-detection. Empty string means no default project is set.
    String::new()
}
fn default_priority() -> Priority {
    Priority::from("Medium")
}

fn default_issue_states() -> ConfigurableField<TaskStatus> {
    ConfigurableField {
        values: vec![
            TaskStatus::from("Todo"),
            TaskStatus::from("InProgress"),
            TaskStatus::from("NeedsReview"),
            TaskStatus::from("Done"),
        ],
    }
}

fn default_issue_types() -> ConfigurableField<TaskType> {
    ConfigurableField {
        values: vec![
            TaskType::from("Feature"),
            TaskType::from("Bug"),
            TaskType::from("Chore"),
        ],
    }
}

fn default_issue_priorities() -> ConfigurableField<Priority> {
    ConfigurableField {
        values: vec![
            Priority::from("Low"),
            Priority::from("Medium"),
            Priority::from("High"),
        ],
    }
}

fn default_tags() -> StringConfigField {
    StringConfigField::new_wildcard()
}

fn default_custom_fields() -> StringConfigField {
    StringConfigField::new_wildcard()
}

fn default_scan_signal_words() -> Vec<String> {
    vec![
        "TODO".to_string(),
        "FIXME".to_string(),
        "HACK".to_string(),
        "BUG".to_string(),
        "NOTE".to_string(),
    ]
}

fn default_attachments_dir() -> String {
    "@attachments".to_string()
}

fn default_sync_reports_dir() -> String {
    "@reports".to_string()
}

fn default_attachments_max_upload_mb() -> i64 {
    10
}

fn default_agent_on_start_action() -> AgentAutomationAction {
    AgentAutomationAction {
        set_status: Some(TaskStatus::from("InProgress")),
        reassign_to: None,
    }
}

fn default_agent_on_success_action() -> AgentAutomationAction {
    AgentAutomationAction {
        set_status: Some(TaskStatus::from("NeedsReview")),
        reassign_to: Some("assignee_or_reporter".to_string()),
    }
}

fn default_agent_worktree_branch_prefix() -> String {
    "agent/".to_string()
}

fn default_agent_context_extension() -> String {
    ".context".to_string()
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            server_port: default_port(),
            default_project: default_project_name(),
            issue_states: default_issue_states(),
            issue_types: default_issue_types(),
            issue_priorities: default_issue_priorities(),
            tags: default_tags(),
            default_assignee: None,
            default_reporter: None,
            default_tags: Vec::new(),
            members: Vec::new(),
            strict_members: false,
            auto_populate_members: true,
            auto_set_reporter: true,
            auto_assign_on_status: true,
            auto_codeowners_assign: true,
            auto_tags_from_path: true,
            auto_branch_infer_type: true,
            auto_branch_infer_status: true,
            auto_branch_infer_priority: true,
            default_priority: default_priority(),
            default_status: None,
            custom_fields: default_custom_fields(),
            scan_signal_words: default_scan_signal_words(),
            scan_ticket_patterns: None,
            scan_enable_ticket_words: true,
            scan_enable_mentions: true,
            sprints: SprintConfig::default(),
            // scan mutation policy
            scan_strip_attributes: true,
            branch_type_aliases: HashMap::new(),
            branch_status_aliases: HashMap::new(),
            branch_priority_aliases: HashMap::new(),
            auto_identity: true,
            auto_identity_git: true,
            attachments_dir: default_attachments_dir(),
            attachments_max_upload_mb: default_attachments_max_upload_mb(),
            sync_reports_dir: default_sync_reports_dir(),
            sync_write_reports: true,
            agent_context_enabled: true,
            agent_context_extension: default_agent_context_extension(),
            agent_logs_dir: None,
            agent_instructions: None,
            agents: HashMap::new(),
            agent_automation: AgentAutomationConfig::default(),
            agent_worktree: AgentWorktreeConfig::default(),
            web_ui_path: None,
            remotes: HashMap::new(),
            auth_profiles: HashMap::new(),
        }
    }
}

fn default_true() -> bool {
    true
}

// Helper accessors for optional fields used by normalization without exposing internals
pub fn maybe_scan_ticket_patterns(cfg: &GlobalConfig) -> Option<&Vec<String>> {
    cfg.scan_ticket_patterns.as_ref()
}

pub fn maybe_project_scan_ticket_patterns(cfg: &ProjectConfig) -> Option<&Vec<String>> {
    cfg.scan_ticket_patterns.as_ref()
}

// Constants that are actually used would go here if needed

// Helper accessor for project-level enable flag
pub fn maybe_project_scan_enable_ticket_words(cfg: &ProjectConfig) -> Option<bool> {
    cfg.scan_enable_ticket_words
}

// Helper accessor for project-level mentions flag
pub fn maybe_project_scan_enable_mentions(cfg: &ProjectConfig) -> Option<bool> {
    cfg.scan_enable_mentions
}
