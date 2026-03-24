use crate::api_types::{AgentJobCreateRequest, TaskDTO, TaskUpdate};
use crate::automation::persistence as automation_persistence;
use crate::automation::template::TemplateContext;
use crate::automation::types::{
    AutomationAction, AutomationActionSet, AutomationFile, AutomationRule, AutomationRunAction,
    AutomationTagAction, StringOrVec,
};
use crate::config::manager::ConfigManager;
use crate::config::types::ResolvedConfig;
use crate::config::validation::errors::ValidationResult;
use crate::errors::{LoTaRError, LoTaRResult};
use crate::services::agent_job_service::{AgentJobService, AgentOrchestratorMode};
use crate::services::agent_queue_service::AgentQueueService;
use crate::services::automation_matching::{ChangeSet, MatchMode, matches_rule};
use crate::services::automation_validation::validate_rules;
use crate::services::sprint_metrics::determine_done_statuses_from_config;
use crate::services::sprint_service::SprintService;
use crate::services::sprint_status;
use crate::services::task_service::{TaskService, TaskUpdateContext};
use crate::storage::manager::Storage;
use crate::types::{Priority, TaskStatus, TaskType};
use crate::utils::identity::resolve_me_alias;
use crate::utils::tags::normalize_tags;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const DEFAULT_AUTO_PROMPT: &str = "Work on this ticket using the provided context and agent instructions. Make concrete changes in the repo (code/config/tests), run or update relevant tests, and summarize what changed and how you verified it. If you are blocked or missing information, say what you need and exit non-zero so automation can request help.";
const DEFAULT_MAX_ITERATIONS: u32 = 10;

// ── Cooldown tracker ────────────────────────────────────────────────────────

/// Key: (rule identity, ticket_id).  Rule identity is the rule `name` if present,
/// otherwise its 0-based index in the automation file.
type CooldownKey = (String, String);

static COOLDOWN_STATE: std::sync::LazyLock<Mutex<HashMap<CooldownKey, Instant>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// Parse a human-friendly duration string ("30s", "5m", "2h", "1d") into `Duration`.
fn parse_duration(s: &str) -> Option<Duration> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (digits, suffix) = s.split_at(s.len() - 1);
    let value: u64 = digits.parse().ok()?;
    match suffix {
        "s" => Some(Duration::from_secs(value)),
        "m" => Some(Duration::from_secs(value * 60)),
        "h" => Some(Duration::from_secs(value * 3600)),
        "d" => Some(Duration::from_secs(value * 86400)),
        _ => None,
    }
}

/// Return a stable identity string for a rule (name if present, else index).
fn rule_identity(rule: &AutomationRule, index: usize) -> String {
    rule.name
        .as_deref()
        .map(|n| n.to_string())
        .unwrap_or_else(|| format!("__rule_{index}"))
}

/// Check whether the rule's cooldown allows firing on this ticket.
/// Returns `true` if the rule is allowed to fire (no cooldown or cooldown expired).
fn cooldown_allows(rule: &AutomationRule, rule_key: &str, ticket_id: &str) -> bool {
    let cooldown_str = match rule.cooldown.as_deref() {
        Some(s) => s,
        None => return true,
    };
    let duration = match parse_duration(cooldown_str) {
        Some(d) => d,
        None => return true, // unparseable → don't block
    };
    let key = (rule_key.to_string(), ticket_id.to_string());
    let map = COOLDOWN_STATE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(last_fired) = map.get(&key) {
        last_fired.elapsed() >= duration
    } else {
        true
    }
}

/// Record the current instant as the last-fired time for this rule+ticket.
fn cooldown_record(rule: &AutomationRule, rule_key: &str, ticket_id: &str) {
    if rule.cooldown.is_some() {
        let key = (rule_key.to_string(), ticket_id.to_string());
        let mut map = COOLDOWN_STATE.lock().unwrap_or_else(|e| e.into_inner());
        map.insert(key, Instant::now());
    }
}

/// Reset all cooldown state (useful for tests).
pub fn cooldown_reset() {
    let mut map = COOLDOWN_STATE.lock().unwrap_or_else(|e| e.into_inner());
    map.clear();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutomationEvent {
    // Ticket events
    Created,
    Updated,
    Assigned,
    Commented,
    SprintChanged,

    // Job events
    JobStarted,
    JobCompleted,
    JobFailed,
    JobCancelled,
}

impl std::fmt::Display for AutomationEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Created => "created",
            Self::Updated => "updated",
            Self::Assigned => "assigned",
            Self::Commented => "commented",
            Self::SprintChanged => "sprint_changed",
            Self::JobStarted => "job_started",
            Self::JobCompleted => "job_completed",
            Self::JobFailed => "job_failed",
            Self::JobCancelled => "job_cancelled",
        })
    }
}

