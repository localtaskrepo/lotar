//! Reading field values out of Jira and GitHub issue JSON.

use super::*;

pub(super) fn read_remote_value(
    provider: SyncProvider,
    issue: &JsonValue,
    field: &str,
) -> Option<FieldValue> {
    match provider {
        SyncProvider::Jira => read_jira_field(issue, field),
        SyncProvider::Github => read_github_field(issue, field),
    }
}

pub(super) fn jira_pull_field_names(remote: &SyncRemoteConfig) -> Vec<String> {
    let mut fields = vec![
        "summary".to_string(),
        "description".to_string(),
        "status".to_string(),
        "issuetype".to_string(),
        "priority".to_string(),
        "assignee".to_string(),
        "reporter".to_string(),
        "labels".to_string(),
    ];
    for (local_key, mapping) in &remote.mapping {
        let detail = normalize_mapping_detail(local_key, mapping);
        let remote_field = detail
            .field
            .clone()
            .unwrap_or_else(|| local_key.to_string());
        fields.push(remote_field);
    }
    fields.sort();
    fields.dedup();
    fields
}

pub(super) fn jira_payload_field_value(field: &str, value: &JsonValue) -> Option<FieldValue> {
    let field_lower = field.to_ascii_lowercase();
    match field_lower.as_str() {
        "summary" => value
            .as_str()
            .map(|text| FieldValue::String(text.to_string())),
        "description" => match value {
            JsonValue::String(text) => Some(FieldValue::String(text.clone())),
            _ => Some(FieldValue::String(jira_adf_to_text(value))),
        },
        "issuetype" | "priority" => value
            .get("name")
            .and_then(|v| v.as_str())
            .map(|text| FieldValue::String(text.to_string())),
        "assignee" | "reporter" => value
            .get("accountId")
            .or_else(|| value.get("name"))
            .or_else(|| value.get("emailAddress"))
            .or_else(|| value.get("displayName"))
            .and_then(|v| v.as_str())
            .map(|text| FieldValue::String(text.to_string())),
        "labels" => json_array_to_list(value),
        _ => read_generic_field(Some(value)),
    }
}

pub(super) fn read_jira_field(issue: &JsonValue, field: &str) -> Option<FieldValue> {
    let fields = issue.get("fields")?;
    let field_lower = field.to_ascii_lowercase();
    match field_lower.as_str() {
        "summary" => fields
            .get("summary")
            .and_then(|v| v.as_str())
            .map(|v| FieldValue::String(v.to_string())),
        "description" => {
            let value = fields.get("description")?;
            let text = match value {
                JsonValue::String(s) => s.clone(),
                JsonValue::Object(_) => jira_adf_to_text(value),
                _ => String::new(),
            };
            Some(FieldValue::String(text))
        }
        "status" => fields
            .get("status")
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str())
            .map(|v| FieldValue::String(v.to_string())),
        "issuetype" => fields
            .get("issuetype")
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str())
            .map(|v| FieldValue::String(v.to_string())),
        "priority" => fields
            .get("priority")
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str())
            .map(|v| FieldValue::String(v.to_string())),
        "assignee" => jira_user_to_string(fields.get("assignee")),
        "reporter" => jira_user_to_string(fields.get("reporter")),
        "labels" => fields.get("labels").and_then(json_array_to_list),
        _ => read_generic_field(fields.get(field)),
    }
}

pub(super) fn jira_user_to_string(value: Option<&JsonValue>) -> Option<FieldValue> {
    let user = value?;
    let value = user
        .get("emailAddress")
        .and_then(|v| v.as_str())
        .or_else(|| user.get("displayName").and_then(|v| v.as_str()))
        .or_else(|| user.get("accountId").and_then(|v| v.as_str()))
        .map(|v| v.to_string())?;
    Some(FieldValue::String(value))
}

pub(super) fn read_github_field(issue: &JsonValue, field: &str) -> Option<FieldValue> {
    let field_lower = field.to_ascii_lowercase();
    match field_lower.as_str() {
        "title" => issue
            .get("title")
            .and_then(|v| v.as_str())
            .map(|v| FieldValue::String(v.to_string())),
        "body" | "description" => issue
            .get("body")
            .and_then(|v| v.as_str())
            .map(|v| FieldValue::String(v.to_string())),
        "state" | "status" => issue
            .get("state")
            .and_then(|v| v.as_str())
            .map(|v| FieldValue::String(v.to_string())),
        "labels" => issue.get("labels").and_then(github_labels_to_list),
        "assignees" => issue.get("assignees").and_then(github_assignees_to_list),
        _ => read_generic_field(issue.get(field)),
    }
}

pub(super) fn read_generic_field(value: Option<&JsonValue>) -> Option<FieldValue> {
    match value? {
        JsonValue::String(s) => Some(FieldValue::String(s.clone())),
        JsonValue::Number(n) => Some(FieldValue::String(n.to_string())),
        JsonValue::Bool(b) => Some(FieldValue::String(b.to_string())),
        JsonValue::Array(values) => Some(FieldValue::List(
            values
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect(),
        )),
        _ => None,
    }
}

pub(super) fn github_labels_to_list(value: &JsonValue) -> Option<FieldValue> {
    let list = value.as_array()?;
    let mut out = Vec::new();
    for item in list {
        if let Some(name) = item.as_str() {
            out.push(name.to_string());
        } else if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
            out.push(name.to_string());
        }
    }
    Some(FieldValue::List(out))
}

pub(super) fn github_assignees_to_list(value: &JsonValue) -> Option<FieldValue> {
    let list = value.as_array()?;
    let mut out = Vec::new();
    for item in list {
        if let Some(login) = item.get("login").and_then(|v| v.as_str()) {
            out.push(login.to_string());
        }
    }
    Some(FieldValue::List(out))
}

pub(super) fn json_array_to_list(value: &JsonValue) -> Option<FieldValue> {
    let list = value.as_array()?;
    let mut out = Vec::new();
    for item in list {
        if let Some(text) = item.as_str() {
            out.push(text.to_string());
        }
    }
    Some(FieldValue::List(out))
}
