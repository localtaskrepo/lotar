use crate::api_types::{AgentJobCreateRequest, TaskDTO, TaskUpdate};
use crate::automation::persistence as automation_persistence;
use crate::automation::template::TemplateContext;
use crate::automation::types::{
    AutomationAction, AutomationActionSet, AutomationFile, AutomationRule, AutomationRunAction,
    AutomationTagAction, StringOrVec,
};
use crate::config::types::ResolvedConfig;
use crate::config::validation::errors::ValidationResult;
use crate::errors::{LoTaRError, LoTaRResult};
use crate::services::agent_job_service::{AgentJobService, AgentOrchestratorMode};
use crate::services::agent_queue_service::AgentQueueService;
use crate::services::automation_matching::{ChangeSet, MatchMode, matches_rule};
use crate::services::automation_validation::{parse_cooldown, validate_rules};
use crate::services::sprint_service::SprintService;
use crate::services::task_service::{CommentContext, TaskService, TaskUpdateContext};
use crate::storage::manager::Storage;
use crate::types::{Priority, TaskStatus, TaskType};
use crate::utils::identity::resolve_me_alias;
use crate::utils::tags::normalize_tags;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;
use std::time::Instant;

const DEFAULT_AUTO_PROMPT: &str = "Work on this ticket using the provided context and agent instructions. Make concrete changes in the repo (code/config/tests), run or update relevant tests, and summarize what changed and how you verified it. If you are blocked or missing information, hand the ticket back as your instructions describe.";
const DEFAULT_MAX_ITERATIONS: u32 = 10;

// ── Cooldown tracker ────────────────────────────────────────────────────────

/// Key: (rule identity, ticket_id).  Rule identity is the rule `name` if present,
/// otherwise its 0-based index in the automation file.
type CooldownKey = (String, String);