impl std::str::FromStr for AutomationEvent {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "created" => Ok(Self::Created),
            "updated" => Ok(Self::Updated),
            "assigned" => Ok(Self::Assigned),
            "commented" => Ok(Self::Commented),
            "sprint_changed" => Ok(Self::SprintChanged),
            "job_started" | "job_start" => Ok(Self::JobStarted),
            "job_completed" | "complete" | "success" => Ok(Self::JobCompleted),
            "job_failed" | "error" | "failure" => Ok(Self::JobFailed),
            "job_cancelled" | "cancel" => Ok(Self::JobCancelled),
            _ => Err(format!(
                "Unknown event: '{}'. Expected: created, updated, assigned, commented, sprint_changed, job_started, job_completed, job_failed, job_cancelled",
                s
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AutomationJobContext {
    pub job_id: String,
    pub runner: String,
    pub agent: Option<String>,
    pub worktree_path: Option<String>,
    pub worktree_branch: Option<String>,
}

#[derive(Debug, Clone)]
struct AutomationActionContext {
    event: AutomationEvent,
    job: Option<AutomationJobContext>,
    previous: Option<TaskDTO>,
    comment_text: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AutomationInspectResult {
    pub scope: AutomationScope,
    pub source: AutomationScope,
    pub scope_exists: bool,
    pub scope_yaml: String,
    pub effective_yaml: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutomationScope {
    Global,
    Home,
    Project,
    BuiltIn,
}

impl AutomationScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            AutomationScope::Global => "global",
            AutomationScope::Home => "home",
            AutomationScope::Project => "project",
            AutomationScope::BuiltIn => "built_in",
        }
    }
}

pub struct AutomationSetOutcome {
    pub updated: bool,
    pub validation: ValidationResult,
}

pub struct AutomationService;

impl AutomationService {
    pub fn inspect(
        tasks_dir: &Path,
        project: Option<&str>,
    ) -> LoTaRResult<AutomationInspectResult> {
        let config = resolve_config_for_project(tasks_dir, project)?;
        let scope = if project.is_some() {
            AutomationScope::Project
        } else {
            AutomationScope::Global
        };
        let (effective, source) = load_effective_automation(tasks_dir, project, Some(&config))?;
        let scope_file = match scope {
            AutomationScope::Project => project
                .and_then(|p| automation_persistence::load_project_automation(tasks_dir, p).ok())
                .flatten(),
            AutomationScope::Global => automation_persistence::load_global_automation(tasks_dir)?,
            AutomationScope::Home => automation_persistence::load_home_automation()?,
            AutomationScope::BuiltIn => None,
        };

        let scope_exists = scope_file.is_some();
        let scope_yaml =
            automation_persistence::to_canonical_yaml(&scope_file.unwrap_or_default())?;
        let effective_yaml = automation_persistence::to_canonical_yaml(&effective)?;

        Ok(AutomationInspectResult {
            scope,
            source,
            scope_exists,
            scope_yaml,
            effective_yaml,
        })
    }

    pub fn set(
        tasks_dir: &Path,
        project: Option<&str>,
        yaml: &str,
    ) -> LoTaRResult<AutomationSetOutcome> {
        let file: AutomationFile = serde_yaml::from_str(yaml).map_err(LoTaRError::from)?;
        let config = resolve_config_for_project(tasks_dir, project)?;
        let validation = validate_rules(&file, &config);

        match project {
            Some(prefix) => {
                automation_persistence::save_project_automation(tasks_dir, prefix, &file)?
            }
            None => automation_persistence::save_global_automation(tasks_dir, &file)?,
        }

        Ok(AutomationSetOutcome {
            updated: true,
            validation,
        })
    }

    pub fn apply_task_update(
        storage: &mut Storage,
        previous: Option<&TaskDTO>,
        current: &TaskDTO,
        config: &ResolvedConfig,
    ) -> LoTaRResult<()> {
        let tasks_dir = storage.root_path.clone();
        let project = current.id.split('-').next().unwrap_or("");
        let (automation, _) = load_effective_automation(
            &tasks_dir,
            if project.is_empty() {
                None
            } else {
                Some(project)
            },
            Some(config),
        )?;
        let changes = ChangeSet::new(previous, current);

        // Implicit agent job queueing: when assignee changes to an agent profile, start a job
        maybe_queue_agent_on_assignment(
            &tasks_dir,
            previous.and_then(|t| t.assignee.as_deref()),
            current.assignee.as_deref(),
            current,
            config,
        );

        // Determine which events to fire
        let is_create = previous.is_none();
        let assignee_changed = previous.is_some()
            && previous.and_then(|t| t.assignee.as_deref()) != current.assignee.as_deref();
        let sprints_changed =
            previous.is_some() && previous.map(|t| &t.sprints) != Some(&current.sprints);

        let events: Vec<AutomationEvent> = if is_create {
            vec![AutomationEvent::Created]
        } else {
            let mut evts = vec![AutomationEvent::Updated];
            if assignee_changed {
                evts.push(AutomationEvent::Assigned);
            }
            if sprints_changed {
                evts.push(AutomationEvent::SprintChanged);
            }
            evts
        };

        let match_mode = MatchMode::OnChange;
        let active_sprints = compute_active_sprint_ids(storage);

        for (rule_idx, rule) in automation.automation.rules().iter().enumerate() {
            if !matches_rule(rule, current, &changes, match_mode, config, &active_sprints) {
                continue;
            }
            let rk = rule_identity(rule, rule_idx);
            if !cooldown_allows(rule, &rk, &current.id) {
                continue;
            }

            let mut fired = false;

            // Fire event-specific hooks
            for &event in &events {
                if let Some(action) = resolve_action_for_event(&rule.on, event) {
                    let action_context = AutomationActionContext {
                        event,
                        job: None,
                        previous: previous.cloned(),
                        comment_text: None,
                    };
                    apply_action(storage, current, config, action, &action_context)?;
                    fired = true;
                }
            }

            // Fire legacy catch-all `start` hook (backward compat)
            if let Some(action) = rule.on.start.as_ref() {
                let primary_event = events.first().copied().unwrap_or(AutomationEvent::Updated);
                let action_context = AutomationActionContext {
                    event: primary_event,
                    job: None,
                    previous: previous.cloned(),
                    comment_text: None,
                };
                apply_action(storage, current, config, action, &action_context)?;
                fired = true;
            }

            if fired {
                cooldown_record(rule, &rk, &current.id);
            }
        }
        Ok(())
    }

    /// Fire automation rules for a comment being added to a task.
    pub fn apply_comment_event(
        storage: &mut Storage,
        task: &TaskDTO,
        comment_text: &str,
        config: &ResolvedConfig,
    ) -> LoTaRResult<()> {
        let tasks_dir = storage.root_path.clone();
        let project = task.id.split('-').next().unwrap_or("");
        let (automation, _) = load_effective_automation(
            &tasks_dir,
            if project.is_empty() {
                None
            } else {
                Some(project)
            },
            Some(config),
        )?;
        let changes = ChangeSet::empty(task);
        let active_sprints = compute_active_sprint_ids(storage);
        let event = AutomationEvent::Commented;

        for (rule_idx, rule) in automation.automation.rules().iter().enumerate() {
            if !matches_rule(
                rule,
                task,
                &changes,
                MatchMode::Current,
                config,
                &active_sprints,
            ) {
                continue;
            }
            let rk = rule_identity(rule, rule_idx);
            if !cooldown_allows(rule, &rk, &task.id) {
                continue;
            }
            if let Some(action) = resolve_action_for_event(&rule.on, event) {
                let action_context = AutomationActionContext {
                    event,
                    job: None,
                    previous: None,
                    comment_text: Some(comment_text.to_string()),
                };
                apply_action(storage, task, config, action, &action_context)?;
                cooldown_record(rule, &rk, &task.id);
            }
        }
        Ok(())
    }

    pub fn apply_job_event(
        tasks_dir: &Path,
        ticket_id: &str,
        event: AutomationEvent,
        job_context: Option<AutomationJobContext>,
    ) -> LoTaRResult<()> {
        let config =
            resolve_config_for_project(tasks_dir, Some(ticket_id.split('-').next().unwrap_or("")))?;
        let mut storage = Storage::new(tasks_dir.to_path_buf());
        let task = TaskService::get(&storage, ticket_id, None)?;
        let (automation, _) = load_effective_automation(
            tasks_dir,
            Some(ticket_id.split('-').next().unwrap_or("")),
            Some(&config),
        )?;

        let action_context = AutomationActionContext {
            event,
            job: job_context,
            previous: None,
            comment_text: None,
        };

        for (rule_idx, rule) in automation.automation.rules().iter().enumerate() {
            if !matches_rule(
                rule,
                &task,
                &ChangeSet::empty(&task),
                MatchMode::Current,
                &config,
                &compute_active_sprint_ids(&storage),
            ) {
                continue;
            }
            let rk = rule_identity(rule, rule_idx);
            if !cooldown_allows(rule, &rk, ticket_id) {
                continue;
            }
            if let Some(action) = resolve_action_for_event(&rule.on, event) {
                apply_action(&mut storage, &task, &config, action, &action_context)?;
                cooldown_record(rule, &rk, ticket_id);
            }
        }
        Ok(())
    }

    /// Simulate automation rules without applying changes
    pub fn simulate(
        tasks_dir: &Path,
        ticket_id: &str,
        event: AutomationEvent,
    ) -> LoTaRResult<SimulateResult> {
        let config =
            resolve_config_for_project(tasks_dir, Some(ticket_id.split('-').next().unwrap_or("")))?;
        let storage = Storage::new(tasks_dir.to_path_buf());
        let task = TaskService::get(&storage, ticket_id, None)?;
        let (automation, _) = load_effective_automation(
            tasks_dir,
            Some(ticket_id.split('-').next().unwrap_or("")),
            Some(&config),
        )?;

        let active_sprints = compute_active_sprint_ids(&storage);

        for rule in automation.automation.rules() {
            if !matches_rule(
                rule,
                &task,
                &ChangeSet::empty(&task),
                MatchMode::Current,
                &config,
                &active_sprints,
            ) {
                continue;
            }
            if let Some(action) = resolve_action_for_event_with_fallback(&rule.on, event) {
                let actions = describe_action(action, &task, &config);
                let task_after = simulate_action(&task, action, &config);
                return Ok(SimulateResult {
                    matched: true,
                    rule_name: rule.name.clone(),
                    actions,
                    task_after: Some(task_after),
                });
            }
        }
        Ok(SimulateResult {
            matched: false,
            rule_name: None,
            actions: vec![],
            task_after: None,
        })
    }
}

/// Result of simulating automation rules
pub struct SimulateResult {
    pub matched: bool,
    pub rule_name: Option<String>,
    pub actions: Vec<SimulatedAction>,
    pub task_after: Option<TaskDTO>,
}

/// A single simulated action
pub struct SimulatedAction {
    pub action: String,
    pub description: String,
}

/// Describe what an automation action would do
fn describe_action(
    action: &AutomationAction,
    current: &TaskDTO,
    config: &ResolvedConfig,
) -> Vec<SimulatedAction> {
    let mut actions = Vec::new();

    if let Some(set) = action.set.as_ref() {
        if let Some(value) = set.status.as_ref() {
            actions.push(SimulatedAction {
                action: "set_status".to_string(),
                description: format!("Set status to '{}'", value),
            });
        }
        if let Some(value) = set.priority.as_ref() {
            actions.push(SimulatedAction {
                action: "set_priority".to_string(),
                description: format!("Set priority to '{}'", value),
            });
        }
        if let Some(value) = set.task_type.as_ref() {
            actions.push(SimulatedAction {
                action: "set_type".to_string(),
                description: format!("Set type to '{}'", value),
            });
        }
        if let Some(value) = set.assignee.as_ref() {
            let resolved = if value == "@me" {
                "current user".to_string()
            } else if value == "@reporter" {
                current
                    .reporter
                    .clone()
                    .unwrap_or_else(|| "reporter".to_string())
            } else if value.starts_with('@') && config.agent_profiles.contains_key(&value[1..]) {
                format!("agent '{}'", &value[1..])
            } else {
                value.clone()
            };
            actions.push(SimulatedAction {
                action: "set_assignee".to_string(),
                description: format!("Set assignee to {}", resolved),
            });
        }
        if let Some(value) = set.reporter.as_ref() {
            actions.push(SimulatedAction {
                action: "set_reporter".to_string(),
                description: format!("Set reporter to '{}'", value),
            });
        }
    }

    if let Some(add) = action.add.as_ref() {
        let tags: Vec<String> = add
            .tags
            .as_ref()
            .map(StringOrVec::as_vec)
            .unwrap_or_default();
        if !tags.is_empty() {
            actions.push(SimulatedAction {
                action: "add_tags".to_string(),
                description: format!("Add tags: {}", tags.join(", ")),
            });
        }
        if let Some(sprint) = add.sprint.as_ref() {
            actions.push(SimulatedAction {
                action: "add_sprint".to_string(),
                description: format!("Add to sprint: {}", sprint),
            });
        }
        for (field, vals) in [
            ("depends_on", &add.depends_on),
            ("blocks", &add.blocks),
            ("related", &add.related),
        ] {
            if let Some(v) = vals {
                actions.push(SimulatedAction {
                    action: format!("add_{}", field),
                    description: format!("Add {}: {}", field, v.as_vec().join(", ")),
                });
            }
        }
    }

    if let Some(remove) = action.remove.as_ref() {
        let tags: Vec<String> = remove
            .tags
            .as_ref()
            .map(StringOrVec::as_vec)
            .unwrap_or_default();
        if !tags.is_empty() {
            actions.push(SimulatedAction {
                action: "remove_tags".to_string(),
                description: format!("Remove tags: {}", tags.join(", ")),
            });
        }
        if let Some(sprint) = remove.sprint.as_ref() {
            actions.push(SimulatedAction {
                action: "remove_sprint".to_string(),
                description: format!("Remove from sprint: {}", sprint),
            });
        }
        for (field, vals) in [
            ("depends_on", &remove.depends_on),
            ("blocks", &remove.blocks),
            ("related", &remove.related),
        ] {
            if let Some(v) = vals {
                actions.push(SimulatedAction {
                    action: format!("remove_{}", field),
                    description: format!("Remove {}: {}", field, v.as_vec().join(", ")),
                });
            }
        }
    }

    if let Some(run) = action.run.as_ref() {
        let description = match run {
            AutomationRunAction::Shell(command) => format!("Run command: {}", command),
            AutomationRunAction::Command(command) => {
                if command.args.is_empty() {
                    format!("Run command: {}", command.command)
                } else {
                    format!(
                        "Run command: {} {}",
                        command.command,
                        command.args.join(" ")
                    )
                }
            }
        };
        actions.push(SimulatedAction {
            action: "run_command".to_string(),
            description,
        });
    }

    if let Some(comment) = action.comment.as_ref() {
        actions.push(SimulatedAction {
            action: "add_comment".to_string(),
            description: format!("Add comment: {}", comment),
        });
    }

    // Detect if assignee change would trigger agent job
    if let Some(set) = action.set.as_ref()
        && let Some(assignee) = set.assignee.as_ref()
        && assignee.starts_with('@')
        && config.agent_profiles.contains_key(&assignee[1..])
    {
        actions.push(SimulatedAction {
            action: "queue_agent".to_string(),
            description: format!("Queue agent job for profile '{}'", &assignee[1..]),
        });
    }

    actions
}

/// Simulate what the task would look like after applying an action
fn simulate_action(
    current: &TaskDTO,
    action: &AutomationAction,
    config: &ResolvedConfig,
) -> TaskDTO {
    let mut simulated = current.clone();

    if let Some(set) = action.set.as_ref() {
        if let Some(value) = set.status.as_ref()
            && let Ok(status) = TaskStatus::parse_with_config(value, config)
        {
            simulated.status = status;
        }
        if let Some(value) = set.priority.as_ref()
            && let Ok(priority) = Priority::parse_with_config(value, config)
        {
            simulated.priority = priority;
        }
        if let Some(value) = set.task_type.as_ref()
            && let Ok(task_type) = TaskType::parse_with_config(value, config)
        {
            simulated.task_type = task_type;
        }
        if let Some(value) = set.title.as_ref() {
            simulated.title = value.clone();
        }
        if let Some(value) = set.description.as_ref() {
            simulated.description = Some(value.clone());
        }
        if let Some(value) = set.assignee.as_ref() {
            simulated.assignee = Some(value.clone());
        }
        if let Some(value) = set.reporter.as_ref() {
            simulated.reporter = Some(value.clone());
        }
        if let Some(values) = set.tags.as_ref().map(StringOrVec::as_vec) {
            simulated.tags = normalize_tags(values);
        }
    }

    if let Some(add) = action.add.as_ref() {
        let tags: Vec<String> = add
            .tags
            .as_ref()
            .map(StringOrVec::as_vec)
            .unwrap_or_default();
        for tag in normalize_tags(tags) {
            if !simulated.tags.contains(&tag) {
                simulated.tags.push(tag);
            }
        }
        apply_relationship_action(add, true, &mut simulated.relationships);
    }

    if let Some(remove) = action.remove.as_ref() {
        let tags: Vec<String> = remove
            .tags
            .as_ref()
            .map(StringOrVec::as_vec)
            .unwrap_or_default();
        let normalized = normalize_tags(tags);
        simulated.tags.retain(|t| !normalized.contains(t));
        apply_relationship_action(remove, false, &mut simulated.relationships);
    }

    simulated
}

/// Look up the action for a specific task event from the `on` block.
/// Does NOT fall back to `start` — the caller handles that for backward compat.
/// Resolve which action a rule should execute for a given event.
///
/// For task events (`Created/Updated/Assigned`) the `start` catch-all is NOT
/// included here; the caller handles the legacy fallback separately so the new
/// event-specific hooks always take priority.
fn resolve_action_for_event(
    on: &crate::automation::types::AutomationRuleActions,
    event: AutomationEvent,
) -> Option<&AutomationAction> {
    match event {
        AutomationEvent::Created => on.created.as_ref(),
        AutomationEvent::Updated => on.updated.as_ref(),
        AutomationEvent::Assigned => on.assigned.as_ref(),
        AutomationEvent::Commented => on.commented.as_ref(),
        AutomationEvent::SprintChanged => on.sprint_changed.as_ref(),
        AutomationEvent::JobStarted => on.job_started.as_ref(),
        AutomationEvent::JobCompleted => on.job_completed.as_ref(),
        AutomationEvent::JobFailed => on.job_failed.as_ref(),
        AutomationEvent::JobCancelled => on.job_cancelled.as_ref(),
    }
}

/// Like `resolve_action_for_event` but falls back to the legacy `start`
/// catch-all for task events (used by the simulate path).
fn resolve_action_for_event_with_fallback(
    on: &crate::automation::types::AutomationRuleActions,
    event: AutomationEvent,
) -> Option<&AutomationAction> {
    resolve_action_for_event(on, event).or(match event {
        AutomationEvent::Created
        | AutomationEvent::Updated
        | AutomationEvent::Assigned
        | AutomationEvent::Commented
        | AutomationEvent::SprintChanged => on.start.as_ref(),
        _ => None,
    })
}

fn resolve_config_for_project(
    tasks_dir: &Path,
    project: Option<&str>,
) -> LoTaRResult<ResolvedConfig> {
    let cfg_mgr = ConfigManager::new_manager_with_tasks_dir_readonly(tasks_dir)
        .map_err(|e| LoTaRError::ValidationError(e.to_string()))?;
    let config = if let Some(prefix) = project
        && !prefix.is_empty()
    {
        cfg_mgr
            .get_project_config(prefix)
            .unwrap_or_else(|_| cfg_mgr.get_resolved_config().clone())
    } else {
        cfg_mgr.get_resolved_config().clone()
    };
    Ok(config)
}

fn compute_active_sprint_ids(storage: &Storage) -> Vec<u32> {
    let records = match SprintService::list(storage) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let now = chrono::Utc::now();
    records
        .iter()
        .filter(|r| {
            matches!(
                sprint_status::derive_status(&r.sprint, now).state,
                sprint_status::SprintLifecycleState::Active
                    | sprint_status::SprintLifecycleState::Overdue
            )
        })
        .map(|r| r.id)
        .collect()
}

fn load_effective_automation(
    tasks_dir: &Path,
    project: Option<&str>,
    _config: Option<&ResolvedConfig>,
) -> LoTaRResult<(AutomationFile, AutomationScope)> {
    if let Some(prefix) = project
        && !prefix.trim().is_empty()
        && let Some(file) = automation_persistence::load_project_automation(tasks_dir, prefix)?
    {
        return Ok((file, AutomationScope::Project));
    }

    if let Some(file) = automation_persistence::load_home_automation()? {
        return Ok((file, AutomationScope::Home));
    }

    if let Some(file) = automation_persistence::load_global_automation(tasks_dir)? {
        return Ok((file, AutomationScope::Global));
    }

    // No automation file found - return empty ruleset
    Ok((AutomationFile::default(), AutomationScope::BuiltIn))
}

/// Build LOTAR_* environment variables for a task in the given project context.
///
/// This is the shared core used by both automation `run` actions and agent runner
/// processes so that every child process sees a consistent set of context vars.
pub(crate) fn build_lotar_env(
    task: &TaskDTO,
    config: &ResolvedConfig,
    tasks_dir: &Path,
    event_label: Option<&str>,
    job: Option<&AutomationJobContext>,
) -> HashMap<String, String> {
    let mut env = HashMap::new();
    let workspace_root = tasks_dir.parent().unwrap_or(tasks_dir);

    env.insert(
        "LOTAR_TASKS_DIR".to_string(),
        tasks_dir.to_string_lossy().to_string(),
    );
    env.insert(
        "LOTAR_WORKSPACE_ROOT".to_string(),
        workspace_root.to_string_lossy().to_string(),
    );
    if let Some(label) = event_label {
        env.insert("LOTAR_AUTOMATION_EVENT".to_string(), label.to_string());
    }
    env.insert("LOTAR_TICKET_ID".to_string(), task.id.clone());
    env.insert("LOTAR_TICKET_STATUS".to_string(), task.status.to_string());
    env.insert(
        "LOTAR_TICKET_PRIORITY".to_string(),
        task.priority.to_string(),
    );
    env.insert("LOTAR_TICKET_TYPE".to_string(), task.task_type.to_string());
    env.insert("LOTAR_TICKET_TITLE".to_string(), task.title.clone());

    if let Some(assignee) = task.assignee.as_ref() {
        env.insert("LOTAR_TICKET_ASSIGNEE".to_string(), assignee.clone());
    }
    if let Some(reporter) = task.reporter.as_ref() {
        env.insert("LOTAR_TICKET_REPORTER".to_string(), reporter.clone());
    }

    if let Some(job) = job {
        env.insert("LOTAR_AGENT_JOB_ID".to_string(), job.job_id.clone());
        env.insert("LOTAR_AGENT_RUNNER".to_string(), job.runner.clone());
        if let Some(agent) = job.agent.as_ref() {
            env.insert("LOTAR_AGENT_PROFILE".to_string(), agent.clone());
        }
        if let Some(path) = job.worktree_path.as_ref() {
            env.insert("LOTAR_AGENT_WORKTREE_PATH".to_string(), path.clone());
        }
        if let Some(branch) = job.worktree_branch.as_ref() {
            env.insert("LOTAR_AGENT_WORKTREE_BRANCH".to_string(), branch.clone());
        }
    }

    if !env.contains_key("LOTAR_AGENT_PROFILE")
        && let Some(assignee) = task.assignee.as_ref()
        && assignee.starts_with('@')
    {
        let trimmed = assignee.trim_start_matches('@');
        if config.agent_profiles.contains_key(trimmed) {
            env.insert("LOTAR_AGENT_PROFILE".to_string(), trimmed.to_string());
        }
    }

    env
}

fn build_automation_env(
    task: &TaskDTO,
    config: &ResolvedConfig,
    tasks_dir: &Path,
    context: &AutomationActionContext,
) -> HashMap<String, String> {
    let event_label = context.event.to_string();
    build_lotar_env(
        task,
        config,
        tasks_dir,
        Some(&event_label),
        context.job.as_ref(),
    )
}

fn resolve_run_cwd(tasks_dir: &Path, raw: Option<&str>) -> LoTaRResult<std::path::PathBuf> {
    let workspace_root = tasks_dir.parent().unwrap_or(tasks_dir);
    let Some(raw) = raw else {
        return Ok(workspace_root.to_path_buf());
    };

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(LoTaRError::ValidationError(
            "Automation run cwd cannot be empty".to_string(),
        ));
    }

    let path = std::path::Path::new(trimmed);
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(LoTaRError::ValidationError(
            "Automation run cwd cannot contain '..'".to_string(),
        ));
    }
    Ok(workspace_root.join(path))
}

