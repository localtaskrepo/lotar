use crate::api_types::{TaskCreate, TaskDTO, TaskListFilter, TaskUpdate};
use crate::errors::{LoTaRError, LoTaRResult};
use crate::storage::{manager::Storage, task::Task};
use crate::utils;
use crate::utils::identity::{resolve_current_user, resolve_me_alias};

pub struct TaskService;

impl TaskService {
    pub fn create(storage: &mut Storage, req: TaskCreate) -> LoTaRResult<TaskDTO> {
        // Prefer explicit project if provided; otherwise, derive from repo folder name
        let project = req.project.clone().unwrap_or_else(|| {
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
            utils::generate_project_prefix(&repo_name)
        });

        let priority = req.priority.unwrap_or_default();
        let mut t = Task::new(storage.root_path.clone(), req.title, priority);
        if let Some(tt) = req.task_type {
            t.task_type = tt;
        }
        // reporter: explicit or auto-detect (configurable)
        let config = crate::config::resolution::load_and_merge_configs(Some(&storage.root_path));
        let auto = config.as_ref().map(|c| c.auto_set_reporter).unwrap_or(true);
        t.reporter = if let Some(rep) = req.reporter {
            // Support @me alias resolution for reporter if provided
            resolve_me_alias(&rep, Some(&storage.root_path))
        } else if auto {
            // Prefer configured default_reporter, then fall back
            if let Ok(cfg) = &config {
                if let Some(rep) = cfg
                    .default_reporter
                    .clone()
                    .filter(|s| !s.trim().is_empty())
                {
                    Some(rep)
                } else {
                    resolve_current_user(Some(&storage.root_path))
                }
            } else {
                resolve_current_user(Some(&storage.root_path))
            }
        } else {
            None
        };
        // Normalize assignee with @me alias if provided
        t.assignee = req
            .assignee
            .and_then(|a| resolve_me_alias(&a, Some(&storage.root_path)));
        t.due_date = req.due_date;
        t.effort = req.effort;
        t.description = req.description;
        t.category = req.category;
        t.tags = req.tags;
        if let Some(cf) = req.custom_fields {
            t.custom_fields = cf;
        }

        let id = storage.add(&t, &project, None);
        Ok(Self::to_dto(&id, t))
    }

    pub fn get(storage: &Storage, id: &str, project: Option<&str>) -> LoTaRResult<TaskDTO> {
        // If project not provided, derive prefix from ID (e.g., ABCD-1 -> ABCD)
        let derived = id.split('-').next().unwrap_or("");
        let p = project.unwrap_or(derived).to_string();
        match storage.get(id, p) {
            Some(t) => Ok(Self::to_dto(id, t)),
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
        if let Some(v) = patch.title {
            t.title = v;
        }
        if let Some(v) = patch.status {
            // First-change semantics: only auto-assign if this is the first time moving
            // away from the initially created status value.
            let old_status = t.status.clone();
            let new_status = v;
            t.status = new_status.clone();
            let auto = crate::config::resolution::load_and_merge_configs(Some(&storage.root_path))
                .map(|c| c.auto_assign_on_status)
                .unwrap_or(true);
            let is_first_change = old_status != new_status && t.assignee.is_none();
            if auto && is_first_change {
                if let Some(me) = resolve_current_user(Some(&storage.root_path)) {
                    t.assignee = Some(me);
                }
            }
        }
        if let Some(v) = patch.priority {
            t.priority = v;
        }
        if let Some(v) = patch.task_type {
            t.task_type = v;
        }
        if let Some(v) = patch.reporter {
            t.reporter = resolve_me_alias(&v, Some(&storage.root_path));
        }
        if let Some(v) = patch.assignee {
            t.assignee = resolve_me_alias(&v, Some(&storage.root_path));
        }
        if let Some(v) = patch.due_date {
            t.due_date = Some(v);
        }
        if let Some(v) = patch.effort {
            t.effort = Some(v);
        }
        if let Some(v) = patch.description {
            t.description = Some(v);
        }
        if let Some(v) = patch.category {
            t.category = Some(v);
        }
        if let Some(v) = patch.tags {
            t.tags = v;
        }
        if let Some(v) = patch.custom_fields {
            t.custom_fields = v;
        }
        t.modified = chrono::Utc::now().to_rfc3339();

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

        storage
            .search(&storage_filter)
            .into_iter()
            .map(|(id, t)| (id.clone(), Self::to_dto(&id, t)))
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
            custom_fields: task.custom_fields,
        }
    }
}
