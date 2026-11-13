use crate::api_types::{AgentJobCreateRequest, TaskDTO, TaskUpdate};
use crate::automation::persistence as automation_persistence;
use crate::automation::template::{Prerequisite, TemplateContext};
use crate::automation::types::{AutomationAction, AutomationActionSet, AutomationFile, AutomationRule, AutomationRunAction, AutomationTagAction, StringOrVec};
use crate::config::types::ResolvedConfig;
use crate::config::validation::errors::ValidationResult;
use crate::errors::{LoTaRError, LoTaRResult};
use crate::services::agent_job_service::{AgentJobService, AgentOrchestratorMode};
use crate::services::agent_queue_service::AgentQueueService;
use crate::services::automation_matching::{ChangeSet, MatchMode, MatchState, UpdateMatch, current_fields, has_comment_condition, matches_rule, update_match};
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

static COOLDOWN_STATE: std::sync::LazyLock<Mutex<HashMap<CooldownKey, Instant>>> = std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// Return a stable identity string for a rule (name if present, else index).
fn rule_identity(rule: &AutomationRule, index: usize) -> String {
    rule.name.as_deref().map(|n| n.to_string()).unwrap_or_else(|| format!("__rule_{index}"))
}

/// Check whether the rule's cooldown allows firing on this ticket.
/// Returns `true` if the rule is allowed to fire (no cooldown or cooldown expired).
/// The last firing is remembered in memory and in a small state file, so
/// separate processes (cron `lotar automation run`, CLI edits) honour it
/// too (DEV-278).
fn cooldown_allows(rule: &AutomationRule, rule_key: &str, ticket_id: &str, tasks_dir: &Path) -> bool {
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
    if let Some(last_fired) = map.get(&key)
        && last_fired.elapsed() < duration
    {
        return false;
    }
    drop(map);
    match cooldown_store::last_fired(tasks_dir, rule_key, ticket_id) {
        Some(at) => chrono::Utc::now().signed_duration_since(at).to_std().is_ok_and(|elapsed| elapsed >= duration),
        None => true,
    }
}

/// Record the current instant as the last-fired time for this rule+ticket.
fn cooldown_record(rule: &AutomationRule, rule_key: &str, ticket_id: &str, tasks_dir: &Path) {
    if rule.cooldown.is_some() {
        let key = (rule_key.to_string(), ticket_id.to_string());
        let mut map = COOLDOWN_STATE.lock().unwrap_or_else(|e| e.into_inner());
        map.insert(key, Instant::now());
        drop(map);
        cooldown_store::record(tasks_dir, rule_key, ticket_id);
    }
}

/// Last-fired times of rules with a cooldown, shared by every process:
/// `<cache dir>/lotar/automation-cooldowns.json` (or the directory in
/// `LOTAR_AUTOMATION_STATE_DIR`), keyed by tasks directory, rule and ticket.
mod cooldown_store {
    use fs2::FileExt;
    use std::collections::BTreeMap;
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::path::{Path, PathBuf};

    const MAX_AGE_DAYS: i64 = 90;

    fn state_file() -> Option<PathBuf> {
        let dir = match std::env::var("LOTAR_AUTOMATION_STATE_DIR") {
            Ok(dir) if !dir.trim().is_empty() => PathBuf::from(dir),
            // Unit tests never touch the user's cache directory.
            _ if cfg!(test) => return None,
            _ => dirs::cache_dir()?.join("lotar"),
        };
        Some(dir.join("automation-cooldowns.json"))
    }

    fn entry_key(tasks_dir: &Path, rule: &str, ticket: &str) -> String {
        let dir = tasks_dir.canonicalize().unwrap_or_else(|_| tasks_dir.to_path_buf());
        format!("{}\u{1f}{rule}\u{1f}{ticket}", dir.display())
    }

    fn with_state<T>(apply: impl FnOnce(&mut BTreeMap<String, String>) -> T) -> Option<T> {
        let path = state_file()?;
        std::fs::create_dir_all(path.parent()?).ok()?;
        let mut file = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&path).ok()?;
        file.lock_exclusive().ok()?;
        let mut text = String::new();
        let _ = file.read_to_string(&mut text);
        let mut state: BTreeMap<String, String> = serde_json::from_str(&text).unwrap_or_default();
        let before = state.clone();
        let result = apply(&mut state);
        if state != before {
            let _ = file.set_len(0);
            let _ = file.seek(SeekFrom::Start(0));
            let _ = file.write_all(serde_json::to_string(&state).unwrap_or_default().as_bytes());
        }
        let _ = file.unlock();
        Some(result)
    }

    pub(super) fn last_fired(tasks_dir: &Path, rule: &str, ticket: &str) -> Option<chrono::DateTime<chrono::Utc>> {
        let key = entry_key(tasks_dir, rule, ticket);
        with_state(|state| state.get(&key).cloned())
            .flatten()
            .and_then(|raw| chrono::DateTime::parse_from_rfc3339(&raw).ok())
            .map(|at| at.with_timezone(&chrono::Utc))
    }

    pub(super) fn record(tasks_dir: &Path, rule: &str, ticket: &str) {
        let key = entry_key(tasks_dir, rule, ticket);
        let now = chrono::Utc::now();
        let _ = with_state(|state| {
            state.retain(|_, at| chrono::DateTime::parse_from_rfc3339(at).is_ok_and(|at| now.signed_duration_since(at) < chrono::Duration::days(MAX_AGE_DAYS)));
            state.insert(key, now.to_rfc3339());
        });
    }
}

/// Report a failed rule action (DEV-278): later rules still run, and the
/// failure is printed to stderr and sent as an `automation_failed` event.
fn rule_failure(rule: &AutomationRule, rule_key: &str, ticket_id: &str, err: &LoTaRError) -> String {
    let name = rule.name.clone().unwrap_or_else(|| rule_key.to_string());
    let message = format!("Automation rule '{name}' failed on {ticket_id}: {err}");
    eprintln!("[lotar][warn] {message}");
    crate::api_events::emit(&crate::api_events::ApiEvent {
        kind: "automation_failed".into(),
        data: serde_json::json!({"id": ticket_id, "rule": name, "message": err.to_string()}),
    });
    message
}

fn failures_result(failures: &[String]) -> LoTaRResult<()> {
    if failures.is_empty() { Ok(()) } else { Err(LoTaRError::ValidationError(failures.join("; "))) }
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
    ReleaseChanged,

    // Job events
    JobStarted,
    JobCompleted,
    JobFailed,
    JobCancelled,

    // Started on purpose: `lotar automation run`, cron, "Run now" (DEV-241)
    Manual,

    // The ticket's last open prerequisite is done (DEV-385)
    Unblocked,
}

