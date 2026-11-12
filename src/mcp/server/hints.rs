use crate::config::manager::ConfigManager;
use crate::config::types::{CustomFieldType, ResolvedConfig};
use crate::workspace::TasksDirectoryResolver;
use serde_json::{Map as JsonMap, Value, json};
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone)]
pub(super) struct EnumHints {
    pub(super) statuses: Vec<String>,
    pub(super) priorities: Vec<String>,
    pub(super) types: Vec<String>,
    pub(super) projects: Vec<String>,
    pub(super) members: Vec<String>,
    pub(super) tags: Vec<String>,
    pub(super) custom_fields: Vec<String>,
    /// Field name to type for custom fields that hold more than text
    /// (DEV-395), so agents learn a field's type before a write is refused.
    pub(super) custom_field_types: BTreeMap<String, String>,
}

pub(super) fn enum_hints_to_value(hints: &EnumHints) -> Value {
    json!({
        "projects": hints.projects,
        "statuses": hints.statuses,
        "priorities": hints.priorities,
        "types": hints.types,
        "members": hints.members,
        "tags": hints.tags,
        "custom_fields": hints.custom_fields,
        "custom_field_types": hints.custom_field_types,
    })
}

impl EnumHints {
    pub(super) fn from_resolved_config(cfg: &ResolvedConfig, projects: &[String]) -> Option<Self> {
        let statuses = collect_labels(&cfg.issue_states.values, |status| status.as_str());
        let priorities = collect_labels(&cfg.issue_priorities.values, |priority| priority.as_str());
        let types = collect_labels(&cfg.issue_types.values, |ty| ty.as_str());
        let members = collect_labels(&cfg.members, |member| member.as_str());
        let tags = collect_labels(&cfg.tags.values, |tag| tag.as_str());
        let custom_fields = collect_labels(&cfg.custom_fields.values, |field| field.as_str());
        let custom_field_types = custom_fields
            .iter()
            .map(|field| (field, cfg.custom_field_type(field)))
            .filter(|(_, kind)| *kind != CustomFieldType::Text)
            .map(|(field, kind)| (field.clone(), kind.as_str().to_string()))
            .collect();
        let projects = collect_labels(projects, |name| name.as_str());
        if statuses.is_empty()
            && priorities.is_empty()
            && types.is_empty()
            && members.is_empty()
            && tags.is_empty()
            && custom_fields.is_empty()
            && projects.is_empty()
        {
            None
        } else {
            Some(Self { statuses, priorities, types, projects, members, tags, custom_fields, custom_field_types })
        }
    }
}

pub(super) fn gather_enum_hints() -> Option<EnumHints> {
    let resolver = TasksDirectoryResolver::resolve(None, None).ok()?;
    let manager = ConfigManager::new_manager_with_tasks_dir_readonly(&resolver.path).ok()?;
    let project_dirs = crate::utils::filesystem::list_visible_subdirs(&resolver.path);

    if project_dirs.len() > 1 {
        return None;
    }

    let mut resolved = manager.get_resolved_config().clone();
    let mut project_names = Vec::new();
    if project_dirs.len() == 1 {
        project_names.push(project_dirs[0].0.clone());
        if let Ok(project_config) = manager.get_project_config(&project_dirs[0].0) {
            resolved = project_config;
        }
    }

    EnumHints::from_resolved_config(&resolved, &project_names)
}

pub(super) fn insert_field_hint(target: &mut JsonMap<String, Value>, field: &str, values: Option<&[String]>, accepts_multiple: bool) {
    if let Some(values) = values {
        if values.is_empty() {
            return;
        }
        let mut hint = JsonMap::new();
        hint.insert("values".into(), Value::Array(values.iter().map(|value| Value::String(value.clone())).collect()));
        if accepts_multiple {
            hint.insert("acceptsMultiple".into(), Value::Bool(true));
        }
        target.insert(field.to_string(), Value::Object(hint));
    }
}

/// Adds the custom field types to the hint of a custom fields argument.
pub(super) fn insert_custom_field_types(target: &mut JsonMap<String, Value>, field: &str, hints: Option<&EnumHints>) {
    let Some(hints) = hints else { return };
    if hints.custom_field_types.is_empty() {
        return;
    }
    if let Some(Value::Object(hint)) = target.get_mut(field) {
        hint.insert("types".into(), json!(hints.custom_field_types));
    }
}

pub(super) fn attach_field_hints(tool: &mut Value, field_hints: JsonMap<String, Value>) {
    if field_hints.is_empty() {
        return;
    }

    if let Value::Object(tool_obj) = tool {
        let mut hints = JsonMap::new();
        hints.insert("fields".into(), Value::Object(field_hints));
        tool_obj.insert("hints".into(), Value::Object(hints));
    }
}

pub(super) fn make_enum_error_data(field: &str, provided: &str, allowed: &[String]) -> Option<Value> {
    if allowed.is_empty() {
        return None;
    }
    Some(json!({
        "field": field,
        "code": "invalid_enum_value",
        "provided": provided,
        "suggestions": allowed,
    }))
}

fn collect_labels<T, F>(values: &[T], mut accessor: F) -> Vec<String>
where
    F: FnMut(&T) -> &str,
{
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for value in values {
        let label = accessor(value).trim();
        if label.is_empty() || label == "*" {
            continue;
        }
        let key = label.to_ascii_lowercase();
        if seen.insert(key) {
            result.push(label.to_string());
        }
    }
    result
}