static COOLDOWN_STATE: std::sync::LazyLock<Mutex<HashMap<CooldownKey, Instant>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

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
    let duration = match parse_cooldown(cooldown_str) {
        Some(d) => d,
        None => return false, // Invalid hand-edited rules must not disable loop protection.
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
    /// What the job used, once it finished (for `${{agent.cost_usd}}` and friends).
    pub usage: Option<crate::api_types::AgentJobUsage>,
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
        let file: AutomationFile = serde_yaml_ng::from_str(yaml).map_err(LoTaRError::from)?;
        let config = resolve_config_for_project(tasks_dir, project)?;
        let validation = validate_rules(&file, &config);
        if validation.has_errors() {
            return Err(LoTaRError::ValidationError(validation.to_string()));
        }

        // DEV-102 no-op boundary: the incoming file is fully parsed and
        // validated first, and the CURRENT stored file must read and parse
        // before equality is considered (a malformed current file blocks
        // the change instead of being silently replaced). When the stored
        // complete rules are already effective-equal to the submission,
        // the write is skipped: bytes, mtime, and formatting (including
        // comments and unknown extensions) are preserved, caches stay
        // untouched, and callers see `updated: false`. A missing file is
        // creation semantics and always writes.
        let scope_label = if project.is_some() {
            "project"
        } else {
            "global"
        };
        let current = match project {
            Some(prefix) => automation_persistence::load_project_automation(tasks_dir, prefix),
            None => automation_persistence::load_global_automation(tasks_dir),
        }
        .map_err(|e| {
            LoTaRError::ValidationError(format!(
                "Failed to read current {} automation configuration: {}",
                scope_label, e
            ))
        })?;
        if current
            .as_ref()
            .is_some_and(|current| automation_persistence::files_effectively_equal(current, &file))
        {
            return Ok(AutomationSetOutcome {
                updated: false,
                validation,
            });
        }

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
        let project = crate::storage::TaskId::parse(&current.id)
            .map(|parsed| parsed.project)
            .unwrap_or_default();
        let (automation, _) = load_effective_automation(
            &tasks_dir,
            if project.is_empty() {
                None
            } else {
                Some(project.as_str())
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
            AgentRunSettings::default(),
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
        let project = crate::storage::TaskId::parse(&task.id)
            .map(|parsed| parsed.project)
            .unwrap_or_default();
        let (automation, _) = load_effective_automation(
            &tasks_dir,
            if project.is_empty() {
                None
            } else {
                Some(project.as_str())
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
        let project_prefix = crate::storage::TaskId::parse(ticket_id)
            .map(|parsed| parsed.project)
            .unwrap_or_default();
        let config = resolve_config_for_project(tasks_dir, Some(&project_prefix))?;
        let mut storage = Storage::new(tasks_dir);
        let task = TaskService::get(&storage, ticket_id, None)?;
        let (automation, _) =
            load_effective_automation(tasks_dir, Some(project_prefix.as_str()), Some(&config))?;

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

    /// Whether an automation rule moves this ticket when its agent job ends,
    /// judged on the ticket as it is now (the assignee that started the job).
    pub fn handles_job_end(tasks_dir: &Path, ticket_id: &str) -> bool {
        let project_prefix = crate::storage::TaskId::parse(ticket_id)
            .map(|parsed| parsed.project)
            .unwrap_or_default();
        let Ok(config) = resolve_config_for_project(tasks_dir, Some(&project_prefix)) else {
            return false;
        };
        let storage = Storage::new(tasks_dir);
        let Ok(task) = TaskService::get(&storage, ticket_id, None) else {
            return false;
        };
        let Ok((automation, _)) =
            load_effective_automation(tasks_dir, Some(project_prefix.as_str()), Some(&config))
        else {
            return false;
        };
        let active_sprints = compute_active_sprint_ids(&storage);
        automation.automation.rules().iter().any(|rule| {
            rule.on.job_completed.is_some()
                && matches_rule(
                    rule,
                    &task,
                    &ChangeSet::empty(&task),
                    MatchMode::Current,
                    &config,
                    &active_sprints,
                )
        })
    }

    /// Simulate automation rules without applying changes
    pub fn simulate(
        tasks_dir: &Path,
        ticket_id: &str,
        event: AutomationEvent,
    ) -> LoTaRResult<SimulateResult> {
        let project_prefix = crate::storage::TaskId::parse(ticket_id)
            .map(|parsed| parsed.project)
            .unwrap_or_default();
        let config = resolve_config_for_project(tasks_dir, Some(&project_prefix))?;
        let storage = Storage::new(tasks_dir);
        let task = TaskService::get(&storage, ticket_id, None)?;
        let (automation, _) =
            load_effective_automation(tasks_dir, Some(project_prefix.as_str()), Some(&config))?;

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
                let mut preview = PreviewContext::new(&task);
                let actions = describe_action(action, &task, &config, &mut preview);
                let task_after = simulate_action(&task, action, &config, &storage, &mut preview);
                return Ok(SimulateResult {
                    matched: true,
                    rule_name: rule.name.clone(),
                    actions,
                    task_after: Some(task_after),
                    warnings: preview.into_warnings(),
                });
            }
        }
        Ok(SimulateResult {
            matched: false,
            rule_name: None,
            actions: vec![],
            task_after: None,
            warnings: vec![],
        })
    }
}

/// Result of simulating automation rules
pub struct SimulateResult {
    pub matched: bool,
    pub rule_name: Option<String>,
    pub actions: Vec<SimulatedAction>,
    pub task_after: Option<TaskDTO>,
    /// Parts of the preview that cannot be known until the rule really runs.
    pub warnings: Vec<String>,
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
    preview: &mut PreviewContext,
) -> Vec<SimulatedAction> {
    let mut actions = Vec::new();
    fn push(actions: &mut Vec<SimulatedAction>, action: &str, description: String) {
        actions.push(SimulatedAction {
            action: action.to_string(),
            description,
        });
    }

    if let Some(set) = action.set.as_ref() {
        let expand = |preview: &mut PreviewContext, value: &String| preview.expand(value);
        if let Some(value) = set.status.as_ref() {
            push(
                &mut actions,
                "set_status",
                format!("Set status to '{}'", expand(preview, value)),
            );
        }
        if let Some(value) = set.priority.as_ref() {
            push(
                &mut actions,
                "set_priority",
                format!("Set priority to '{}'", expand(preview, value)),
            );
        }
        if let Some(value) = set.task_type.as_ref() {
            push(
                &mut actions,
                "set_type",
                format!("Set type to '{}'", expand(preview, value)),
            );
        }
        if let Some(value) = set.assignee.as_ref() {
            let value = expand(preview, value);
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
            push(
                &mut actions,
                "set_assignee",
                format!("Set assignee to {}", resolved),
            );
        }
        if let Some(value) = set.reporter.as_ref() {
            push(
                &mut actions,
                "set_reporter",
                format!("Set reporter to '{}'", expand(preview, value)),
            );
        }
        if let Some(value) = set.title.as_ref() {
            push(
                &mut actions,
                "set_title",
                format!("Set title to '{}'", expand(preview, value)),
            );
        }
        if let Some(value) = set.description.as_ref() {
            push(
                &mut actions,
                "set_description",
                format!("Set description to '{}'", expand(preview, value)),
            );
        }
        if let Some(value) = set.due_date.as_ref() {
            push(
                &mut actions,
                "set_due_date",
                format!("Set due date to '{}'", expand(preview, value)),
            );
        }
        if let Some(value) = set.effort.as_ref() {
            push(
                &mut actions,
                "set_effort",
                format!("Set effort to '{}'", expand(preview, value)),
            );
        }
        if let Some(values) = set
            .tags
            .as_ref()
            .or(set.labels.as_ref())
            .map(StringOrVec::as_vec)
        {
            push(
                &mut actions,
                "set_tags",
                format!("Set tags: {}", values.join(", ")),
            );
        }
        if let Some(map) = set.custom_fields.as_ref() {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for key in keys {
                let shown = convert_custom_field_value(&map[key], key)
                    .map(|value| crate::types::custom_value_to_string(&value))
                    .unwrap_or_default();
                push(
                    &mut actions,
                    "set_custom_field",
                    format!("Set custom field '{}' to '{}'", key, shown),
                );
            }
        }
    }

    if let Some(add) = action.add.as_ref() {
        let tags = tag_action_values(add);
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
        let tags = tag_action_values(remove);
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
            AutomationRunAction::Shell(command) => {
                format!("Run command: {}", preview.expand(command))
            }
            AutomationRunAction::Command(command) => {
                if command.args.is_empty() {
                    format!("Run command: {}", preview.expand(&command.command))
                } else {
                    let args: Vec<String> =
                        command.args.iter().map(|arg| preview.expand(arg)).collect();
                    format!(
                        "Run command: {} {}",
                        preview.expand(&command.command),
                        args.join(" ")
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
            description: format!("Add comment: {}", preview.expand(comment)),
        });
    }

    // Detect if assignee change would trigger agent job
    if let Some(set) = action.set.as_ref()
        && let Some(assignee) = set.assignee.as_ref()
        && assignee.starts_with('@')
        && config.agent_profiles.contains_key(&assignee[1..])
    {
        let mut description = format!("Queue agent job for profile '{}'", &assignee[1..]);
        if let Some(agent) = action.agent.as_ref() {
            let settings: Vec<String> = [("model", &agent.model), ("effort", &agent.effort)]
                .into_iter()
                .filter_map(|(name, value)| value.as_ref().map(|value| format!("{name} {value}")))
                .chain(
                    agent
                        .prompt
                        .as_ref()
                        .map(|prompt| format!("prompt: {}", preview.expand(prompt))),
                )
                .collect();
            if !settings.is_empty() {
                description.push_str(&format!(" ({})", settings.join(", ")));
            }
        }
        actions.push(SimulatedAction {
            action: "queue_agent".to_string(),
            description,
        });
    }

    actions
}

/// Tag values of an add/remove action, including the `labels` alias.
fn tag_action_values(action: &AutomationTagAction) -> Vec<String> {
    action
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
        .collect()
}

/// Template expansion and warnings for one preview (DEV-118).
struct PreviewContext {
    tmpl: TemplateContext,
    unresolved: Vec<String>,
    warnings: Vec<String>,
}

impl PreviewContext {
    fn new(task: &TaskDTO) -> Self {
        Self {
            tmpl: TemplateContext::from_task(task),
            unresolved: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn expand(&mut self, input: &str) -> String {
        self.tmpl.expand_preview(input, &mut self.unresolved)
    }

    fn warn(&mut self, message: String) {
        if !self.warnings.contains(&message) {
            self.warnings.push(message);
        }
    }

    fn into_warnings(mut self) -> Vec<String> {
        for key in std::mem::take(&mut self.unresolved) {
            self.warnings.push(format!(
                "Template value '${{{{{}}}}}' comes from the triggering event and is shown unexpanded",
                key
            ));
        }
        self.warnings
    }
}

/// Assignee tokens whose result is only decided when the rule runs.
/// `@round_robin` advances a shared counter, so previewing it would change
/// the next real assignment.
fn is_runtime_assignee(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "@round_robin" | "@random"
    )
}

/// Preview what the task would look like after applying an action, with the
/// same field coverage as `apply_action` (DEV-118). Nothing is written:
/// sprint membership, comments and run commands are listed in the described
/// actions but not applied to the preview.
fn simulate_action(
    current: &TaskDTO,
    action: &AutomationAction,
    config: &ResolvedConfig,
    storage: &Storage,
    preview: &mut PreviewContext,
) -> TaskDTO {
    let mut simulated = current.clone();

    if let Some(set) = action.set.as_ref() {
        let plan = SetPlan::parse(set, config, |value| preview.expand(value));
        for (field, value) in &plan.rejected {
            preview.warn(format!(
                "Unknown {} '{}' would be skipped",
                set_enum_label(*field),
                value
            ));
        }
        if let Some(status) = plan.status {
            simulated.status = status;
        }
        if let Some(priority) = plan.priority {
            simulated.priority = priority;
        }
        if let Some(task_type) = plan.task_type {
            simulated.task_type = task_type;
        }
        if let Some(title) = plan.title {
            simulated.title = title;
        }
        if let Some(description) = plan.description {
            simulated.description = (!description.is_empty()).then_some(description);
        }
        if let Some(due_date) = plan.due_date {
            simulated.due_date = (!due_date.trim().is_empty()).then_some(due_date);
        }
        if let Some(effort) = plan.effort {
            simulated.effort = if effort.is_empty() {
                None
            } else {
                Some(
                    crate::utils::effort::parse_effort(&effort)
                        .map(|parsed| parsed.canonical)
                        .unwrap_or(effort),
                )
            };
        }
        if let Some(value) = plan.assignee.as_deref() {
            if is_runtime_assignee(value) {
                preview.warn(format!(
                    "Assignee '{}' is chosen when the rule runs; the preview shows the token",
                    value.trim()
                ));
                simulated.assignee = Some(value.trim().to_string());
            } else if let Some(resolved) = resolve_assignee_value(value, current, config, storage) {
                simulated.assignee = Some(resolved);
            }
        }
        if let Some(value) = plan.reporter.as_deref()
            && let Some(resolved) =
                resolve_reporter_value(value, current, storage.root_path.as_path())
        {
            simulated.reporter = Some(resolved);
        }
        // `tags` wins over the `labels` alias, as on apply.
        if let Some(values) = plan.tags.or(plan.labels) {
            simulated.tags = normalize_tags(values);
        }
        if let Some(map) = plan.custom_fields {
            for (key, value) in map {
                match convert_custom_field_value(value, key) {
                    Ok(converted) => {
                        simulated.custom_fields.insert(key.clone(), converted);
                    }
                    Err(err) => preview.warn(format!(
                        "Custom field '{}' would fail to apply: {}",
                        key, err
                    )),
                }
            }
        }
    }

    for (tag_action, add) in [(action.add.as_ref(), true), (action.remove.as_ref(), false)] {
        let Some(tag_action) = tag_action else {
            continue;
        };
        apply_tag_action(tag_action, add, &mut simulated.tags);
        apply_relationship_action(tag_action, add, &mut simulated.relationships);
        if tag_action.sprint.is_some() {
            preview.warn(
                "Sprint membership changes are listed in actions but not reflected in task_after"
                    .to_string(),
            );
        }
    }

    // The simulated state must reflect the projected completion/due
    // classification (DEV-21), not the pre-action snapshot.
    simulated.task_state = Some(crate::services::completion::compute_task_state(
        &simulated.status,
        simulated.due_date.as_deref(),
        config,
        chrono::Utc::now(),
    ));

    simulated
}

fn set_enum_label(field: SetEnumField) -> &'static str {
    match field {
        SetEnumField::Status => "status",
        SetEnumField::Priority => "priority",
        SetEnumField::Type => "type",
    }
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
    crate::config::resolution::config_for_project(tasks_dir, project)
        .map_err(|e| LoTaRError::ValidationError(e.to_string()))
}

fn compute_active_sprint_ids(storage: &Storage) -> Vec<u32> {
    match SprintService::list(storage) {
        Ok(records) => crate::services::sprint_assignment::active_sprint_ids(&records),
        Err(_) => Vec::new(),
    }
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
    if let Some((project, _)) = task.id.rsplit_once('-') {
        env.insert("LOTAR_TICKET_PROJECT".to_string(), project.to_string());
    }
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
        // Async: spawn, then monitor in background threads for logging.
        // Keep detached commands independent of the caller's stdio handles.
        // stdin/stdout are NUL and stderr is captured by a pipe that a
        // dedicated reader drains while the exit is awaited, so a chatty
        // child cannot deadlock on a full pipe before its exit is observed.
        //
        // Windows additionally needs the child to not inherit THIS
        // process's own stdio pipe write ends: see
        // `windows_async_stdio::detach_stdio_for_async_children`.
        #[cfg(windows)]
        windows_async_stdio::detach_stdio_for_async_children();
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped());
        let child = cmd.spawn().map_err(|e| {
            LoTaRError::ValidationError(format!("Failed to spawn async run command: {e}"))
        })?;
        spawn_async_run_monitor(child, task.id.clone());
    }
    Ok(())
}

/// Bounded stderr retained for async run failure logging.
const ASYNC_STDERR_LOG_LIMIT: usize = 8 * 1024;

/// How long the monitor waits for stderr EOF after the async child exits.
/// A descendant that inherited the pipe's write end keeps EOF away for its
/// own lifetime; the monitor must report without following it.
const ASYNC_STDERR_EOF_GRACE: std::time::Duration = std::time::Duration::from_secs(1);

/// Bounded byte buffer keeping the final `limit` bytes and counting what
/// was dropped from the front, so logged stderr can say how much was
/// skipped instead of growing without bound.
#[derive(Default)]
struct BoundedTail {
    data: Vec<u8>,
    dropped: usize,
    limit: usize,
}

impl BoundedTail {
    fn new(limit: usize) -> Self {
        Self {
            data: Vec::new(),
            dropped: 0,
            limit,
        }
    }

    fn push(&mut self, bytes: &[u8]) {
        if self.limit == 0 {
            self.dropped += bytes.len();
            return;
        }
        self.data.extend_from_slice(bytes);
        if self.data.len() > self.limit {
            let overflow = self.data.len() - self.limit;
            self.data.drain(..overflow);
            self.dropped += overflow;
        }
    }

    fn snapshot(&self) -> String {
        let mut text = String::from_utf8_lossy(&self.data).into_owned();
        if self.dropped > 0 && !self.data.is_empty() {
            text.insert_str(0, &format!("[+{} earlier stderr bytes]\n", self.dropped));
        }
        text
    }
}

/// Monitor a fire-and-forget automation child: one reader thread drains
/// stderr concurrently (so pipe backpressure can never wedge the child
/// while it runs), while this thread observes the exit and then waits at
/// most [`ASYNC_STDERR_EOF_GRACE`] for EOF. If the pipe stays open past
/// the child's exit — a descendant inherited its write end — the monitor
/// logs the bounded tail collected so far instead of blocking until that
/// descendant exits.
fn spawn_async_run_monitor(mut child: std::process::Child, task_id: String) {
    use std::io::Read;

    let stderr = child.stderr.take();
    let tail = std::sync::Arc::new(std::sync::Mutex::new(BoundedTail::new(
        ASYNC_STDERR_LOG_LIMIT,
    )));
    let (eof_tx, eof_rx) = std::sync::mpsc::channel::<()>();
    if let Some(mut stderr) = stderr {
        let tail = std::sync::Arc::clone(&tail);
        std::thread::spawn(move || {
            let mut chunk = [0u8; 4096];
            loop {
                match stderr.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => tail
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .push(&chunk[..n]),
                }
            }
            let _ = eof_tx.send(());
        });
    } else {
        let _ = eof_tx.send(());
    }

    std::thread::spawn(move || {
        let status = match child.wait() {
            Ok(status) => status,
            Err(e) => {
                eprintln!(
                    "[lotar][warn] Async automation run for task {task_id} failed to complete: {e}"
                );
                return;
            }
        };
        let drained = stderr_eof_within_grace(&eof_rx, ASYNC_STDERR_EOF_GRACE);
        let stderr_text = tail.lock().unwrap_or_else(|e| e.into_inner()).snapshot();
        let held_note = if drained {
            ""
        } else {
            " (stderr pipe still open, likely held by a descendant; tail truncated)"
        };
        if !status.success() {
            if stderr_text.trim().is_empty() {
                eprintln!(
                    "[lotar][warn] Async automation run for task {task_id} exited with {status}{held_note}"
                );
            } else {
                eprintln!(
                    "[lotar][warn] Async automation run for task {task_id} exited with {status}{held_note}: {}",
                    stderr_text.trim()
                );
            }
        } else if !drained {
            eprintln!(
                "[lotar][warn] Async automation run for task {task_id} exited successfully; a descendant still holds its stderr pipe"
            );
        }
    });
}