impl std::fmt::Display for AutomationEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Created => "created",
            Self::Updated => "updated",
            Self::Assigned => "assigned",
            Self::Commented => "commented",
            Self::SprintChanged => "sprint_changed",
            Self::ReleaseChanged => "release_changed",
            Self::JobStarted => "job_started",
            Self::JobCompleted => "job_completed",
            Self::JobFailed => "job_failed",
            Self::JobCancelled => "job_cancelled",
            Self::Manual => "manual",
            Self::Unblocked => "unblocked",
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
            "release_changed" => Ok(Self::ReleaseChanged),
            "job_started" | "job_start" => Ok(Self::JobStarted),
            "job_completed" | "complete" | "success" => Ok(Self::JobCompleted),
            "job_failed" | "error" | "failure" => Ok(Self::JobFailed),
            "job_cancelled" | "cancel" => Ok(Self::JobCancelled),
            "manual" => Ok(Self::Manual),
            "unblocked" => Ok(Self::Unblocked),
            _ => Err(format!(
                "Unknown event: '{}'. Expected: created, updated, assigned, commented, sprint_changed, release_changed, job_started, job_completed, job_failed, job_cancelled, manual, unblocked",
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
    comment: Option<crate::types::TaskComment>,
    /// The prerequisites an `unblocked` ticket waited for, all done now.
    unblocked_by: Vec<Prerequisite>,
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
    pub fn inspect(tasks_dir: &Path, project: Option<&str>) -> LoTaRResult<AutomationInspectResult> {
        let config = resolve_config_for_project(tasks_dir, project)?;
        let scope = if project.is_some() { AutomationScope::Project } else { AutomationScope::Global };
        let (effective, source) = load_effective_automation(tasks_dir, project, Some(&config))?;
        let scope_file = match scope {
            AutomationScope::Project => project.and_then(|p| automation_persistence::load_project_automation(tasks_dir, p).ok()).flatten(),
            AutomationScope::Global => automation_persistence::load_global_automation(tasks_dir)?,
            AutomationScope::Home => automation_persistence::load_home_automation()?,
            AutomationScope::BuiltIn => None,
        };

        let scope_exists = scope_file.is_some();
        let scope_yaml = automation_persistence::to_canonical_yaml(&scope_file.unwrap_or_default())?;
        let effective_yaml = automation_persistence::to_canonical_yaml(&effective)?;

        Ok(AutomationInspectResult { scope, source, scope_exists, scope_yaml, effective_yaml })
    }

    pub fn set(tasks_dir: &Path, project: Option<&str>, yaml: &str) -> LoTaRResult<AutomationSetOutcome> {
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
        let scope_label = if project.is_some() { "project" } else { "global" };
        let current = match project {
            Some(prefix) => automation_persistence::load_project_automation(tasks_dir, prefix),
            None => automation_persistence::load_global_automation(tasks_dir),
        }
        .map_err(|e| LoTaRError::ValidationError(format!("Failed to read current {} automation configuration: {}", scope_label, e)))?;
        if current.as_ref().is_some_and(|current| automation_persistence::files_effectively_equal(current, &file)) {
            return Ok(AutomationSetOutcome { updated: false, validation });
        }

        match project {
            Some(prefix) => automation_persistence::save_project_automation(tasks_dir, prefix, &file)?,
            None => automation_persistence::save_global_automation(tasks_dir, &file)?,
        }

        Ok(AutomationSetOutcome { updated: true, validation })
    }

    pub fn apply_task_update(storage: &mut Storage, previous: Option<&TaskDTO>, current: &TaskDTO, config: &ResolvedConfig) -> LoTaRResult<()> {
        let tasks_dir = storage.root_path.clone();
        let project = crate::storage::TaskId::parse(&current.id).map(|parsed| parsed.project).unwrap_or_default();
        let (automation, _) = load_effective_automation(&tasks_dir, if project.is_empty() { None } else { Some(project.as_str()) }, Some(config))?;
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
        let assignee_changed = previous.is_some() && previous.and_then(|t| t.assignee.as_deref()) != current.assignee.as_deref();
        let sprints_changed = previous.is_some() && previous.map(|t| &t.sprints) != Some(&current.sprints);
        let releases_changed = previous.is_some() && previous.map(|t| &t.releases) != Some(&current.releases);

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
            if releases_changed {
                evts.push(AutomationEvent::ReleaseChanged);
            }
            evts
        };

        let match_mode = MatchMode::OnChange;
        let active_sprints = compute_active_sprint_ids(storage);
        let mut failures: Vec<String> = Vec::new();

        for (rule_idx, rule) in automation.automation.rules().iter().enumerate() {
            if !matches_rule(rule, current, &changes, match_mode, config, &active_sprints) {
                continue;
            }
            let rk = rule_identity(rule, rule_idx);
            if !cooldown_allows(rule, &rk, &current.id, &tasks_dir) {
                continue;
            }

            let mut fired = false;

            // Fire event-specific hooks
            for &event in &events {
                if let Some(action) = resolve_action_for_event(&rule.on, event) {
                    let action_context = AutomationActionContext { event, job: None, previous: previous.cloned(), comment: None, unblocked_by: Vec::new() };
                    if let Err(err) = apply_action(storage, current, config, action, &action_context) {
                        failures.push(rule_failure(rule, &rk, &current.id, &err));
                    }
                    fired = true;
                }
            }

            // Fire legacy catch-all `start` hook (backward compat)
            if let Some(action) = rule.on.start.as_ref() {
                let primary_event = events.first().copied().unwrap_or(AutomationEvent::Updated);
                let action_context =
                    AutomationActionContext { event: primary_event, job: None, previous: previous.cloned(), comment: None, unblocked_by: Vec::new() };
                if let Err(err) = apply_action(storage, current, config, action, &action_context) {
                    failures.push(rule_failure(rule, &rk, &current.id, &err));
                }
                fired = true;
            }

            if fired {
                cooldown_record(rule, &rk, &current.id, &tasks_dir);
            }
        }
        failures_result(&failures)
    }

    /// Fire automation rules for a comment being added to a task.
    pub fn apply_comment_event(storage: &mut Storage, task: &TaskDTO, comment: &crate::types::TaskComment, config: &ResolvedConfig) -> LoTaRResult<()> {
        let tasks_dir = storage.root_path.clone();
        let project = crate::storage::TaskId::parse(&task.id).map(|parsed| parsed.project).unwrap_or_default();
        let (automation, _) = load_effective_automation(&tasks_dir, if project.is_empty() { None } else { Some(project.as_str()) }, Some(config))?;
        let changes = ChangeSet::commented(task, comment.author.as_deref());
        let active_sprints = compute_active_sprint_ids(storage);
        let event = AutomationEvent::Commented;
        let mut failures: Vec<String> = Vec::new();

        for (rule_idx, rule) in automation.automation.rules().iter().enumerate() {
            if !matches_rule(rule, task, &changes, MatchMode::Current, config, &active_sprints) {
                continue;
            }
            let rk = rule_identity(rule, rule_idx);
            if !cooldown_allows(rule, &rk, &task.id, &tasks_dir) {
                continue;
            }
            if let Some(action) = resolve_action_for_event(&rule.on, event) {
                let action_context = AutomationActionContext { event, job: None, previous: None, comment: Some(comment.clone()), unblocked_by: Vec::new() };
                if let Err(err) = apply_action(storage, task, config, action, &action_context) {
                    failures.push(rule_failure(rule, &rk, &task.id, &err));
                }
                cooldown_record(rule, &rk, &task.id, &tasks_dir);
            }
        }
        failures_result(&failures)
    }

    /// Called after every committed update (DEV-385): when `current` went
    /// from open to done, by its own project's policy, fire `unblocked` on
    /// each open dependent whose prerequisites are all done now.
    pub(crate) fn fire_unblocked_dependents(storage: &mut Storage, previous: &TaskDTO, current: &TaskDTO) {
        let is_done = |task: &TaskDTO| task.task_state.as_ref().is_some_and(|state| state.is_done);
        if is_done(previous) || !is_done(current) {
            return;
        }
        let mut dependents: Vec<String> = current.relationships.blocks.clone();
        for (id, task, _) in storage.search_located(&crate::storage::TaskFilter::default()) {
            if task.deleted_at.is_none()
                && task.moved_to.is_none()
                && task.relationships.depends_on.iter().any(|dep| crate::storage::identity::aliases_match(dep, &current.id))
                && !dependents.iter().any(|known| crate::storage::identity::aliases_match(known, &id))
            {
                dependents.push(id);
            }
        }
        for id in dependents {
            let Ok(dependent) = TaskService::get(storage, &id, None) else {
                continue;
            };
            if is_done(&dependent) || !find_blocked_dependencies(storage, &dependent).is_empty() {
                continue;
            }
            let mut by: Vec<Prerequisite> =
                prerequisites_of(storage, &dependent).into_iter().filter(|dep| !crate::storage::identity::aliases_match(&dep.id, &current.id)).collect();
            by.push(Prerequisite::from(current));
            // Each failed rule already reported itself (rule_failure).
            let _ = Self::apply_unblocked_event(storage, &dependent.id, &by);
        }
    }

    /// Fire the `unblocked` rules of `dependent_id` (DEV-385); `by` are the
    /// prerequisites it waited for, for `${{unblocked.by}}`.
    pub fn apply_unblocked_event(storage: &mut Storage, dependent_id: &str, by: &[Prerequisite]) -> LoTaRResult<()> {
        let tasks_dir = storage.root_path.clone();
        let project = crate::storage::TaskId::parse(dependent_id).map(|parsed| parsed.project).unwrap_or_default();
        let config = resolve_config_for_project(&tasks_dir, Some(&project))?;
        let task = TaskService::get(storage, dependent_id, None)?;
        let (automation, _) = load_effective_automation(&tasks_dir, Some(project.as_str()), Some(&config))?;
        let active_sprints = compute_active_sprint_ids(storage);
        let event = AutomationEvent::Unblocked;
        let mut failures: Vec<String> = Vec::new();

        for (rule_idx, rule) in automation.automation.rules().iter().enumerate() {
            let Some(action) = resolve_action_for_event(&rule.on, event) else {
                continue;
            };
            if !matches_rule(rule, &task, &ChangeSet::empty(&task), MatchMode::Current, &config, &active_sprints) {
                continue;
            }
            let rk = rule_identity(rule, rule_idx);
            if !cooldown_allows(rule, &rk, &task.id, &tasks_dir) {
                continue;
            }
            let action_context = AutomationActionContext { event, job: None, previous: None, comment: None, unblocked_by: by.to_vec() };
            if let Err(err) = apply_action(storage, &task, &config, action, &action_context) {
                failures.push(rule_failure(rule, &rk, &task.id, &err));
            }
            cooldown_record(rule, &rk, &task.id, &tasks_dir);
        }
        failures_result(&failures)
    }

    pub fn apply_job_event(tasks_dir: &Path, ticket_id: &str, event: AutomationEvent, job_context: Option<AutomationJobContext>) -> LoTaRResult<()> {
        let project_prefix = crate::storage::TaskId::parse(ticket_id).map(|parsed| parsed.project).unwrap_or_default();
        let config = resolve_config_for_project(tasks_dir, Some(&project_prefix))?;
        let mut storage = Storage::new(tasks_dir);
        let task = TaskService::get(&storage, ticket_id, None)?;
        let (automation, _) = load_effective_automation(tasks_dir, Some(project_prefix.as_str()), Some(&config))?;

        // A status/assignee set while the job ran is the job's outcome
        // (DEV-320): it replaces those fields of the end rules' `set`.
        let ending_job = job_context
            .as_ref()
            .filter(|_| matches!(event, AutomationEvent::JobCompleted | AutomationEvent::JobFailed | AutomationEvent::JobCancelled))
            .map(|job| job.job_id.clone());
        let recorded = ending_job.as_deref().and_then(|job_id| crate::services::agent_active_jobs::peek_outcome(tasks_dir, job_id));
        let action_context = AutomationActionContext { event, job: job_context, previous: None, comment: None, unblocked_by: Vec::new() };
        let mut failures: Vec<String> = Vec::new();

        for (rule_idx, rule) in automation.automation.rules().iter().enumerate() {
            if !matches_rule(rule, &task, &ChangeSet::empty(&task), MatchMode::Current, &config, &compute_active_sprint_ids(&storage)) {
                continue;
            }
            let rk = rule_identity(rule, rule_idx);
            if !cooldown_allows(rule, &rk, ticket_id, tasks_dir) {
                continue;
            }
            if let Some(action) = resolve_action_for_event(&rule.on, event) {
                let action = without_outcome_fields(action, recorded.as_ref());
                if let Err(err) = apply_action(&mut storage, &task, &config, &action, &action_context) {
                    failures.push(rule_failure(rule, &rk, ticket_id, &err));
                }
                cooldown_record(rule, &rk, ticket_id, tasks_dir);
            }
        }

        // The rules have run: end the job's guard and apply the outcome,
        // including anything recorded while they ran.
        if let Some(job_id) = ending_job.as_deref()
            && let Some(outcome) = crate::services::agent_active_jobs::end_and_take_outcome(tasks_dir, job_id)
            && let Err(err) = apply_job_outcome(&mut storage, ticket_id, &config, &outcome)
        {
            failures.push(format!("Outcome recorded during job {job_id} for {ticket_id} failed: {err}"));
        }
        failures_result(&failures)
    }

    /// Whether an automation rule moves this ticket when its agent job ends,
    /// judged on the ticket as it is now (the assignee that started the job).
    pub fn handles_job_end(tasks_dir: &Path, ticket_id: &str) -> bool {
        let project_prefix = crate::storage::TaskId::parse(ticket_id).map(|parsed| parsed.project).unwrap_or_default();
        let Ok(config) = resolve_config_for_project(tasks_dir, Some(&project_prefix)) else {
            return false;
        };
        let storage = Storage::new(tasks_dir);
        let Ok(task) = TaskService::get(&storage, ticket_id, None) else {
            return false;
        };
        let Ok((automation, _)) = load_effective_automation(tasks_dir, Some(project_prefix.as_str()), Some(&config)) else {
            return false;
        };
        let active_sprints = compute_active_sprint_ids(&storage);
        automation
            .automation
            .rules()
            .iter()
            .any(|rule| rule.on.job_completed.is_some() && matches_rule(rule, &task, &ChangeSet::empty(&task), MatchMode::Current, &config, &active_sprints))
    }

    /// Simulate automation rules without applying changes
    pub fn simulate(tasks_dir: &Path, ticket_id: &str, event: AutomationEvent) -> LoTaRResult<SimulateResult> {
        let project_prefix = crate::storage::TaskId::parse(ticket_id).map(|parsed| parsed.project).unwrap_or_default();
        let config = resolve_config_for_project(tasks_dir, Some(&project_prefix))?;
        let storage = Storage::new(tasks_dir);
        let task = TaskService::get(&storage, ticket_id, None)?;
        let (automation, _) = load_effective_automation(tasks_dir, Some(project_prefix.as_str()), Some(&config))?;
        Self::simulate_rules(&storage, &task, &config, automation.automation.rules(), event)
    }

    /// [`Self::simulate`] for one rule that is not saved yet, e.g. the rule
    /// being edited in the web UI (DEV-241). A rule that is switched off is
    /// simulated anyway: the editor shows what it would do once on.
    pub fn simulate_rule(tasks_dir: &Path, ticket_id: &str, event: AutomationEvent, rule: &AutomationRule) -> LoTaRResult<SimulateResult> {
        let project_prefix = crate::storage::TaskId::parse(ticket_id).map(|parsed| parsed.project).unwrap_or_default();
        let config = resolve_config_for_project(tasks_dir, Some(&project_prefix))?;
        let storage = Storage::new(tasks_dir);
        let task = TaskService::get(&storage, ticket_id, None)?;
        let mut rule = rule.clone();
        rule.enabled = None;
        Self::simulate_rules(&storage, &task, &config, std::slice::from_ref(&rule), event)
    }

    fn simulate_rules(
        storage: &Storage,
        task: &TaskDTO,
        config: &ResolvedConfig,
        rules: &[AutomationRule],
        event: AutomationEvent,
    ) -> LoTaRResult<SimulateResult> {
        let active_sprints = compute_active_sprint_ids(storage);
        // Rules that only an update changing several fields at once fires.
        let mut skipped: Vec<String> = Vec::new();

        for (rule_idx, rule) in rules.iter().enumerate() {
            let mut notes = Vec::new();
            // Change events match the way a real update does (DEV-362).
            let matched = match event {
                AutomationEvent::Created => matches_rule(rule, task, &ChangeSet::new(None, task), MatchMode::OnChange, config, &active_sprints),
                AutomationEvent::Updated | AutomationEvent::Assigned | AutomationEvent::SprintChanged | AutomationEvent::ReleaseChanged => {
                    let base: &[&str] = if matches!(event, AutomationEvent::Assigned) { &["assignee"] } else { &[] };
                    match update_match(rule, task, base, config, &active_sprints) {
                        UpdateMatch::Always => true,
                        UpdateMatch::OneOf(fields) => {
                            notes.push(format!("Matches only when {} changes in the same update.", fields.join(" or ")));
                            true
                        }
                        UpdateMatch::Together(fields) => {
                            if resolve_action_for_event_with_fallback(&rule.on, event).is_some() {
                                let name = rule.name.clone().unwrap_or_else(|| format!("Rule {}", rule_idx + 1));
                                skipped.push(format!(
                                    "{name}: matches only when {} change in the same update; a change of one of them alone does not fire it.",
                                    join_and(&fields)
                                ));
                            }
                            false
                        }
                        UpdateMatch::Never => false,
                    }
                }
                // The latest comment stands in for the new one (DEV-383).
                AutomationEvent::Commented => {
                    let author = task.comments.last().and_then(|comment| comment.author.as_deref());
                    matches_rule(rule, task, &ChangeSet::commented(task, author), MatchMode::Current, config, &active_sprints)
                }
                _ => matches_rule(rule, task, &ChangeSet::empty(task), MatchMode::Current, config, &active_sprints),
            };
            if !matched {
                continue;
            }
            if matches!(event, AutomationEvent::Commented) && has_comment_condition(rule) {
                match task.comments.last().and_then(|comment| comment.author.as_deref()) {
                    Some(author) => notes.push(format!("Checked against the latest comment, by {author}.")),
                    None => notes.push("Checked against the latest comment, which has no author.".to_string()),
                }
            }
            let now = current_fields(rule);
            if !now.is_empty()
                && matches!(
                    event,
                    AutomationEvent::Created
                        | AutomationEvent::Updated
                        | AutomationEvent::Assigned
                        | AutomationEvent::SprintChanged
                        | AutomationEvent::ReleaseChanged
                )
            {
                notes.push(format!("Checked on the ticket as it is now: {}.", now.join(", ")));
            }
            if let Some(action) = resolve_action_for_event_with_fallback(&rule.on, event) {
                let mut preview = PreviewContext::new(task);
                if event == AutomationEvent::Unblocked {
                    // The ticket's prerequisites stand in for the ones that just finished.
                    let by = prerequisites_of(storage, task);
                    preview.tmpl = std::mem::take(&mut preview.tmpl).with_unblocked(&by);
                }
                let actions = describe_action(action, task, config, &mut preview);
                let task_after = simulate_action(task, action, config, storage, &mut preview);
                notes.extend(preview.into_warnings());
                return Ok(SimulateResult { matched: true, rule_name: rule.name.clone(), actions, task_after: Some(task_after), warnings: notes });
            }
        }
        Ok(SimulateResult { matched: false, rule_name: None, actions: vec![], task_after: None, warnings: skipped })
    }
}

