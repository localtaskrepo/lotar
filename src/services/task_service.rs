use crate::api_types::{
    DeletionFilter, TaskCreate, TaskDTO, TaskDeleteResponse, TaskListFilter, TaskUpdate,
};
use crate::config::types::{GlobalConfig, ResolvedConfig};
use crate::errors::{LoTaRError, LoTaRResult};
use crate::services::agent_job_service::AgentJobService;
use crate::services::attachment_service::AttachmentService;
use crate::services::automation_service::AutomationService;
use crate::services::sprint_service::{SprintRecord, SprintService};
use crate::services::task_validation::{self as validation};
use crate::storage::TaskId;
use crate::storage::identity::TaskLookupError;
use crate::storage::locator::StorageLocator;
use crate::storage::manager::Storage;
use crate::storage::operations::StorageOperations;
use crate::storage::sprint::SprintTaskEntry;
use crate::storage::task::Task;
use crate::storage::transaction::MultiFileTransaction;
use crate::types::{Priority, TaskChange, TaskChangeLogEntry, TaskStatus, TaskType};
use crate::utils::identity::{resolve_current_user, resolve_me_alias};
use crate::utils::project::generate_project_prefix;
use crate::utils::tags::normalize_tags;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

pub struct TaskService;

#[derive(Debug, Clone, Copy)]
pub struct TaskUpdateContext {
    pub allow_agent_automation: bool,
    /// Internal bypass used by automation/job lifecycle handlers so they can
    /// mutate assignee/status while a job is active.
    pub bypass_active_job_lock: bool,
    /// External callers must respect review ownership checks.
    pub enforce_review_owner: bool,
    /// Internal automation updates can emit API task events for live UI refresh.
    pub emit_api_event: bool,
}

impl TaskUpdateContext {
    pub fn automation_disabled() -> Self {
        Self {
            allow_agent_automation: false,
            bypass_active_job_lock: true,
            enforce_review_owner: false,
            emit_api_event: true,
        }
    }
}

impl Default for TaskUpdateContext {
    fn default() -> Self {
        Self {
            allow_agent_automation: true,
            bypass_active_job_lock: false,
            enforce_review_owner: true,
            emit_api_event: false,
        }
    }
}

/// Execution context for comment mutations, shared by every surface so
/// single and bulk operations record identical history and fire identical
/// post-commit hooks.
#[derive(Debug, Clone)]
pub struct CommentContext {
    /// Overrides the actor recorded in history (e.g. `automation`); when
    /// absent the current user is resolved as usual.
    pub actor_override: Option<String>,
    /// Whether to fire `on.commented` automation rules post-commit.
    /// Comments written by automation itself disable this so comment
    /// actions can never recurse.
    pub fire_commented: bool,
}

impl Default for CommentContext {
    fn default() -> Self {
        Self {
            actor_override: None,
            fire_commented: true,
        }
    }
}

impl CommentContext {
    /// Context for comments written by automation: recorded under the
    /// `automation` actor, never re-dispatching `on.commented` rules.
    pub fn automation() -> Self {
        Self {
            actor_override: Some("automation".to_string()),
            fire_commented: false,
        }
    }
}

#[cfg(test)]
#[path = "../../tests/common/dev55_transaction_cases.rs"]
mod dev55_transaction_cases;

#[cfg(test)]
#[path = "../../tests/common/dev89_prepared_create_cases.rs"]
mod dev89_prepared_create_cases;

/// A CLI-validated task creation ready for transactional commit (DEV-89).
///
/// The CLI builds and validates the [`Task`] itself (smart defaults,
/// branch inference, member rules, project-name vs prefix resolution),
/// so this carries the already-prepared artifacts instead of a
/// [`TaskCreate`] request, which would re-infer defaults or override
/// explicit fields.
pub struct PreparedTaskCreate<'a> {
    /// Fully validated task, exactly as the CLI built it.
    pub task: &'a Task,
    /// Storage project prefix the task is written under.
    pub project: &'a str,
    /// Human-readable project name the config write should record when
    /// the project config is created or still carries the prefix as its
    /// name (the legacy `StorageOperations::add` naming rule).
    pub original_project_name: Option<&'a str>,
    /// Effective config with pending members already merged; used for the
    /// membership recheck exactly like the CLI validated.
    pub validation_config: &'a ResolvedConfig,
    /// Effective members before the pending merge; the fallback list the
    /// config plan starts from when the project config sets no members.
    pub base_members: &'a [String],
    /// Members the task introduces that are missing from the project
    /// config, computed with the CLI member rules.
    pub pending_members: &'a [String],
}

impl TaskService {
    pub(crate) fn resolve_create_project(tasks_root: &Path, project: Option<String>) -> String {
        project.unwrap_or_else(|| {
            let repo_name = tasks_root
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
                .or_else(crate::project::get_project_name)
                .unwrap_or_else(|| "default".to_string());
            generate_project_prefix(&repo_name)
        })
    }