/// EOF (or a vanished reader) counts as drained; a pipe that stays silent
/// past `grace` is treated as held open by a descendant.
fn stderr_eof_within_grace(
    eof_rx: &std::sync::mpsc::Receiver<()>,
    grace: std::time::Duration,
) -> bool {
    matches!(
        eof_rx.recv_timeout(grace),
        Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected)
    )
}

/// Windows stdio inheritance scope for fire-and-forget automation runs.
///
/// Stable `std` spawns Windows children through `CreateProcessW` with
/// `bInheritHandles=TRUE` and cannot attach
/// `PROC_THREAD_ATTRIBUTE_HANDLE_LIST` (the `CommandExt` attribute APIs
/// are nightly-only), so every child inherits ALL currently-inheritable
/// handles of this process — including our own stdout/stderr pipe write
/// ends, which whatever spawned us (Node/execa/libuv) created inheritable.
/// A `wait: false` child then holds those write ends for its whole
/// lifetime, and any observer awaiting OUR stdio EOF (execa resolves on
/// `close` = exit + streams ended) blocks until the child exits: a
/// fire-and-forget run silently couples the caller's return to the child.
///
/// Clearing `HANDLE_FLAG_INHERIT` on our own std slots removes exactly
/// those handles from every future child. `Stdio::inherit` spawns keep
/// working because std duplicates the std slot into a fresh inheritable
/// handle at spawn time (`Stdio::to_handle` → `Handle::duplicate(0, true,
/// DUPLICATE_SAME_ACCESS)`), so explicit inheritance never depended on
/// the parent flag. Only this process's own handle-table entries change;
/// no system-wide state is touched, and console handles that refuse the
/// flag change are left as-is.
#[cfg(windows)]
mod windows_async_stdio {
    use std::sync::Once;

