use crate::api_types::{TaskCreate, TaskDTO, TaskListFilter, TaskUpdate};
use crate::config::types::{GlobalConfig, ResolvedConfig};
use crate::errors::{LoTaRError, LoTaRResult};
use crate::services::sprint_service::{SprintRecord, SprintService};
use crate::storage::manager::Storage;
use crate::storage::sprint::SprintTaskEntry;
use crate::storage::task::Task;
use crate::types::{Priority, TaskChange, TaskChangeLogEntry, TaskStatus, TaskType};
use crate::utils::identity::{resolve_current_user, resolve_me_alias};
use crate::utils::project::generate_project_prefix;
use crate::utils::tags::normalize_tags;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

pub struct TaskService;

impl TaskService {
    pub fn create(storage: &mut Storage, req: TaskCreate) -> LoTaRResult<TaskDTO> {
        let TaskCreate {
            title,
            project,
            priority,
            task_type,
            reporter,
            assignee,
            due_date,
            effort,
            description,
            tags,
            relationships,
            custom_fields,
            sprints,
        } = req;
        let normalized_sprints = Self::normalize_sprint_ids(&sprints);

        // Prefer explicit project if provided; otherwise, derive from repo folder name
        let project = project.unwrap_or_else(|| {
            // If tasks dir looks like /path/to/repo/.tasks, use repo folder name
            let repo_name = storage
                .root_path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
                // Fallback to detected project name
                .or_else(crate::project::get_project_name)
                .unwrap_or_else(|| "default".to_string());
            generate_project_prefix(&repo_name)
        });

        let mut config = Self::resolve_config_for_project(storage.root_path.as_path(), &project);

        let resolved_priority = priority
            .or_else(|| crate::utils::task_intel::infer_priority_from_branch(&config))
            .or_else(|| config.effective_default_priority())
            .unwrap_or_else(|| Priority::from("Medium"));

        let mut resolved_type = task_type
            .clone()
            .or_else(|| crate::utils::task_intel::infer_task_type_from_branch(&config))
            .or_else(|| config.effective_default_task_type())
            .unwrap_or_else(|| TaskType::from("Feature"));

        if task_type.is_none() {
            resolved_type.ensure_leading_uppercase();
        }

        let mut t = Task::new(storage.root_path.clone(), title, resolved_priority.clone());
        t.priority = resolved_priority;
        t.task_type = resolved_type;
        // reporter: explicit or auto-detect (configurable)
        t.status = crate::utils::task_intel::infer_status_from_branch(&config)
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
        t.due_date = due_date;
        // Normalize effort on write
        t.effort = effort.map(|e| match crate::utils::effort::parse_effort(&e) {
            Ok(parsed) => parsed.canonical,
            Err(_) => e,
        });
        t.description = description;
        let mut normalized_tags = normalize_tags(tags);
        if normalized_tags.is_empty() {
            if !config.default_tags.is_empty() {
                normalized_tags.extend(config.default_tags.clone());
            }
            if let Some(label) = crate::utils::task_intel::auto_tag_from_path(&config) {
                if !normalized_tags
                    .iter()
                    .any(|existing| existing.eq_ignore_ascii_case(&label))
                {
                    normalized_tags.push(label);
                }
            }
        }
        t.tags = normalize_tags(normalized_tags);
        if let Some(rel) = relationships {
            t.relationships = rel;
        }
        if let Some(cf) = custom_fields {
            t.custom_fields = cf;
        }

        Self::ensure_task_defaults(&mut t, &config);
        config =
            Self::maybe_auto_populate_members(storage.root_path.as_path(), &project, &t, config)?;
        Self::enforce_membership(&t, &config, &project)?;

        let id = storage.add(&t, &project, None)?;
        if !normalized_sprints.is_empty() {
            Self::replace_sprint_memberships(storage, &id, &normalized_sprints)?;
        }
        let sprint_lookup = Self::load_sprint_lookup(storage);
        Ok(Self::to_dto(&id, t, Some(&sprint_lookup)))
    }

    pub fn get(storage: &Storage, id: &str, project: Option<&str>) -> LoTaRResult<TaskDTO> {
        // If project not provided, derive prefix from ID (e.g., ABCD-1 -> ABCD)
        let derived = id.split('-').next().unwrap_or("");
        let p = project.unwrap_or(derived).to_string();
        match storage.get(id, p.clone()) {
            Some(mut t) => {
                let config = Self::resolve_config_for_project(storage.root_path.as_path(), &p);
                Self::ensure_task_defaults(&mut t, &config);
                let sprint_lookup = Self::load_sprint_lookup(storage);
                Ok(Self::to_dto(id, t, Some(&sprint_lookup)))
            }
            None => Err(LoTaRError::TaskNotFound(id.to_string())),
        }
    }