/// One rule acting on one task in a manual run (DEV-241).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ManualRunEntry {
    pub rule: String,
    pub task: String,
    /// What the action does (or would do, in a dry run).
    pub actions: Vec<SimulatedAction>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// Why the action failed; the run carries on with the next task.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The outcome of [`AutomationService::run_manual`].
#[derive(Debug, Clone, serde::Serialize)]
pub struct ManualRunReport {
    pub dry_run: bool,
    /// Rules with an `on.manual` action that were considered, and
    /// `unblocked` rules run for a prerequisite in another repo (DEV-388).
    pub rules: Vec<String>,
    /// How many tasks were checked against them.
    pub checked: usize,
    pub entries: Vec<ManualRunEntry>,
}

impl AutomationService {
    /// Run the `on.manual` actions of the named rules (or of every rule that
    /// has one) on each task they match right now (DEV-241). This is how
    /// automations run on a schedule: from cron, `lotar automation run`.
    /// `query` narrows the tasks with the search syntax of `lotar list`.
    /// Each task's own project rules apply; disabled rules never run.
    pub fn run_manual(storage: &mut Storage, rule_names: &[String], query: Option<&str>, dry_run: bool) -> LoTaRResult<ManualRunReport> {
        let tasks_dir = storage.root_path.clone();
        let base = resolve_config_for_project(&tasks_dir, None)?;
        let _least_busy = LeastBusyMemo::new();
        // One listing for the whole run: reading each task on its own
        // reloaded the sprints and walked the project tree per task, which
        // took minutes in a repo with hundreds of tasks (DEV-400).
        let tasks: Vec<(String, TaskDTO)> = match query.map(str::trim).filter(|q| !q.is_empty()) {
            Some(search) => {
                crate::services::task_list::select(storage, &base, &tasks_dir, search).map_err(|err| LoTaRError::ValidationError(err.cli_message()))?
            }
            None => TaskService::list(storage, &crate::api_types::TaskListFilter::default()),
        };
        let checked = tasks.len();
        let moves = crate::storage::moves::MoveResolver::new(&tasks_dir);
        let wanted: Vec<String> = rule_names.iter().map(|name| name.trim().to_lowercase()).filter(|name| !name.is_empty()).collect();
        let active_sprints = compute_active_sprint_ids(storage);
        let mut rules_seen: Vec<String> = Vec::new();
        let mut found: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut entries = Vec::new();
        let mut cache: HashMap<String, (ResolvedConfig, AutomationFile)> = HashMap::new();
        let written = crate::storage::write_log::WriteLog::open();

        for (_, mut task) in tasks {
            if written.wrote(&task.id) {
                // An earlier action in this run changed it (closing a task
                // fires its dependents' `unblocked` rules): match what it is
                // now, not the listing (DEV-410).
                match TaskService::get(storage, &task.id, None) {
                    Ok(current) => task = current,
                    Err(_) => continue,
                }
            } else {
                // As a single read would: tombstones stay out, links to moved
                // tasks show their current IDs.
                if task.deleted_at.is_some() {
                    continue;
                }
                crate::storage::moves::current_relationships(&moves, &mut task.relationships);
            }
            let mut changed = false;
            let project = crate::storage::TaskId::parse(&task.id).map(|parsed| parsed.project).unwrap_or_default();
            if !cache.contains_key(&project) {
                let config = resolve_config_for_project(&tasks_dir, Some(&project))?;
                let (file, _) = load_effective_automation(&tasks_dir, Some(&project), None)?;
                cache.insert(project.clone(), (config, file));
            }
            let (config, file) = cache.get(&project).expect("cached above").clone();
            for (index, rule) in file.automation.rules().iter().enumerate() {
                if rule.on.manual.is_none() && rule.on.unblocked.is_none() {
                    continue;
                }
                let name = rule.name.clone().unwrap_or_else(|| format!("Rule {}", index + 1));
                if !wanted.is_empty() && !wanted.contains(&name.to_lowercase()) {
                    continue;
                }
                found.insert(name.to_lowercase());
                if !rule.is_enabled() {
                    continue;
                }
                if !rules_seen.contains(&name) {
                    rules_seen.push(name.clone());
                }
                let Some(action) = rule.on.manual.as_ref() else {
                    continue;
                };
                let changes = ChangeSet::empty(&task);
                if !matches_rule(rule, &task, &changes, MatchMode::Current, &config, &active_sprints) {
                    continue;
                }
                let key = rule_identity(rule, index);
                if !dry_run && !cooldown_allows(rule, &key, &task.id, &tasks_dir) {
                    continue;
                }
                let mut preview = PreviewContext::new(&task);
                let actions = describe_action(action, &task, &config, &mut preview);
                let mut entry = ManualRunEntry {
                    rule: name.clone(),
                    task: task.id.clone(),
                    actions,
                    warnings: if dry_run { preview.into_warnings() } else { Vec::new() },
                    error: None,
                };
                if !dry_run {
                    let context =
                        AutomationActionContext { event: AutomationEvent::Manual, job: None, previous: None, comment: None, unblocked_by: Vec::new() };
                    match apply_action(storage, &task, &config, action, &context) {
                        Ok(()) => {
                            changed = true;
                            cooldown_record(rule, &key, &task.id, &tasks_dir);
                        }
                        Err(err) => entry.error = Some(err.to_string()),
                    }
                }
                entries.push(entry);
            }
            entries.extend(Self::run_cross_repo_unblocked(storage, &task, changed, &config, &file, &wanted, dry_run));
        }

        if let Some(missing) = wanted.iter().find(|name| checked > 0 && !found.contains(*name)) {
            return Err(LoTaRError::ValidationError(format!(
                "No rule named '{missing}' with a manual or unblocked action (on.manual, on.unblocked) applies to the selected tasks"
            )));
        }
        Ok(ManualRunReport { dry_run, rules: rules_seen, checked, entries })
    }
}

impl AutomationService {
    /// A prerequisite in another repo never tells this one it closed
    /// (DEV-388), so a manual run fires `unblocked` on each open ticket that
    /// waits for one and has no open prerequisite left. The rule's own
    /// conditions (`when: status: Blocked` plus a `set` that leaves Blocked)
    /// keep it from firing on every run.
    fn run_cross_repo_unblocked(
        storage: &mut Storage,
        task: &TaskDTO,
        changed: bool,
        config: &ResolvedConfig,
        file: &AutomationFile,
        wanted: &[String],
        dry_run: bool,
    ) -> Vec<ManualRunEntry> {
        let tasks_dir = storage.root_path.clone();
        let mut entries = Vec::new();
        // Read again when a manual rule just changed it.
        let reread;
        let task = if changed {
            let Ok(current) = TaskService::get(storage, &task.id, None) else {
                return entries;
            };
            reread = current;
            &reread
        } else {
            task
        };
        if !task.relationships.depends_on.iter().any(|dep| crate::services::external_tasks::parse(dep).is_some())
            || task.task_state.as_ref().is_some_and(|state| state.is_done)
            || !find_blocked_dependencies(storage, task).is_empty()
        {
            return entries;
        }
        let by = prerequisites_of(storage, task);
        let active_sprints = compute_active_sprint_ids(storage);
        for (index, rule) in file.automation.rules().iter().enumerate() {
            let Some(action) = rule.on.unblocked.as_ref() else {
                continue;
            };
            let name = rule.name.clone().unwrap_or_else(|| format!("Rule {}", index + 1));
            if !wanted.is_empty() && !wanted.contains(&name.to_lowercase()) {
                continue;
            }
            if !rule.is_enabled() || !matches_rule(rule, task, &ChangeSet::empty(task), MatchMode::Current, config, &active_sprints) {
                continue;
            }
            let key = rule_identity(rule, index);
            if !dry_run && !cooldown_allows(rule, &key, &task.id, &tasks_dir) {
                continue;
            }
            let mut preview = PreviewContext::new(task);
            preview.tmpl = std::mem::take(&mut preview.tmpl).with_unblocked(&by);
            let actions = describe_action(action, task, config, &mut preview);
            let mut entry = ManualRunEntry {
                rule: name,
                task: task.id.clone(),
                actions,
                warnings: if dry_run { preview.into_warnings() } else { Vec::new() },
                error: None,
            };
            if !dry_run {
                let context = AutomationActionContext { event: AutomationEvent::Unblocked, job: None, previous: None, comment: None, unblocked_by: by.clone() };
                match apply_action(storage, task, config, action, &context) {
                    Ok(()) => cooldown_record(rule, &key, &task.id, &tasks_dir),
                    Err(err) => entry.error = Some(err.to_string()),
                }
            }
            entries.push(entry);
        }
        entries
    }
}