fn build_shell_command(raw: &str) -> (String, Vec<String>) {
    if cfg!(windows) {
        ("cmd".to_string(), vec!["/C".to_string(), raw.to_string()])
    } else {
        ("sh".to_string(), vec!["-c".to_string(), raw.to_string()])
    }
}

fn execute_run_action(
    run: &AutomationRunAction,
    task: &TaskDTO,
    config: &ResolvedConfig,
    tasks_dir: &Path,
    context: &AutomationActionContext,
    tmpl: &TemplateContext,
) -> LoTaRResult<()> {
    let (command, args, env, cwd, ignore_failure, wait) = match run {
        AutomationRunAction::Shell(raw) => {
            let expanded = tmpl.expand_shell_safe(raw);
            let trimmed = expanded.trim().to_string();
            if trimmed.is_empty() {
                return Err(LoTaRError::ValidationError(
                    "Automation run command cannot be empty".to_string(),
                ));
            }
            let (program, args) = build_shell_command(&trimmed);
            (program, args, HashMap::new(), None, false, true)
        }
        AutomationRunAction::Command(command) => {
            let expanded_cmd = tmpl.expand(&command.command);
            let trimmed = expanded_cmd.trim().to_string();
            if trimmed.is_empty() {
                return Err(LoTaRError::ValidationError(
                    "Automation run command cannot be empty".to_string(),
                ));
            }
            let args = command.args.iter().map(|a| tmpl.expand(a)).collect();
            let env = command
                .env
                .iter()
                .map(|(k, v)| (k.clone(), tmpl.expand(v)))
                .collect();
            (
                trimmed,
                args,
                env,
                command.cwd.as_ref().map(|c| tmpl.expand(c)),
                command.ignore_failure,
                command.wait,
            )
        }
    };

    let cwd = resolve_run_cwd(tasks_dir, cwd.as_deref())?;
    let mut cmd = Command::new(command);
    cmd.args(args).current_dir(cwd);

    let mut merged_env = build_automation_env(task, config, tasks_dir, context);
    for (key, value) in env {
        merged_env.insert(key, value);
    }
    cmd.envs(merged_env);

    if wait {
        let status = cmd.status()?;
        if !status.success() && !ignore_failure {
            return Err(LoTaRError::ValidationError(format!(
                "Automation run command failed with status {}",
                status
            )));
        }
    } else {
        // Async: spawn, then monitor in a background thread for logging
        cmd.stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| {
            LoTaRError::ValidationError(format!("Failed to spawn async run command: {e}"))
        })?;
        let task_id = task.id.clone();
        std::thread::spawn(move || {
            match child.wait() {
                Ok(status) if !status.success() => {
                    let mut stderr_output = String::new();
                    if let Some(mut stderr) = child.stderr.take() {
                        use std::io::Read;
                        let _ = stderr.read_to_string(&mut stderr_output);
                    }
                    if stderr_output.is_empty() {
                        eprintln!(
                            "[lotar][warn] Async automation run for task {} exited with {}",
                            task_id, status
                        );
                    } else {
                        eprintln!(
                            "[lotar][warn] Async automation run for task {} exited with {}: {}",
                            task_id,
                            status,
                            stderr_output.trim()
                        );
                    }
                }
                Err(e) => {
                    eprintln!(
                        "[lotar][warn] Async automation run for task {} failed to complete: {}",
                        task_id, e
                    );
                }
                _ => {} // success — nothing to log
            }
        });
    }
    Ok(())
}

