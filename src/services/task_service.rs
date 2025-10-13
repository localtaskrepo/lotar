use crate::api_types::{TaskCreate, TaskDTO, TaskListFilter, TaskUpdate};
use crate::errors::{LoTaRError, LoTaRResult};
use crate::storage::{manager::Storage, task::Task};
use crate::utils::project::generate_project_prefix;
use crate::utils::tags::normalize_tags;
// ...existing code...
use crate::config::types::{GlobalConfig, ResolvedConfig};
use crate::types::{Priority, TaskChange, TaskChangeLogEntry, TaskStatus, TaskType};
use crate::utils::identity::{resolve_current_user, resolve_me_alias};
use std::collections::HashMap;
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
            category,
            tags,
            relationships,
            custom_fields,
        } = req;

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

        let config = Self::resolve_config_for_project(storage.root_path.as_path(), &project);

        let resolved_priority = priority
            .or_else(|| config.effective_default_priority())
            .unwrap_or_else(|| Priority::from("Medium"));

        let resolved_type = task_type
            .or_else(|| config.effective_default_task_type())
            .unwrap_or_else(|| TaskType::from("Feature"));

        let mut t = Task::new(storage.root_path.clone(), title, resolved_priority.clone());
        t.priority = resolved_priority;
        t.task_type = resolved_type;
        // reporter: explicit or auto-detect (configurable)
        t.status = config
            .effective_default_status()
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
            if let Some(rep) = config
                .default_reporter
                .clone()
                .filter(|s| !s.trim().is_empty())
            {
                Some(rep)
            } else {
                resolve_current_user(Some(&storage.root_path))
            }
        } else {
            None
        };
        // Normalize assignee with @me alias if provided
        t.assignee = assignee.as_ref().and_then(|a| {
            let trimmed = a.trim();
            if trimmed.is_empty() {
                None
            } else {
                resolve_me_alias(trimmed, Some(&storage.root_path))
            }
        });
        t.due_date = due_date;
        // Normalize effort on write
        t.effort = effort.map(|e| match crate::utils::effort::parse_effort(&e) {
            Ok(parsed) => parsed.canonical,
            Err(_) => e,
        });
        t.description = description;
        t.category = category;
        t.tags = normalize_tags(tags);
        if let Some(rel) = relationships {
            t.relationships = rel;
        }
        if let Some(cf) = custom_fields {
            t.custom_fields = cf;
        }

        Self::ensure_task_defaults(&mut t, &config);

        let history_actor = resolve_current_user(Some(&storage.root_path));
        t.history.push(TaskChangeLogEntry {
            at: chrono::Utc::now().to_rfc3339(),
            actor: history_actor.clone(),
            changes: vec![TaskChange {
                field: "created".into(),
                old: None,
                new: Some(t.title.clone()),
            }],
        });

        let id = storage.add(&t, &project, None);
        Ok(Self::to_dto(&id, t))
    }

    pub fn get(storage: &Storage, id: &str, project: Option<&str>) -> LoTaRResult<TaskDTO> {
        // If project not provided, derive prefix from ID (e.g., ABCD-1 -> ABCD)
        let derived = id.split('-').next().unwrap_or("");
        let p = project.unwrap_or(derived).to_string();
        match storage.get(id, p.clone()) {
            Some(mut t) => {
                let config = Self::resolve_config_for_project(storage.root_path.as_path(), &p);
                Self::ensure_task_defaults(&mut t, &config);
                Ok(Self::to_dto(id, t))
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
        let config = Self::resolve_config_for_project(storage.root_path.as_path(), derived);
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
        if let Some(v) = patch.category {
            let new_value = Some(v.clone());
            let previous = t.category.clone();
            if previous != new_value {
                record_change("category", previous, new_value.clone());
                t.category = new_value;
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

        storage.edit(id, &t);
        Ok(Self::to_dto(id, t))
    }

    pub fn delete(storage: &mut Storage, id: &str, project: Option<&str>) -> LoTaRResult<bool> {
        let derived = id.split('-').next().unwrap_or("");
        let p = project.unwrap_or(derived).to_string();
        Ok(storage.delete(id, p))
    }

    pub fn list(storage: &Storage, filter: &TaskListFilter) -> Vec<(String, TaskDTO)> {
        // Map API filter to storage filter
        let storage_filter = crate::storage::TaskFilter {
            status: filter.status.clone(),
            priority: filter.priority.clone(),
            task_type: filter.task_type.clone(),
            project: filter.project.clone(),
            category: filter.category.clone(),
            tags: filter.tags.clone(),
            text_query: filter.text_query.clone(),
        };

        let mut config_cache: HashMap<String, ResolvedConfig> = HashMap::new();

        storage
            .search(&storage_filter)
            .into_iter()
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
                (id.clone(), Self::to_dto(&id, t))
            })
            .collect()
    }

    fn to_dto(id: &str, task: Task) -> TaskDTO {
        TaskDTO {
            id: id.to_string(),
            title: task.title,
            status: task.status,
            priority: task.priority,
            task_type: task.task_type,
            reporter: task.reporter,
            assignee: task.assignee,
            created: task.created,
            modified: task.modified,
            due_date: task.due_date,
            effort: task.effort,
            subtitle: task.subtitle,
            description: task.description,
            category: task.category,
            tags: task.tags,
            relationships: task.relationships,
            comments: task.comments,
            references: task.references,
            history: task.history,
            custom_fields: task.custom_fields,
        }
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
            if let Some(default_type) = config.effective_default_task_type() {
                task.task_type = default_type;
            }
        }

        if task.category.is_none() {
            if let Some(default_category) = config.default_category.clone() {
                let trimmed = default_category.trim();
                if !trimmed.is_empty() {
                    task.category = Some(trimmed.to_string());
                }
            }
        }

        if task.tags.is_empty() && !config.default_tags.is_empty() {
            task.tags = config.default_tags.clone();
        }
    }
}
