use crate::api_types::TaskSelection;
use crate::config::types::ResolvedConfig;
use crate::services::task_service::TaskService;
use crate::storage::manager::Storage;
use crate::workspace::TasksDirectoryResolver;
use std::collections::{BTreeMap, BTreeSet};

pub fn select_task_ids(
    storage: &Storage,
    selection: &TaskSelection,
    resolver: &TasksDirectoryResolver,
    config: &ResolvedConfig,
) -> Result<Vec<String>, String> {
    let mut tasks = TaskService::list(storage, &selection.filter);
    if selection.r#where.is_empty() {
        return Ok(tasks.into_iter().map(|(id, _)| id).collect());
    }

    let Some(filters) = build_filter_map(selection, resolver)? else {
        return Ok(Vec::new());
    };
    tasks.retain(|(id, task)| match_where_filters(id, task, &filters, config));

    Ok(tasks.into_iter().map(|(id, _)| id).collect())
}

fn build_filter_map(
    selection: &TaskSelection,
    resolver: &TasksDirectoryResolver,
) -> Result<Option<BTreeMap<String, BTreeSet<String>>>, String> {
    let mut filters: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (key, value) in &selection.r#where {
        if key.trim().is_empty() || value.trim().is_empty() {
            continue;
        }
        let mut candidate = value.clone();
        if key.eq_ignore_ascii_case("assignee") && value.trim() == "@me" {
            match crate::utils::identity::resolve_current_user(Some(resolver.path.as_path())) {
                Some(user) => candidate = user,
                None => return Ok(None),
            }
        }
        filters.entry(key.clone()).or_default().insert(candidate);
    }
    Ok(Some(filters))
}

fn match_where_filters(
    id: &str,
    task: &crate::api_types::TaskDTO,
    filters: &BTreeMap<String, BTreeSet<String>>,
    config: &ResolvedConfig,
) -> bool {
    for (key, allowed) in filters {
        let values = match resolve_values(id, task, key, config) {
            Some(v) => v.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>(),
            None => return false,
        };
        if values.is_empty() {
            return false;
        }
        let allow_vec: Vec<String> = allowed.iter().cloned().collect();
        if !crate::utils::fuzzy_match::fuzzy_set_match(&values, &allow_vec) {
            return false;
        }
    }

    true
}

fn resolve_values(
    id: &str,
    task: &crate::api_types::TaskDTO,
    key: &str,
    config: &ResolvedConfig,
) -> Option<Vec<String>> {
    let raw = key.trim();

    if let Some(canonical) = crate::utils::fields::is_reserved_field(raw) {
        return match canonical {
            "assignee" => Some(vec![task.assignee.clone().unwrap_or_default()]),
            "reporter" => Some(vec![task.reporter.clone().unwrap_or_default()]),
            "type" => Some(vec![task.task_type.to_string()]),
            "status" => Some(vec![task.status.to_string()]),
            "priority" => Some(vec![task.priority.to_string()]),
            "project" => Some(vec![id.split('-').next().unwrap_or("").to_string()]),
            "tags" => Some(task.tags.clone()),
            _ => None,
        };
    }

    if let Some(name) = crate::utils::custom_fields::resolve_filter_name(raw, config) {
        return crate::utils::custom_fields::extract_value_strings(&task.custom_fields, &name);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_types::TaskListFilter;
    use crate::config::types::{GlobalConfig, StringConfigField};
    use crate::storage::manager::Storage;
    use crate::storage::task::Task;
    use crate::types::{Priority, TaskStatus, custom_value_string};
    use crate::workspace::{TasksDirectoryResolver, TasksDirectorySource};
    use tempfile::TempDir;

    fn config_with_custom_field(name: &str) -> ResolvedConfig {
        let global = GlobalConfig {
            custom_fields: StringConfigField {
                values: vec![name.to_string()],
            },
            ..GlobalConfig::default()
        };
        ResolvedConfig::from_global(global)
    }

    #[test]
    fn select_task_ids_applies_custom_field_and_where_filters() {
        let temp = TempDir::new().expect("tempdir");
        let tasks_root = temp.path().join(".tasks");
        std::fs::create_dir_all(&tasks_root).expect("tasks dir");
        let mut storage = Storage::new(tasks_root.clone());
        let config = config_with_custom_field("iteration");
        let resolver = TasksDirectoryResolver {
            path: tasks_root.clone(),
            source: TasksDirectorySource::CommandLineFlag,
        };

        let mut manual = Task::new(
            tasks_root.clone(),
            "Manual".into(),
            Priority::from("Medium"),
        );
        manual.status = TaskStatus::from("Todo");
        manual.assignee = Some("alice".into());
        let manual_id = storage
            .add(&manual, "TEST", Some("Test Project"))
            .expect("add manual");

        let mut selected = Task::new(
            tasks_root.clone(),
            "Selected".into(),
            Priority::from("Medium"),
        );
        selected.status = TaskStatus::from("Todo");
        selected.assignee = Some("bob".into());
        selected
            .custom_fields
            .insert("iteration".into(), custom_value_string("beta"));
        let selected_id = storage
            .add(&selected, "TEST", Some("Test Project"))
            .expect("add selected");

        let mut filter = TaskListFilter {
            project: Some("TEST".into()),
            ..TaskListFilter::default()
        };
        filter
            .custom_fields
            .entry("iteration".into())
            .or_default()
            .push("beta".into());
        let selection = TaskSelection {
            filter,
            r#where: vec![("assignee".into(), "bob".into())],
        };

        let ids = select_task_ids(&storage, &selection, &resolver, &config).unwrap();
        assert_eq!(ids, vec![selected_id.clone()]);

        let mut unmatched = TaskListFilter {
            project: Some("TEST".into()),
            ..TaskListFilter::default()
        };
        unmatched
            .custom_fields
            .entry("iteration".into())
            .or_default()
            .push("beta".into());
        let selection_no_match = TaskSelection {
            filter: unmatched,
            r#where: vec![("assignee".into(), "nobody".into())],
        };
        let none = select_task_ids(&storage, &selection_no_match, &resolver, &config).unwrap();
        assert!(none.is_empty(), "unexpected ids: {:?}", none);

        // manual id is unused but ensures storage contains other tasks to guard regressions
        assert!(!manual_id.is_empty());
    }
}
