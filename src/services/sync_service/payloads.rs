//! Remote issue payloads built for push, and trimming them to real changes.

use super::*;

#[derive(Default, Debug)]
pub(super) struct GithubIssuePayload {
    pub(super) title: Option<String>,
    pub(super) body: Option<String>,
    pub(super) state: Option<String>,
    pub(super) labels: Vec<String>,
    pub(super) assignees: Vec<String>,
    pub(super) labels_explicit: bool,
    pub(super) assignees_explicit: bool,
}

impl GithubIssuePayload {
    pub(super) fn is_empty(&self, include_state: bool) -> bool {
        self.title.is_none()
            && self.body.is_none()
            && self.labels.is_empty()
            && self.assignees.is_empty()
            && !self.labels_explicit
            && !self.assignees_explicit
            && (!include_state || self.state.is_none())
    }

    pub(super) fn to_json(&self, include_state: bool) -> JsonValue {
        let mut map = JsonMap::new();
        if let Some(title) = self.title.as_ref() {
            map.insert("title".to_string(), JsonValue::String(title.clone()));
        }
        if let Some(body) = self.body.as_ref() {
            map.insert("body".to_string(), JsonValue::String(body.clone()));
        }
        if include_state && let Some(state) = self.state.as_ref() {
            map.insert("state".to_string(), JsonValue::String(state.clone()));
        }
        if self.labels_explicit || !self.labels.is_empty() {
            map.insert(
                "labels".to_string(),
                JsonValue::Array(
                    self.labels
                        .iter()
                        .map(|label| JsonValue::String(label.clone()))
                        .collect(),
                ),
            );
        }
        if self.assignees_explicit || !self.assignees.is_empty() {
            map.insert(
                "assignees".to_string(),
                JsonValue::Array(
                    self.assignees
                        .iter()
                        .map(|assignee| JsonValue::String(assignee.clone()))
                        .collect(),
                ),
            );
        }
        JsonValue::Object(map)
    }
}

#[derive(Default, Debug)]
pub(super) struct JiraIssuePayload {
    pub(super) fields: JsonMap<String, JsonValue>,
    pub(super) desired_status: Option<String>,
}

impl JiraIssuePayload {
    pub(super) fn is_empty(&self) -> bool {
        self.fields.is_empty() && self.desired_status.is_none()
    }
}

pub(super) fn build_jira_payload(
    remote: &SyncRemoteConfig,
    task: &TaskDTO,
    op: SyncOperation,
    client: Option<&SyncClient>,
    lookup: &mut JiraLookupCache,
    warnings: &mut Vec<String>,
) -> JiraIssuePayload {
    let mut payload = JiraIssuePayload::default();
    for (local_key, mapping) in &remote.mapping {
        let detail = normalize_mapping_detail(local_key, mapping);
        let remote_field = detail
            .field
            .clone()
            .unwrap_or_else(|| local_key.to_string());
        let value = local_value_for_field(task, local_key);
        let mapped = apply_mapping_for_push(remote.provider, &remote_field, value, &detail);
        let Some(value) = mapped else {
            continue;
        };
        if remote_field.eq_ignore_ascii_case("status") {
            payload.desired_status = value.to_string_value();
            continue;
        }
        apply_jira_field(
            &mut payload.fields,
            &remote_field,
            value,
            client,
            lookup,
            warnings,
        );
    }

    if matches!(op, SyncOperation::Create) {
        if !payload.fields.contains_key("summary") {
            payload
                .fields
                .insert("summary".to_string(), JsonValue::String(task.title.clone()));
        }
        if !payload.fields.contains_key("project")
            && let Some(project) = remote.project.as_deref()
        {
            payload
                .fields
                .insert("project".to_string(), json!({"key": project}));
        }
        if !payload.fields.contains_key("issuetype") {
            payload
                .fields
                .insert("issuetype".to_string(), json!({"name": "Task"}));
        }
    }

    payload
}