    type Handle = *mut core::ffi::c_void;

    pub(super) const STD_OUTPUT_HANDLE: u32 = 0xFFFF_FFF5;
    pub(super) const STD_ERROR_HANDLE: u32 = 0xFFFF_FFF4;
    pub(super) const HANDLE_FLAG_INHERIT: u32 = 0x0000_0001;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetStdHandle(nStdHandle: u32) -> Handle;
        fn SetHandleInformation(hObject: Handle, dwMask: u32, dwFlags: u32) -> i32;
        #[cfg(test)]
        fn GetHandleInformation(hObject: Handle, lpdwFlags: *mut u32) -> i32;
        #[cfg(test)]
        fn GetFileType(hObject: Handle) -> u32;
    }

    static DETACH_ONCE: Once = Once::new();

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(super) enum SlotDetach {
        NoHandle,
        Cleared,
        NotClearable,
    }

    pub(super) fn clear_std_slot(slot: u32) -> SlotDetach {
        // SAFETY: both calls take plain handle values; null and
        // INVALID_HANDLE_VALUE mean "no such stdio slot".
        unsafe {
            let handle = GetStdHandle(slot);
            if handle.is_null() || handle == usize::MAX as Handle {
                return SlotDetach::NoHandle;
            }
            if SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) != 0 {
                SlotDetach::Cleared
            } else {
                SlotDetach::NotClearable
            }
        }
    }

    /// Idempotent, process-lifetime detach of this process's own stdio
    /// write ends from every future child's inherited handle set.
    pub(super) fn detach_stdio_for_async_children() {
        DETACH_ONCE.call_once(|| {
            let _ = clear_std_slot(STD_OUTPUT_HANDLE);
            let _ = clear_std_slot(STD_ERROR_HANDLE);
        });
    }

    #[cfg(test)]
    pub(super) fn std_slot_flags(slot: u32) -> Option<u32> {
        // SAFETY: GetHandleInformation writes a u32 flags value.
        unsafe {
            let handle = GetStdHandle(slot);
            if handle.is_null() || handle == usize::MAX as Handle {
                return None;
            }
            let mut flags: u32 = 0;
            if GetHandleInformation(handle, &mut flags) != 0 {
                Some(flags)
            } else {
                None
            }
        }
    }

    /// Windows `GetFileType` code for the std slot, so tests can separate
    /// captured pipe stdio (the detach must verifiably succeed there) from
    /// console/character handles (which legitimately refuse the flag
    /// change). `FILE_TYPE_UNKNOWN` (0) covers absent handles too.
    #[cfg(test)]
    pub(super) fn std_slot_file_type(slot: u32) -> u32 {
        // SAFETY: GetFileType returns a DWORD file-type code for the handle.
        unsafe {
            let handle = GetStdHandle(slot);
            if handle.is_null() || handle == usize::MAX as Handle {
                return 0;
            }
            GetFileType(handle)
        }
    }
}

#[cfg(test)]
mod action_read_barrier {
    use std::cell::RefCell;
    thread_local! { static HOOK: RefCell<Option<Box<dyn FnOnce()>>> = const { RefCell::new(None) }; }
    pub(super) fn install(hook: impl FnOnce() + 'static) {
        HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
    }
    pub(super) fn pause() {
        if let Some(hook) = HOOK.with(|slot| slot.borrow_mut().take()) {
            hook();
        }
    }
}

/// Apply one automation action.
///
/// Collection updates (tags, custom fields, relationships) compose against
/// the freshest persisted task state so sequential actions across rules and
/// hooks in one dispatch keep earlier committed changes instead of
/// overwriting them with event-time snapshots. `current` stays the
/// event-time snapshot: templates and assignee alias tokens keep resolving
/// the pre-change values that conditions were matched against.
fn apply_action(
    storage: &mut Storage,
    current: &TaskDTO,
    config: &ResolvedConfig,
    action: &AutomationAction,
    action_context: &AutomationActionContext,
) -> LoTaRResult<()> {
    // Compose against the latest persisted state, including side effects of
    // earlier actions in this dispatch (collection commits, sprint
    // assignment, comments).
    let base = TaskService::get(storage, &current.id, None)?;
    #[cfg(test)]
    action_read_barrier::pause();

    let tmpl = TemplateContext::from_task(current)
        .with_previous(action_context.previous.as_ref())
        .with_job(action_context.job.as_ref())
        .with_comment(action_context.comment_text.as_deref());

    let mut patch = TaskUpdate::default();
    let mut updated_tags = base.tags.clone();
    let mut custom_fields = base.custom_fields.clone();
    let has_relationship_delta = [action.add.as_ref(), action.remove.as_ref()]
        .into_iter()
        .flatten()
        .any(|action| {
            action.depends_on.is_some() || action.blocks.is_some() || action.related.is_some()
        });

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
        if let Some(sprint_ref) = add.sprint.as_ref() {
            apply_sprint_action(storage, &current.id, sprint_ref, true)?;
        }
    }
    if let Some(remove) = action.remove.as_ref() {
        apply_tag_action(remove, false, &mut updated_tags);
        if let Some(sprint_ref) = remove.sprint.as_ref() {
            apply_sprint_action(storage, &current.id, sprint_ref, false)?;
        }
    }

    if patch.tags.is_none() && updated_tags != base.tags {
        patch.tags = Some(updated_tags);
    }
    if patch.custom_fields.is_none() && custom_fields != base.custom_fields {
        patch.custom_fields = Some(custom_fields);
    }

    let mut next_task = None;
    if has_relationship_delta {
        next_task = Some(TaskService::update_with_relationship_delta(
            storage,
            &base.id,
            patch,
            TaskUpdateContext::automation_disabled(),
            |relationships| {
                if let Some(add) = &action.add {
                    apply_relationship_action(add, true, relationships);
                }
                if let Some(remove) = &action.remove {
                    apply_relationship_action(remove, false, relationships);
                }
            },
        )?);
    } else if patch_has_changes(&patch) {
        next_task = Some(TaskService::update_with_context(
            storage,
            &base.id,
            patch,
            TaskUpdateContext::automation_disabled(),
        )?);
    }
    if let Some(updated) = next_task.as_ref() {
        let run = action.agent.as_ref().map(|agent| AgentRunSettings {
            model: agent.model.clone(),
            effort: agent.effort.clone(),
            prompt: agent
                .prompt
                .as_ref()
                .map(|prompt| tmpl.expand(prompt))
                .filter(|prompt| !prompt.trim().is_empty()),
        });
        maybe_queue_agent_on_assignment(
            storage.root_path.as_path(),
            base.assignee.as_deref(),
            updated.assignee.as_deref(),
            updated,
            config,
            run.unwrap_or_default(),
        );
    }

    let task_for_env = next_task.as_ref().unwrap_or(&base);

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