    pub fn update(storage: &mut Storage, id: &str, patch: TaskUpdate) -> LoTaRResult<TaskDTO> {
        // Derive project prefix from ID (e.g., ABCD-1 -> ABCD) to locate the task
        let derived = id.split('-').next().unwrap_or("");
        let existing = storage
            .get(id, derived.to_string())
            .ok_or_else(|| LoTaRError::TaskNotFound(id.to_string()))?;
        let mut t = existing.clone();
        let mut config = Self::resolve_config_for_project(storage.root_path.as_path(), derived);
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
            let previous = t.title.clone();
            if previous != v {
                record_change("title", Some(previous), Some(v.clone()));
                t.title = v;
            }
        }
        if let Some(v) = patch.status {
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
            if auto && is_first_change {
                if let Some(me) = resolve_current_user(Some(&storage.root_path)) {
                    let previous_assignee = t.assignee.clone();
                    if previous_assignee.as_deref() != Some(me.as_str()) {
                        record_change("assignee", previous_assignee, Some(me.clone()));
                    }
                    t.assignee = Some(me);
                }
            }
        }
        if let Some(v) = patch.priority {
            let previous = t.priority.clone();
            if previous != v {
                record_change("priority", Some(previous.to_string()), Some(v.to_string()));
                t.priority = v;
            }
        }
        if let Some(v) = patch.task_type {
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
                resolve_me_alias(trimmed, Some(&storage.root_path))
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
                resolve_me_alias(trimmed, Some(&storage.root_path))
            };
            let previous = t.assignee.clone();
            if previous != new_value {
                record_change("assignee", previous, new_value.clone());
                t.assignee = new_value;
            }
        }
        if let Some(v) = patch.due_date {
            let new_value = Some(v.clone());
            let previous = t.due_date.clone();
            if previous != new_value {
                record_change("due_date", previous, new_value.clone());
                t.due_date = new_value;
            }
        }
        if let Some(v) = patch.effort {
            let parsed = match crate::utils::effort::parse_effort(&v) {
                Ok(parsed) => Some(parsed.canonical),
                Err(_) => Some(v),
            };
            let previous = t.effort.clone();
            if previous != parsed {
                record_change("effort", previous, parsed.clone());
                t.effort = parsed;
            }
        }
        if let Some(v) = patch.description {
            let new_value = Some(v.clone());
            let previous = t.description.clone();
            if previous != new_value {
                record_change("description", previous, new_value.clone());
                t.description = new_value;
            }
        }
        if let Some(v) = patch.tags {
            let new_tags = normalize_tags(v);
            let previous = t.tags.clone();
            if previous != new_tags {
                record_change("tags", Some(previous.join(", ")), Some(new_tags.join(", ")));
                t.tags = new_tags;
            }
        }
        if let Some(v) = patch.relationships {
            if t.relationships != v {
                let old_json = serde_json::to_string(&t.relationships).ok();
                let new_json = serde_json::to_string(&v).ok();
                record_change("relationships", old_json, new_json.clone());
                t.relationships = v;
            }
        }
        if let Some(v) = patch.custom_fields {
            if t.custom_fields != v {
                let old_yaml = serde_yaml::to_string(&t.custom_fields).ok();
                let new_yaml = serde_yaml::to_string(&v).ok();
                record_change("custom_fields", old_yaml, new_yaml.clone());
                t.custom_fields = v;
            }
        }

        if let Some(sprint_ids) = patch.sprints.clone() {
            let normalized = Self::normalize_sprint_ids(&sprint_ids);
            let desired_set: BTreeSet<u32> = normalized.iter().copied().collect();
            let current_lookup = Self::load_sprint_lookup(storage);
            let current_orders = current_lookup.get(id).cloned().unwrap_or_default();
            let current_set: BTreeSet<u32> = current_orders.keys().copied().collect();

            if current_set != desired_set {
                Self::replace_sprint_memberships(storage, id, &normalized)?;
                let old_display = Self::format_sprint_change(&current_set);
                let new_display = Self::format_sprint_change(&desired_set);
                record_change("sprints", old_display, new_display);
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

        Self::ensure_task_defaults(&mut t, &config);
        config =
            Self::maybe_auto_populate_members(storage.root_path.as_path(), derived, &t, config)?;
        Self::enforce_membership(&t, &config, derived)?;

        t.sprints.clear();

        storage.edit(id, &t)?;

        let sprint_lookup = Self::load_sprint_lookup(storage);
        Ok(Self::to_dto(id, t, Some(&sprint_lookup)))
    }

    pub fn delete(storage: &mut Storage, id: &str, project: Option<&str>) -> LoTaRResult<bool> {
        let derived = id.split('-').next().unwrap_or("");
        let p = project.unwrap_or(derived).to_string();
        storage.delete(id, p)
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
        };

        let mut config_cache: HashMap<String, ResolvedConfig> = HashMap::new();

        let sprint_lookup = Self::load_sprint_lookup(storage);
        let requested_sprints: HashSet<u32> = filter.sprints.iter().copied().collect();

        storage
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
                let project_prefix = id.split('-').next().unwrap_or("").to_string();
                let config = config_cache
                    .entry(project_prefix.clone())
                    .or_insert_with(|| {
                        Self::resolve_config_for_project(
                            storage.root_path.as_path(),
                            &project_prefix,
                        )
                    });
                Self::ensure_task_defaults(&mut t, config);
                (id.clone(), Self::to_dto(&id, t, Some(&sprint_lookup)))
            })
            .collect()
    }

    fn to_dto(
        id: &str,
        task: Task,
        sprint_lookup: Option<&HashMap<String, BTreeMap<u32, u32>>>,
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
        TaskDTO {
            id: id.to_string(),
            title: task.title,
            status: task.status,
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
            sprints,
            sprint_order,
            history: task.history,
            custom_fields: task.custom_fields,
        }
    }

    pub(crate) fn load_sprint_lookup(storage: &Storage) -> HashMap<String, BTreeMap<u32, u32>> {
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
                let slot = map.entry(task_id.to_string()).or_default();
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

    fn maybe_auto_populate_members(
        tasks_root: &Path,
        project: &str,
        task: &Task,
        mut config: ResolvedConfig,
    ) -> LoTaRResult<ResolvedConfig> {
        if project.trim().is_empty() || !config.auto_populate_members {
            return Ok(config);
        }

        let missing = Self::missing_members_for_task(task, &config);
        if missing.is_empty() {
            return Ok(config);
        }

        match crate::config::operations::auto_populate_project_members(
            tasks_root,
            project,
            &config.members,
            &missing,
        ) {
            Ok(Some(updated)) => {
                config.members = updated;
                Ok(config)
            }
            Ok(None) => Ok(config),
            Err(err) => Err(LoTaRError::ValidationError(format!(
                "Failed to auto-populate members for project '{}': {}",
                project, err
            ))),
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
            let contains = record.sprint.tasks.iter().any(|entry| entry.id == task_id);
            let should_have = desired.contains(&record.id);

            if contains && !should_have {
                record.sprint.tasks.retain(|entry| entry.id != task_id);
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

    pub(crate) fn persist_sprint_records(
        storage: &mut Storage,
        records: &[SprintRecord],
        touched: &HashSet<u32>,
    ) -> LoTaRResult<()> {
        for record in records {
            if touched.contains(&record.id) {
                SprintService::update(storage, record.id, record.sprint.clone())?;
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
            if !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("@me") {
                candidates.push(trimmed.to_string());
            }
        }
        if let Some(assignee) = task.assignee.as_deref() {
            let trimmed = assignee.trim();
            if !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("@me") {
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

    pub(crate) fn replace_sprint_memberships(
        storage: &mut Storage,
        task_id: &str,
        desired: &[u32],
    ) -> LoTaRResult<()> {
        let normalized = Self::normalize_sprint_ids(desired);
        let desired_set: BTreeSet<u32> = normalized.iter().copied().collect();
        let mut records = SprintService::list(storage)?;
        let touched =
            Self::apply_memberships_to_records(records.as_mut_slice(), task_id, &desired_set)?;
        if touched.is_empty() {
            return Ok(());
        }
        Self::persist_sprint_records(storage, &records, &touched)
    }
    fn resolve_config_for_project(tasks_root: &Path, project_prefix: &str) -> ResolvedConfig {
        let base = crate::config::resolution::load_and_merge_configs(Some(tasks_root))
            .unwrap_or_else(|_| ResolvedConfig::from_global(GlobalConfig::default()));

        if project_prefix.trim().is_empty() {
            return base;
        }

        crate::config::resolution::get_project_config(&base, project_prefix, tasks_root)
            .unwrap_or(base)
    }

    fn ensure_task_defaults(task: &mut Task, config: &ResolvedConfig) {
        if task.status.is_empty() {
            if let Some(default_status) = config.effective_default_status() {
                task.status = default_status;
            }
        }

        if task.priority.is_empty() {
            if let Some(default_priority) = config.effective_default_priority() {
                task.priority = default_priority;
            }
        }

        if task.task_type.is_empty() {
            if let Some(mut default_type) = config.effective_default_task_type() {
                default_type.ensure_leading_uppercase();
                task.task_type = default_type;
            }
        }
        if task.tags.is_empty() && !config.default_tags.is_empty() {
            task.tags = config.default_tags.clone();
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

        let normalized = trimmed.to_ascii_lowercase();
        let permitted = allowed
            .iter()
            .any(|candidate| candidate.to_ascii_lowercase() == normalized);

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