    pub fn create(storage: &mut Storage, req: TaskCreate) -> LoTaRResult<TaskDTO> {
        let TaskCreate {
            title,
            project,
            status,
            priority,
            task_type,
            reporter,
            assignee,
            due_date,
            effort,
            description,
            tags,
            acceptance_criteria,
            relationships,
            custom_fields,
            sprints,
        } = req;
        if title.trim().is_empty() {
            return Err(LoTaRError::ValidationError(
                "Title cannot be empty.".to_string(),
            ));
        }
        let normalized_sprints = Self::normalize_sprint_ids(&sprints);

        // Prefer explicit project if provided; otherwise, derive from repo folder name
        let project = Self::resolve_create_project(&storage.root_path, project);

        let config = Self::resolve_config_for_project(storage.root_path.as_path(), &project);

        let parsed_status = match status.as_deref() {
            Some(raw) => Some(validation::parse_status(raw, &config)?),
            None => None,
        };
        let parsed_priority = match priority.as_deref() {
            Some(raw) => Some(validation::parse_priority(raw, &config)?),
            None => None,
        };
        let parsed_type = match task_type.as_deref() {
            Some(raw) => Some(validation::parse_task_type(raw, &config)?),
            None => None,
        };
        let custom_fields = match custom_fields.as_ref() {
            Some(cf) => Some(validation::resolve_custom_fields(cf, &config, None)?),
            None => None,
        };

        let resolved_priority = parsed_priority
            .or_else(|| crate::utils::task_intel::infer_priority_from_branch(&config))
            .or_else(|| config.effective_default_priority())
            .unwrap_or_else(|| Priority::from("Medium"));

        let mut resolved_type = parsed_type
            .clone()
            .or_else(|| crate::utils::task_intel::infer_task_type_from_branch(&config))
            .or_else(|| config.effective_default_task_type())
            .unwrap_or_else(|| TaskType::from("Feature"));

        if parsed_type.is_none() {
            resolved_type.ensure_leading_uppercase();
        }

        let mut t = Task::new(storage.root_path.clone(), title, resolved_priority.clone());
        t.priority = resolved_priority;
        t.task_type = resolved_type;
        t.status = parsed_status
            .or_else(|| {
                crate::utils::task_intel::infer_status_from_branch(&config).filter(|status| {
                    TaskStatus::parse_with_config(status.as_str(), &config).is_ok()
                })
            })
            .or_else(|| config.effective_default_status())
            .unwrap_or_else(|| TaskStatus::from("Todo"));
        let auto = config.auto_set_reporter;
        let explicit_reporter = reporter.as_ref().and_then(|rep| {
            let trimmed = rep.trim();
            if trimmed.is_empty() {
                None
            } else {
                resolve_me_alias(trimmed, Some(&storage.root_path))
            }
        });
        t.reporter = if let Some(rep) = explicit_reporter {
            Some(rep)
        } else if auto {
            // Prefer configured default_reporter, then fall back
            if let Some(rep) = config.default_reporter.clone().and_then(|s| {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    resolve_me_alias(trimmed, Some(&storage.root_path))
                }
            }) {
                Some(rep)
            } else {
                resolve_current_user(Some(&storage.root_path))
            }
        } else {
            None
        };
        // Normalize assignee with @me alias if provided
        t.assignee = assignee
            .as_ref()
            .and_then(|a| {
                let trimmed = a.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    resolve_me_alias(trimmed, Some(&storage.root_path))
                }
            })
            .or_else(|| {
                config.default_assignee.as_ref().and_then(|raw| {
                    let trimmed = raw.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        resolve_me_alias(trimmed, Some(&storage.root_path))
                    }
                })
            });
        t.due_date = due_date.filter(|v| !v.trim().is_empty());

        // Normalize member values: strip @ from non-directive usernames.
        let is_agent = |name: &str| config.agent_profiles.contains_key(name);
        t.reporter = t
            .reporter
            .map(|v| crate::utils::member::normalize_member_value(&v, is_agent));
        t.assignee = t
            .assignee
            .map(|v| crate::utils::member::normalize_member_value(&v, is_agent));

        // Normalize effort on write
        t.effort =
            effort.filter(|e| !e.trim().is_empty()).map(
                |e| match crate::utils::effort::parse_effort(&e) {
                    Ok(parsed) => parsed.canonical,
                    Err(_) => e,
                },
            );
        t.description = description.filter(|v| !v.is_empty());
        let mut normalized_tags = normalize_tags(tags);
        if normalized_tags.is_empty() {
            if !config.default_tags.is_empty() {
                normalized_tags.extend(config.default_tags.clone());
            }
            if let Some(label) = crate::utils::task_intel::auto_tag_from_path(&config)
                && !normalized_tags
                    .iter()
                    .any(|existing| existing.eq_ignore_ascii_case(&label))
            {
                normalized_tags.push(label);
            }
        }
        t.tags = normalize_tags(normalized_tags);
        if let Some(rel) = relationships {
            t.relationships = rel;
        }
        if let Some(cf) = custom_fields {
            t.custom_fields = cf;
        }
        t.acceptance_criteria = acceptance_criteria
            .into_iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect();

        Self::ensure_task_defaults(&mut t, &config, true);

        // Coordinated transaction (DEV-55): task file, sprint memberships, and
        // any auto-populated project config are validated in full and then
        // published together under the sprints + project locks. Any failure
        // before or during publication leaves every affected file unchanged
        // (rollback failure retains the journal and fails closed); automation
        // and events run only after the commit.
        crate::storage::safety::validate_project_prefix(&project)
            .map_err(LoTaRError::ValidationError)?;
        let project_path = storage.root_path.join(&project);
        let config_path = crate::utils::paths::project_config_path(&storage.root_path, &project);
        let desired_set: BTreeSet<u32> = normalized_sprints.iter().copied().collect();

        let mut txn =
            MultiFileTransaction::begin(&storage.root_path, std::slice::from_ref(&project))?;
        let (id, config, config_populated) = Self::stage_create(
            &mut txn,
            storage,
            &project_path,
            &config_path,
            &project,
            &t,
            &desired_set,
            config,
        )?;
        txn.commit()?;
        if config_populated {
            Self::invalidate_config_caches(storage.root_path.as_path());
        }

        let sprint_lookup = Self::load_sprint_lookup(storage);
        let dto = Self::to_dto(&id, t, Some(&sprint_lookup), &config, chrono::Utc::now());

        let _ = AutomationService::apply_task_update(storage, None, &dto, &config);

        // Re-fetch after automation may have mutated the task
        let dto = Self::get(storage, &id, Some(&project)).unwrap_or(dto);
        Ok(dto)
    }

    /// Stage every write a task creation needs under the coordinated
    /// transaction: sprint memberships, auto-populated project config, and the
    /// new task file itself. Returns the allocated task ID. Any error here
    /// leaves the workspace untouched, except the fail-closed case where the
    /// transaction's own rollback cannot complete (journal retained).
    #[allow(clippy::too_many_arguments)]
    fn stage_create(
        txn: &mut MultiFileTransaction,
        storage: &Storage,
        project_path: &Path,
        config_path: &Path,
        project: &str,
        t: &Task,
        desired_set: &BTreeSet<u32>,
        config: ResolvedConfig,
    ) -> LoTaRResult<(String, ResolvedConfig, bool)> {
        let (config, config_yaml) =
            Self::plan_auto_populate_members(&storage.root_path, project, t, config)?;
        Self::enforce_membership(t, &config, project)?;

        let mut records = SprintService::list(storage)?;
        let next_numeric_id = StorageOperations::get_current_id(project_path) + 1;
        let id = format!("{}-{}", project, next_numeric_id);
        let touched = Self::apply_memberships_to_records(records.as_mut_slice(), &id, desired_set)?;
        for record in &records {
            if touched.contains(&record.id) {
                SprintService::stage_update(
                    txn,
                    &storage.root_path,
                    record.id,
                    record.sprint.clone(),
                )?;
            }
        }
        let config_populated = config_yaml.is_some();
        if let Some(yaml) = config_yaml {
            txn.stage(config_path, yaml)?;
        }

        let file_path =
            StorageOperations::get_file_path(project, next_numeric_id, &storage.root_path);
        if std::env::var("LOTAR_DEBUG_STATUS").is_ok() {
            eprintln!("[lotar][debug] writing task file {}", file_path.display());
        }
        let file_string = serde_yaml_ng::to_string(t)?;
        txn.stage(&file_path, file_string)?;
        Ok((id, config, config_populated))
    }

    /// Commit a prepared CLI task creation through the coordinated
    /// transaction (DEV-89): the auto-populated project config (including the
    /// project-name creation/update rule) and the task file are validated in
    /// full and published together under the project lock. Any failure before
    /// or during publication leaves every affected file unchanged. Returns
    /// the allocated task ID; automation and events stay with the caller so
    /// the CLI keeps firing them exactly once after the commit.
    #[allow(clippy::needless_pass_by_value)]
    pub fn commit_prepared_create(
        storage: &Storage,
        prepared: PreparedTaskCreate<'_>,
    ) -> LoTaRResult<String> {
        let PreparedTaskCreate {
            task,
            project,
            original_project_name,
            validation_config,
            base_members,
            pending_members,
        } = prepared;

        crate::storage::safety::validate_project_prefix(project)
            .map_err(LoTaRError::ValidationError)?;
        let project_path = storage.root_path.join(project);
        let config_path = crate::utils::paths::project_config_path(&storage.root_path, project);

        let mut txn = MultiFileTransaction::begin(
            &storage.root_path,
            std::slice::from_ref(&project.to_string()),
        )?;

        let (config_yaml, planned_members) = Self::plan_prepared_project_config(
            &storage.root_path,
            project,
            original_project_name,
            base_members,
            pending_members,
        )?;
        let config_staged = config_yaml.is_some();

        // Full-candidate membership recheck under the coordinated locks,
        // against the member list the staged config would persist.
        let mut enforcement_config = validation_config.clone();
        if let Some(members) = planned_members {
            enforcement_config.members = members;
        }
        Self::enforce_membership(task, &enforcement_config, project)?;

        // ID allocation rechecked under the lock, tombstones included.
        let next_numeric_id = StorageOperations::get_current_id(&project_path) + 1;
        let id = format!("{}-{}", project, next_numeric_id);

        if let Some(yaml) = config_yaml {
            txn.stage(&config_path, yaml)?;
        }

        let file_path =
            StorageOperations::get_file_path(project, next_numeric_id, &storage.root_path);
        if std::env::var("LOTAR_DEBUG_STATUS").is_ok() {
            eprintln!("[lotar][debug] writing task file {}", file_path.display());
        }
        let file_string = serde_yaml_ng::to_string(task)?;
        txn.stage(&file_path, file_string)?;
        txn.commit()?;
        if config_staged {
            Self::invalidate_config_caches(storage.root_path.as_path());
        }
        Ok(id)
    }

    /// Plan the project config a prepared CLI creation would write, without
    /// writing it: the legacy naming rule from `StorageOperations::add`
    /// (record the original project name when the config is created or still
    /// holds the prefix) unified with the auto-populated member merge from
    /// [`crate::config::operations::plan_auto_populated_project_config`].
    /// Returns the canonical YAML to stage plus the effective member list
    /// when members were merged.
    fn plan_prepared_project_config(
        tasks_root: &Path,
        project: &str,
        original_project_name: Option<&str>,
        base_members: &[String],
        pending_members: &[String],
    ) -> LoTaRResult<(Option<String>, Option<Vec<String>>)> {
        let needs_name = original_project_name
            .map(str::trim)
            .is_some_and(|name| !name.is_empty());
        if !needs_name && pending_members.is_empty() {
            return Ok((None, None));
        }

        let mut project_config = crate::config::persistence::load_project_config_from_dir(
            project, tasks_root,
        )
        .map_err(|err| {
            LoTaRError::ValidationError(format!(
                "Failed to load project config for project '{}': {}",
                project, err
            ))
        })?;
        let mut changed = false;

        if let Some(original_name) = original_project_name
            .map(str::trim)
            .filter(|n| !n.is_empty())
        {
            // Legacy `StorageOperations::add` rule: record the human-readable
            // name whenever the config is created, or still carries the
            // prefix (or nothing) as its name.
            let config_exists =
                crate::utils::paths::project_config_path(tasks_root, project).is_file();
            let current = project_config.project_name.trim();
            if !config_exists || current.is_empty() || current.eq_ignore_ascii_case(project) {
                if current != original_name {
                    project_config.project_name = original_name.to_string();
                }
                changed = true;
            }
        }

        let mut planned_members = None;
        if !pending_members.is_empty() {
            let mut effective = match project_config.members.clone() {
                Some(existing) => existing,
                None => base_members
                    .iter()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                    .collect(),
            };
            let mut members_changed = false;
            for candidate in pending_members {
                let trimmed = candidate.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let already_present = effective
                    .iter()
                    .any(|existing| existing.eq_ignore_ascii_case(trimmed));
                if !already_present {
                    effective.push(trimmed.to_string());
                    members_changed = true;
                }
            }
            if members_changed {
                effective.sort_by_key(|a| a.to_ascii_lowercase());
                project_config.members = Some(effective.clone());
                planned_members = Some(effective);
                changed = true;
            } else {
                planned_members = Some(effective);
            }
        }

        if !changed {
            return Ok((None, planned_members));
        }
        let yaml = crate::config::normalization::to_canonical_project_yaml(&project_config);
        Ok((Some(yaml), planned_members))
    }

    fn invalidate_config_caches(tasks_root: &Path) {
        crate::config::resolution::invalidate_config_cache_for(Some(tasks_root));
        crate::utils::identity::invalidate_identity_cache(Some(tasks_root));
    }

    pub fn get(storage: &Storage, id: &str, project: Option<&str>) -> LoTaRResult<TaskDTO> {
        Self::get_with_deletion(storage, id, project, false)
    }

    /// Fetch a task including its soft-deletion tombstone state (DEV-92).
    /// Returns the DTO with `deleted_at` set for deleted tasks instead of
    /// the default `TaskNotFound` projection.
    pub fn get_including_deleted(
        storage: &Storage,
        id: &str,
        project: Option<&str>,
    ) -> LoTaRResult<TaskDTO> {
        Self::get_with_deletion(storage, id, project, true)
    }

    fn get_with_deletion(
        storage: &Storage,
        id: &str,
        project: Option<&str>,
        include_deleted: bool,
    ) -> LoTaRResult<TaskDTO> {
        // Canonical parse: the project prefix is everything before the FINAL
        // numeric suffix (ABCD-1 -> ABCD, ABC-OPS-12 -> ABC-OPS).
        let parsed =
            TaskId::parse(id).map_err(|err| LoTaRError::InvalidTaskId(format!("{id}: {err}")))?;
        let p = project.unwrap_or(&parsed.project).to_string();
        match storage.get(id, &p) {
            Some(mut t) => {
                // Default reads hide tombstones: a deleted task is
                // indistinguishable from a missing one unless the caller
                // explicitly opts into lifecycle state.
                if !include_deleted && t.deleted_at.is_some() {
                    return Err(LoTaRError::TaskNotFound(id.to_string()));
                }
                // Config and sprint memberships come from the tasks root that
                // actually holds the task, not necessarily the primary one.
                let config_root = match storage.resolve_task_location(id) {
                    Ok(location) => location.root.clone(),
                    Err(_) => storage.root_path.clone(),
                };
                let config = Self::resolve_config_for_project(config_root.as_path(), &p);
                Self::ensure_task_defaults(&mut t, &config, false);
                let sprint_lookup = Self::load_sprint_lookup(storage);
                // Padded aliases (TP-001) surface the canonical spelling (TP-1)
                // so all transports agree on the task identity.
                Ok(Self::to_dto(
                    &parsed.canonical(),
                    t,
                    Some(&sprint_lookup),
                    &config,
                    chrono::Utc::now(),
                ))
            }
            None => Err(LoTaRError::TaskNotFound(id.to_string())),
        }
    }

    /// Fail closed for mutations addressed to a soft-deleted task: a
    /// tombstone is read-only until restored (DEV-92).
    pub(crate) fn ensure_not_deleted(task: &Task, id: &str) -> LoTaRResult<()> {
        if task.deleted_at.is_some() {
            return Err(LoTaRError::ValidationError(format!(
                "Task '{id}' is deleted; restore it before modifying it"
            )));
        }
        Ok(())
    }

    /// Add a comment to a task and fire `on.commented` automation rules.
    pub fn add_comment(storage: &mut Storage, id: &str, text: &str) -> LoTaRResult<TaskDTO> {
        Self::add_comment_with_context(storage, id, text, CommentContext::default())
    }

    /// Add a comment through the shared locked mutation pipeline, honoring
    /// the given [`CommentContext`] for actor attribution and
    /// `on.commented` dispatch. Bulk surfaces delegate here so single and
    /// bulk operations produce identical history, timestamps, and
    /// automation behavior. The comment append and its history entry are
    /// applied to the freshest persisted state under the project task lock,
    /// so concurrent reference (or other) writes can never be lost.
    pub fn add_comment_with_context(
        storage: &mut Storage,
        id: &str,
        text: &str,
        context: CommentContext,
    ) -> LoTaRResult<TaskDTO> {
        let parsed =
            TaskId::parse(id).map_err(|err| LoTaRError::InvalidTaskId(format!("{id}: {err}")))?;
        let canonical = parsed.canonical();
        Self::ensure_local_task(storage, id)?;

        let root_path = storage.root_path.clone();
        let canonical_guard = canonical.clone();
        let outcome = storage.mutate_task(&canonical, move |task| {
            // Lifecycle guard (DEV-92): no comments on a tombstone.
            Self::ensure_not_deleted(task, &canonical_guard)?;
            let now = chrono::Utc::now().to_rfc3339();
            let actor = context
                .actor_override
                .clone()
                .or_else(|| resolve_current_user(Some(root_path.as_path())));
            task.comments.push(crate::types::TaskComment {
                date: now.clone(),
                text: text.to_string(),
            });
            // Record that a comment was added without copying the body into
            // the changelog (the comment itself is the audit trail).
            task.history.push(TaskChangeLogEntry {
                at: now.clone(),
                actor,
                changes: vec![TaskChange {
                    field: "comment_added".into(),
                    old: None,
                    new: None,
                }],
            });
            task.modified = now;
            Ok(true)
        })?;

        let config = Self::resolve_config_for_project(storage.root_path.as_path(), &parsed.project);
        let sprint_lookup = Self::load_sprint_lookup(storage);
        let dto = Self::to_dto(
            &canonical,
            outcome.after,
            Some(&sprint_lookup),
            &config,
            chrono::Utc::now(),
        );
        if context.fire_commented {
            let _ = AutomationService::apply_comment_event(storage, &dto, text, &config);
        }
        Ok(dto)
    }

    /// Edit an existing comment (0-based index) and record a `comment#N`
    /// history entry through the shared locked mutation pipeline. Returns
    /// the unchanged task (no write, no history, no timestamp churn) when
    /// the new text equals the old one. Comment edits deliberately do not
    /// fire `on.commented`; that trigger belongs to new comments only.
    pub fn update_comment(
        storage: &mut Storage,
        id: &str,
        index: usize,
        text: &str,
    ) -> LoTaRResult<TaskDTO> {
        let parsed =
            TaskId::parse(id).map_err(|err| LoTaRError::InvalidTaskId(format!("{id}: {err}")))?;
        let canonical = parsed.canonical();
        Self::ensure_local_task(storage, id)?;

        let root_path = storage.root_path.clone();
        let canonical_guard = canonical.clone();
        let outcome = storage.mutate_task(&canonical, move |task| {
            // Lifecycle guard (DEV-92): tombstones are read-only.
            Self::ensure_not_deleted(task, &canonical_guard)?;
            if index >= task.comments.len() {
                return Err(LoTaRError::ValidationError(format!(
                    "Invalid comment index {index}"
                )));
            }

            let previous = task.comments[index].text.clone();
            if previous == text {
                return Ok(false);
            }
            task.comments[index].text = text.to_string();
            let now = chrono::Utc::now().to_rfc3339();
            task.history.push(TaskChangeLogEntry {
                at: now.clone(),
                actor: resolve_current_user(Some(root_path.as_path())),
                changes: vec![TaskChange {
                    field: format!("comment#{}", index + 1),
                    old: Some(previous),
                    new: Some(text.to_string()),
                }],
            });
            task.modified = now;
            Ok(true)
        })?;

        let config = Self::resolve_config_for_project(storage.root_path.as_path(), &parsed.project);
        let sprint_lookup = Self::load_sprint_lookup(storage);
        Ok(Self::to_dto(
            &canonical,
            outcome.after,
            Some(&sprint_lookup),
            &config,
            chrono::Utc::now(),
        ))
    }

    /// Fail fast for comment/reference mutations addressed to a task that
    /// is missing, ambiguous, or stored in another workspace root.
    /// Missing/ambiguous identities surface as [`LoTaRError::TaskNotFound`]
    /// (matching the previous unlocked `storage.get` lookup); cross-root
    /// refusals keep the actionable "run the command inside that workspace"
    /// message.
    pub(crate) fn ensure_local_task(storage: &Storage, id: &str) -> LoTaRResult<()> {
        match storage.resolve_task_location(id) {
            Ok(location) if !location.is_in_root(&storage.root_path) => {
                Err(LoTaRError::ValidationError(format!(
                    "Task '{}' is stored in workspace tasks root {} and cannot be modified from {}; run the command inside that workspace",
                    location.full_id(),
                    location.root.display(),
                    storage.root_path.display()
                )))
            }
            Ok(_) => Ok(()),
            Err(_) => Err(LoTaRError::TaskNotFound(id.to_string())),
        }
    }

    pub fn update(storage: &mut Storage, id: &str, patch: TaskUpdate) -> LoTaRResult<TaskDTO> {
        Self::update_with_context(storage, id, patch, TaskUpdateContext::default())
    }

    pub fn update_with_context(
        storage: &mut Storage,
        id: &str,
        patch: TaskUpdate,
        context: TaskUpdateContext,
    ) -> LoTaRResult<TaskDTO> {
        // Derive the project prefix from the canonical ID parse (final numeric
        // suffix) to locate the task. `canonical` (padded aliases collapsed,
        // e.g. TP-001 -> TP-1) is the single identity used for every
        // comparison, staged write, event, automation run, and DTO response.
        let parsed =
            TaskId::parse(id).map_err(|err| LoTaRError::InvalidTaskId(format!("{id}: {err}")))?;
        let derived = parsed.project.clone();
        let canonical = parsed.canonical();
        // Cheap unlocked location check keeps TaskNotFound lock-free and
        // refuses cross-root or ambiguous mutations before any side effect:
        // transactions hold locks and a journal only for the primary root.
        match storage.resolve_task_location(id) {
            Ok(location) if !location.is_in_root(&storage.root_path) => {
                return Err(LoTaRError::ValidationError(format!(
                    "Task '{}' is stored in workspace tasks root {} and cannot be modified from {}; run the command inside that workspace",
                    id,
                    location.root.display(),
                    storage.root_path.display()
                )));
            }
            Ok(_) => {}
            Err(TaskLookupError::NotFound(_)) => {
                return Err(LoTaRError::TaskNotFound(id.to_string()));
            }
            Err(err) => return Err(LoTaRError::ValidationError(err.to_string())),
        }

        // Coordinated transaction (DEV-55): sprint memberships, task fields,
        // and any auto-populated project config validate fully and publish
        // together under the sprints + project locks. Errors leave every
        // affected file unchanged; automation runs strictly post-commit.
        let mut txn =
            MultiFileTransaction::begin(&storage.root_path, std::slice::from_ref(&derived))?;
        let existing = storage
            .get(id, &derived)
            .ok_or_else(|| LoTaRError::TaskNotFound(id.to_string()))?;
        // Lifecycle guard (DEV-92): a tombstone is read-only until restored.
        Self::ensure_not_deleted(&existing, id)?;

        let config = Self::resolve_config_for_project(storage.root_path.as_path(), &derived);

        // Shared project-aware enum validation: raw patch strings are checked
        // against this task's project configuration and canonicalized before
        // any mutation is applied.
        let parsed_status = match patch.status.as_deref() {
            Some(raw) => Some(validation::parse_status(raw, &config)?),
            None => None,
        };
        let parsed_priority = match patch.priority.as_deref() {
            Some(raw) => Some(validation::parse_priority(raw, &config)?),
            None => None,
        };
        let parsed_task_type = match patch.task_type.as_deref() {
            Some(raw) => Some(validation::parse_task_type(raw, &config)?),
            None => None,
        };
        let resolved_custom_fields = match patch.custom_fields.as_ref() {
            Some(cf) => Some(validation::resolve_custom_fields(
                cf,
                &config,
                Some(&existing.custom_fields),
            )?),
            None => None,
        };

        if context.enforce_review_owner {
            Self::enforce_review_owner_transition(
                storage,
                id,
                &existing,
                parsed_status.as_ref(),
                patch.assignee.as_deref(),
            )?;
        }

        // Canonical id for the active-job carveout: a job running for TP-1
        // covers updates addressed through the padded alias TP-001, and the
        // registry lookup must not miss it either.
        let current_job_can_mutate = Self::current_job_matches_ticket(&canonical);

        if AgentJobService::has_active_job(&canonical)
            && !context.bypass_active_job_lock
            && !current_job_can_mutate
        {
            // Block assignee/status changes while a job is active for external callers.
            // Other fields remain editable so the user can communicate with the agent (e.g. description/links).
            if let Some(next) = patch.assignee.as_ref() {
                let trimmed = next.trim();
                let next_value = if trimmed.is_empty() {
                    None
                } else {
                    resolve_me_alias(trimmed, Some(&storage.root_path))
                };
                let same = match (&existing.assignee, &next_value) {
                    (Some(a), Some(b)) => {
                        crate::utils::member::member_for_comparison(a)
                            == crate::utils::member::member_for_comparison(b)
                    }
                    (None, None) => true,
                    _ => false,
                };
                if !same {
                    return Err(LoTaRError::ValidationError(format!(
                        "Ticket '{}' has an active agent job. Stop/cancel the running job before changing assignee.",
                        id
                    )));
                }
            }
            if let Some(next_status) = parsed_status.as_ref()
                && &existing.status != next_status
            {
                return Err(LoTaRError::ValidationError(format!(
                    "Ticket '{}' has an active agent job. Stop/cancel the running job before changing status.",
                    id
                )));
            }
        }
        let mut t = existing.clone();
        let mut changes: Vec<TaskChange> = Vec::new();
        let mut record_change = |field: &str, old: Option<String>, new: Option<String>| {
            if old != new {
                changes.push(TaskChange {
                    field: field.to_string(),
                    old,
                    new,
                });
            }
        };

        if let Some(v) = patch.title {
            if v.trim().is_empty() {
                return Err(LoTaRError::ValidationError(
                    "Title cannot be empty.".to_string(),
                ));
            }
            let previous = t.title.clone();
            if previous != v {
                record_change("title", Some(previous), Some(v.clone()));
                t.title = v;
            }
        }
        if let Some(v) = parsed_status {
            let old_status = t.status.clone();
            let new_status = v;
            if old_status != new_status {
                record_change(
                    "status",
                    Some(old_status.to_string()),
                    Some(new_status.to_string()),
                );
            }
            t.status = new_status.clone();
            let auto = config.auto_assign_on_status;
            let is_first_change = old_status != new_status && t.assignee.is_none();
            if auto
                && is_first_change
                && let Some(me) = resolve_current_user(Some(&storage.root_path))
            {
                let previous_assignee = t.assignee.clone();
                if previous_assignee.as_deref() != Some(me.as_str()) {
                    record_change("assignee", previous_assignee, Some(me.clone()));
                }
                t.assignee = Some(me);
            }
        }
        if let Some(v) = parsed_priority {
            let previous = t.priority.clone();
            if previous != v {
                record_change("priority", Some(previous.to_string()), Some(v.to_string()));
                t.priority = v;
            }
        }
        if let Some(v) = parsed_task_type {
            let previous = t.task_type.clone();
            if previous != v {
                record_change("task_type", Some(previous.to_string()), Some(v.to_string()));
                t.task_type = v;
            }
        }
        if let Some(v) = patch.reporter {
            let trimmed = v.trim();
            let new_value = if trimmed.is_empty() {
                None
            } else {
                resolve_me_alias(trimmed, Some(&storage.root_path)).map(|resolved| {
                    crate::utils::member::normalize_member_value(&resolved, |name| {
                        config.agent_profiles.contains_key(name)
                    })
                })
            };
            let previous = t.reporter.clone();
            if previous != new_value {
                record_change("reporter", previous, new_value.clone());
                t.reporter = new_value;
            }
        }
        if let Some(v) = patch.assignee {
            let trimmed = v.trim();
            let new_value = if trimmed.is_empty() {
                None
            } else {
                resolve_me_alias(trimmed, Some(&storage.root_path)).map(|resolved| {
                    crate::utils::member::normalize_member_value(&resolved, |name| {
                        config.agent_profiles.contains_key(name)
                    })
                })
            };
            let previous = t.assignee.clone();
            if previous != new_value {
                record_change("assignee", previous, new_value.clone());
                t.assignee = new_value;
            }
        }
        if let Some(v) = patch.due_date {
            let trimmed = v.trim();
            let new_value = if trimmed.is_empty() {
                None
            } else {
                Some(v.clone())
            };
            let previous = t.due_date.clone();
            if previous != new_value {
                record_change("due_date", previous, new_value.clone());
                t.due_date = new_value;
            }
        }
        if let Some(v) = patch.effort {
            let parsed = if v.is_empty() {
                None // empty string means "clear effort"
            } else {
                match crate::utils::effort::parse_effort(&v) {
                    Ok(parsed) => Some(parsed.canonical),
                    Err(_) => Some(v),
                }
            };
            let previous = t.effort.clone();
            if previous != parsed {
                record_change("effort", previous, parsed.clone());
                t.effort = parsed;
            }
        }
        if let Some(v) = patch.description {
            let new_value = if v.is_empty() { None } else { Some(v.clone()) };
            let previous = t.description.clone();
            if previous != new_value {
                record_change("description", previous, new_value.clone());
                t.description = new_value;
            }
        }
        if let Some(v) = patch.acceptance_criteria {
            let new_criteria: Vec<String> = v
                .into_iter()
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect();
            let previous = t.acceptance_criteria.clone();
            if previous != new_criteria {
                record_change(
                    "acceptance_criteria",
                    Some(previous.join("\n")),
                    Some(new_criteria.join("\n")),
                );
                t.acceptance_criteria = new_criteria;
            }
        }
        let explicit_tags = patch.tags.is_some();
        if let Some(v) = patch.tags {
            let new_tags = normalize_tags(v);
            let previous = t.tags.clone();
            if previous != new_tags {
                record_change("tags", Some(previous.join(", ")), Some(new_tags.join(", ")));
                t.tags = new_tags;
            }
        }
        if let Some(v) = patch.relationships
            && t.relationships != v
        {
            let old_json = serde_json::to_string(&t.relationships).ok();
            let new_json = serde_json::to_string(&v).ok();
            record_change("relationships", old_json, new_json.clone());
            t.relationships = v;
        }
        if let Some(v) = resolved_custom_fields
            && t.custom_fields != v
        {
            let old_yaml = serde_yaml_ng::to_string(&t.custom_fields).ok();
            let new_yaml = serde_yaml_ng::to_string(&v).ok();
            record_change("custom_fields", old_yaml, new_yaml.clone());
            t.custom_fields = v;
        }

        let mut sprint_change: Option<(Vec<SprintRecord>, BTreeSet<u32>)> = None;
        if let Some(sprint_ids) = patch.sprints.clone() {
            let desired_set: BTreeSet<u32> = Self::normalize_sprint_ids(&sprint_ids)
                .into_iter()
                .collect();
            let records = SprintService::list(storage)?;
            let current_set: BTreeSet<u32> = records
                .iter()
                .filter(|record| {
                    // Canonical match: legacy padded spellings (TP-001) count
                    // as membership of the same task (TP-1).
                    record
                        .sprint
                        .tasks
                        .iter()
                        .any(|entry| entry_matches_task(&entry.id, &canonical))
                })
                .map(|record| record.id)
                .collect();

            if current_set != desired_set {
                let old_display = Self::format_sprint_change(&current_set);
                let new_display = Self::format_sprint_change(&desired_set);
                record_change("sprints", old_display, new_display);
                sprint_change = Some((records, desired_set));
            }
        }

        let modified = chrono::Utc::now().to_rfc3339();
        t.modified = modified.clone();

        if !changes.is_empty() {
            let history_actor = resolve_current_user(Some(&storage.root_path));
            t.history.push(TaskChangeLogEntry {
                at: modified,
                actor: history_actor,
                changes,
            });
        }

        Self::ensure_task_defaults(&mut t, &config, !explicit_tags);
        let (config, config_yaml) =
            Self::plan_auto_populate_members(&storage.root_path, &derived, &t, config)?;
        Self::enforce_membership(&t, &config, &derived)?;

        t.sprints.clear();

        // Stage in deterministic order: sprints, config, task file.
        if let Some((mut records, desired_set)) = sprint_change {
            let touched = Self::apply_memberships_to_records(
                records.as_mut_slice(),
                &canonical,
                &desired_set,
            )?;
            for record in &records {
                if touched.contains(&record.id) {
                    SprintService::stage_update(
                        &mut txn,
                        &storage.root_path,
                        record.id,
                        record.sprint.clone(),
                    )?;
                }
            }
        }
        let config_populated = config_yaml.is_some();
        if let Some(yaml) = config_yaml {
            let config_path =
                crate::utils::paths::project_config_path(&storage.root_path, &derived);
            txn.stage(&config_path, yaml)?;
        }
        let project_path = storage.root_path.join(&derived);
        let (file_path, file_string) =
            StorageOperations::prepare_task_edit(&project_path, &canonical, &t)
                .map_err(crate::storage::manager::map_storage_error)?;
        txn.stage(&file_path, file_string)?;
        txn.commit()?;
        if config_populated {
            Self::invalidate_config_caches(storage.root_path.as_path());
        }

        let sprint_lookup = Self::load_sprint_lookup(storage);
        let now = chrono::Utc::now();
        let previous_dto = Self::to_dto(&canonical, existing, Some(&sprint_lookup), &config, now);
        let dto = Self::to_dto(&canonical, t, Some(&sprint_lookup), &config, now);

        if context.emit_api_event {
            let actor = resolve_current_user(Some(storage.root_path.as_path()));
            crate::api_events::emit_task_updated(&dto, actor.as_deref());
        }

        if context.allow_agent_automation {
            let _ =
                AutomationService::apply_task_update(storage, Some(&previous_dto), &dto, &config);
        }

        Ok(dto)
    }

    fn current_job_matches_ticket(id: &str) -> bool {
        let Ok(job_id) = std::env::var("LOTAR_AGENT_JOB_ID") else {
            return false;
        };
        let Ok(ticket_id) = std::env::var("LOTAR_TICKET_ID") else {
            return false;
        };
        let trimmed_job_id = job_id.trim();
        // Canonical alias comparison: a job's env identity matches updates
        // addressed through any valid padded alias of the same ticket.
        // Unparseable values never match, keeping unrelated callers locked out.
        if trimmed_job_id.is_empty() || !ticket_alias_matches(ticket_id.trim(), id) {
            return false;
        }

        AgentJobService::get_job(trimmed_job_id).is_some_and(|job| {
            ticket_alias_matches(job.ticket_id.trim(), id)
                && matches!(job.status.as_str(), "queued" | "running")
        })
    }

    fn enforce_review_owner_transition(
        storage: &Storage,
        id: &str,
        existing: &Task,
        parsed_status: Option<&TaskStatus>,
        patch_assignee: Option<&str>,
    ) -> LoTaRResult<()> {
        if !matches_review_state(&existing.status) {
            return Ok(());
        }

        let status_changed = parsed_status.is_some_and(|next| next != &existing.status);
        let assignee_changed = patch_assignee.is_some_and(|next| {
            let trimmed = next.trim();
            let next_value = if trimmed.is_empty() {
                None
            } else {
                resolve_me_alias(trimmed, Some(&storage.root_path))
            };
            match (&existing.assignee, &next_value) {
                (Some(a), Some(b)) => {
                    crate::utils::member::member_for_comparison(a)
                        != crate::utils::member::member_for_comparison(b)
                }
                (None, None) => false,
                _ => true,
            }
        });

        if !status_changed && !assignee_changed {
            return Ok(());
        }

        let Some(reporter) = existing.reporter.as_deref() else {
            return Err(LoTaRError::ValidationError(format!(
                "Ticket '{}' is awaiting review, but it has no reporter to authorize review transitions.",
                id
            )));
        };

        let actor = resolve_current_user(Some(storage.root_path.as_path()));
        let is_reporter = actor
            .as_deref()
            .is_some_and(|current| current.eq_ignore_ascii_case(reporter));

        if is_reporter {
            return Ok(());
        }

        Err(LoTaRError::ValidationError(format!(
            "Ticket '{}' is awaiting review. Only reporter '{}' can change status or assignee until review is resolved.",
            id, reporter
        )))
    }

    /// Legacy deletion entry point for existing callers: SOFT by default
    /// (DEV-92). Returns whether the task is in the deleted state after the
    /// call; a missing task maps to `Ok(false)` exactly like the previous
    /// physical deletion contract, while cross-project/cross-root refusals
    /// stay errors. Callers that need warnings, hard deletion, or explicit
    /// not-found errors use [`Self::delete_with_options`].
    pub fn delete(storage: &mut Storage, id: &str, project: Option<&str>) -> LoTaRResult<bool> {
        match Self::delete_with_options(storage, id, project, false) {
            Ok(outcome) => Ok(outcome.deleted),
            Err(LoTaRError::TaskNotFound(_)) => Ok(false),
            Err(err) => Err(err),
        }
    }

    /// Shared preflight for every deletion surface: canonical identity,
    /// project isolation, cross-root refusal, and existence (including
    /// tombstones, which can be deleted again or hard-deleted).
    fn ensure_deletable(
        storage: &Storage,
        id: &str,
        project: Option<&str>,
    ) -> LoTaRResult<(String, String)> {
        let parsed =
            TaskId::parse(id).map_err(|err| LoTaRError::InvalidTaskId(format!("{id}: {err}")))?;
        let derived = parsed.project.clone();
        let canonical = parsed.canonical();
        if let Some(explicit) = project
            && explicit != derived
        {
            return Err(LoTaRError::ValidationError(format!(
                "Task ID '{id}' belongs to project '{derived}', not '{explicit}'; refusing to cross projects"
            )));
        }
        match storage.resolve_task_location(&canonical) {
            Ok(location) if !location.is_in_root(&storage.root_path) => {
                Err(LoTaRError::ValidationError(format!(
                    "Task '{}' is stored in workspace tasks root {} and cannot be modified from {}; run the command inside that workspace",
                    location.full_id(),
                    location.root.display(),
                    storage.root_path.display()
                )))
            }
            Ok(_) => Ok((canonical, derived)),
            Err(TaskLookupError::NotFound(_)) => Err(LoTaRError::TaskNotFound(canonical.clone())),
            Err(err) => Err(LoTaRError::ValidationError(err.to_string())),
        }
    }

    /// Read-only deletion preview (DEV-92): validates exactly like
    /// [`Self::delete_with_options`] but mutates nothing. Returns
    /// `{deleted: false, hard, warnings}` where `warnings` is populated for
    /// hard previews (retained attachment blobs with stored paths and
    /// incoming relationships with canonical dependent IDs) and empty for
    /// soft previews.
    pub fn preview_delete(
        storage: &Storage,
        id: &str,
        project: Option<&str>,
        hard: bool,
    ) -> LoTaRResult<TaskDeleteResponse> {
        let (canonical, derived) = Self::ensure_deletable(storage, id, project)?;
        let warnings = if hard {
            let task = storage
                .get(&canonical, &derived)
                .ok_or_else(|| LoTaRError::TaskNotFound(canonical.clone()))?;
            Self::hard_delete_warnings(storage, &canonical, &task)
        } else {
            Vec::new()
        };
        Ok(TaskDeleteResponse {
            deleted: false,
            hard,
            warnings,
        })
    }

    /// Delete a task (DEV-92). Soft deletion (default) writes an in-place
    /// `deleted_at` tombstone and appends a `deleted` history entry while
    /// leaving `modified` unchanged; a repeated soft deletion of an already
    /// deleted task is a no-op that keeps reporting `deleted: true`. Hard
    /// deletion physically removes the task file (`Storage::delete` stays
    /// physical for rollback paths), preserves attachment blobs, and returns
    /// warnings describing retained blobs and incoming relationships
    /// detected across stored tasks (including other tombstones).
    pub fn delete_with_options(
        storage: &mut Storage,
        id: &str,
        project: Option<&str>,
        hard: bool,
    ) -> LoTaRResult<TaskDeleteResponse> {
        let (canonical, derived) = Self::ensure_deletable(storage, id, project)?;

        if hard {
            let task = storage
                .get(&canonical, &derived)
                .ok_or_else(|| LoTaRError::TaskNotFound(canonical.clone()))?;
            let warnings = Self::hard_delete_warnings(storage, &canonical, &task);
            let removed = storage.delete(&canonical, &derived)?;
            if !removed {
                return Err(LoTaRError::TaskNotFound(canonical.clone()));
            }
            return Ok(TaskDeleteResponse {
                deleted: true,
                hard: true,
                warnings,
            });
        }

        // Soft delete: locked read-modify-write over the freshest state so a
        // concurrent edit can never be lost, and a tombstone never overwrites
        // a concurrent restore. `modified` is deliberately untouched.
        let root_path = storage.root_path.clone();
        storage.mutate_task(&canonical, move |task| {
            if task.deleted_at.is_some() {
                // Idempotent no-op: no write, no extra history entry.
                return Ok(false);
            }
            let now = chrono::Utc::now().to_rfc3339();
            task.history.push(TaskChangeLogEntry {
                at: now.clone(),
                actor: resolve_current_user(Some(root_path.as_path())),
                changes: vec![TaskChange {
                    field: "deleted".into(),
                    old: None,
                    new: Some(now.clone()),
                }],
            });
            task.deleted_at = Some(now);
            Ok(true)
        })?;

        Ok(TaskDeleteResponse {
            deleted: true,
            hard: false,
            warnings: Vec::new(),
        })
    }

    /// Warnings for a hard deletion: every managed attachment blob that
    /// stays behind (with its stored path) and every stored task — active or
    /// tombstoned — whose relationships point at the deleted task. Nothing
    /// is cleaned up; the caller merely reports the hazards.
    fn hard_delete_warnings(storage: &Storage, canonical: &str, task: &Task) -> Vec<String> {
        let mut warnings = Vec::new();

        let parsed = TaskId::parse(canonical).ok();
        let config_root = match storage.resolve_task_location(canonical) {
            Ok(location) => location.root.clone(),
            Err(_) => storage.root_path.clone(),
        };
        let attachments_root = parsed.as_ref().and_then(|parsed| {
            let config = Self::resolve_config_for_project(config_root.as_path(), &parsed.project);
            AttachmentService::compute_attachments_root(&config_root, &config).ok()
        });

        let mut seen_blobs = std::collections::BTreeSet::new();
        for name in task.references.iter().filter_map(|r| r.attachment.clone()) {
            if !seen_blobs.insert(name.clone()) {
                continue;
            }
            let location = match &attachments_root {
                Some(root) => root.join(&name).display().to_string(),
                None => name.clone(),
            };
            warnings.push(format!(
                "retained attachment '{name}' ({location}); blob preserved, no cleanup performed"
            ));
        }

        for (id, other) in storage.search(&crate::storage::TaskFilter {
            deletion: DeletionFilter::All,
            ..Default::default()
        }) {
            let mut fields: Vec<&str> = Vec::new();
            let rel = &other.relationships;
            if rel
                .depends_on
                .iter()
                .any(|v| entry_matches_task(v, canonical))
            {
                fields.push("depends_on");
            }
            if rel.blocks.iter().any(|v| entry_matches_task(v, canonical)) {
                fields.push("blocks");
            }
            if rel.related.iter().any(|v| entry_matches_task(v, canonical)) {
                fields.push("related");
            }
            if rel
                .parent
                .as_deref()
                .is_some_and(|v| entry_matches_task(v, canonical))
            {
                fields.push("parent");
            }
            if rel
                .children
                .iter()
                .any(|v| entry_matches_task(v, canonical))
            {
                fields.push("children");
            }
            if rel.fixes.iter().any(|v| entry_matches_task(v, canonical)) {
                fields.push("fixes");
            }
            if rel
                .duplicate_of
                .as_deref()
                .is_some_and(|v| entry_matches_task(v, canonical))
            {
                fields.push("duplicate_of");
            }
            if fields.is_empty() {
                continue;
            }
            let dependent = TaskId::parse(&id)
                .map(|parsed| parsed.canonical())
                .unwrap_or(id);
            warnings.push(format!(
                "task '{dependent}' references the deleted task via {}; left unchanged",
                fields.join(", ")
            ));
        }

        warnings
    }

    /// Restore a soft-deleted task (DEV-92): clears `deleted_at` and appends
    /// a `restored` history entry while leaving `modified` unchanged. The
    /// stored status/priority/type are validated against the project's
    /// CURRENT config (fail closed, no status mutation). Restoring an
    /// active task is a no-op that returns the current DTO without history.
    pub fn restore(storage: &mut Storage, id: &str, project: Option<&str>) -> LoTaRResult<TaskDTO> {
        let (canonical, derived) = Self::ensure_deletable(storage, id, project)?;

        let config = Self::restore_config(storage.root_path.as_path(), &derived)?;
        let root_path = storage.root_path.clone();
        let canonical_guard = canonical.clone();
        let outcome = storage.mutate_task(&canonical, move |task| {
            let Some(deleted_at) = task.deleted_at.clone() else {
                // Active task: idempotent no-op, no history entry.
                return Ok(false);
            };

            // Validate the stored fields against the CURRENT config before
            // clearing the tombstone; a config change that invalidated the
            // stored values blocks the restore without any mutation.
            Self::validate_stored_enums(task, &config, &canonical_guard)?;

            let now = chrono::Utc::now().to_rfc3339();
            task.history.push(TaskChangeLogEntry {
                at: now.clone(),
                actor: resolve_current_user(Some(root_path.as_path())),
                changes: vec![TaskChange {
                    field: "restored".into(),
                    old: Some(deleted_at),
                    new: None,
                }],
            });
            task.deleted_at = None;
            Ok(true)
        })?;

        Ok(Self::dto_from_task(storage, &canonical, outcome.after))
    }

    /// Restoration and its preview must not silently skip unreadable layers.
    pub fn restore_config(root: &Path, project: &str) -> LoTaRResult<ResolvedConfig> {
        use crate::config::{persistence, types::ConfigError};
        let validate_layer = |result: Result<(), ConfigError>| match result {
            Ok(()) | Err(ConfigError::FileNotFound(_)) => Ok(()),
            Err(error) => Err(LoTaRError::ValidationError(format!(
                "cannot restore: invalid current configuration: {error}"
            ))),
        };
        validate_layer(persistence::load_global_config(Some(root)).map(|_| ()))?;
        validate_layer(persistence::load_project_config_from_dir(project, root).map(|_| ()))?;
        if persistence::home_config_honored() {
            validate_layer(persistence::load_home_config().map(|_| ()))?;
        }
        crate::config::resolution::config_for_project(root, Some(project)).map_err(|error| {
            LoTaRError::ValidationError(format!(
                "cannot restore: invalid current configuration: {error}"
            ))
        })
    }

    /// Enum validation of already-stored values against a resolved config:
    /// canonicalizes nothing and mutates nothing, it only fails closed with
    /// a field-specific message.
    fn validate_stored_enums(task: &Task, config: &ResolvedConfig, id: &str) -> LoTaRResult<()> {
        if !task.status.is_empty() {
            validation::parse_status(task.status.as_str(), config).map_err(|err| {
                LoTaRError::ValidationError(format!(
                    "cannot restore '{id}': stored status is invalid under the current config: {err}"
                ))
            })?;
        }
        if !task.priority.is_empty() {
            validation::parse_priority(task.priority.as_str(), config).map_err(|err| {
                LoTaRError::ValidationError(format!(
                    "cannot restore '{id}': stored priority is invalid under the current config: {err}"
                ))
            })?;
        }
        if !task.task_type.is_empty() {
            validation::parse_task_type(task.task_type.as_str(), config).map_err(|err| {
                LoTaRError::ValidationError(format!(
                    "cannot restore '{id}': stored task type is invalid under the current config: {err}"
                ))
            })?;
        }
        Ok(())
    }

    pub fn list(storage: &Storage, filter: &TaskListFilter) -> Vec<(String, TaskDTO)> {
        // Map API filter to storage filter
        let storage_filter = crate::storage::TaskFilter {
            status: filter.status.clone(),
            priority: filter.priority.clone(),
            task_type: filter.task_type.clone(),
            project: filter.project.clone(),
            tags: filter.tags.clone(),
            text_query: filter.text_query.clone(),
            sprints: Vec::new(),
            custom_fields: filter.custom_fields.clone(),
            deletion: filter.deletion,
        };

        // DEV-21 review F1: cache keyed by (actual root, prefix). A
        // prefix-only (or primary-root) key misclassifies DTO policies when
        // two different IDs share one prefix across sibling roots.
        let mut config_cache: HashMap<(std::path::PathBuf, String), ResolvedConfig> =
            HashMap::new();

        let sprint_lookup = Self::load_sprint_lookup(storage);
        let requested_sprints: HashSet<u32> = filter.sprints.iter().copied().collect();
        // Documented filter semantics: case-, `@`-, and separator-insensitive
        // member matching (docs/openapi.json).
        let assignee_targets: Vec<String> = filter
            .assignee
            .iter()
            .map(|a| crate::utils::fuzzy_match::member_key(a))
            .collect();

        let mut results: Vec<(String, TaskDTO)> = storage
            .search(&storage_filter)
            .into_iter()
            .filter(|(id, _)| {
                if requested_sprints.is_empty() {
                    return true;
                }
                sprint_lookup
                    .get(id)
                    .map(|orders| {
                        orders
                            .keys()
                            .any(|sprint_id| requested_sprints.contains(sprint_id))
                    })
                    .unwrap_or(false)
            })
            .map(|(id, mut t)| {
                let project_prefix = TaskId::parse(&id)
                    .map(|parsed| parsed.project)
                    .unwrap_or_default();
                // Actual root per task ID (nested/sibling workspaces); the
                // primary root is only the fallback for unresolvable ids.
                let config_root = match storage.resolve_task_location(&id) {
                    Ok(location) => location.root,
                    Err(_) => storage.root_path.clone(),
                };
                let config = config_cache
                    .entry((config_root.clone(), project_prefix.clone()))
                    .or_insert_with(|| {
                        Self::resolve_config_for_project(config_root.as_path(), &project_prefix)
                    });
                Self::ensure_task_defaults(&mut t, config, false);
                (
                    id.clone(),
                    Self::to_dto(&id, t, Some(&sprint_lookup), config, chrono::Utc::now()),
                )
            })
            .collect();

        // Assignee filtering (shared by all frontends; @me resolved by caller)
        if filter.assignee_none {
            results.retain(|(_, t)| t.assignee.as_deref().unwrap_or("").trim().is_empty());
        } else if !assignee_targets.is_empty() {
            results.retain(|(_, t)| {
                t.assignee.as_deref().is_some_and(|a| {
                    let key = crate::utils::fuzzy_match::member_key(a);
                    assignee_targets.contains(&key)
                })
            });
        }

        results
    }

    /// Build a DTO from an in-memory task state exactly the way
    /// [`Self::get`] normalizes it (actual-root config defaults plus the
    /// sprint lookup), without re-reading the file. Used for the
    /// previous/current receipt pair of locked reference mutations so both
    /// DTOs describe one linearized mutation.
    pub(crate) fn dto_from_task(storage: &Storage, canonical_id: &str, task: Task) -> TaskDTO {
        let project = TaskId::parse(canonical_id)
            .map(|parsed| parsed.project)
            .unwrap_or_default();
        let config_root = match storage.resolve_task_location(canonical_id) {
            Ok(location) => location.root.clone(),
            Err(_) => storage.root_path.clone(),
        };
        let config = Self::resolve_config_for_project(config_root.as_path(), &project);
        let mut task = task;
        Self::ensure_task_defaults(&mut task, &config, false);
        let sprint_lookup = Self::load_sprint_lookup(storage);
        Self::to_dto(
            canonical_id,
            task,
            Some(&sprint_lookup),
            &config,
            chrono::Utc::now(),
        )
    }

    fn to_dto(
        id: &str,
        task: Task,
        sprint_lookup: Option<&HashMap<String, BTreeMap<u32, u32>>>,
        config: &ResolvedConfig,
        now: chrono::DateTime<chrono::Utc>,
    ) -> TaskDTO {
        let modified = if task.modified.is_empty() {
            task.created.clone()
        } else {
            task.modified.clone()
        };
        let sprint_order = sprint_lookup
            .and_then(|lookup| lookup.get(id))
            .cloned()
            .unwrap_or_default();
        let sprints: Vec<u32> = sprint_order.keys().copied().collect();
        let task_state = Some(crate::services::completion::compute_task_state(
            &task.status,
            task.due_date.as_deref(),
            config,
            now,
        ));
        TaskDTO {
            id: id.to_string(),
            title: task.title,
            status: task.status,
            task_state,
            deleted_at: task.deleted_at,
            priority: task.priority,
            task_type: task.task_type,
            reporter: task.reporter,
            assignee: task.assignee,
            created: task.created,
            modified,
            due_date: task.due_date,
            effort: task.effort,
            subtitle: task.subtitle,
            description: task.description,
            tags: task.tags,
            relationships: task.relationships,
            comments: task.comments,
            references: task.references,
            acceptance_criteria: task.acceptance_criteria,
            sprints,
            sprint_order,
            history: task.history,
            custom_fields: task.custom_fields,
        }
    }

    pub(crate) fn load_sprint_lookup(storage: &Storage) -> HashMap<String, BTreeMap<u32, u32>> {
        let mut map = Self::sprint_lookup_for_root(storage);

        // DEV-56: merge sprint orders from sibling workspace roots so
        // nested-root tasks resolve memberships from the root that actually
        // holds them. Primary-root entries win when both roots mention the
        // same task ID.
        let primary = storage
            .root_path
            .canonicalize()
            .unwrap_or_else(|_| storage.root_path.clone());
        for candidate in StorageLocator::candidate_task_roots(&storage.root_path) {
            if candidate == primary {
                continue;
            }
            let sibling = Storage {
                root_path: candidate,
            };
            for (task_id, orders) in Self::sprint_lookup_for_root(&sibling) {
                map.entry(task_id).or_insert(orders);
            }
        }

        map
    }

    fn sprint_lookup_for_root(storage: &Storage) -> HashMap<String, BTreeMap<u32, u32>> {
        let mut map: HashMap<String, BTreeMap<u32, u32>> = HashMap::new();
        let records = match SprintService::list(storage) {
            Ok(records) => records,
            Err(_) => return map,
        };

        for record in records {
            let sprint_id = record.id;
            let mut fallback_order = 1u32;
            for entry in record.sprint.tasks.iter() {
                let task_id = entry.id.trim();
                if task_id.is_empty() {
                    continue;
                }
                // Key canonically so legacy padded spellings (TP-001) are
                // found by canonical lookups; unparseable ids stay verbatim.
                let key = canonical_membership_key(task_id);
                let slot = map.entry(key).or_default();
                let order = entry.order.unwrap_or_else(|| {
                    let value = fallback_order;
                    fallback_order += 1;
                    value
                });
                slot.insert(sprint_id, order);
            }
        }

        map
    }

    fn format_sprint_change(values: &BTreeSet<u32>) -> Option<String> {
        if values.is_empty() {
            return None;
        }
        Some(
            values
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", "),
        )
    }

    pub(crate) fn normalize_sprint_ids(ids: &[u32]) -> Vec<u32> {
        let normalized: BTreeSet<u32> = ids.iter().copied().filter(|id| *id > 0).collect();
        normalized.into_iter().collect()
    }

    /// Plan the auto-populated member list without writing the project config.
    /// Returns the updated config (for validation and automation) plus the
    /// exact config bytes to stage inside the transaction, so a validation or
    /// publish failure can never leave a mutated config behind (DEV-55).
    fn plan_auto_populate_members(
        tasks_root: &Path,
        project: &str,
        task: &Task,
        mut config: ResolvedConfig,
    ) -> LoTaRResult<(ResolvedConfig, Option<String>)> {
        if project.trim().is_empty() || !config.auto_populate_members {
            return Ok((config, None));
        }

        let missing = Self::missing_members_for_task(task, &config);
        if missing.is_empty() {
            return Ok((config, None));
        }

        let plan = crate::config::operations::plan_auto_populated_project_config(
            tasks_root,
            project,
            &config.members,
            &missing,
        )
        .map_err(|err| {
            LoTaRError::ValidationError(format!(
                "Failed to auto-populate members for project '{}': {}",
                project, err
            ))
        })?;

        match plan {
            Some((project_config, effective)) => {
                let yaml = crate::config::normalization::to_canonical_project_yaml(&project_config);
                config.members = effective;
                Ok((config, Some(yaml)))
            }
            None => Ok((config, None)),
        }
    }

    pub(crate) fn apply_memberships_to_records(
        records: &mut [SprintRecord],
        task_id: &str,
        desired: &BTreeSet<u32>,
    ) -> LoTaRResult<HashSet<u32>> {
        let mut touched: HashSet<u32> = HashSet::new();
        let mut found: BTreeSet<u32> = BTreeSet::new();

        for record in records.iter_mut() {
            // Canonical matching: legacy padded spellings (TP-001) are the
            // same member as the canonical id (TP-1), so no duplicate
            // entries are created and removal drops every spelling.
            let contains = record
                .sprint
                .tasks
                .iter()
                .any(|entry| entry_matches_task(&entry.id, task_id));
            let should_have = desired.contains(&record.id);

            if contains && !should_have {
                record
                    .sprint
                    .tasks
                    .retain(|entry| !entry_matches_task(&entry.id, task_id));
                touched.insert(record.id);
            }

            if should_have {
                found.insert(record.id);
                if !contains {
                    record.sprint.tasks.push(SprintTaskEntry {
                        id: task_id.to_string(),
                        order: None,
                    });
                    touched.insert(record.id);
                }
            }
        }

        if let Some(missing) = desired.iter().find(|id| !found.contains(id)) {
            return Err(LoTaRError::SprintNotFound(*missing));
        }

        Ok(touched)
    }

    /// Stage the touched sprint records inside a coordinated transaction
    /// instead of writing them one by one (DEV-55).
    pub(crate) fn stage_sprint_records(
        txn: &mut crate::storage::transaction::MultiFileTransaction,
        tasks_root: &Path,
        records: &[SprintRecord],
        touched: &HashSet<u32>,
    ) -> LoTaRResult<()> {
        for record in records {
            if touched.contains(&record.id) {
                SprintService::stage_update(txn, tasks_root, record.id, record.sprint.clone())?;
            }
        }
        Ok(())
    }

    pub(crate) fn missing_members_for_task(task: &Task, config: &ResolvedConfig) -> Vec<String> {
        if !config.auto_populate_members {
            return Vec::new();
        }

        let mut candidates: Vec<String> = Vec::new();
        if let Some(reporter) = task.reporter.as_deref() {
            let trimmed = reporter.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('@') {
                candidates.push(trimmed.to_string());
            }
        }
        if let Some(assignee) = task.assignee.as_deref() {
            let trimmed = assignee.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('@') {
                candidates.push(trimmed.to_string());
            }
        }

        if candidates.is_empty() {
            return Vec::new();
        }

        let mut missing: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        for candidate in candidates {
            let lower = candidate.to_ascii_lowercase();
            if !seen.insert(lower.clone()) {
                continue;
            }
            let already_present = config
                .members
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(&candidate));
            if !already_present {
                missing.push(candidate);
            }
        }

        missing
    }

    pub(crate) fn resolve_config_for_project(
        tasks_root: &Path,
        project_prefix: &str,
    ) -> ResolvedConfig {
        crate::config::resolution::config_for_project(tasks_root, Some(project_prefix))
            .unwrap_or_else(|error| {
                let mut fallback = ResolvedConfig::from_global(GlobalConfig::default());
                crate::config::resolution::apply_cli_overrides(&mut fallback);
                // Read-only DTO paths retain legacy defaults on load errors,
                // but an invalid explicit completion policy must never become
                // the built-in Done policy. Config show/inspect report the
                // policy error; these projections conservatively finish none.
                if matches!(error, crate::config::types::ConfigError::PolicyError(_)) {
                    fallback.issue_done_states =
                        Some(crate::config::types::ConfigurableField { values: Vec::new() });
                }
                fallback
            })
    }

    fn ensure_task_defaults(task: &mut Task, config: &ResolvedConfig, apply_default_tags: bool) {
        if task.status.is_empty()
            && let Some(default_status) = config.effective_default_status()
        {
            task.status = default_status;
        }

        if task.priority.is_empty()
            && let Some(default_priority) = config.effective_default_priority()
        {
            task.priority = default_priority;
        }

        if task.task_type.is_empty()
            && let Some(mut default_type) = config.effective_default_task_type()
        {
            default_type.ensure_leading_uppercase();
            task.task_type = default_type;
        }
        // An explicit tags patch (including a clear) must not resurrect
        // configured default tags; only implicit emptiness does.
        if apply_default_tags && task.tags.is_empty() && !config.default_tags.is_empty() {
            task.tags = config.default_tags.clone();
        }
        // Normalize legacy @-prefixed member values for display consistency.
        let is_agent = |name: &str| config.agent_profiles.contains_key(name);
        if let Some(ref a) = task.assignee {
            let normalized = crate::utils::member::normalize_member_value(a, is_agent);
            if normalized != *a {
                task.assignee = Some(normalized);
            }
        }
        if let Some(ref r) = task.reporter {
            let normalized = crate::utils::member::normalize_member_value(r, is_agent);
            if normalized != *r {
                task.reporter = Some(normalized);
            }
        }
    }

    pub(crate) fn enforce_membership(
        task: &Task,
        config: &ResolvedConfig,
        project: &str,
    ) -> LoTaRResult<()> {
        if !config.strict_members {
            return Ok(());
        }

        let allowed: Vec<String> = config
            .members
            .iter()
            .map(|member| member.trim().to_string())
            .filter(|member| !member.is_empty())
            .collect();

        if allowed.is_empty() {
            return Err(LoTaRError::ValidationError(format!(
                "Strict members are enabled for project '{}' but no members are configured. Add entries under members or disable strict_members.",
                project
            )));
        }

        if let Some(reporter) = task.reporter.as_deref() {
            Self::enforce_member_value("Reporter", reporter, &allowed, project)?;
        }

        if let Some(assignee) = task.assignee.as_deref() {
            Self::enforce_member_value("Assignee", assignee, &allowed, project)?;
        }

        Ok(())
    }

    fn enforce_member_value(
        field_label: &str,
        value: &str,
        allowed: &[String],
        project: &str,
    ) -> LoTaRResult<()> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Ok(());
        }

        let norm_val = crate::utils::member::member_for_comparison(trimmed);
        let permitted = allowed
            .iter()
            .any(|candidate| crate::utils::member::member_for_comparison(candidate) == norm_val);

        if permitted {
            return Ok(());
        }

        let preview_count = allowed.len();
        let preview = if preview_count <= 10 {
            allowed.join(", ")
        } else {
            format!(
                "{} ... (+{} more)",
                allowed[..10].join(", "),
                preview_count - 10
            )
        };

        Err(LoTaRError::ValidationError(format!(
            "{} '{}' is not in configured members for project '{}'. Allowed members: {}.",
            field_label, trimmed, project, preview
        )))
    }
}