/// Enum fields of a `set` block, named as in automation YAML.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SetEnumField {
    Status,
    Priority,
    Type,
}

/// Typed view of an action's `set` block, parsed once and shared by apply,
/// preview (simulate) and validation so the supported fields and enum
/// parsing cannot drift between them (DEV-118).
///
/// Text values are passed through `expand` (template expansion on apply,
/// identity elsewhere). Enum values that do not parse against the project
/// config are skipped and reported in `rejected`. Assignee and reporter stay
/// raw: resolving aliases such as `@me` or agent profiles is the caller's job.
#[derive(Debug, Default)]
pub(crate) struct SetPlan<'a> {
    pub status: Option<TaskStatus>,
    pub priority: Option<Priority>,
    pub task_type: Option<TaskType>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub due_date: Option<String>,
    pub effort: Option<String>,
    pub assignee: Option<String>,
    pub reporter: Option<String>,
    /// Values from `tags`.
    pub tags: Option<Vec<String>>,
    /// Values from the `labels`/`label` alias.
    pub labels: Option<Vec<String>>,
    pub custom_fields: Option<&'a HashMap<String, serde_yaml_ng::Value>>,
    /// Enum inputs (after expansion) that did not parse.
    pub rejected: Vec<(SetEnumField, String)>,
}

impl<'a> SetPlan<'a> {
    pub(crate) fn parse(
        set: &'a AutomationActionSet,
        config: &ResolvedConfig,
        mut expand: impl FnMut(&str) -> String,
    ) -> Self {
        let mut plan = SetPlan::default();
        if let Some(value) = set.status.as_ref() {
            let value = expand(value);
            match TaskStatus::parse_with_config(&value, config) {
                Ok(status) => plan.status = Some(status),
                Err(_) => plan.rejected.push((SetEnumField::Status, value)),
            }
        }
        if let Some(value) = set.priority.as_ref() {
            let value = expand(value);
            match Priority::parse_with_config(&value, config) {
                Ok(priority) => plan.priority = Some(priority),
                Err(_) => plan.rejected.push((SetEnumField::Priority, value)),
            }
        }
        if let Some(value) = set.task_type.as_ref() {
            let value = expand(value);
            match TaskType::parse_with_config(&value, config) {
                Ok(task_type) => plan.task_type = Some(task_type),
                Err(_) => plan.rejected.push((SetEnumField::Type, value)),
            }
        }
        plan.title = set.title.as_deref().map(&mut expand);
        plan.description = set.description.as_deref().map(&mut expand);
        plan.due_date = set.due_date.as_deref().map(&mut expand);
        plan.effort = set.effort.as_deref().map(&mut expand);
        plan.assignee = set.assignee.as_deref().map(&mut expand);
        plan.reporter = set.reporter.as_deref().map(&mut expand);
        plan.tags = set.tags.as_ref().map(StringOrVec::as_vec);
        plan.labels = set.labels.as_ref().map(StringOrVec::as_vec);
        plan.custom_fields = set.custom_fields.as_ref();
        plan
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "independent inputs from the single caller; bundling them only for this lint would not clarify the call"
)]
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
    let plan = SetPlan::parse(set, config, |value| tmpl.expand(value));
    if let Some(status) = plan.status {
        patch.status = Some(status.to_string());
    }
    if let Some(priority) = plan.priority {
        patch.priority = Some(priority.to_string());
    }
    if let Some(task_type) = plan.task_type {
        patch.task_type = Some(task_type.to_string());
    }
    if let Some(title) = plan.title {
        patch.title = Some(title);
    }
    if let Some(description) = plan.description {
        patch.description = Some(description);
    }
    if let Some(due_date) = plan.due_date {
        patch.due_date = Some(due_date);
    }
    if let Some(effort) = plan.effort {
        patch.effort = Some(effort);
    }
    if let Some(value) = plan.assignee.as_deref() {
        patch.assignee = resolve_assignee_value(value, current, config, storage);
    }
    if let Some(value) = plan.reporter.as_deref() {
        patch.reporter = resolve_reporter_value(value, current, tasks_dir);
    }

    // `tags` wins over the `labels` alias when both are present.
    if let Some(values) = plan.tags.or(plan.labels) {
        *updated_tags = normalize_tags(values);
    }

    if let Some(map) = plan.custom_fields {
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
        fn identity(value: &str) -> String {
            crate::storage::TaskId::parse(value)
                .map(|id| id.canonical())
                .unwrap_or_else(|_| value.to_string())
        }
        if add {
            for v in values {
                let trimmed = v.trim();
                let key = identity(trimmed);
                if !trimmed.is_empty() && !vec.iter().any(|e| identity(e) == key) {
                    vec.push(key);
                }
            }
        } else {
            let remove: HashSet<String> = values.iter().map(|v| identity(v.trim())).collect();
            vec.retain(|e| !remove.contains(&identity(e)));
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

    let mut records = SprintService::list(storage)?;
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
            &mut records,
            &tasks,
            Some(&sprint_id.to_string()),
            true,  // allow_closed
            false, // force_single
        );
    } else {
        let _ = sprint_assignment::remove_tasks(
            storage,
            &mut records,
            &tasks,
            Some(&sprint_id.to_string()),
        );
    }
    Ok(())
}

