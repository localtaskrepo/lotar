use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AutomationFile {
    #[serde(default)]
    pub automation: AutomationRuleSet,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AutomationRuleSet {
    List(Vec<AutomationRule>),
    Rules {
        rules: Vec<AutomationRule>,
        #[serde(default)]
        max_iterations: Option<u32>,
    },
}

impl Default for AutomationRuleSet {
    fn default() -> Self {
        AutomationRuleSet::Rules {
            rules: Vec::new(),
            max_iterations: None,
        }
    }
}

impl AutomationRuleSet {
    pub fn rules(&self) -> &[AutomationRule] {
        match self {
            AutomationRuleSet::List(rules) => rules,
            AutomationRuleSet::Rules { rules, .. } => rules,
        }
    }

    pub fn into_rules(self) -> Vec<AutomationRule> {
        match self {
            AutomationRuleSet::List(rules) => rules,
            AutomationRuleSet::Rules { rules, .. } => rules,
        }
    }

    pub fn max_iterations(&self) -> Option<u32> {
        match self {
            AutomationRuleSet::List(_) => None,
            AutomationRuleSet::Rules { max_iterations, .. } => *max_iterations,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AutomationRule {
    #[serde(default)]
    pub name: Option<String>,
    /// Per-rule cooldown duration string (e.g. "60s", "5m").
    /// When set, the rule will not fire again on the same ticket within this duration.
    #[serde(default)]
    pub cooldown: Option<String>,
    #[serde(default, alias = "triggers")]
    pub when: Option<AutomationConditionGroup>,
    #[serde(default)]
    pub on: AutomationRuleActions,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AutomationRuleActions {
    // Legacy catch-all: fires on any matching task change (create or update).
    // New configs should use specific event hooks below instead.
    #[serde(default, alias = "match")]
    pub start: Option<AutomationAction>,

    // Ticket events
    #[serde(default)]
    pub created: Option<AutomationAction>,
    #[serde(default)]
    pub updated: Option<AutomationAction>,
    #[serde(default)]
    pub assigned: Option<AutomationAction>,
    #[serde(default)]
    pub commented: Option<AutomationAction>,
    #[serde(default)]
    pub sprint_changed: Option<AutomationAction>,

    // Job events (canonical names; old names kept as serde aliases)
    #[serde(default, alias = "job_start")]
    pub job_started: Option<AutomationAction>,
    #[serde(default, alias = "complete", alias = "job_success", alias = "success")]
    pub job_completed: Option<AutomationAction>,
    #[serde(default, alias = "error", alias = "job_failure", alias = "failure")]
    pub job_failed: Option<AutomationAction>,
    #[serde(default, alias = "cancel", alias = "job_cancel")]
    pub job_cancelled: Option<AutomationAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AutomationConditionGroup {
    #[serde(default)]
    pub all: Option<Vec<AutomationConditionGroup>>,
    #[serde(default)]
    pub any: Option<Vec<AutomationConditionGroup>>,
    #[serde(default)]
    pub not: Option<Box<AutomationConditionGroup>>,
    #[serde(default)]
    pub changes: Option<HashMap<String, AutomationChangeCondition>>,
    #[serde(default)]
    pub custom_fields: HashMap<String, AutomationFieldCondition>,
    #[serde(flatten)]
    pub fields: HashMap<String, AutomationFieldCondition>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AutomationChangeCondition {
    #[serde(default)]
    pub from: Option<AutomationFieldCondition>,
    #[serde(default)]
    pub to: Option<AutomationFieldCondition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AutomationFieldCondition {
    Scalar(String),
    List(Vec<String>),
    Map(Box<AutomationFieldConditionMap>),
}

impl AutomationFieldCondition {
    pub fn as_map(&self) -> AutomationFieldConditionMap {
        match self {
            AutomationFieldCondition::Scalar(value) => AutomationFieldConditionMap {
                equals: Some(value.clone()),
                ..AutomationFieldConditionMap::default()
            },
            AutomationFieldCondition::List(values) => AutomationFieldConditionMap {
                any: Some(values.clone()),
                ..AutomationFieldConditionMap::default()
            },
            AutomationFieldCondition::Map(map) => *map.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AutomationFieldConditionMap {
    #[serde(default)]
    pub equals: Option<String>,
    #[serde(default)]
    pub r#in: Option<Vec<String>>,
    #[serde(default)]
    pub contains: Option<String>,
    #[serde(default)]
    pub any: Option<Vec<String>>,
    #[serde(default)]
    pub all: Option<Vec<String>>,
    #[serde(default)]
    pub none: Option<Vec<String>>,
    #[serde(default)]
    pub starts_with: Option<String>,
    #[serde(default)]
    pub exists: Option<bool>,
    #[serde(default)]
    pub matches: Option<String>,
    /// Date is before this reference ("today", "YYYY-MM-DD")
    #[serde(default)]
    pub before: Option<String>,
    /// Date is within this duration from now ("3d", "1w")
    #[serde(default)]
    pub within: Option<String>,
    /// Date is older than this duration ("30d", "2w")
    #[serde(default)]
    pub older_than: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AutomationAction {
    #[serde(default)]
    pub set: Option<AutomationActionSet>,
    #[serde(default)]
    pub add: Option<AutomationTagAction>,
    #[serde(default)]
    pub remove: Option<AutomationTagAction>,
    #[serde(default)]
    pub run: Option<AutomationRunAction>,
    #[serde(default)]
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AutomationRunCommand {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub ignore_failure: bool,
    #[serde(default = "default_wait")]
    pub wait: bool,
}

fn default_wait() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AutomationRunAction {
    Shell(String),
    Command(AutomationRunCommand),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AutomationActionSet {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub reporter: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default, rename = "type")]
    pub task_type: Option<String>,
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Option<StringOrVec>,
    #[serde(default, alias = "label", alias = "labels")]
    pub labels: Option<StringOrVec>,
    #[serde(default)]
    pub custom_fields: Option<HashMap<String, serde_yaml::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AutomationTagAction {
    #[serde(default)]
    pub tags: Option<StringOrVec>,
    #[serde(default, alias = "label", alias = "labels")]
    pub labels: Option<StringOrVec>,
    #[serde(default)]
    pub sprint: Option<String>,
    #[serde(default, alias = "blocked_by")]
    pub depends_on: Option<StringOrVec>,
    #[serde(default)]
    pub blocks: Option<StringOrVec>,
    #[serde(default, alias = "references")]
    pub related: Option<StringOrVec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StringOrVec {
    Scalar(String),
    List(Vec<String>),
}

impl StringOrVec {
    pub fn into_vec(self) -> Vec<String> {
        match self {
            StringOrVec::Scalar(value) => vec![value],
            StringOrVec::List(values) => values,
        }
    }

    pub fn as_vec(&self) -> Vec<String> {
        match self {
            StringOrVec::Scalar(value) => vec![value.clone()],
            StringOrVec::List(values) => values.clone(),
        }
    }
}