/// Canonical membership comparison: two sprint entry ids denote the same
/// member when their canonical parses agree. Unparseable ids compare
/// verbatim so malformed legacy entries are still matched exactly.
fn entry_matches_task(entry_id: &str, task_id: &str) -> bool {
    match (
        TaskId::parse(entry_id.trim()),
        TaskId::parse(task_id.trim()),
    ) {
        (Ok(entry), Ok(task)) => entry == task,
        _ => entry_id.trim() == task_id.trim(),
    }
}

/// Canonical map key for a sprint membership entry id.
fn canonical_membership_key(raw: &str) -> String {
    TaskId::parse(raw.trim())
        .map(|parsed| parsed.canonical())
        .unwrap_or_else(|_| raw.trim().to_string())
}

/// Canonical alias equality for ticket identifiers (TP-1 == TP-001);
/// unparseable values match nothing (fail closed).
fn ticket_alias_matches(a: &str, b: &str) -> bool {
    crate::storage::identity::aliases_match(a, b)
}

fn matches_review_state(status: &TaskStatus) -> bool {
    let value = status.as_str();
    value.eq_ignore_ascii_case("NeedsReview") || value.eq_ignore_ascii_case("Review")
}

#[cfg(test)]
mod ticket_alias_tests {
    use super::ticket_alias_matches;

    #[test]
    fn padded_aliases_match_their_canonical_ticket() {
        assert!(ticket_alias_matches("TP-1", "TP-1"));
        assert!(ticket_alias_matches("TP-001", "TP-1"));
        assert!(ticket_alias_matches("TP-1", "TP-001"));
        assert!(ticket_alias_matches("ABC-OPS-012", "ABC-OPS-12"));
    }

    #[test]
    fn different_tickets_and_malformed_values_never_match() {
        assert!(!ticket_alias_matches("TP-1", "TP-2"));
        assert!(!ticket_alias_matches("TP-1", "DEV-1"));
        assert!(!ticket_alias_matches("abc", "TP-1"));
        assert!(!ticket_alias_matches("", "TP-1"));
        assert!(!ticket_alias_matches("TP-+1", "TP-1"));
        assert!(!ticket_alias_matches("TP-1-extra", "TP-1"));
    }
}
