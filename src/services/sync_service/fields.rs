//! Field values shared by both sync directions, plus custom field and task
//! update helpers.

use super::*;

#[derive(Clone, Debug)]
pub(super) enum FieldValue {
    String(String),
    List(Vec<String>),
}

impl FieldValue {
    pub(super) fn is_empty(&self) -> bool {
        match self {
            FieldValue::String(value) => value.trim().is_empty(),
            FieldValue::List(values) => values.iter().all(|v| v.trim().is_empty()),
        }
    }

    pub(super) fn to_string_value(&self) -> Option<String> {
        match self {
            FieldValue::String(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            FieldValue::List(values) => values
                .iter()
                .find(|v| !v.trim().is_empty())
                .map(|v| v.trim().to_string()),
        }
    }

    pub(super) fn to_list_value(&self) -> Vec<String> {
        match self {
            FieldValue::String(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    Vec::new()
                } else {
                    vec![trimmed.to_string()]
                }
            }
            FieldValue::List(values) => values
                .iter()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .collect(),
        }
    }
}

pub(super) fn normalize_field_values(value: &FieldValue) -> Vec<String> {
    let mut values = value
        .to_list_value()
        .into_iter()
        .map(|item| item.trim().to_ascii_lowercase())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

pub(super) fn field_values_match(a: &FieldValue, b: &FieldValue) -> bool {
    normalize_field_values(a) == normalize_field_values(b)
}

#[derive(Default)]
pub(super) struct LocalFieldValues {
    pub(super) title: Option<String>,
    pub(super) description: Option<String>,
    pub(super) status: Option<TaskStatus>,
    pub(super) task_type: Option<TaskType>,
    pub(super) priority: Option<Priority>,
    pub(super) assignee: Option<String>,
    pub(super) reporter: Option<String>,
    pub(super) tags: Option<Vec<String>>,
    pub(super) custom_fields: Option<CustomFields>,
}

pub(super) fn dedupe_list(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = trimmed.to_ascii_lowercase();
        if seen.insert(key) {
            out.push(trimmed.to_string());
        }
    }
    out
}

pub(super) fn merge_custom_fields(
    existing: Option<&CustomFields>,
    updates: CustomFields,
) -> CustomFields {
    let mut merged = existing.cloned().unwrap_or_default();
    for (key, value) in updates {
        if custom_field_value_is_null(&value) {
            merged.remove(&key);
        } else {
            merged.insert(key, value);
        }
    }
    merged
}

pub(super) fn custom_field_value_is_null(value: &CustomFieldValue) -> bool {
    #[cfg(feature = "schema")]
    {
        matches!(value, serde_json::Value::Null)
    }
    #[cfg(not(feature = "schema"))]
    {
        matches!(value, serde_yaml_ng::Value::Null)
    }
}

pub(super) fn task_update_is_empty(update: &TaskUpdate) -> bool {
    update.title.is_none()
        && update.status.is_none()
        && update.priority.is_none()
        && update.task_type.is_none()
        && update.reporter.is_none()
        && update.assignee.is_none()
        && update.due_date.is_none()
        && update.effort.is_none()
        && update.description.is_none()
        && update.tags.is_none()
        && update.relationships.is_none()
        && update.custom_fields.is_none()
        && update.sprints.is_none()
}

pub(super) fn task_update_field_names(update: &TaskUpdate) -> Vec<String> {
    let mut fields = Vec::new();
    if update.title.is_some() {
        fields.push("title".to_string());
    }
    if update.description.is_some() {
        fields.push("description".to_string());
    }
    if update.status.is_some() {
        fields.push("status".to_string());
    }
    if update.task_type.is_some() {
        fields.push("type".to_string());
    }
    if update.priority.is_some() {
        fields.push("priority".to_string());
    }
    if update.assignee.is_some() {
        fields.push("assignee".to_string());
    }
    if update.reporter.is_some() {
        fields.push("reporter".to_string());
    }
    if update.due_date.is_some() {
        fields.push("due_date".to_string());
    }
    if update.effort.is_some() {
        fields.push("effort".to_string());
    }
    if update.tags.is_some() {
        fields.push("tags".to_string());
    }
    if update.relationships.is_some() {
        fields.push("relationships".to_string());
    }
    if update.custom_fields.is_some() {
        fields.push("custom_fields".to_string());
    }
    if update.sprints.is_some() {
        fields.push("sprints".to_string());
    }
    fields
}

pub(super) fn custom_field_value_to_field_value(value: &CustomFieldValue) -> Option<FieldValue> {
    #[cfg(feature = "schema")]
    {
        match value {
            serde_json::Value::Null => None,
            serde_json::Value::String(s) => Some(FieldValue::String(s.clone())),
            serde_json::Value::Number(n) => Some(FieldValue::String(n.to_string())),
            serde_json::Value::Bool(b) => Some(FieldValue::String(b.to_string())),
            serde_json::Value::Array(items) => {
                let list = items
                    .iter()
                    .filter_map(|item| item.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>();
                Some(FieldValue::List(list))
            }
            _ => None,
        }
    }
    #[cfg(not(feature = "schema"))]
    {
        match value {
            serde_yaml_ng::Value::Null => None,
            serde_yaml_ng::Value::String(s) => Some(FieldValue::String(s.clone())),
            serde_yaml_ng::Value::Number(n) => Some(FieldValue::String(n.to_string())),
            serde_yaml_ng::Value::Bool(b) => Some(FieldValue::String(b.to_string())),
            serde_yaml_ng::Value::Sequence(items) => {
                let list = items
                    .iter()
                    .filter_map(|item| item.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>();
                Some(FieldValue::List(list))
            }
            _ => None,
        }
    }
}

pub(super) fn custom_value_from_field(value: FieldValue) -> CustomFieldValue {
    match value {
        FieldValue::String(text) => crate::types::custom_value_string(text),
        FieldValue::List(values) => custom_value_list(values),
    }
}

pub(super) fn custom_value_list(values: Vec<String>) -> CustomFieldValue {
    #[cfg(feature = "schema")]
    {
        serde_json::Value::Array(values.into_iter().map(serde_json::Value::String).collect())
    }
    #[cfg(not(feature = "schema"))]
    {
        serde_yaml_ng::Value::Sequence(
            values
                .into_iter()
                .map(serde_yaml_ng::Value::String)
                .collect(),
        )
    }
}

pub(super) fn custom_value_null() -> CustomFieldValue {
    #[cfg(feature = "schema")]
    {
        serde_json::Value::Null
    }
    #[cfg(not(feature = "schema"))]
    {
        serde_yaml_ng::Value::Null
    }
}