/// The ticket's prerequisites for `${{unblocked.by}}`: local tasks and
/// tasks in another repo (DEV-388) that can be read.
fn prerequisites_of(storage: &Storage, task: &TaskDTO) -> Vec<Prerequisite> {
    task.relationships
        .depends_on
        .iter()
        .filter_map(|dep| match crate::services::external_tasks::parse(dep) {
            Some(_) => crate::services::external_tasks::resolve(&storage.root_path, dep).ok().map(Prerequisite::from),
            None => TaskService::get(storage, dep, None).ok().map(|task| Prerequisite::from(&task)),
        })
        .collect()
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
#[derive(Debug, Clone, serde::Serialize)]
pub struct SimulatedAction {
    pub action: String,
    pub description: String,
}

/// Describe what an automation action would do
fn describe_action(action: &AutomationAction, current: &TaskDTO, config: &ResolvedConfig, preview: &mut PreviewContext) -> Vec<SimulatedAction> {
    let mut actions = Vec::new();
    fn push(actions: &mut Vec<SimulatedAction>, action: &str, description: String) {
        actions.push(SimulatedAction { action: action.to_string(), description });
    }

    if let Some(set) = action.set.as_ref() {
        let expand = |preview: &mut PreviewContext, value: &String| preview.expand(value);
        if let Some(value) = set.status.as_ref() {
            push(&mut actions, "set_status", format!("Set status to '{}'", expand(preview, value)));
        }
        if let Some(value) = set.priority.as_ref() {
            push(&mut actions, "set_priority", format!("Set priority to '{}'", expand(preview, value)));
        }
        if let Some(value) = set.task_type.as_ref() {
            push(&mut actions, "set_type", format!("Set type to '{}'", expand(preview, value)));
        }
        if let Some(value) = set.assignee.as_ref() {
            let value = expand(preview, value);
            let resolved = if value == "@me" {
                "current user".to_string()
            } else if value == "@reporter" {
                current.reporter.clone().unwrap_or_else(|| "reporter".to_string())
            } else if value.starts_with('@') && config.agent_profiles.contains_key(&value[1..]) {
                format!("agent '{}'", &value[1..])
            } else {
                value.clone()
            };
            push(&mut actions, "set_assignee", format!("Set assignee to {}", resolved));
        }
        if let Some(value) = set.reporter.as_ref() {
            push(&mut actions, "set_reporter", format!("Set reporter to '{}'", expand(preview, value)));
        }
        if let Some(value) = set.title.as_ref() {
            push(&mut actions, "set_title", format!("Set title to '{}'", expand(preview, value)));
        }
        if let Some(value) = set.description.as_ref() {
            push(&mut actions, "set_description", format!("Set description to '{}'", expand(preview, value)));
        }
        if let Some(value) = set.due_date.as_ref() {
            push(&mut actions, "set_due_date", format!("Set due date to '{}'", expand(preview, value)));
        }
        if let Some(value) = set.effort.as_ref() {
            push(&mut actions, "set_effort", format!("Set effort to '{}'", expand(preview, value)));
        }
        if let Some(values) = set.tags.as_ref().or(set.labels.as_ref()).map(StringOrVec::as_vec) {
            push(&mut actions, "set_tags", format!("Set tags: {}", values.join(", ")));
        }
        if let Some(map) = set.custom_fields.as_ref() {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for key in keys {
                let shown = convert_custom_field_value(&map[key], key).map(|value| crate::types::custom_value_to_string(&value)).unwrap_or_default();
                push(&mut actions, "set_custom_field", format!("Set custom field '{}' to '{}'", key, shown));
            }
        }
    }

    if let Some(add) = action.add.as_ref() {
        let tags = tag_action_values(add);
        if !tags.is_empty() {
            actions.push(SimulatedAction { action: "add_tags".to_string(), description: format!("Add tags: {}", tags.join(", ")) });
        }
        if let Some(sprint) = add.sprint.as_ref() {
            actions.push(SimulatedAction { action: "add_sprint".to_string(), description: format!("Add to sprint: {}", sprint) });
        }
        if let Some(release) = add.release.as_ref() {
            actions.push(SimulatedAction { action: "add_release".to_string(), description: format!("Add to release: {}", release) });
        }
        for (field, vals) in [("depends_on", &add.depends_on), ("blocks", &add.blocks), ("related", &add.related)] {
            if let Some(v) = vals {
                actions.push(SimulatedAction { action: format!("add_{}", field), description: format!("Add {}: {}", field, v.as_vec().join(", ")) });
            }
        }
    }

    if let Some(remove) = action.remove.as_ref() {
        let tags = tag_action_values(remove);
        if !tags.is_empty() {
            actions.push(SimulatedAction { action: "remove_tags".to_string(), description: format!("Remove tags: {}", tags.join(", ")) });
        }
        if let Some(sprint) = remove.sprint.as_ref() {
            actions.push(SimulatedAction { action: "remove_sprint".to_string(), description: format!("Remove from sprint: {}", sprint) });
        }
        if let Some(release) = remove.release.as_ref() {
            actions.push(SimulatedAction { action: "remove_release".to_string(), description: format!("Remove from release: {}", release) });
        }
        for (field, vals) in [("depends_on", &remove.depends_on), ("blocks", &remove.blocks), ("related", &remove.related)] {
            if let Some(v) = vals {
                actions.push(SimulatedAction { action: format!("remove_{}", field), description: format!("Remove {}: {}", field, v.as_vec().join(", ")) });
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
                    let args: Vec<String> = command.args.iter().map(|arg| preview.expand(arg)).collect();
                    format!("Run command: {} {}", preview.expand(&command.command), args.join(" "))
                }
            }
        };
        actions.push(SimulatedAction { action: "run_command".to_string(), description });
    }

    if let Some(comment) = action.comment.as_ref() {
        actions.push(SimulatedAction { action: "add_comment".to_string(), description: format!("Add comment: {}", preview.expand(comment)) });
    }

    if let Some(tell) = action.tell_agent.as_ref() {
        let fallback =
            tell.agent.as_deref().map(|agent| format!(" (or start @{} if the ticket has no agent)", agent.trim().trim_start_matches('@'))).unwrap_or_default();
        actions.push(SimulatedAction {
            action: "tell_agent".to_string(),
            description: format!("Tell the ticket's agent{fallback}: {}", preview.expand(&tell.message)),
        });
    }

    if let Some(archive) = action.archive {
        actions.push(SimulatedAction {
            action: if archive { "archive" } else { "unarchive" }.to_string(),
            description: if archive { "Archive the task (move it into @archive)".to_string() } else { "Bring the task back out of @archive".to_string() },
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
            let mut settings: Vec<String> = Vec::new();
            for (name, value) in [("model", &agent.model), ("effort", &agent.effort), ("provider", &agent.provider)] {
                if let Some(value) = expand_run_setting(value.as_deref(), |raw| preview.expand(raw)) {
                    settings.push(format!("{name} {value}"));
                }
            }
            if let Some(prompt) = agent.prompt.as_ref() {
                settings.push(format!("prompt: {}", preview.expand(prompt)));
            }
            if !settings.is_empty() {
                description.push_str(&format!(" ({})", settings.join(", ")));
            }
        }
        actions.push(SimulatedAction { action: "queue_agent".to_string(), description });
    }

    actions
}

/// Tag values of an add/remove action, including the `labels` alias.
fn tag_action_values(action: &AutomationTagAction) -> Vec<String> {
    action.tags.as_ref().map(StringOrVec::as_vec).into_iter().flatten().chain(action.labels.as_ref().map(StringOrVec::as_vec).into_iter().flatten()).collect()
}

/// Template expansion and warnings for one preview (DEV-118).
struct PreviewContext {
    tmpl: TemplateContext,
    unresolved: Vec<String>,
    warnings: Vec<String>,
}

impl PreviewContext {
    fn new(task: &TaskDTO) -> Self {
        Self { tmpl: TemplateContext::from_task(task), unresolved: Vec::new(), warnings: Vec::new() }
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
            self.warnings.push(format!("Template value '${{{{{}}}}}' comes from the triggering event and is shown unexpanded", key));
        }
        self.warnings
    }
}

/// Assignee tokens whose result is only decided when the rule runs.
/// `@round_robin` advances a shared counter, so previewing it would change
/// the next real assignment.
fn is_runtime_assignee(value: &str) -> bool {
    matches!(value.trim().to_ascii_lowercase().as_str(), "@round_robin" | "@random")
}