fn apply_action(
    storage: &mut Storage,
    current: &TaskDTO,
    config: &ResolvedConfig,
    action: &AutomationAction,
    action_context: &AutomationActionContext,
) -> LoTaRResult<()> {
    let tmpl = TemplateContext::from_task(current)
        .with_previous(action_context.previous.as_ref())
        .with_job(action_context.job.as_ref())
        .with_comment(action_context.comment_text.as_deref());

    let mut patch = TaskUpdate::default();
    let mut updated_tags = current.tags.clone();
    let mut custom_fields = current.custom_fields.clone();
    let mut updated_relationships = current.relationships.clone();

    if let Some(set) = action.set.as_ref() {
        apply_set_action(
            set,
            current,
            config,
            storage,
            &mut patch,
            &mut updated_tags,
            &mut custom_fields,
            &tmpl,
        )?;
    }

    if let Some(add) = action.add.as_ref() {
        apply_tag_action(add, true, &mut updated_tags);
        apply_relationship_action(add, true, &mut updated_relationships);
        if let Some(sprint_ref) = add.sprint.as_ref() {
            apply_sprint_action(storage, &current.id, sprint_ref, true)?;
        }
    }
    if let Some(remove) = action.remove.as_ref() {
        apply_tag_action(remove, false, &mut updated_tags);
        apply_relationship_action(remove, false, &mut updated_relationships);
        if let Some(sprint_ref) = remove.sprint.as_ref() {
            apply_sprint_action(storage, &current.id, sprint_ref, false)?;
        }
    }

    if patch.tags.is_none() && updated_tags != current.tags {
        patch.tags = Some(updated_tags);
    }
    if patch.custom_fields.is_none() && custom_fields != current.custom_fields {
        patch.custom_fields = Some(custom_fields);
    }
    if patch.relationships.is_none() && updated_relationships != current.relationships {
        patch.relationships = Some(updated_relationships);
    }

    let mut next_task = None;
    if patch_has_changes(&patch) {
        next_task = Some(TaskService::update_with_context(
            storage,
            &current.id,
            patch,
            TaskUpdateContext::automation_disabled(),
        )?);
    }
    if let Some(updated) = next_task.as_ref() {
        maybe_queue_agent_on_assignment(
            storage.root_path.as_path(),
            current.assignee.as_deref(),
            updated.assignee.as_deref(),
            updated,
            config,
        );
    }

    let task_for_env = next_task.as_ref().unwrap_or(current);

    if let Some(comment_template) = action.comment.as_ref() {
        let text = tmpl.expand(comment_template);
        if !text.trim().is_empty() {
            append_automation_comment(storage, &current.id, &text)?;
        }
    }

    if let Some(run) = action.run.as_ref() {
        execute_run_action(
            run,
            task_for_env,
            config,
            storage.root_path.as_path(),
            action_context,
            &tmpl,
        )?;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn apply_set_action(
    set: &AutomationActionSet,
    current: &TaskDTO,
    config: &ResolvedConfig,
    storage: &Storage,
    patch: &mut TaskUpdate,
    updated_tags: &mut Vec<String>,
    custom_fields: &mut crate::types::CustomFields,
    tmpl: &TemplateContext,
) -> LoTaRResult<()> {
    let tasks_dir = storage.root_path.as_path();
    if let Some(value) = set.status.as_ref()
        && let Ok(status) = TaskStatus::parse_with_config(&tmpl.expand(value), config)
    {
        patch.status = Some(status);
    }
    if let Some(value) = set.priority.as_ref()
        && let Ok(priority) = Priority::parse_with_config(&tmpl.expand(value), config)
    {
        patch.priority = Some(priority);
    }
    if let Some(value) = set.task_type.as_ref()
        && let Ok(task_type) = TaskType::parse_with_config(&tmpl.expand(value), config)
    {
        patch.task_type = Some(task_type);
    }
    if let Some(value) = set.title.as_ref() {
        patch.title = Some(tmpl.expand(value));
    }
    if let Some(value) = set.description.as_ref() {
        patch.description = Some(tmpl.expand(value));
    }
    if let Some(value) = set.due_date.as_ref() {
        patch.due_date = Some(tmpl.expand(value));
    }
    if let Some(value) = set.effort.as_ref() {
        patch.effort = Some(tmpl.expand(value));
    }
    if let Some(value) = set.assignee.as_ref() {
        patch.assignee = resolve_assignee_value(&tmpl.expand(value), current, config, storage);
    }
    if let Some(value) = set.reporter.as_ref() {
        patch.reporter = resolve_reporter_value(&tmpl.expand(value), current, tasks_dir);
    }

    if let Some(values) = set
        .tags
        .as_ref()
        .map(StringOrVec::as_vec)
        .or_else(|| set.labels.as_ref().map(StringOrVec::as_vec))
    {
        *updated_tags = normalize_tags(values);
    }

    if let Some(map) = set.custom_fields.as_ref() {
        for (key, value) in map {
            let converted = convert_custom_field_value(value, key)?;
            custom_fields.insert(key.clone(), converted);
        }
    }

    Ok(())
}

fn apply_tag_action(action: &AutomationTagAction, add: bool, tags: &mut Vec<String>) {
    let mut merged = normalize_tags(tags.clone());
    let values = action
        .tags
        .as_ref()
        .map(StringOrVec::as_vec)
        .into_iter()
        .flatten()
        .chain(
            action
                .labels
                .as_ref()
                .map(StringOrVec::as_vec)
                .into_iter()
                .flatten(),
        )
        .collect::<Vec<_>>();
    if values.is_empty() {
        return;
    }
    let mut set: HashSet<String> = merged.iter().map(|v| v.to_ascii_lowercase()).collect();
    if add {
        for value in values {
            if value.trim().is_empty() {
                continue;
            }
            let key = value.to_ascii_lowercase();
            if !set.contains(&key) {
                set.insert(key.clone());
                merged.push(value);
            }
        }
    } else {
        let remove: HashSet<String> = values.iter().map(|v| v.to_ascii_lowercase()).collect();
        merged.retain(|value| !remove.contains(&value.to_ascii_lowercase()));
    }
    *tags = merged;
}

fn apply_relationship_action(
    action: &AutomationTagAction,
    add: bool,
    relationships: &mut crate::types::TaskRelationships,
) {
    fn modify_vec(vec: &mut Vec<String>, values: &[String], add: bool) {
        if add {
            for v in values {
                let trimmed = v.trim();
                if !trimmed.is_empty() && !vec.iter().any(|e| e.eq_ignore_ascii_case(trimmed)) {
                    vec.push(trimmed.to_string());
                }
            }
        } else {
            let remove: HashSet<String> = values
                .iter()
                .map(|v| v.trim().to_ascii_lowercase())
                .collect();
            vec.retain(|e| !remove.contains(&e.to_ascii_lowercase()));
        }
    }

    if let Some(vals) = action.depends_on.as_ref() {
        modify_vec(&mut relationships.depends_on, &vals.as_vec(), add);
    }
    if let Some(vals) = action.blocks.as_ref() {
        modify_vec(&mut relationships.blocks, &vals.as_vec(), add);
    }
    if let Some(vals) = action.related.as_ref() {
        modify_vec(&mut relationships.related, &vals.as_vec(), add);
    }
}

fn apply_sprint_action(
    storage: &mut Storage,
    task_id: &str,
    sprint_ref: &str,
    add: bool,
) -> LoTaRResult<()> {
    use crate::services::sprint_assignment;

    let records = SprintService::list(storage)?;
    if records.is_empty() {
        return Ok(());
    }

    let reference = match sprint_ref.trim().to_ascii_lowercase().as_str() {
        "@active" | "active" => None, // resolve_sprint_id defaults to active
        other => Some(other.to_string()),
    };

    let sprint_id = match sprint_assignment::resolve_sprint_id(&records, reference.as_deref()) {
        Ok(id) => id,
        Err(_) => return Ok(()), // silently skip if no matching sprint
    };

    let tasks = vec![task_id.to_string()];
    if add {
        let _ = sprint_assignment::assign_tasks(
            storage,
            &records,
            &tasks,
            Some(&sprint_id.to_string()),
            true,  // allow_closed
            false, // force_single
        );
    } else {
        let _ = sprint_assignment::remove_tasks(
            storage,
            &records,
            &tasks,
            Some(&sprint_id.to_string()),
        );
    }
    Ok(())
}

/// Append a comment to a task via raw storage edit (used by automation comment action).
fn append_automation_comment(storage: &mut Storage, task_id: &str, text: &str) -> LoTaRResult<()> {
    let project_prefix = task_id.split('-').next().unwrap_or("");
    let mut task = storage
        .get(task_id, project_prefix.to_string())
        .ok_or_else(|| LoTaRError::TaskNotFound(task_id.to_string()))?;
    let now = chrono::Utc::now().to_rfc3339();
    task.comments.push(crate::types::TaskComment {
        date: now.clone(),
        text: text.to_string(),
    });
    task.history.push(crate::types::TaskChangeLogEntry {
        at: now.clone(),
        actor: Some("automation".to_string()),
        changes: vec![crate::types::TaskChange {
            field: "comment".into(),
            old: None,
            new: Some(text.to_string()),
        }],
    });
    task.modified = now;
    storage.edit(task_id, &task)
}

fn patch_has_changes(patch: &TaskUpdate) -> bool {
    patch.title.is_some()
        || patch.description.is_some()
        || patch.status.is_some()
        || patch.priority.is_some()
        || patch.task_type.is_some()
        || patch.assignee.is_some()
        || patch.reporter.is_some()
        || patch.due_date.is_some()
        || patch.effort.is_some()
        || patch.tags.is_some()
        || patch.custom_fields.is_some()
        || patch.relationships.is_some()
        || patch.sprints.is_some()
}

/// Automatically queue an agent job when the assignee changes to an agent profile.
/// This is the implicit behavior: assigning a ticket to an agent starts a job.
fn maybe_queue_agent_on_assignment(
    tasks_dir: &Path,
    previous_assignee: Option<&str>,
    new_assignee: Option<&str>,
    task: &TaskDTO,
    config: &ResolvedConfig,
) {
    // No change in assignee
    if previous_assignee == new_assignee {
        return;
    }

    // Check if new assignee is an agent profile
    let agent_name = match new_assignee {
        Some(assignee) if assignee.starts_with('@') => {
            let name = assignee.trim_start_matches('@');
            if config.agent_profiles.contains_key(name) {
                name.to_string()
            } else {
                return;
            }
        }
        _ => return,
    };

    // Don't queue if there's already an active job for this ticket
    if AgentJobService::has_active_job(&task.id) {
        return;
    }

    // Check max_iterations safety net
    if let Ok((automation, _)) = load_effective_automation(
        tasks_dir,
        Some(task.id.split('-').next().unwrap_or("")),
        Some(config),
    ) {
        let limit = automation
            .automation
            .max_iterations()
            .unwrap_or(DEFAULT_MAX_ITERATIONS);
        let terminal_count = count_terminal_jobs_for_ticket(&task.id);
        if terminal_count >= limit as usize {
            let mut storage = Storage::new(tasks_dir.to_path_buf());
            let _ = TaskService::update_with_context(
                &mut storage,
                &task.id,
                TaskUpdate {
                    tags: Some({
                        let mut tags = task.tags.clone();
                        if !tags.iter().any(|t| t == "automation-limit-reached") {
                            tags.push("automation-limit-reached".to_string());
                        }
                        tags
                    }),
                    ..Default::default()
                },
                TaskUpdateContext::automation_disabled(),
            );
            return;
        }
    }

    let mut storage = Storage::new(tasks_dir.to_path_buf());
    let blocked_by = find_blocked_dependencies(&storage, task, config);
    if !blocked_by.is_empty() {
        let _ = mark_task_blocked(&mut storage, task, config, &blocked_by);
        return;
    }

    let req = AgentJobCreateRequest {
        ticket_id: task.id.clone(),
        prompt: DEFAULT_AUTO_PROMPT.to_string(),
        runner: None,
        agent: Some(agent_name),
    };

    if matches!(
        AgentJobService::orchestrator_mode(),
        AgentOrchestratorMode::Server
    ) {
        let _ = AgentJobService::start_job_with_tasks_dir(req, tasks_dir);
    } else {
        let _ = AgentQueueService::enqueue(tasks_dir, req);
    }
}

/// Count how many jobs have reached a terminal state (completed/failed) for the given ticket.
fn count_terminal_jobs_for_ticket(ticket_id: &str) -> usize {
    AgentJobService::list_jobs()
        .iter()
        .filter(|j| j.ticket_id == ticket_id && (j.status == "completed" || j.status == "failed"))
        .count()
}

fn find_blocked_dependencies(
    storage: &Storage,
    task: &TaskDTO,
    config: &ResolvedConfig,
) -> Vec<String> {
    if task.relationships.depends_on.is_empty() {
        return Vec::new();
    }

    let done_statuses = determine_done_statuses_from_config(config);
    let mut blocked = Vec::new();

    for dep in &task.relationships.depends_on {
        let trimmed = dep.trim();
        if trimmed.is_empty() {
            continue;
        }
        let prefix = trimmed.split('-').next().unwrap_or("");
        let status = storage
            .get(trimmed, prefix.to_string())
            .map(|task| task.status)
            .map(|status| status.as_str().to_ascii_lowercase());

        let is_done = status
            .as_deref()
            .is_some_and(|value| done_statuses.contains(value));
        if !is_done {
            blocked.push(trimmed.to_string());
        }
    }

    blocked
}

fn mark_task_blocked(
    storage: &mut Storage,
    task: &TaskDTO,
    config: &ResolvedConfig,
    blocked_by: &[String],
) -> LoTaRResult<()> {
    let mut patch = TaskUpdate::default();
    if let Ok(status) = TaskStatus::parse_with_config("HelpNeeded", config) {
        patch.status = Some(status);
    }
    if let Some(reporter) = task.reporter.as_ref() {
        patch.assignee = Some(reporter.clone());
    }

    let mut tags = task.tags.clone();
    push_unique_tag(&mut tags, "blocked");
    for dep in blocked_by {
        push_unique_tag(&mut tags, &format!("blocked-by:{}", dep));
    }
    patch.tags = Some(normalize_tags(tags));

    if patch_has_changes(&patch) {
        TaskService::update_with_context(
            storage,
            &task.id,
            patch,
            TaskUpdateContext::automation_disabled(),
        )?;
    }

    Ok(())
}

fn push_unique_tag(tags: &mut Vec<String>, tag: &str) {
    let trimmed = tag.trim();
    if trimmed.is_empty() {
        return;
    }
    if tags
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(trimmed))
    {
        return;
    }
    tags.push(trimmed.to_string());
}

fn resolve_assignee_value(
    value: &str,
    task: &TaskDTO,
    config: &ResolvedConfig,
    storage: &Storage,
) -> Option<String> {
    let normalized = value.trim();
    match normalized.to_ascii_lowercase().as_str() {
        "@assignee" => task.assignee.clone(),
        "@reporter" => task.reporter.clone(),
        "@assignee_or_reporter" => {
            if let Some(assignee) = task.assignee.clone()
                && config
                    .agent_profiles
                    .keys()
                    .any(|name| assignee.eq_ignore_ascii_case(&format!("@{}", name)))
            {
                return task.reporter.clone();
            }
            task.assignee.clone().or_else(|| task.reporter.clone())
        }
        "@round_robin" => resolve_round_robin(config, storage),
        "@random" => resolve_random(config),
        "@least_busy" => resolve_least_busy(config, storage),
        _ => resolve_me_alias(normalized, Some(storage.root_path.as_path())),
    }
}

/// Global round-robin counter for cycling through project members.
static ROUND_ROBIN_COUNTER: std::sync::LazyLock<Mutex<usize>> =
    std::sync::LazyLock::new(|| Mutex::new(0));

/// Filter project members to exclude @-prefixed special tokens and agent profiles.
fn real_members(config: &ResolvedConfig) -> Vec<String> {
    config
        .members
        .iter()
        .filter(|m| !m.starts_with('@'))
        .filter(|m| {
            !config
                .agent_profiles
                .keys()
                .any(|name| m.eq_ignore_ascii_case(name))
        })
        .cloned()
        .collect()
}

fn resolve_round_robin(config: &ResolvedConfig, _storage: &Storage) -> Option<String> {
    let members = real_members(config);
    if members.is_empty() {
        return None;
    }
    let mut counter = ROUND_ROBIN_COUNTER
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let idx = *counter % members.len();
    *counter = counter.wrapping_add(1);
    Some(members[idx].clone())
}

fn resolve_random(config: &ResolvedConfig) -> Option<String> {
    let members = real_members(config);
    if members.is_empty() {
        return None;
    }
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    // Use a deterministic but varied seed based on current time
    let mut hasher = DefaultHasher::new();
    std::time::SystemTime::now().hash(&mut hasher);
    std::thread::current().id().hash(&mut hasher);
    let idx = (hasher.finish() as usize) % members.len();
    Some(members[idx].clone())
}

fn resolve_least_busy(config: &ResolvedConfig, storage: &Storage) -> Option<String> {
    let members = real_members(config);
    if members.is_empty() {
        return None;
    }
    // Count active (non-done) tasks per member
    let done_statuses = determine_done_statuses_from_config(config);
    let all_tasks = TaskService::list(storage, &crate::api_types::TaskListFilter::default());

    let mut counts: HashMap<String, usize> = members.iter().map(|m| (m.clone(), 0)).collect();
    for (_id, task) in &all_tasks {
        if let Some(assignee) = task.assignee.as_deref() {
            let is_active = !done_statuses
                .iter()
                .any(|s| s.to_string().eq_ignore_ascii_case(&task.status.to_string()));
            if is_active && let Some(count) = counts.get_mut(assignee) {
                *count += 1;
            }
        }
    }

    members
        .into_iter()
        .min_by_key(|m| counts.get(m).copied().unwrap_or(0))
}

fn resolve_reporter_value(value: &str, task: &TaskDTO, tasks_dir: &Path) -> Option<String> {
    let normalized = value.trim();
    match normalized.to_ascii_lowercase().as_str() {
        "@assignee" => task.assignee.clone(),
        "@reporter" => task.reporter.clone(),
        "@assignee_or_reporter" => task.assignee.clone().or_else(|| task.reporter.clone()),
        _ => resolve_me_alias(normalized, Some(tasks_dir)),
    }
}

#[cfg(not(feature = "schema"))]
fn convert_custom_field_value(
    value: &serde_yaml::Value,
    _key: &str,
) -> LoTaRResult<crate::types::CustomFieldValue> {
    Ok(value.clone())
}

#[cfg(feature = "schema")]
fn convert_custom_field_value(
    value: &serde_yaml::Value,
    key: &str,
) -> LoTaRResult<crate::types::CustomFieldValue> {
    serde_json::to_value(value).map_err(|err| {
        LoTaRError::SerializationError(format!("Invalid custom field value for {}: {}", key, err))
    })
}