/// Append a comment to a task through the shared comment pipeline (used
/// by the automation comment action). The comment is recorded under the
/// `automation` actor and never re-dispatches `on.commented` rules, so
/// comment actions cannot recurse.
fn append_automation_comment(storage: &mut Storage, task_id: &str, text: &str) -> LoTaRResult<()> {
    TaskService::add_comment_with_context(storage, task_id, text, CommentContext::automation())
        .map(|_| ())
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

/// Run settings an automation action gives the job its assignment starts.
#[derive(Debug, Default)]
struct AgentRunSettings {
    model: Option<String>,
    effort: Option<String>,
    prompt: Option<String>,
}

/// Automatically queue an agent job when the assignee changes to an agent profile.
/// This is the implicit behavior: assigning a ticket to an agent starts a job.
fn maybe_queue_agent_on_assignment(
    tasks_dir: &Path,
    previous_assignee: Option<&str>,
    new_assignee: Option<&str>,
    task: &TaskDTO,
    config: &ResolvedConfig,
    run: AgentRunSettings,
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
    let project_prefix = crate::storage::TaskId::parse(&task.id)
        .ok()
        .map(|parsed| parsed.project)
        .unwrap_or_default();
    if let Ok((automation, _)) =
        load_effective_automation(tasks_dir, Some(project_prefix.as_str()), Some(config))
    {
        let limit = automation
            .automation
            .max_iterations()
            .unwrap_or(DEFAULT_MAX_ITERATIONS);
        let terminal_count = count_terminal_jobs_for_ticket(&task.id);
        if terminal_count >= limit as usize {
            let mut storage = Storage::new(tasks_dir);
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

    let mut storage = Storage::new(tasks_dir);
    let blocked_by = find_blocked_dependencies(&storage, task, config);
    if !blocked_by.is_empty() {
        let _ = mark_task_blocked(&mut storage, task, config, &blocked_by);
        return;
    }

    let req = AgentJobCreateRequest {
        ticket_id: task.id.clone(),
        prompt: run
            .prompt
            .unwrap_or_else(|| DEFAULT_AUTO_PROMPT.to_string()),
        runner: None,
        agent: Some(agent_name),
        model: run.model,
        effort: run.effort,
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
        .filter(|j| {
            crate::storage::identity::aliases_match(&j.ticket_id, ticket_id)
                && (j.status == "completed" || j.status == "failed")
        })
        .count()
}

fn find_blocked_dependencies(
    storage: &Storage,
    task: &TaskDTO,
    _config: &ResolvedConfig,
) -> Vec<String> {
    crate::services::relationships::blocked_dependencies(
        storage,
        &task.id,
        &task.relationships.depends_on,
    )
    .unwrap_or_else(|error| vec![format!("dependency graph unavailable: {error}")])
}

fn mark_task_blocked(
    storage: &mut Storage,
    task: &TaskDTO,
    config: &ResolvedConfig,
    blocked_by: &[String],
) -> LoTaRResult<()> {
    let mut patch = TaskUpdate::default();
    if let Ok(status) = TaskStatus::parse_with_config("HelpNeeded", config) {
        patch.status = Some(status.to_string());
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
    // Count active (non-done) tasks per member. DEV-21: each task's
    // activity classifies under its own project completion policy via the
    // server-computed task state, so cross-project workloads compare
    // fairly.
    let all_tasks = TaskService::list(storage, &crate::api_types::TaskListFilter::default());

    let mut counts: HashMap<String, usize> = members.iter().map(|m| (m.clone(), 0)).collect();
    for (_id, task) in &all_tasks {
        if let Some(assignee) = task.assignee.as_deref() {
            let is_active = !task.task_state.as_ref().is_some_and(|state| state.is_done);
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
    value: &serde_yaml_ng::Value,
    _key: &str,
) -> LoTaRResult<crate::types::CustomFieldValue> {
    Ok(value.clone())
}

#[cfg(feature = "schema")]
fn convert_custom_field_value(
    value: &serde_yaml_ng::Value,
    key: &str,
) -> LoTaRResult<crate::types::CustomFieldValue> {
    serde_json::to_value(value).map_err(|err| {
        LoTaRError::SerializationError(format!("Invalid custom field value for {}: {}", key, err))
    })
}

#[cfg(test)]
mod cooldown_tests {
    use super::*;
    use crate::automation::types::AutomationRuleSet;
    use std::time::Duration;

    #[test]
    fn cooldown_parser_checks_units_and_arithmetic() {
        for (input, seconds) in [
            ("0s", 0),
            ("30s", 30),
            (" 5m ", 300),
            ("2h", 7200),
            ("1d", 86400),
        ] {
            assert_eq!(parse_cooldown(input), Some(Duration::from_secs(seconds)));
        }
        for input in [
            "",
            "5",
            "-1s",
            "1.5m",
            "1\u{e9}",
            "\u{1d11e}",
            "18446744073709551615d",
            "18446744073709551616s",
        ] {
            assert_eq!(parse_cooldown(input), None, "invalid cooldown: {input}");
            let rule = AutomationRule {
                cooldown: Some(input.into()),
                ..Default::default()
            };
            assert!(!cooldown_allows(&rule, "invalid", "TEST-1"));
        }
        assert!(cooldown_allows(
            &AutomationRule::default(),
            "absent",
            "TEST-1"
        ));
    }

    #[test]
    fn invalid_cooldown_save_preserves_existing_rules() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("automation.yml");
        let original = "automation: []\n";
        std::fs::write(&path, original).unwrap();
        for input in ["", "1\u{e9}", "18446744073709551615h"] {
            let file = AutomationFile {
                automation: AutomationRuleSet::List(vec![AutomationRule {
                    cooldown: Some(input.into()),
                    ..Default::default()
                }]),
            };
            let yaml = serde_yaml_ng::to_string(&file).unwrap();
            let result = AutomationService::set(dir.path(), None, &yaml);
            assert!(
                matches!(result, Err(LoTaRError::ValidationError(ref message)) if message.contains("cooldown"))
            );
            assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        }
        let outcome =
            AutomationService::set(dir.path(), None, "automation:\n- cooldown: 30s\n").unwrap();
        assert!(outcome.updated);
        assert!(!outcome.validation.has_errors());
    }
}

#[cfg(test)]
mod dev21_cross_root_dependency_tests {
    use super::*;
    use crate::api_types::TaskDTO;

    #[test]
    fn padded_alias_actions_preserve_noop_bytes_history_events_and_remove_both_assertions() {
        use crate::api_types::{
            RelationshipField, RelationshipMutationRequest, RelationshipOperation,
        };
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        write_config(&workspace, "Done");
        write_task(&workspace, "PX", 1, "Todo");
        write_task(&workspace, "BB", 1, "Todo");
        let mut storage = Storage {
            root_path: workspace.join(".tasks"),
        };
        crate::services::relationships::mutate(
            &storage,
            &RelationshipMutationRequest {
                source: "PX-1".into(),
                target: "BB-1".into(),
                kind: RelationshipField::DependsOn,
                operation: RelationshipOperation::Add,
                from_kind: None,
                expected_current: None,
            },
        )
        .unwrap();
        let current = TaskService::get(&storage, "PX-1", None).unwrap();
        let config = TaskService::resolve_config_for_project(&storage.root_path, "PX");
        let context = AutomationActionContext {
            event: AutomationEvent::Updated,
            job: None,
            previous: None,
            comment_text: None,
        };
        let source = storage.root_path.join("PX/1.yml");
        let target = storage.root_path.join("BB/1.yml");
        let before_source = std::fs::read(&source).unwrap();
        let before_target = std::fs::read(&target).unwrap();
        let events = crate::api_events::subscribe();
        let add: AutomationAction =
            serde_yaml_ng::from_str("add:\n  depends_on: [BB-001]\n").unwrap();
        apply_action(&mut storage, &current, &config, &add, &context).unwrap();
        assert_eq!(std::fs::read(&source).unwrap(), before_source);
        assert_eq!(std::fs::read(&target).unwrap(), before_target);
        assert!(events.try_iter().next().is_none());
        let remove: AutomationAction =
            serde_yaml_ng::from_str("remove:\n  depends_on: [BB-001]\n").unwrap();
        apply_action(&mut storage, &current, &config, &remove, &context).unwrap();
        let after = TaskService::get(&storage, "PX-1", None).unwrap();
        assert!(after.relationships.depends_on.is_empty());
        assert!(
            TaskService::get(&storage, "BB-1", None)
                .unwrap()
                .relationships
                .blocks
                .is_empty()
        );
        assert_eq!(after.history.len(), current.history.len() + 1);
        let emitted: Vec<_> = events.try_iter().collect();
        assert_eq!(
            emitted
                .iter()
                .filter(|event| event.kind == "task_updated")
                .count(),
            2
        );
    }

    #[test]
    fn relationship_deltas_compose_after_concurrent_user_commit_and_keep_rule_atomic() {
        use crate::api_types::{
            RelationshipField, RelationshipMutationRequest, RelationshipOperation,
        };
        use std::fs;
        use std::sync::mpsc;
        use std::time::Duration;
        for removing in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let workspace = temp.path().join("workspace");
            write_config(&workspace, "Done");
            for number in 1..=3 {
                write_task(&workspace, "PX", number, "Todo");
            }
            let storage = Storage {
                root_path: workspace.join(".tasks"),
            };
            let request = RelationshipMutationRequest {
                source: "PX-1".into(),
                target: "PX-2".into(),
                kind: RelationshipField::DependsOn,
                operation: RelationshipOperation::Add,
                from_kind: None,
                expected_current: None,
            };
            if removing {
                crate::services::relationships::mutate(&storage, &request).unwrap();
            }
            let current = TaskService::get(&storage, "PX-1", None).unwrap();
            let config = TaskService::resolve_config_for_project(&storage.root_path, "PX");
            let action: AutomationAction = serde_yaml_ng::from_str(&format!(
                "set:\n  title: rule edit\n{}:\n  depends_on: [PX-2]\n",
                if removing { "remove" } else { "add" }
            ))
            .unwrap();
            let (read_tx, read_rx) = mpsc::channel();
            let (resume_tx, resume_rx) = mpsc::channel();
            let root = storage.root_path.clone();
            let worker = std::thread::spawn(move || {
                action_read_barrier::install(move || {
                    read_tx.send(()).unwrap();
                    resume_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                });
                apply_action(
                    &mut Storage { root_path: root },
                    &current,
                    &config,
                    &action,
                    &AutomationActionContext {
                        event: AutomationEvent::Updated,
                        job: None,
                        previous: None,
                        comment_text: None,
                    },
                )
            });
            read_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            crate::services::relationships::mutate(
                &storage,
                &RelationshipMutationRequest {
                    target: "PX-3".into(),
                    kind: RelationshipField::Blocks,
                    ..request
                },
            )
            .unwrap();
            resume_tx.send(()).unwrap();
            worker.join().unwrap().unwrap();
            let updated = TaskService::get(&storage, "PX-1", None).unwrap();
            assert_eq!(updated.title, "rule edit");
            assert_eq!(updated.relationships.blocks, ["PX-3"]);
            assert_eq!(
                TaskService::get(&storage, "PX-3", None)
                    .unwrap()
                    .relationships
                    .depends_on,
                ["PX-1"]
            );
            assert_eq!(
                updated.relationships.depends_on,
                if removing {
                    vec![]
                } else {
                    vec!["PX-2".to_string()]
                }
            );
            assert_eq!(
                TaskService::get(&storage, "PX-2", None)
                    .unwrap()
                    .relationships
                    .blocks,
                if removing {
                    vec![]
                } else {
                    vec!["PX-1".to_string()]
                }
            );

            let before: Vec<_> = (1..=3)
                .map(|n| fs::read(storage.root_path.join(format!("PX/{n}.yml"))).unwrap())
                .collect();
            let invalid: AutomationAction = serde_yaml_ng::from_str(
                "set:\n  title: must not publish\nadd:\n  depends_on: [ABSENT-99]\n",
            )
            .unwrap();
            let mut current_storage = Storage {
                root_path: storage.root_path.clone(),
            };
            let config = TaskService::resolve_config_for_project(&storage.root_path, "PX");
            assert!(
                apply_action(
                    &mut current_storage,
                    &updated,
                    &config,
                    &invalid,
                    &AutomationActionContext {
                        event: AutomationEvent::Updated,
                        job: None,
                        previous: None,
                        comment_text: None
                    }
                )
                .is_err()
            );
            for n in 1..=3 {
                assert_eq!(
                    fs::read(storage.root_path.join(format!("PX/{n}.yml"))).unwrap(),
                    before[n - 1]
                );
            }
            assert!(!storage.root_path.join("ABSENT").exists());
        }
    }

    #[test]
    fn relationship_actions_preserve_exact_case_task_identities() {
        let action: AutomationTagAction = serde_yaml_ng::from_str("depends_on: [aa-1]\n").unwrap();
        let mut relationships = crate::types::TaskRelationships {
            depends_on: vec!["AA-1".into()],
            ..Default::default()
        };
        apply_relationship_action(&action, true, &mut relationships);
        assert_eq!(relationships.depends_on, ["AA-1", "aa-1"]);
        apply_relationship_action(&action, false, &mut relationships);
        assert_eq!(relationships.depends_on, ["AA-1"]);
    }

    fn write_task(root: &std::path::Path, prefix: &str, num: u32, status: &str) {
        let dir = root.join(".tasks").join(prefix);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{num}.yml")),
            format!(
                "title: t{num}\nstatus: {status}\npriority: medium\ntype: task\ncreated: 2026-01-01T00:00:00Z\nmodified: 2026-01-01T00:00:00Z\ntags: []\n"
            ),
        )
        .unwrap();
    }

    fn write_config(root: &std::path::Path, done: &str) {
        let tasks = root.join(".tasks");
        std::fs::create_dir_all(&tasks).unwrap();
        std::fs::write(
            tasks.join("config.yml"),
            format!("issue:\n  states: [Todo, Done, Shipped]\n  done_states: [{done}]\n"),
        )
        .unwrap();
    }

    fn dependent_dto(id: &str, deps: &[&str]) -> TaskDTO {
        TaskDTO {
            task_state: None,
            deleted_at: None,
            id: id.to_string(),
            title: "dependent".to_string(),
            status: crate::types::TaskStatus::from("Todo"),
            priority: crate::types::Priority::from("Medium"),
            task_type: crate::types::TaskType::from("Feature"),
            reporter: None,
            assignee: None,
            created: "2026-01-01T00:00:00Z".to_string(),
            modified: "2026-01-01T00:00:00Z".to_string(),
            due_date: None,
            effort: None,
            subtitle: None,
            description: None,
            tags: Vec::new(),
            relationships: crate::types::TaskRelationships {
                depends_on: deps.iter().map(|d| (*d).to_string()).collect(),
                ..Default::default()
            },
            comments: Vec::new(),
            references: Vec::new(),
            acceptance_criteria: Vec::new(),
            sprints: Vec::new(),
            sprint_order: Default::default(),
            history: Vec::new(),
            custom_fields: Default::default(),
        }
    }

    /// DEV-21 review F1: dependencies classify under the policy of the root
    /// that actually holds them. Two IDs share prefix PX across sibling
    /// roots with different terminal sets; both dep input orders agree.
    #[test]
    fn blocked_dependencies_classify_each_dep_by_actual_root() {
        let tmp = tempfile::tempdir().unwrap();
        // Nested-workspace layout (the locator's monorepo discovery): B is a
        // separate tasks root discovered from A.
        let root_a = tmp.path().join("wsa");
        let root_b = tmp.path().join("wsa").join("wsb");
        write_config(&root_a, "Shipped");
        write_config(&root_b, "Done");
        // PX-1 lives in A where Done is NOT terminal; PX-2 lives in B where
        // Done IS terminal. Different IDs, same prefix: unambiguous.
        write_task(&root_a, "PX", 1, "Done");
        write_task(&root_b, "PX", 2, "Done");

        let storage = Storage::new(&root_a.join(".tasks"));
        let config =
            crate::config::resolution::load_and_merge_configs(Some(&root_a.join(".tasks")))
                .unwrap();

        for deps in [["PX-1", "PX-2"], ["PX-2", "PX-1"]] {
            let blocked =
                find_blocked_dependencies(&storage, &dependent_dto("PX-9", &deps), &config);
            // PX-1 (Done, non-terminal in A) blocks; PX-2 (Done, terminal
            // in its own root B) does not — regardless of input order.
            assert_eq!(blocked, vec!["PX-1".to_string()], "deps: {deps:?}");
        }
    }

    #[test]
    fn deleted_terminal_and_missing_dependencies_remain_unresolved() {
        let tmp = tempfile::tempdir().unwrap();
        write_config(tmp.path(), "Done");
        write_task(tmp.path(), "PX", 1, "Done");
        write_task(tmp.path(), "PX", 2, "Done");
        let file = tmp.path().join(".tasks/PX/2.yml");
        let mut yaml = std::fs::read_to_string(&file).unwrap();
        yaml.push_str("deleted_at: 2026-10-01T00:00:00Z\n");
        std::fs::write(file, yaml).unwrap();
        let storage = Storage::new(&tmp.path().join(".tasks"));
        let config =
            crate::config::resolution::config_for_project(&storage.root_path, Some("PX")).unwrap();
        assert_eq!(
            find_blocked_dependencies(
                &storage,
                &dependent_dto("PX-9", &["PX-1", "PX-2", "PX-3"]),
                &config
            ),
            vec!["PX-2".to_string(), "PX-3".to_string()]
        );
    }
}

#[cfg(test)]
mod async_run_monitor_tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    #[test]
    fn bounded_tail_keeps_final_bytes_and_reports_dropped_prefix() {
        let mut tail = BoundedTail::new(8);
        tail.push(b"abcdefgh");
        assert_eq!(tail.dropped, 0);
        assert_eq!(tail.data, b"abcdefgh".to_vec());

        tail.push(b"ijklmn");
        assert_eq!(tail.data, b"ghijklmn".to_vec());
        assert_eq!(tail.dropped, 6);

        let snapshot = tail.snapshot();
        assert!(
            snapshot.contains("[+6 earlier stderr bytes]\n"),
            "{snapshot}"
        );
        assert!(snapshot.ends_with("ghijklmn"), "{snapshot}");
    }

    #[test]
    fn bounded_tail_zero_limit_drops_everything() {
        let mut tail = BoundedTail::new(0);
        tail.push(b"xyz");
        assert_eq!(tail.dropped, 3);
        assert!(tail.data.is_empty());
        assert_eq!(tail.snapshot(), "");
    }

    #[test]
    fn eof_grace_treats_closed_channel_as_drained_and_silence_as_held() {
        // A live-but-silent pipe (descendant holds the write end) only
        // waits out the grace period before being reported as held.
        let (silent_tx, silent_rx) = mpsc::channel::<()>();
        let start = Instant::now();
        assert!(!stderr_eof_within_grace(
            &silent_rx,
            Duration::from_millis(50)
        ));
        assert!(
            start.elapsed() >= Duration::from_millis(45),
            "grace returned early: {:?}",
            start.elapsed()
        );

        // A reader that finished (message sent, then channel closed) is
        // drained, including the already-disconnected case.
        drop(silent_tx);
        assert!(stderr_eof_within_grace(
            &silent_rx,
            Duration::from_millis(1)
        ));

        let (tx, rx) = mpsc::channel::<()>();
        tx.send(()).unwrap();
        assert!(stderr_eof_within_grace(&rx, Duration::from_millis(1)));
    }
}

/// Native Windows proof of the stdio-inheritance scope. Runs wherever the
/// lib tests run on Windows; the end-to-end coupling regression lives in
/// the portable smoke selection (`cli.automation` /
/// `api.automation.smoke.spec.ts`).
#[cfg(all(test, windows))]
mod windows_async_stdio_tests {
    use super::windows_async_stdio::{self as sys};

    const FILE_TYPE_PIPE: u32 = 0x0003;

    #[test]
    fn detach_clears_inherit_bit_on_pipe_stdio() {
        for (name, slot) in [
            ("stdout", sys::STD_OUTPUT_HANDLE),
            ("stderr", sys::STD_ERROR_HANDLE),
        ] {
            let file_type = sys::std_slot_file_type(slot);
            let outcome = sys::clear_std_slot(slot);
            let flags = sys::std_slot_flags(slot);
            if file_type == FILE_TYPE_PIPE {
                // Captured pipe stdio (nextest/CI): the detach must
                // verifiably clear the bit. A refusal on a real pipe is a
                // genuine failure and must not be silently accepted.
                assert_eq!(
                    outcome,
                    sys::SlotDetach::Cleared,
                    "{name} std slot is a pipe; detach refused (file_type={file_type:#x})"
                );
                assert_eq!(
                    flags.unwrap_or(u32::MAX) & sys::HANDLE_FLAG_INHERIT,
                    0,
                    "{name} pipe std slot still inheritable after detach"
                );
            } else {
                // Console/character handles legitimately refuse
                // SetHandleInformation; interactive runs take this arm and
                // only report/query consistency is enforced there.
                match (outcome, flags) {
                    (sys::SlotDetach::NoHandle, None) => {}
                    (sys::SlotDetach::NotClearable, _) => {}
                    (sys::SlotDetach::Cleared, Some(flags)) => {
                        assert_eq!(
                            flags & sys::HANDLE_FLAG_INHERIT,
                            0,
                            "{name} std slot still inheritable after detach"
                        );
                    }
                    other => panic!(
                        "{name}: inconsistent detach state {other:?} (file_type={file_type:#x})"
                    ),
                }
            }
        }
    }

    #[test]
    fn detach_is_idempotent_across_calls() {
        sys::detach_stdio_for_async_children();
        let first = sys::clear_std_slot(sys::STD_OUTPUT_HANDLE);
        let second = sys::clear_std_slot(sys::STD_OUTPUT_HANDLE);
        assert_eq!(first, second);
        // Clearing an already-cleared slot stays cleared (idempotent
        // outcome, not a flip-flop of the inherit bit).
        if let sys::SlotDetach::Cleared = second {
            let flags = sys::std_slot_flags(sys::STD_OUTPUT_HANDLE);
            assert_eq!(
                flags.unwrap_or(0) & sys::HANDLE_FLAG_INHERIT,
                0,
                "repeat detach re-armed the inherit flag"
            );
        }
    }
}