/// Preview what the task would look like after applying an action, with the
/// same field coverage as `apply_action` (DEV-118). Nothing is written:
/// sprint membership, comments and run commands are listed in the described
/// actions but not applied to the preview.
fn simulate_action(current: &TaskDTO, action: &AutomationAction, config: &ResolvedConfig, storage: &Storage, preview: &mut PreviewContext) -> TaskDTO {
    let mut simulated = current.clone();

    if let Some(set) = action.set.as_ref() {
        let plan = SetPlan::parse(set, config, |value| preview.expand(value));
        for (field, value) in &plan.rejected {
            preview.warn(format!("Unknown {} '{}' would be skipped", set_enum_label(*field), value));
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
            simulated.effort =
                if effort.is_empty() { None } else { Some(crate::utils::effort::parse_effort(&effort).map(|parsed| parsed.canonical).unwrap_or(effort)) };
        }
        if let Some(value) = plan.assignee.as_deref() {
            if is_runtime_assignee(value) {
                preview.warn(format!("Assignee '{}' is chosen when the rule runs; the preview shows the token", value.trim()));
                simulated.assignee = Some(value.trim().to_string());
            } else if let Some(resolved) = resolve_assignee_value(value, current, config, storage) {
                simulated.assignee = Some(resolved);
            }
        }
        if let Some(value) = plan.reporter.as_deref()
            && let Some(resolved) = resolve_reporter_value(value, current, storage.root_path.as_path())
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
                    Err(err) => preview.warn(format!("Custom field '{}' would fail to apply: {}", key, err)),
                }
            }
        }
    }

    for (tag_action, add) in [(action.add.as_ref(), true), (action.remove.as_ref(), false)] {
        let Some(tag_action) = tag_action else {
            continue;
        };
        apply_tag_action(tag_action, add, &mut simulated.tags);
        apply_relationship_action(Some(storage.root_path.as_path()), tag_action, add, &mut simulated.relationships);
        if tag_action.sprint.is_some() {
            preview.warn("Sprint membership changes are listed in actions but not reflected in task_after".to_string());
        }
        if let Some(reference) = tag_action.release.as_deref() {
            apply_release_change(&storage.root_path, &mut simulated.releases, reference, add);
        }
    }

    // The simulated state must reflect the projected completion/due
    // classification (DEV-21), not the pre-action snapshot.
    simulated.task_state = Some(crate::services::completion::compute_task_state(&simulated.status, simulated.due_date.as_deref(), config, chrono::Utc::now()));

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
fn resolve_action_for_event(on: &crate::automation::types::AutomationRuleActions, event: AutomationEvent) -> Option<&AutomationAction> {
    match event {
        AutomationEvent::Created => on.created.as_ref(),
        AutomationEvent::Updated => on.updated.as_ref(),
        AutomationEvent::Assigned => on.assigned.as_ref(),
        AutomationEvent::Commented => on.commented.as_ref(),
        AutomationEvent::SprintChanged => on.sprint_changed.as_ref(),
        AutomationEvent::ReleaseChanged => on.release_changed.as_ref(),
        AutomationEvent::JobStarted => on.job_started.as_ref(),
        AutomationEvent::JobCompleted => on.job_completed.as_ref(),
        AutomationEvent::JobFailed => on.job_failed.as_ref(),
        AutomationEvent::JobCancelled => on.job_cancelled.as_ref(),
        AutomationEvent::Manual => on.manual.as_ref(),
        AutomationEvent::Unblocked => on.unblocked.as_ref(),
    }
}

/// Like `resolve_action_for_event` but falls back to the legacy `start`
/// catch-all for task events (used by the simulate path).
/// "a", "a and b", "a, b and c".
fn join_and(items: &[String]) -> String {
    match items.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        _ => items.join(""),
    }
}

fn resolve_action_for_event_with_fallback(on: &crate::automation::types::AutomationRuleActions, event: AutomationEvent) -> Option<&AutomationAction> {
    resolve_action_for_event(on, event).or(match event {
        AutomationEvent::Created
        | AutomationEvent::Updated
        | AutomationEvent::Assigned
        | AutomationEvent::Commented
        | AutomationEvent::SprintChanged
        | AutomationEvent::ReleaseChanged => on.start.as_ref(),
        _ => None,
    })
}

fn resolve_config_for_project(tasks_dir: &Path, project: Option<&str>) -> LoTaRResult<ResolvedConfig> {
    crate::config::resolution::config_for_project(tasks_dir, project).map_err(|e| LoTaRError::ValidationError(e.to_string()))
}

fn compute_active_sprint_ids(storage: &Storage) -> MatchState {
    MatchState {
        active_sprints: match SprintService::list(storage) {
            Ok(records) => crate::services::sprint_assignment::active_sprint_ids(&records),
            Err(_) => Vec::new(),
        },
        releases: crate::services::release_service::ReleaseService::list(&storage.root_path).unwrap_or_default(),
    }
}

fn load_effective_automation(tasks_dir: &Path, project: Option<&str>, _config: Option<&ResolvedConfig>) -> LoTaRResult<(AutomationFile, AutomationScope)> {
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

    env.insert("LOTAR_TASKS_DIR".to_string(), tasks_dir.to_string_lossy().to_string());
    env.insert("LOTAR_WORKSPACE_ROOT".to_string(), workspace_root.to_string_lossy().to_string());
    if let Some(label) = event_label {
        env.insert("LOTAR_AUTOMATION_EVENT".to_string(), label.to_string());
    }
    env.insert("LOTAR_TICKET_ID".to_string(), task.id.clone());
    if let Some((project, _)) = task.id.rsplit_once('-') {
        env.insert("LOTAR_TICKET_PROJECT".to_string(), project.to_string());
    }
    env.insert("LOTAR_TICKET_STATUS".to_string(), task.status.to_string());
    env.insert("LOTAR_TICKET_PRIORITY".to_string(), task.priority.to_string());
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

fn build_automation_env(task: &TaskDTO, config: &ResolvedConfig, tasks_dir: &Path, context: &AutomationActionContext) -> HashMap<String, String> {
    let event_label = context.event.to_string();
    build_lotar_env(task, config, tasks_dir, Some(&event_label), context.job.as_ref())
}

fn resolve_run_cwd(tasks_dir: &Path, raw: Option<&str>) -> LoTaRResult<std::path::PathBuf> {
    let workspace_root = tasks_dir.parent().unwrap_or(tasks_dir);
    let Some(raw) = raw else {
        return Ok(workspace_root.to_path_buf());
    };

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(LoTaRError::ValidationError("Automation run cwd cannot be empty".to_string()));
    }

    let path = std::path::Path::new(trimmed);
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    if path.components().any(|component| matches!(component, std::path::Component::ParentDir)) {
        return Err(LoTaRError::ValidationError("Automation run cwd cannot contain '..'".to_string()));
    }
    Ok(workspace_root.join(path))
}

/// Whether `run` actions may execute for this task: rules from the home
/// automation file always may; repository rules need a trusted workspace.
fn run_allowed(tasks_dir: &Path, task: &TaskDTO) -> bool {
    let project = crate::storage::TaskId::parse(&task.id).map(|parsed| parsed.project).unwrap_or_default();
    let project_file = !project.is_empty() && crate::utils::paths::project_automation_path(tasks_dir, &project).exists();
    if !project_file && matches!(automation_persistence::load_home_automation(), Ok(Some(_))) {
        return true;
    }
    crate::config::trust::TrustService::allows_commands(tasks_dir)
}

/// Say once per process that repository run commands were held back.
fn warn_untrusted_run_once(tasks_dir: &Path) {
    static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    eprintln!("[lotar][warn] {}", crate::config::trust::TrustService::untrusted_message(tasks_dir, "automation run commands"));
}

