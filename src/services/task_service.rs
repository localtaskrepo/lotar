use crate::api_types::{TaskCreate, TaskDTO, TaskListFilter, TaskUpdate};
use crate::errors::{LoTaRError, LoTaRResult};
use crate::storage::{manager::Storage, task::Task};
use crate::utils;

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
        t.assignee = req.assignee;
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
            t.status = v;
        }
        if let Some(v) = patch.priority {
            t.priority = v;
        }
        if let Some(v) = patch.task_type {
            t.task_type = v;
        }
        if let Some(v) = patch.assignee {
            t.assignee = Some(v);
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