pub(super) fn build_github_payload(
    remote: &SyncRemoteConfig,
    task: &TaskDTO,
    op: SyncOperation,
) -> GithubIssuePayload {
    let mut payload = GithubIssuePayload::default();
    for (local_key, mapping) in &remote.mapping {
        let detail = normalize_mapping_detail(local_key, mapping);
        let remote_field = detail
            .field
            .clone()
            .unwrap_or_else(|| local_key.to_string());
        let value = local_value_for_field(task, local_key);
        let mapped = apply_mapping_for_push(remote.provider, &remote_field, value, &detail);
        let Some(value) = mapped else {
            continue;
        };
        apply_github_field(&mut payload, &remote_field, value);
    }

    if matches!(op, SyncOperation::Create) && payload.title.is_none() {
        payload.title = Some(task.title.clone());
    }

    payload.labels = dedupe_list(payload.labels);
    payload.assignees = dedupe_list(payload.assignees);
    payload
}

pub(super) fn apply_jira_field(
    fields: &mut JsonMap<String, JsonValue>,
    field: &str,
    value: FieldValue,
    client: Option<&SyncClient>,
    lookup: &mut JiraLookupCache,
    warnings: &mut Vec<String>,
) {
    let field_lower = field.to_ascii_lowercase();
    match field_lower.as_str() {
        "summary" => {
            if let Some(text) = value.to_string_value() {
                fields.insert(field.to_string(), JsonValue::String(text));
            }
        }
        "description" => {
            let text = value.to_string_value().unwrap_or_default();
            fields.insert(field.to_string(), jira_text_to_adf(&text));
        }
        "issuetype" => {
            if let Some(text) = value.to_string_value() {
                let resolved = resolve_jira_issue_type(&text, lookup, warnings);
                fields.insert(field.to_string(), json!({"name": resolved}));
            }
        }
        "priority" => {
            if let Some(text) = value.to_string_value() {
                fields.insert(field.to_string(), json!({"name": text}));
            }
        }
        "assignee" | "reporter" => {
            if let Some(text) = value.to_string_value() {
                if let Some(payload) = jira_user_value(&text, field, client, lookup, warnings) {
                    fields.insert(field.to_string(), payload);
                }
            } else {
                fields.insert(field.to_string(), JsonValue::Null);
            }
        }
        "labels" => {
            let mut list = value.to_list_value();
            list = dedupe_list(list);
            let next = JsonValue::Array(list.into_iter().map(JsonValue::String).collect());
            if let Some(existing) = fields.get(field).cloned() {
                let mut merged = match existing {
                    JsonValue::Array(items) => items
                        .iter()
                        .filter_map(|item| item.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>(),
                    _ => Vec::new(),
                };
                if let JsonValue::Array(next_items) = next {
                    merged.extend(
                        next_items
                            .into_iter()
                            .filter_map(|item| item.as_str().map(|s| s.to_string())),
                    );
                }
                fields.insert(
                    field.to_string(),
                    JsonValue::Array(
                        dedupe_list(merged)
                            .into_iter()
                            .map(JsonValue::String)
                            .collect(),
                    ),
                );
            } else {
                fields.insert(field.to_string(), next);
            }
        }
        _ => {
            let json_value = match value {
                FieldValue::String(text) => JsonValue::String(text),
                FieldValue::List(values) => {
                    JsonValue::Array(values.into_iter().map(JsonValue::String).collect())
                }
            };
            fields.insert(field.to_string(), json_value);
        }
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "the caller hands this value over and does not use it afterwards"
)]
pub(super) fn apply_github_field(payload: &mut GithubIssuePayload, field: &str, value: FieldValue) {
    let field_lower = field.to_ascii_lowercase();
    match field_lower.as_str() {
        "title" => payload.title = value.to_string_value(),
        "body" | "description" => payload.body = value.to_string_value(),
        "state" | "status" => payload.state = value.to_string_value(),
        "labels" => {
            payload.labels_explicit = true;
            payload.labels.extend(value.to_list_value());
        }
        "assignees" => {
            payload.assignees_explicit = true;
            payload.assignees.extend(value.to_list_value());
        }
        _ => {}
    }
}

pub(super) fn jira_payload_field_names(payload: &JiraIssuePayload) -> Vec<String> {
    let mut names = payload.fields.keys().cloned().collect::<Vec<_>>();
    if payload.desired_status.is_some() {
        names.push("status".to_string());
    }
    names
}

pub(super) fn github_payload_field_names(payload: &GithubIssuePayload) -> Vec<String> {
    let mut names = Vec::new();
    if payload.title.is_some() {
        names.push("title".to_string());
    }
    if payload.body.is_some() {
        names.push("body".to_string());
    }
    if payload.state.is_some() {
        names.push("state".to_string());
    }
    if payload.labels_explicit || !payload.labels.is_empty() {
        names.push("labels".to_string());
    }
    if payload.assignees_explicit || !payload.assignees.is_empty() {
        names.push("assignees".to_string());
    }
    names
}

pub(super) fn filter_jira_payload_against_issue(
    issue: &JsonValue,
    payload: &mut JiraIssuePayload,
) -> Vec<String> {
    let mut changed = Vec::new();
    let mut next = JsonMap::new();
    for (field, value) in &payload.fields {
        let desired = jira_payload_field_value(field, value);
        let current = read_jira_field(issue, field);
        let matches = match (desired.as_ref(), current.as_ref()) {
            (Some(desired), Some(current)) => field_values_match(desired, current),
            _ => false,
        };
        if !matches {
            changed.push(field.clone());
            next.insert(field.clone(), value.clone());
        }
    }
    payload.fields = next;

    if let Some(status) = payload.desired_status.clone() {
        let desired = FieldValue::String(status.clone());
        let matches = read_jira_field(issue, "status")
            .as_ref()
            .map(|current| field_values_match(&desired, current))
            .unwrap_or(false);
        if matches {
            payload.desired_status = None;
        } else {
            changed.push("status".to_string());
        }
    }

    changed
}

pub(super) fn filter_github_payload_against_issue(
    issue: &JsonValue,
    payload: &mut GithubIssuePayload,
) -> Vec<String> {
    let mut changed = Vec::new();

    if let Some(title) = payload.title.clone() {
        let desired = FieldValue::String(title);
        let matches = read_github_field(issue, "title")
            .as_ref()
            .map(|current| field_values_match(&desired, current))
            .unwrap_or(false);
        if matches {
            payload.title = None;
        } else {
            changed.push("title".to_string());
        }
    }

    if let Some(body) = payload.body.clone() {
        let desired = FieldValue::String(body);
        let matches = read_github_field(issue, "body")
            .as_ref()
            .map(|current| field_values_match(&desired, current))
            .unwrap_or(false);
        if matches {
            payload.body = None;
        } else {
            changed.push("body".to_string());
        }
    }

    if let Some(state) = payload.state.clone() {
        let desired = FieldValue::String(state);
        let matches = read_github_field(issue, "state")
            .as_ref()
            .map(|current| field_values_match(&desired, current))
            .unwrap_or(false);
        if matches {
            payload.state = None;
        } else {
            changed.push("state".to_string());
        }
    }

    if payload.labels_explicit || !payload.labels.is_empty() {
        let desired = FieldValue::List(dedupe_list(payload.labels.clone()));
        let matches = read_github_field(issue, "labels")
            .as_ref()
            .map(|current| field_values_match(&desired, current))
            .unwrap_or(false);
        if matches {
            payload.labels.clear();
            payload.labels_explicit = false;
        } else {
            changed.push("labels".to_string());
        }
    }

    if payload.assignees_explicit || !payload.assignees.is_empty() {
        let desired = FieldValue::List(dedupe_list(payload.assignees.clone()));
        let matches = read_github_field(issue, "assignees")
            .as_ref()
            .map(|current| field_values_match(&desired, current))
            .unwrap_or(false);
        if matches {
            payload.assignees.clear();
            payload.assignees_explicit = false;
        } else {
            changed.push("assignees".to_string());
        }
    }

    changed
}