/// Default time a waited-for `run` command may take.
const DEFAULT_RUN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// Run a waited-for command without handing it this process's stdio
/// (DEV-278): stdin is empty, so it can't swallow MCP requests, and its
/// output goes to stderr, so it can't corrupt MCP frames or `--format json`
/// output. A command that outlives `timeout` is stopped with its children.
fn run_and_wait(mut cmd: Command, timeout: std::time::Duration, ignore_failure: bool) -> LoTaRResult<()> {
    use std::io::Read;
    cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn()?;
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut tail = BoundedTail { limit: ASYNC_STDERR_LOG_LIMIT, ..BoundedTail::default() };
            if let Some(mut pipe) = pipe {
                let mut buf = [0u8; 4096];
                while let Ok(n) = pipe.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    tail.push(&buf[..n]);
                }
            }
            tail.snapshot()
        })
    };
    let stdout = drain(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let stderr = drain(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if Instant::now() >= deadline {
            #[cfg(unix)]
            // SAFETY: plain kill(2) on the child's own process group.
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    let out = stdout.join().unwrap_or_default();
    let err = stderr.join().unwrap_or_default();
    for text in [&out, &err] {
        let text = text.trim_end();
        if !text.is_empty() {
            eprintln!("{text}");
        }
    }
    match status {
        None => Err(LoTaRError::ValidationError(format!("Automation run command did not finish within {}s and was stopped", timeout.as_secs()))),
        Some(status) if !status.success() && !ignore_failure => {
            let detail = err.trim();
            Err(LoTaRError::ValidationError(if detail.is_empty() {
                format!("Automation run command failed with status {status}")
            } else {
                format!("Automation run command failed with status {status}: {detail}")
            }))
        }
        Some(_) => Ok(()),
    }
}

fn build_shell_command(raw: &str) -> (String, Vec<String>) {
    if cfg!(windows) { ("cmd".to_string(), vec!["/C".to_string(), raw.to_string()]) } else { ("sh".to_string(), vec!["-c".to_string(), raw.to_string()]) }
}

fn execute_run_action(
    run: &AutomationRunAction,
    task: &TaskDTO,
    config: &ResolvedConfig,
    tasks_dir: &Path,
    context: &AutomationActionContext,
    tmpl: &TemplateContext,
) -> LoTaRResult<()> {
    // Repository rules run commands only in a trusted workspace (DEV-259).
    if !run_allowed(tasks_dir, task) {
        warn_untrusted_run_once(tasks_dir);
        return Ok(());
    }

    let (command, args, env, cwd, ignore_failure, wait, timeout) = match run {
        AutomationRunAction::Shell(raw) => {
            let expanded = tmpl.expand_for_shell(raw);
            let trimmed = expanded.script.trim().to_string();
            if trimmed.is_empty() {
                return Err(LoTaRError::ValidationError("Automation run command cannot be empty".to_string()));
            }
            // cmd.exe expands %VARS% before parsing, so it cannot keep
            // values out of the command line; refuse rather than inject.
            if cfg!(windows) && !expanded.env.is_empty() {
                return Err(LoTaRError::ValidationError(
                    "Template values in a short-form run command are not supported on Windows; use the command/args form or the LOTAR_TICKET_* environment variables".to_string(),
                ));
            }
            let (program, args) = build_shell_command(&trimmed);
            (program, args, expanded.env.into_iter().collect::<HashMap<String, String>>(), None, false, true, DEFAULT_RUN_TIMEOUT)
        }
        AutomationRunAction::Command(command) => {
            let expanded_cmd = tmpl.expand(&command.command);
            let trimmed = expanded_cmd.trim().to_string();
            if trimmed.is_empty() {
                return Err(LoTaRError::ValidationError("Automation run command cannot be empty".to_string()));
            }
            let args = command.args.iter().map(|a| tmpl.expand(a)).collect();
            let env = command.env.iter().map(|(k, v)| (k.clone(), tmpl.expand(v))).collect();
            (
                trimmed,
                args,
                env,
                command.cwd.as_ref().map(|c| tmpl.expand(c)),
                command.ignore_failure,
                command.wait,
                match command.timeout.as_deref() {
                    None => DEFAULT_RUN_TIMEOUT,
                    Some(raw) => parse_cooldown(raw)
                        .ok_or_else(|| LoTaRError::ValidationError(format!("Invalid run timeout '{raw}': use a number followed by s, m, h or d")))?,
                },
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
        run_and_wait(cmd, timeout, ignore_failure)?;
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
        cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::piped());
        let child = cmd.spawn().map_err(|e| LoTaRError::ValidationError(format!("Failed to spawn async run command: {e}")))?;
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
        Self { data: Vec::new(), dropped: 0, limit }
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
    let tail = std::sync::Arc::new(std::sync::Mutex::new(BoundedTail::new(ASYNC_STDERR_LOG_LIMIT)));
    let (eof_tx, eof_rx) = std::sync::mpsc::channel::<()>();
    if let Some(mut stderr) = stderr {
        let tail = std::sync::Arc::clone(&tail);
        std::thread::spawn(move || {
            let mut chunk = [0u8; 4096];
            loop {
                match stderr.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => tail.lock().unwrap_or_else(|e| e.into_inner()).push(&chunk[..n]),
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
                eprintln!("[lotar][warn] Async automation run for task {task_id} failed to complete: {e}");
                return;
            }
        };
        let drained = stderr_eof_within_grace(&eof_rx, ASYNC_STDERR_EOF_GRACE);
        let stderr_text = tail.lock().unwrap_or_else(|e| e.into_inner()).snapshot();
        let held_note = if drained { "" } else { " (stderr pipe still open, likely held by a descendant; tail truncated)" };
        if !status.success() {
            if stderr_text.trim().is_empty() {
                eprintln!("[lotar][warn] Async automation run for task {task_id} exited with {status}{held_note}");
            } else {
                eprintln!("[lotar][warn] Async automation run for task {task_id} exited with {status}{held_note}: {}", stderr_text.trim());
            }
        } else if !drained {
            eprintln!("[lotar][warn] Async automation run for task {task_id} exited successfully; a descendant still holds its stderr pipe");
        }
    });
}

/// EOF (or a vanished reader) counts as drained; a pipe that stays silent
/// past `grace` is treated as held open by a descendant.
fn stderr_eof_within_grace(eof_rx: &std::sync::mpsc::Receiver<()>, grace: std::time::Duration) -> bool {
    matches!(eof_rx.recv_timeout(grace), Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected))
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
            if SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) != 0 { SlotDetach::Cleared } else { SlotDetach::NotClearable }
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
            if GetHandleInformation(handle, &mut flags) != 0 { Some(flags) } else { None }
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
        .with_comment(action_context.comment.as_ref())
        .with_unblocked(&action_context.unblocked_by);

    let mut patch = TaskUpdate::default();
    let mut updated_tags = base.tags.clone();
    let mut custom_fields = base.custom_fields.clone();
    let has_relationship_delta = [action.add.as_ref(), action.remove.as_ref()]
        .into_iter()
        .flatten()
        .any(|action| action.depends_on.is_some() || action.blocks.is_some() || action.related.is_some());

    if let Some(set) = action.set.as_ref() {
        apply_set_action(set, current, config, storage, &mut patch, &mut updated_tags, &mut custom_fields, &tmpl)?;
    }

    let mut releases = base.releases.clone();
    if let Some(add) = action.add.as_ref() {
        apply_tag_action(add, true, &mut updated_tags);
        if let Some(sprint_ref) = add.sprint.as_ref() {
            apply_sprint_action(storage, &current.id, sprint_ref, true)?;
        }
        if let Some(reference) = add.release.as_deref() {
            apply_release_change(&storage.root_path, &mut releases, reference, true);
        }
    }
    if let Some(remove) = action.remove.as_ref() {
        apply_tag_action(remove, false, &mut updated_tags);
        if let Some(sprint_ref) = remove.sprint.as_ref() {
            apply_sprint_action(storage, &current.id, sprint_ref, false)?;
        }
        if let Some(reference) = remove.release.as_deref() {
            apply_release_change(&storage.root_path, &mut releases, reference, false);
        }
    }
    if patch.releases.is_none() && releases != base.releases {
        patch.releases = Some(releases);
    }

    if patch.tags.is_none() && updated_tags != base.tags {
        patch.tags = Some(updated_tags);
    }
    if patch.custom_fields.is_none() && custom_fields != base.custom_fields {
        patch.custom_fields = Some(custom_fields);
    }

    let mut next_task = None;
    if has_relationship_delta {
        let moves_root = storage.root_path.clone();
        next_task = Some(TaskService::update_with_relationship_delta(storage, &base.id, patch, TaskUpdateContext::automation_disabled(), |relationships| {
            if let Some(add) = &action.add {
                apply_relationship_action(Some(&moves_root), add, true, relationships);
            }
            if let Some(remove) = &action.remove {
                apply_relationship_action(Some(&moves_root), remove, false, relationships);
            }
        })?);
    } else if patch_has_changes(&patch) {
        next_task = Some(TaskService::update_with_context(storage, &base.id, patch, TaskUpdateContext::automation_disabled())?);
    }
    if let Some(updated) = next_task.as_ref() {
        let run = action.agent.as_ref().map(|agent| AgentRunSettings {
            model: expand_run_setting(agent.model.as_deref(), |raw| tmpl.expand(raw)),
            effort: expand_run_setting(agent.effort.as_deref(), |raw| tmpl.expand(raw)),
            provider: expand_run_setting(agent.provider.as_deref(), |raw| tmpl.expand(raw)),
            session: agent.session,
            prompt: agent.prompt.as_ref().map(|prompt| tmpl.expand(prompt)).filter(|prompt| !prompt.trim().is_empty()),
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
        execute_run_action(run, task_for_env, config, storage.root_path.as_path(), action_context, &tmpl)?;
    }

    if let Some(tell) = action.tell_agent.as_ref() {
        let message = tmpl.expand(&tell.message).trim().to_string();
        if !message.is_empty() {
            let run = action.agent.as_ref().map(|agent| AgentRunSettings {
                model: expand_run_setting(agent.model.as_deref(), |raw| tmpl.expand(raw)),
                effort: expand_run_setting(agent.effort.as_deref(), |raw| tmpl.expand(raw)),
                provider: expand_run_setting(agent.provider.as_deref(), |raw| tmpl.expand(raw)),
                ..AgentRunSettings::default()
            });
            tell_agent(storage, &current.id, config, &message, tell.agent.as_deref(), run.unwrap_or_default())?;
        }
    }

    if let Some(archive) = action.archive {
        TaskService::archive_with_receipt(storage, &current.id, None, archive)?;
    }

    Ok(())
}

/// The action without the `set` fields a job outcome decides (DEV-320).
fn without_outcome_fields<'a>(action: &'a AutomationAction, outcome: Option<&crate::api_types::JobOutcome>) -> std::borrow::Cow<'a, AutomationAction> {
    let Some(outcome) = outcome else {
        return std::borrow::Cow::Borrowed(action);
    };
    let mut action = action.clone();
    if let Some(set) = action.set.as_mut() {
        if outcome.status.is_some() {
            set.status = None;
        }
        if outcome.assignee.is_some() {
            set.assignee = None;
        }
    }
    std::borrow::Cow::Owned(action)
}

/// Apply the status/assignee recorded while a job ran. The agent made
/// that change, so it fires the change rules (updated, assigned) as the
/// same change outside a job would, with `previous` the ticket after the
/// end rules ran (DEV-406); an agent profile as the new assignee starts
/// its job.
fn apply_job_outcome(storage: &mut Storage, ticket_id: &str, config: &ResolvedConfig, outcome: &crate::api_types::JobOutcome) -> LoTaRResult<()> {
    let base = TaskService::get(storage, ticket_id, None)?;
    let patch = TaskUpdate { status: outcome.status.clone(), assignee: outcome.assignee.clone(), ..TaskUpdate::default() };
    let receipt = TaskService::update_with_receipt(storage, &base.id, patch, TaskUpdateContext::automation_disabled())?;
    if !receipt.changed {
        return Ok(());
    }
    AutomationService::apply_task_update(storage, Some(&base), &receipt.dto, config)
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
    pub(crate) fn parse(set: &'a AutomationActionSet, config: &ResolvedConfig, mut expand: impl FnMut(&str) -> String) -> Self {
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

#[expect(clippy::too_many_arguments, reason = "independent inputs from the single caller; bundling them only for this lint would not clarify the call")]
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
        .chain(action.labels.as_ref().map(StringOrVec::as_vec).into_iter().flatten())
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

fn apply_relationship_action(root: Option<&std::path::Path>, action: &AutomationTagAction, add: bool, relationships: &mut crate::types::TaskRelationships) {
    let moves = root.map(crate::storage::moves::MoveResolver::new);
    // Old IDs of moved tasks name the task they became (DEV-183).
    let identity = |value: &str| match (&moves, crate::storage::TaskId::parse(value)) {
        (Some(moves), Ok(_)) => moves.current(value),
        (None, Ok(id)) => id.canonical(),
        (_, Err(_)) => value.to_string(),
    };
    let modify_vec = |vec: &mut Vec<String>, values: &[String], add: bool| {
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
    };

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

/// Add or remove the release a reference names (`@next`: the first planned
/// release in version order). An unknown release is skipped, as for sprints.
fn apply_release_change(tasks_root: &Path, releases: &mut Vec<u32>, reference: &str, add: bool) {
    let records = crate::services::release_service::ReleaseService::list(tasks_root).unwrap_or_default();
    let found = if reference.trim().eq_ignore_ascii_case("@next") {
        records.iter().find(|record| !record.release.is_released())
    } else {
        crate::services::release_service::resolve_in(&records, reference).ok()
    };
    let Some(record) = found else {
        return;
    };
    if add {
        releases.push(record.id);
        releases.sort_unstable();
        releases.dedup();
    } else {
        releases.retain(|id| *id != record.id);
    }
}

fn apply_sprint_action(storage: &mut Storage, task_id: &str, sprint_ref: &str, add: bool) -> LoTaRResult<()> {
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
        let _ = sprint_assignment::remove_tasks(storage, &mut records, &tasks, Some(&sprint_id.to_string()));
    }
    Ok(())
}

/// Append a comment to a task through the shared comment pipeline (used
/// by the automation comment action). The comment is recorded under the
/// `automation` actor and never re-dispatches `on.commented` rules, so
/// comment actions cannot recurse.
fn append_automation_comment(storage: &mut Storage, task_id: &str, text: &str) -> LoTaRResult<()> {
    TaskService::add_comment_with_context(storage, task_id, text, CommentContext::automation()).map(|_| ())
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
        || patch.releases.is_some()
}

/// An action's model or effort with its `${{...}}` templates filled in
/// (DEV-316). An empty result means "not set", so the job keeps the
/// profile's own value instead of getting an empty flag.
fn expand_run_setting(raw: Option<&str>, expand: impl FnOnce(&str) -> String) -> Option<String> {
    raw.map(|raw| expand(raw).trim().to_string()).filter(|value| !value.is_empty())
}

/// Run settings an automation action gives the job its assignment starts.
#[derive(Debug, Default)]
struct AgentRunSettings {
    model: Option<String>,
    effort: Option<String>,
    provider: Option<String>,
    session: Option<crate::config::types::AgentSession>,
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
    let project_prefix = crate::storage::TaskId::parse(&task.id).ok().map(|parsed| parsed.project).unwrap_or_default();
    if let Ok((automation, _)) = load_effective_automation(tasks_dir, Some(project_prefix.as_str()), Some(config)) {
        let limit = automation.automation.max_iterations().unwrap_or(DEFAULT_MAX_ITERATIONS);
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
    let blocked_by = find_blocked_dependencies(&storage, task);
    if !blocked_by.is_empty() {
        let _ = mark_task_blocked(&mut storage, task, config, &blocked_by);
        return;
    }

    let req = AgentJobCreateRequest {
        ticket_id: task.id.clone(),
        prompt: run.prompt.unwrap_or_else(|| DEFAULT_AUTO_PROMPT.to_string()),
        runner: None,
        agent: Some(agent_name),
        model: run.model,
        effort: run.effort,
        provider: run.provider,
        session: run.session,
    };

    let ticket = req.ticket_id.clone();
    let started = if matches!(AgentJobService::orchestrator_mode(), AgentOrchestratorMode::Server) {
        AgentJobService::start_job_with_tasks_dir(req, tasks_dir).map(|_| ())
    } else {
        AgentQueueService::enqueue(tasks_dir, req).map(|_| ())
    };
    // An assignment that can't start its job must say so (DEV-278).
    if let Err(err) = started {
        eprintln!("[lotar][warn] Could not start an agent job for {ticket}: {err}");
        crate::api_events::emit(&crate::api_events::ApiEvent {
            kind: "automation_failed".into(),
            data: serde_json::json!({"id": ticket, "rule": "agent assignment", "message": err.to_string()}),
        });
    }
}

/// The `tell_agent` action (DEV-384), also the start-or-queue path for
/// other hooks: a running job on the ticket gets the message as its next
/// turn; a job that can't take it gets a follow-up job queued behind it.
/// Otherwise the ticket goes to its agent, which starts with the message as
/// the next turn of the ticket's last session where there is one. A job
/// never tells itself: an agent's own comment doesn't come back to it.
fn tell_agent(
    storage: &mut Storage,
    ticket_id: &str,
    config: &ResolvedConfig,
    message: &str,
    fallback: Option<&str>,
    mut run: AgentRunSettings,
) -> LoTaRResult<()> {
    let tasks_dir = storage.root_path.clone();
    let task = TaskService::get(storage, ticket_id, None)?;
    run.session = Some(crate::config::types::AgentSession::Continue);
    run.prompt = Some(message.to_string());
    if let Some(job) = crate::services::agent_active_jobs::for_ticket(&tasks_dir, &task.id) {
        if std::env::var("LOTAR_AGENT_JOB_ID").is_ok_and(|own| own == job.job_id) {
            return Ok(());
        }
        if crate::services::agent_job_service::tell_running_job(&tasks_dir, &job, message) {
            return Ok(());
        }
        let Some(agent) = job.agent.clone() else {
            return Err(LoTaRError::ValidationError(format!("The job running on {} has no agent profile to tell", task.id)));
        };
        let req = AgentJobCreateRequest {
            ticket_id: task.id.clone(),
            prompt: message.to_string(),
            runner: None,
            agent: Some(agent),
            model: run.model,
            effort: run.effort,
            provider: run.provider,
            session: run.session,
        };
        AgentQueueService::enqueue(&tasks_dir, req)?;
        return AgentQueueService::ensure_worker(&tasks_dir);
    }
    if AgentJobService::has_active_job(&task.id) {
        // Queued in this process: the job reads the ticket when it starts.
        return Ok(());
    }
    let Some(agent) = ticket_agent(&task, config, fallback) else {
        return Err(LoTaRError::ValidationError(format!("{} has no agent to tell: set tell_agent.agent", task.id)));
    };
    if !config.agent_profiles.contains_key(&agent) {
        return Err(LoTaRError::ValidationError(format!("Unknown agent profile '{agent}' for {}", task.id)));
    }
    let assignee = format!("@{agent}");
    let (previous, updated) = if task.assignee.as_deref() == Some(assignee.as_str()) {
        (None, task)
    } else {
        let patch = TaskUpdate { assignee: Some(assignee), ..TaskUpdate::default() };
        let updated = TaskService::update_with_context(storage, &task.id, patch, TaskUpdateContext::automation_disabled())?;
        (task.assignee.clone(), updated)
    };
    maybe_queue_agent_on_assignment(&tasks_dir, previous.as_deref(), updated.assignee.as_deref(), &updated, config, run);
    Ok(())
}

/// The ticket's agent profile: its assignee when that is one, else the
/// rule's `agent`, else the last one it was assigned to. The rule's profile
/// comes first, so a ticket a merge agent handed back doesn't go to @merge
/// (DEV-396).
fn ticket_agent(task: &TaskDTO, config: &ResolvedConfig, rule_agent: Option<&str>) -> Option<String> {
    let profile =
        |value: Option<&str>| value.and_then(|value| value.strip_prefix('@')).filter(|name| config.agent_profiles.contains_key(*name)).map(str::to_string);
    let rule_agent = rule_agent.map(|name| name.trim().trim_start_matches('@').to_string());
    profile(task.assignee.as_deref()).or(rule_agent).or_else(|| {
        task.history
            .iter()
            .rev()
            .flat_map(|entry| entry.changes.iter().rev())
            .filter(|change| change.field == "assignee")
            .find_map(|change| profile(change.new.as_deref()).or_else(|| profile(change.old.as_deref())))
    })
}

/// Count how many jobs have reached a terminal state (completed/failed) for the given ticket.
/// Terminal jobs on this ticket that finished in the last day. A window
/// keeps the loop guard from blocking a ticket forever in a long-running
/// server (DEV-279); loops run within minutes.
fn count_terminal_jobs_for_ticket(ticket_id: &str) -> usize {
    let since = chrono::Utc::now() - chrono::Duration::hours(24);
    AgentJobService::list_jobs()
        .iter()
        .filter(|j| {
            crate::storage::identity::aliases_match(&j.ticket_id, ticket_id)
                && (j.status == "completed" || j.status == "failed")
                && j.finished_at
                    .as_deref()
                    .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
                    .is_none_or(|at| at.with_timezone(&chrono::Utc) >= since)
        })
        .count()
}

fn find_blocked_dependencies(storage: &Storage, task: &TaskDTO) -> Vec<String> {
    crate::services::relationships::blocked_dependencies(storage, &task.id, &task.relationships.depends_on)
        .unwrap_or_else(|error| vec![format!("dependency graph unavailable: {error}")])
}

fn mark_task_blocked(storage: &mut Storage, task: &TaskDTO, config: &ResolvedConfig, blocked_by: &[String]) -> LoTaRResult<()> {
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
        TaskService::update_with_context(storage, &task.id, patch, TaskUpdateContext::automation_disabled())?;
    }

    Ok(())
}

fn push_unique_tag(tags: &mut Vec<String>, tag: &str) {
    let trimmed = tag.trim();
    if trimmed.is_empty() {
        return;
    }
    if tags.iter().any(|existing| existing.eq_ignore_ascii_case(trimmed)) {
        return;
    }
    tags.push(trimmed.to_string());
}

fn resolve_assignee_value(value: &str, task: &TaskDTO, config: &ResolvedConfig, storage: &Storage) -> Option<String> {
    let normalized = value.trim();
    match normalized.to_ascii_lowercase().as_str() {
        "@assignee" => task.assignee.clone(),
        "@reporter" => task.reporter.clone(),
        "@assignee_or_reporter" => {
            if let Some(assignee) = task.assignee.clone()
                && config.agent_profiles.keys().any(|name| assignee.eq_ignore_ascii_case(&format!("@{}", name)))
            {
                return task.reporter.clone();
            }
            task.assignee.clone().or_else(|| task.reporter.clone())
        }
        "@round_robin" => resolve_round_robin(config, storage),
        "@random" => resolve_random(config),
        "@least_busy" => resolve_least_busy(config, storage, task),
        _ => resolve_me_alias(normalized, Some(storage.root_path.as_path())),
    }
}

/// Global round-robin counter for cycling through project members.
static ROUND_ROBIN_COUNTER: std::sync::LazyLock<Mutex<usize>> = std::sync::LazyLock::new(|| Mutex::new(0));

/// Filter project members to exclude @-prefixed special tokens and agent profiles.
fn real_members(config: &ResolvedConfig) -> Vec<String> {
    config.members.iter().filter(|m| !m.starts_with('@')).filter(|m| !config.agent_profiles.keys().any(|name| m.eq_ignore_ascii_case(name))).cloned().collect()
}

fn resolve_round_robin(config: &ResolvedConfig, _storage: &Storage) -> Option<String> {
    let members = real_members(config);
    if members.is_empty() {
        return None;
    }
    let mut counter = ROUND_ROBIN_COUNTER.lock().unwrap_or_else(|e| e.into_inner());
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

thread_local! {
    /// Active-task counts per member, kept for one manual automation run so
    /// each `@least_busy` resolution does not rescan the workspace (DEV-292).
    /// `None`: no run active; `Some(None)`: active, not counted yet.
    static LEAST_BUSY_MEMO: std::cell::RefCell<Option<Option<HashMap<String, usize>>>> =
        const { std::cell::RefCell::new(None) };
}

/// Keeps `@least_busy` counts in memory until dropped. Assignments made
/// while it lives update the counts, so a run spreads work the same way
/// one-at-a-time resolutions would.
struct LeastBusyMemo;

impl LeastBusyMemo {
    fn new() -> Self {
        LEAST_BUSY_MEMO.with(|memo| *memo.borrow_mut() = Some(None));
        Self
    }
}

impl Drop for LeastBusyMemo {
    fn drop(&mut self) {
        LEAST_BUSY_MEMO.with(|memo| *memo.borrow_mut() = None);
    }
}

/// Active (not done) tasks per assignee. Reads stored tasks without building
/// DTOs; each task is classified under its own project's completion policy
/// (DEV-21), with one config resolution per (root, project).
fn active_counts_by_assignee(storage: &Storage) -> HashMap<String, usize> {
    let mut done_sets: HashMap<(std::path::PathBuf, String), HashSet<String>> = HashMap::new();
    let mut counts: HashMap<String, usize> = HashMap::new();
    for (id, task, root) in storage.search_located(&crate::storage::TaskFilter::default()) {
        if task.moved_to.is_some() {
            continue;
        }
        let Some(assignee) = task.assignee.as_deref().filter(|a| !a.trim().is_empty()) else {
            continue;
        };
        let prefix = crate::storage::TaskId::parse(&id).map(|parsed| parsed.project).unwrap_or_default();
        let done = done_sets.entry((root.clone(), prefix.clone())).or_insert_with(|| {
            crate::config::resolution::config_for_project(root.as_path(), Some(&prefix))
                .map(|config| crate::services::completion::effective_done_statuses(&config))
                .unwrap_or_default()
        });
        if !done.contains(&task.status.as_str().to_ascii_lowercase()) {
            *counts.entry(assignee.to_string()).or_insert(0) += 1;
        }
    }
    counts
}

fn resolve_least_busy(config: &ResolvedConfig, storage: &Storage, task: &TaskDTO) -> Option<String> {
    let members = real_members(config);
    if members.is_empty() {
        return None;
    }
    let memoized = LEAST_BUSY_MEMO.with(|memo| memo.borrow().is_some());
    let counts = if memoized {
        LEAST_BUSY_MEMO.with(|memo| {
            let mut memo = memo.borrow_mut();
            memo.as_mut().expect("memo is active").get_or_insert_with(|| active_counts_by_assignee(storage)).clone()
        })
    } else {
        active_counts_by_assignee(storage)
    };

    let chosen = members.into_iter().min_by_key(|m| counts.get(m).copied().unwrap_or(0))?;
    if memoized {
        let task_active = !task.task_state.as_ref().is_some_and(|state| state.is_done);
        LEAST_BUSY_MEMO.with(|memo| {
            if let Some(Some(counts)) = memo.borrow_mut().as_mut()
                && task_active
                && task.assignee.as_deref() != Some(chosen.as_str())
            {
                if let Some(previous) = task.assignee.as_deref()
                    && let Some(count) = counts.get_mut(previous)
                {
                    *count = count.saturating_sub(1);
                }
                *counts.entry(chosen.clone()).or_insert(0) += 1;
            }
        });
    }
    Some(chosen)
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
fn convert_custom_field_value(value: &serde_yaml_ng::Value, _key: &str) -> LoTaRResult<crate::types::CustomFieldValue> {
    Ok(value.clone())
}

#[cfg(feature = "schema")]
fn convert_custom_field_value(value: &serde_yaml_ng::Value, key: &str) -> LoTaRResult<crate::types::CustomFieldValue> {
    serde_json::to_value(value).map_err(|err| LoTaRError::SerializationError(format!("Invalid custom field value for {}: {}", key, err)))
}

#[cfg(test)]
mod cooldown_tests {
    use super::*;
    use crate::automation::types::AutomationRuleSet;
    use std::time::Duration;

    #[test]
    fn cooldown_parser_checks_units_and_arithmetic() {
        for (input, seconds) in [("0s", 0), ("30s", 30), (" 5m ", 300), ("2h", 7200), ("1d", 86400)] {
            assert_eq!(parse_cooldown(input), Some(Duration::from_secs(seconds)));
        }
        for input in ["", "5", "-1s", "1.5m", "1\u{e9}", "\u{1d11e}", "18446744073709551615d", "18446744073709551616s"] {
            assert_eq!(parse_cooldown(input), None, "invalid cooldown: {input}");
            let rule = AutomationRule { cooldown: Some(input.into()), ..Default::default() };
            assert!(!cooldown_allows(&rule, "invalid", "TEST-1", Path::new(".")));
        }
        assert!(cooldown_allows(&AutomationRule::default(), "absent", "TEST-1", Path::new(".")));
    }

    #[test]
    fn invalid_cooldown_save_preserves_existing_rules() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("automation.yml");
        let original = "automation: []\n";
        std::fs::write(&path, original).unwrap();
        for input in ["", "1\u{e9}", "18446744073709551615h"] {
            let file = AutomationFile { automation: AutomationRuleSet::List(vec![AutomationRule { cooldown: Some(input.into()), ..Default::default() }]) };
            let yaml = serde_yaml_ng::to_string(&file).unwrap();
            let result = AutomationService::set(dir.path(), None, &yaml);
            assert!(matches!(result, Err(LoTaRError::ValidationError(ref message)) if message.contains("cooldown")));
            assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        }
        let outcome = AutomationService::set(dir.path(), None, "automation:\n- cooldown: 30s\n").unwrap();
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
        use crate::api_types::{RelationshipField, RelationshipMutationRequest, RelationshipOperation};
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        write_config(&workspace, "Done");
        write_task(&workspace, "PX", 1, "Todo");
        write_task(&workspace, "BB", 1, "Todo");
        let mut storage = Storage { root_path: workspace.join(".tasks") };
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
        let context = AutomationActionContext { event: AutomationEvent::Updated, job: None, previous: None, comment: None, unblocked_by: Vec::new() };
        let source = storage.root_path.join("PX/1.yml");
        let target = storage.root_path.join("BB/1.yml");
        let before_source = std::fs::read(&source).unwrap();
        let before_target = std::fs::read(&target).unwrap();
        let events = crate::api_events::subscribe();
        let add: AutomationAction = serde_yaml_ng::from_str("add:\n  depends_on: [BB-001]\n").unwrap();
        apply_action(&mut storage, &current, &config, &add, &context).unwrap();
        assert_eq!(std::fs::read(&source).unwrap(), before_source);
        assert_eq!(std::fs::read(&target).unwrap(), before_target);
        assert!(events.try_iter().next().is_none());
        let remove: AutomationAction = serde_yaml_ng::from_str("remove:\n  depends_on: [BB-001]\n").unwrap();
        apply_action(&mut storage, &current, &config, &remove, &context).unwrap();
        let after = TaskService::get(&storage, "PX-1", None).unwrap();
        assert!(after.relationships.depends_on.is_empty());
        assert!(TaskService::get(&storage, "BB-1", None).unwrap().relationships.blocks.is_empty());
        assert_eq!(after.history.len(), current.history.len() + 1);
        let emitted: Vec<_> = events.try_iter().collect();
        assert_eq!(emitted.iter().filter(|event| event.kind == "task_updated").count(), 2);
    }

    #[test]
    fn relationship_deltas_compose_after_concurrent_user_commit_and_keep_rule_atomic() {
        use crate::api_types::{RelationshipField, RelationshipMutationRequest, RelationshipOperation};
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
            let storage = Storage { root_path: workspace.join(".tasks") };
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
            let action: AutomationAction =
                serde_yaml_ng::from_str(&format!("set:\n  title: rule edit\n{}:\n  depends_on: [PX-2]\n", if removing { "remove" } else { "add" })).unwrap();
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
                    &AutomationActionContext { event: AutomationEvent::Updated, job: None, previous: None, comment: None, unblocked_by: Vec::new() },
                )
            });
            read_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            crate::services::relationships::mutate(
                &storage,
                &RelationshipMutationRequest { target: "PX-3".into(), kind: RelationshipField::Blocks, ..request },
            )
            .unwrap();
            resume_tx.send(()).unwrap();
            worker.join().unwrap().unwrap();
            let updated = TaskService::get(&storage, "PX-1", None).unwrap();
            assert_eq!(updated.title, "rule edit");
            assert_eq!(updated.relationships.blocks, ["PX-3"]);
            assert_eq!(TaskService::get(&storage, "PX-3", None).unwrap().relationships.depends_on, ["PX-1"]);
            assert_eq!(updated.relationships.depends_on, if removing { vec![] } else { vec!["PX-2".to_string()] });
            assert_eq!(TaskService::get(&storage, "PX-2", None).unwrap().relationships.blocks, if removing { vec![] } else { vec!["PX-1".to_string()] });

            let before: Vec<_> = (1..=3).map(|n| fs::read(storage.root_path.join(format!("PX/{n}.yml"))).unwrap()).collect();
            let invalid: AutomationAction = serde_yaml_ng::from_str("set:\n  title: must not publish\nadd:\n  depends_on: [ABSENT-99]\n").unwrap();
            let mut current_storage = Storage { root_path: storage.root_path.clone() };
            let config = TaskService::resolve_config_for_project(&storage.root_path, "PX");
            assert!(
                apply_action(
                    &mut current_storage,
                    &updated,
                    &config,
                    &invalid,
                    &AutomationActionContext { event: AutomationEvent::Updated, job: None, previous: None, comment: None, unblocked_by: Vec::new() }
                )
                .is_err()
            );
            for n in 1..=3 {
                assert_eq!(fs::read(storage.root_path.join(format!("PX/{n}.yml"))).unwrap(), before[n - 1]);
            }
            assert!(!storage.root_path.join("ABSENT").exists());
        }
    }

    #[test]
    fn relationship_actions_preserve_exact_case_task_identities() {
        let action: AutomationTagAction = serde_yaml_ng::from_str("depends_on: [aa-1]\n").unwrap();
        let mut relationships = crate::types::TaskRelationships { depends_on: vec!["AA-1".into()], ..Default::default() };
        apply_relationship_action(None, &action, true, &mut relationships);
        assert_eq!(relationships.depends_on, ["AA-1", "aa-1"]);
        apply_relationship_action(None, &action, false, &mut relationships);
        assert_eq!(relationships.depends_on, ["AA-1"]);
    }

    fn write_task(root: &std::path::Path, prefix: &str, num: u32, status: &str) {
        let dir = root.join(".tasks").join(prefix);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{num}.yml")),
            format!("title: t{num}\nstatus: {status}\npriority: medium\ntype: task\ncreated: 2026-01-01T00:00:00Z\nmodified: 2026-01-01T00:00:00Z\ntags: []\n"),
        )
        .unwrap();
    }

    fn write_config(root: &std::path::Path, done: &str) {
        let tasks = root.join(".tasks");
        std::fs::create_dir_all(&tasks).unwrap();
        std::fs::write(tasks.join("config.yml"), format!("issue:\n  states: [Todo, Done, Shipped]\n  done_states: [{done}]\n")).unwrap();
    }

    fn dependent_dto(id: &str, deps: &[&str]) -> TaskDTO {
        TaskDTO {
            url: None,
            task_state: None,
            deleted_at: None,
            archived: false,
            moved_from: Vec::new(),
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
            relationships: crate::types::TaskRelationships { depends_on: deps.iter().map(|d| (*d).to_string()).collect(), ..Default::default() },
            comments: Vec::new(),
            references: Vec::new(),
            acceptance_criteria: Vec::new(),
            sprints: Vec::new(),
            sprint_order: Default::default(),
            releases: Vec::new(),
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

        for deps in [["PX-1", "PX-2"], ["PX-2", "PX-1"]] {
            let blocked = find_blocked_dependencies(&storage, &dependent_dto("PX-9", &deps));
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
        assert_eq!(find_blocked_dependencies(&storage, &dependent_dto("PX-9", &["PX-1", "PX-2", "PX-3"])), vec!["PX-2".to_string(), "PX-3".to_string()]);
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
        assert!(snapshot.contains("[+6 earlier stderr bytes]\n"), "{snapshot}");
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
        assert!(!stderr_eof_within_grace(&silent_rx, Duration::from_millis(50)));
        assert!(start.elapsed() >= Duration::from_millis(45), "grace returned early: {:?}", start.elapsed());

        // A reader that finished (message sent, then channel closed) is
        // drained, including the already-disconnected case.
        drop(silent_tx);
        assert!(stderr_eof_within_grace(&silent_rx, Duration::from_millis(1)));

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
        for (name, slot) in [("stdout", sys::STD_OUTPUT_HANDLE), ("stderr", sys::STD_ERROR_HANDLE)] {
            let file_type = sys::std_slot_file_type(slot);
            let outcome = sys::clear_std_slot(slot);
            let flags = sys::std_slot_flags(slot);
            if file_type == FILE_TYPE_PIPE {
                // Captured pipe stdio (nextest/CI): the detach must
                // verifiably clear the bit. A refusal on a real pipe is a
                // genuine failure and must not be silently accepted.
                assert_eq!(outcome, sys::SlotDetach::Cleared, "{name} std slot is a pipe; detach refused (file_type={file_type:#x})");
                assert_eq!(flags.unwrap_or(u32::MAX) & sys::HANDLE_FLAG_INHERIT, 0, "{name} pipe std slot still inheritable after detach");
            } else {
                // Console/character handles legitimately refuse
                // SetHandleInformation; interactive runs take this arm and
                // only report/query consistency is enforced there.
                match (outcome, flags) {
                    (sys::SlotDetach::NoHandle, None) => {}
                    (sys::SlotDetach::NotClearable, _) => {}
                    (sys::SlotDetach::Cleared, Some(flags)) => {
                        assert_eq!(flags & sys::HANDLE_FLAG_INHERIT, 0, "{name} std slot still inheritable after detach");
                    }
                    other => panic!("{name}: inconsistent detach state {other:?} (file_type={file_type:#x})"),
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
            assert_eq!(flags.unwrap_or(0) & sys::HANDLE_FLAG_INHERIT, 0, "repeat detach re-armed the inherit flag");
        }
    }
}
