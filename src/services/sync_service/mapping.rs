//! Field mapping between local tasks and remote issues, in both directions,
//! and matching of task references to a remote.

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

#[derive(Debug)]
pub(super) enum ReferenceState {
    Matching(String),
    ProviderOnly(String),
    None,
}

pub(super) fn determine_reference_state(
    remote: &SyncRemoteConfig,
    task: &TaskDTO,
) -> ReferenceState {
    let mut provider_reference = None;
    let mut other_reference = None;

    for reference in &task.references {
        let (candidate, other_candidate) = match remote.provider {
            SyncProvider::Jira => (reference.jira.as_deref(), reference.github.as_deref()),
            SyncProvider::Github => (reference.github.as_deref(), reference.jira.as_deref()),
        };
        if let Some(raw) = candidate {
            if provider_reference.is_none() {
                provider_reference = Some(raw);
            }
            if let Some(normalized) = normalize_reference_for_remote(remote, raw) {
                return ReferenceState::Matching(normalized);
            }
        }
        if other_reference.is_none()
            && let Some(raw) = other_candidate
        {
            other_reference = Some(raw);
        }
    }

    if let Some(reference) = provider_reference {
        ReferenceState::ProviderOnly(reference.to_string())
    } else if let Some(reference) = other_reference {
        ReferenceState::ProviderOnly(reference.to_string())
    } else {
        ReferenceState::None
    }
}

pub(super) fn normalize_reference_for_remote(
    remote: &SyncRemoteConfig,
    value: &str,
) -> Option<String> {
    match remote.provider {
        SyncProvider::Jira => {
            let normalized = normalize_jira_reference(value)?;
            if let Some(project) = remote.project.as_ref() {
                let prefix = format!("{}-", project.trim().to_ascii_uppercase());
                if !normalized.to_ascii_uppercase().starts_with(&prefix) {
                    return None;
                }
            }
            Some(normalized)
        }
        SyncProvider::Github => {
            let (repo, number) = parse_github_reference(value, remote.repo.as_deref())?;
            if let Some(expected) = remote.repo.as_deref() {
                let expected_norm = normalize_github_repo(expected);
                if normalize_github_repo(&repo) != expected_norm {
                    return None;
                }
            }
            Some(format!("{}#{}", normalize_github_repo(&repo), number))
        }
    }
}

pub(super) fn normalize_jira_reference(value: &str) -> Option<String> {
    let trimmed = trim_platform_prefix(value, "jira");
    let (prefix, rest) = trimmed.split_once('-')?;
    if rest.trim().is_empty() {
        return None;
    }
    Some(format!(
        "{}-{}",
        prefix.trim().to_ascii_uppercase(),
        rest.trim()
    ))
}

pub(super) fn parse_github_reference(
    value: &str,
    default_repo: Option<&str>,
) -> Option<(String, u64)> {
    let trimmed = trim_platform_prefix(value, "github");
    if let Some((repo, number)) = trimmed.split_once('#') {
        let repo_trimmed = repo.trim();
        let num = number.trim().parse::<u64>().ok()?;
        if repo_trimmed.is_empty() {
            let default_repo = default_repo?;
            return Some((default_repo.to_string(), num));
        }
        return Some((repo_trimmed.to_string(), num));
    }

    if let Ok(num) = trimmed.trim().parse::<u64>() {
        let default_repo = default_repo?;
        return Some((default_repo.to_string(), num));
    }

    None
}

pub(super) fn trim_platform_prefix(value: &str, prefix: &str) -> String {
    let trimmed = value.trim();
    let lower = trimmed.to_ascii_lowercase();
    let needle = format!("{}:", prefix.to_ascii_lowercase());
    if lower.starts_with(&needle) {
        trimmed[needle.len()..].trim().to_string()
    } else {
        trimmed.to_string()
    }
}

pub(super) fn build_reference_index(
    remote: &SyncRemoteConfig,
    tasks: &[(String, TaskDTO)],
) -> HashMap<String, String> {
    let mut index = HashMap::new();
    for (_id, task) in tasks {
        for reference in &task.references {
            let raw = match remote.provider {
                SyncProvider::Jira => reference.jira.as_deref(),
                SyncProvider::Github => reference.github.as_deref(),
            };
            if let Some(raw) = raw
                && let Some(normalized) = normalize_reference_for_remote(remote, raw)
            {
                index.insert(normalized, task.id.clone());
            }
        }
    }
    index
}

pub(super) fn issue_reference_for_remote(
    remote: &SyncRemoteConfig,
    issue: &JsonValue,
) -> Option<String> {
    match remote.provider {
        SyncProvider::Jira => issue
            .get("key")
            .and_then(|v| v.as_str())
            .and_then(|key| normalize_reference_for_remote(remote, key)),
        SyncProvider::Github => {
            let repo = remote.repo.as_deref()?;
            let number = issue.get("number").and_then(|v| v.as_u64())?;
            Some(format!("{}#{}", normalize_github_repo(repo), number))
        }
    }
}

pub(super) fn map_issue_to_local_fields(
    provider: SyncProvider,
    remote: &SyncRemoteConfig,
    issue: &JsonValue,
    existing: Option<&TaskDTO>,
) -> LocalFieldValues {
    let mut out = LocalFieldValues::default();

    // Collect label values consumed by scalar field mappings (type, priority, status)
    // so we can exclude them when mapping labels to tags
    let mut consumed_labels: HashSet<String> = HashSet::new();
    for (local_key, mapping) in &remote.mapping {
        if !is_scalar_local_field(local_key) {
            continue;
        }
        let detail = normalize_mapping_detail(local_key, mapping);
        let remote_field = detail
            .field
            .clone()
            .unwrap_or_else(|| local_key.to_string());
        // Only track consumption from labels field
        if !matches!(remote_field.to_ascii_lowercase().as_str(), "labels") {
            continue;
        }
        // All mapped remote values are consumed
        for remote_val in detail.values.values() {
            consumed_labels.insert(remote_val.to_ascii_lowercase());
        }
    }

    for (local_key, mapping) in &remote.mapping {
        let detail = normalize_mapping_detail(local_key, mapping);
        let remote_field = detail
            .field
            .clone()
            .unwrap_or_else(|| local_key.to_string());
        let remote_value = read_remote_value(provider, issue, &remote_field);
        let existing_value = existing.and_then(|task| local_value_for_field(task, local_key));

        // When mapping tags from labels, filter out consumed label values
        let remote_value = if local_key == "tags"
            && matches!(remote_field.to_ascii_lowercase().as_str(), "labels")
            && !consumed_labels.is_empty()
        {
            remote_value.map(|v| filter_consumed_labels(v, &consumed_labels))
        } else {
            remote_value
        };

        let mapped =
            apply_mapping_for_pull(local_key, &detail, remote_value, existing_value.as_ref());
        if let Some(value) = mapped {
            apply_local_value(&mut out, local_key, value);
        }
    }
    out
}

pub(super) fn filter_consumed_labels(value: FieldValue, consumed: &HashSet<String>) -> FieldValue {
    match value {
        FieldValue::List(values) => FieldValue::List(
            values
                .into_iter()
                .filter(|v| !consumed.contains(&v.to_ascii_lowercase()))
                .collect(),
        ),
        other => other,
    }
}

pub(super) fn default_title_from_issue(
    provider: SyncProvider,
    issue: &JsonValue,
) -> Option<String> {
    match provider {
        SyncProvider::Jira => issue
            .get("fields")
            .and_then(|v| v.get("summary"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        SyncProvider::Github => issue
            .get("title")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    }
}

pub(super) fn apply_local_value(out: &mut LocalFieldValues, key: &str, value: FieldValue) {
    match key {
        "title" => out.title = value.to_string_value(),
        "description" => out.description = field_value_to_string_allow_empty(&value),
        "status" => {
            if let Some(value) = value.to_string_value() {
                out.status = Some(TaskStatus::from(value));
            }
        }
        "task_type" | "type" => {
            if let Some(value) = value.to_string_value() {
                out.task_type = Some(TaskType::from(value));
            }
        }
        "priority" => {
            if let Some(value) = value.to_string_value() {
                out.priority = Some(Priority::from(value));
            }
        }
        "assignee" => out.assignee = field_value_to_string_allow_empty(&value),
        "reporter" => out.reporter = field_value_to_string_allow_empty(&value),
        "tags" => {
            out.tags = Some(dedupe_list(value.to_list_value()));
        }
        other => {
            let mut map = out.custom_fields.take().unwrap_or_default();
            if value.is_empty() {
                map.insert(other.to_string(), custom_value_null());
            } else {
                map.insert(other.to_string(), custom_value_from_field(value));
            }
            out.custom_fields = Some(map);
        }
    }
}

pub(super) fn field_value_to_string_allow_empty(value: &FieldValue) -> Option<String> {
    match value {
        FieldValue::String(raw) => Some(raw.clone()),
        FieldValue::List(values) => values
            .iter()
            .find(|v| !v.trim().is_empty())
            .map(|v| v.trim().to_string()),
    }
}

pub(super) fn normalize_mapping_detail(
    local_key: &str,
    mapping: &SyncFieldMapping,
) -> SyncFieldMappingDetail {
    match mapping {
        SyncFieldMapping::Simple(field) => SyncFieldMappingDetail {
            field: Some(field.clone()),
            ..Default::default()
        },
        SyncFieldMapping::Detailed(detail) => {
            let mut out = detail.clone();
            if out.field.is_none() {
                out.field = Some(local_key.to_string());
            }
            out
        }
    }
}

pub(super) fn apply_mapping_for_pull(
    local_key: &str,
    detail: &SyncFieldMappingDetail,
    remote_value: Option<FieldValue>,
    existing_value: Option<&FieldValue>,
) -> Option<FieldValue> {
    let empty = remote_value.as_ref().map(|v| v.is_empty()).unwrap_or(true);
    if empty {
        return match detail.when_empty {
            Some(SyncWhenEmpty::Clear) => Some(clear_value_for_pull(local_key, &remote_value)),
            Some(SyncWhenEmpty::Skip) | None => None,
        };
    }

    let mut value = remote_value?;
    if !detail.values.is_empty() {
        value = map_remote_value_for_pull(local_key, detail, value, existing_value);
        if value.is_empty() {
            return None;
        }
    }
    Some(value)
}

pub(super) fn clear_value_for_pull(
    local_key: &str,
    remote_value: &Option<FieldValue>,
) -> FieldValue {
    if local_key == "tags" || matches!(remote_value, Some(FieldValue::List(_))) {
        FieldValue::List(Vec::new())
    } else {
        FieldValue::String(String::new())
    }
}

pub(super) fn map_remote_value_for_pull(
    local_key: &str,
    detail: &SyncFieldMappingDetail,
    value: FieldValue,
    existing_value: Option<&FieldValue>,
) -> FieldValue {
    let mut reverse: HashMap<String, Vec<String>> = HashMap::new();
    for (local, remote) in &detail.values {
        reverse
            .entry(remote.to_ascii_lowercase())
            .or_default()
            .push(local.clone());
    }

    let existing_scalar = existing_value.and_then(|value| value.to_string_value());
    let pick_candidate = |candidates: &[String]| {
        if let Some(existing) = existing_scalar.as_ref() {
            let existing_norm = existing.to_ascii_lowercase();
            if let Some(found) = candidates
                .iter()
                .find(|candidate| candidate.to_ascii_lowercase() == existing_norm)
            {
                return found.clone();
            }
        }
        candidates.first().cloned().unwrap_or_default()
    };

    match value {
        FieldValue::String(raw) => {
            let lookup = raw.to_ascii_lowercase();
            if let Some(candidates) = reverse.get(&lookup) {
                FieldValue::String(pick_candidate(candidates))
            } else {
                FieldValue::String(raw)
            }
        }
        FieldValue::List(values) => {
            if is_scalar_local_field(local_key) {
                for raw in &values {
                    let lookup = raw.to_ascii_lowercase();
                    if let Some(candidates) = reverse.get(&lookup) {
                        return FieldValue::String(pick_candidate(candidates));
                    }
                }
                if let Some(existing) = existing_scalar {
                    FieldValue::String(existing)
                } else {
                    FieldValue::String(String::new())
                }
            } else {
                FieldValue::List(
                    values
                        .into_iter()
                        .map(|raw| {
                            let lookup = raw.to_ascii_lowercase();
                            reverse
                                .get(&lookup)
                                .and_then(|candidates| candidates.first().cloned())
                                .unwrap_or(raw)
                        })
                        .collect(),
                )
            }
        }
    }
}

pub(super) fn is_scalar_local_field(local_key: &str) -> bool {
    matches!(
        local_key,
        "title"
            | "description"
            | "status"
            | "task_type"
            | "type"
            | "priority"
            | "assignee"
            | "reporter"
    )
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

pub(super) fn apply_mapping_for_push(
    provider: SyncProvider,
    remote_field: &str,
    local_value: Option<FieldValue>,
    detail: &SyncFieldMappingDetail,
) -> Option<FieldValue> {
    if let Some(set) = detail.set.as_ref() {
        return Some(FieldValue::String(set.clone()));
    }

    let mut value = local_value;
    let is_list = field_expects_list(provider, remote_field);

    if value.as_ref().map(|v| v.is_empty()).unwrap_or(true)
        && let Some(default) = detail.default.as_ref()
    {
        value = Some(FieldValue::String(default.clone()));
    }

    let empty = value.as_ref().map(|v| v.is_empty()).unwrap_or(true);
    if empty {
        return match detail.when_empty {
            Some(SyncWhenEmpty::Clear) => Some(if is_list {
                FieldValue::List(Vec::new())
            } else {
                FieldValue::String(String::new())
            }),
            Some(SyncWhenEmpty::Skip) | None => None,
        };
    }

    let mut value = value?;
    if !detail.values.is_empty() {
        value = map_local_value(detail, value);
    }
    if is_list && !detail.add.is_empty() {
        let mut list = value.to_list_value();
        list.extend(detail.add.iter().cloned());
        value = FieldValue::List(list);
    }
    Some(value)
}

pub(super) fn map_local_value(detail: &SyncFieldMappingDetail, value: FieldValue) -> FieldValue {
    match value {
        FieldValue::String(raw) => {
            FieldValue::String(lookup_mapping_value(&detail.values, &raw).unwrap_or(raw))
        }
        FieldValue::List(values) => FieldValue::List(
            values
                .into_iter()
                .map(|raw| lookup_mapping_value(&detail.values, &raw).unwrap_or(raw))
                .collect(),
        ),
    }
}

pub(super) fn lookup_mapping_value(map: &HashMap<String, String>, value: &str) -> Option<String> {
    if let Some(found) = map.get(value) {
        return Some(found.clone());
    }
    let target = value.to_ascii_lowercase();
    map.iter()
        .find(|(key, _)| key.to_ascii_lowercase() == target)
        .map(|(_, val)| val.clone())
}

pub(super) fn field_expects_list(provider: SyncProvider, remote_field: &str) -> bool {
    let name = remote_field.to_ascii_lowercase();
    match provider {
        SyncProvider::Jira => matches!(name.as_str(), "labels"),
        SyncProvider::Github => matches!(name.as_str(), "labels" | "assignees"),
    }
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

pub(super) fn local_value_for_field(task: &TaskDTO, field: &str) -> Option<FieldValue> {
    match field {
        "title" => Some(FieldValue::String(task.title.clone())),
        "description" => task
            .description
            .as_ref()
            .map(|value| FieldValue::String(value.clone())),
        "status" => Some(FieldValue::String(task.status.to_string())),
        "task_type" | "type" => Some(FieldValue::String(task.task_type.to_string())),
        "priority" => Some(FieldValue::String(task.priority.to_string())),
        "assignee" => task
            .assignee
            .as_ref()
            .map(|value| FieldValue::String(value.clone())),
        "reporter" => task
            .reporter
            .as_ref()
            .map(|value| FieldValue::String(value.clone())),
        "tags" => Some(FieldValue::List(task.tags.clone())),
        other => task
            .custom_fields
            .get(other)
            .and_then(custom_field_value_to_field_value),
    }
}

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

pub(super) fn format_field_list(fields: &[String]) -> Option<String> {
    let mut values = fields
        .iter()
        .map(|field| field.trim().to_string())
        .filter(|field| !field.is_empty())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    if values.is_empty() {
        None
    } else {
        Some(values.join(", "))
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

#[cfg(test)]
mod sync_mapping_tests {
    use super::*;
    use crate::types::ReferenceEntry;
    use serde_json::json;
    use std::collections::HashMap;

    fn sample_task() -> TaskDTO {
        TaskDTO {
            task_state: None,
            deleted_at: None,
            id: "TEST-1".to_string(),
            title: "Example".to_string(),
            status: TaskStatus::from("Todo"),
            priority: Priority::from("High"),
            task_type: TaskType::from("Feature"),
            reporter: Some("me@example.com".to_string()),
            assignee: Some("dev".to_string()),
            created: "2024-01-01T00:00:00Z".to_string(),
            modified: "2024-01-01T00:00:00Z".to_string(),
            due_date: None,
            effort: None,
            subtitle: None,
            description: Some("Body".to_string()),
            tags: vec!["alpha".to_string()],
            relationships: Default::default(),
            comments: vec![],
            references: vec![],
            acceptance_criteria: vec![],
            sprints: vec![],
            sprint_order: Default::default(),
            history: vec![],
            custom_fields: Default::default(),
        }
    }

    #[test]
    fn jira_query_for_remote_prefers_filter() {
        let remote = SyncRemoteConfig {
            provider: SyncProvider::Jira,
            project: Some("DEMO".to_string()),
            repo: None,
            filter: Some("project = HELLO".to_string()),
            auth_profile: None,
            mapping: HashMap::new(),
        };
        let jql = jira_query_for_remote(&remote).expect("jql should build");
        assert_eq!(jql, "project = HELLO");
    }

    #[test]
    fn jira_query_for_remote_uses_project_when_no_filter() {
        let remote = SyncRemoteConfig {
            provider: SyncProvider::Jira,
            project: Some("DEMO".to_string()),
            repo: None,
            filter: None,
            auth_profile: None,
            mapping: HashMap::new(),
        };
        let jql = jira_query_for_remote(&remote).expect("jql should build");
        assert_eq!(jql, "project = DEMO");
    }

    #[test]
    fn github_query_adds_state_when_missing() {
        let query = github_build_search_query("owner/repo", "label:bug");
        assert!(query.contains("repo:owner/repo"));
        assert!(query.contains("state:all"));
        assert!(query.contains("label:bug"));
    }

    #[test]
    fn github_label_mapping_prefers_matching_scalar_values() {
        let mut remote = SyncRemoteConfig {
            provider: SyncProvider::Github,
            project: None,
            repo: Some("org/repo".to_string()),
            filter: None,
            auth_profile: None,
            mapping: HashMap::new(),
        };
        remote.mapping.insert(
            "task_type".to_string(),
            SyncFieldMapping::Detailed(SyncFieldMappingDetail {
                field: Some("labels".to_string()),
                values: HashMap::from([
                    (String::from("Feature"), String::from("type:feature")),
                    (String::from("Bug"), String::from("type:bug")),
                ]),
                ..Default::default()
            }),
        );
        remote.mapping.insert(
            "priority".to_string(),
            SyncFieldMapping::Detailed(SyncFieldMappingDetail {
                field: Some("labels".to_string()),
                values: HashMap::from([(String::from("Medium"), String::from("priority:medium"))]),
                ..Default::default()
            }),
        );

        let issue = json!({
            "labels": [
                {"name": "priority:medium"},
                {"name": "type:feature"}
            ]
        });

        let update = build_task_update_from_issue(SyncProvider::Github, &remote, &issue, None);
        assert_eq!(update.task_type.unwrap().to_string(), "Feature");
        assert_eq!(update.priority.unwrap().to_string(), "Medium");
    }

    #[test]
    fn github_status_mapping_prefers_existing_value_on_collisions() {
        let mut remote = SyncRemoteConfig {
            provider: SyncProvider::Github,
            project: None,
            repo: Some("org/repo".to_string()),
            filter: None,
            auth_profile: None,
            mapping: HashMap::new(),
        };
        remote.mapping.insert(
            "status".to_string(),
            SyncFieldMapping::Detailed(SyncFieldMappingDetail {
                field: Some("state".to_string()),
                values: HashMap::from([
                    (String::from("Todo"), String::from("open")),
                    (String::from("InProgress"), String::from("open")),
                ]),
                ..Default::default()
            }),
        );

        let issue = json!({"state": "open"});
        let mut existing = sample_task();
        existing.status = TaskStatus::from("Todo");

        let update =
            build_task_update_from_issue(SyncProvider::Github, &remote, &issue, Some(&existing));
        assert!(update.status.is_none());
    }

    #[test]
    fn sync_mapped_status_outside_project_enums_fails_loudly() {
        let tmp = tempfile::tempdir().unwrap();
        let tasks_dir = tmp.path().join(".tasks");
        std::fs::create_dir_all(tasks_dir.join("SYNC")).unwrap();
        std::fs::write(
            tasks_dir.join("SYNC").join("config.yml"),
            "issue_states: [Todo, Done]\n",
        )
        .unwrap();

        let mut remote = SyncRemoteConfig {
            provider: SyncProvider::Github,
            project: None,
            repo: Some("org/repo".to_string()),
            filter: None,
            auth_profile: None,
            mapping: HashMap::new(),
        };
        remote.mapping.insert(
            "status".to_string(),
            SyncFieldMapping::Detailed(SyncFieldMappingDetail {
                field: Some("state".to_string()),
                values: HashMap::from([(
                    String::from("Bogus"), // not in SYNC's issue_states
                    String::from("open"),
                )]),
                ..Default::default()
            }),
        );

        let issue = json!({"state": "open", "title": "Sync enum mismatch", "number": 1});
        let create =
            build_task_create_from_issue(SyncProvider::Github, &remote, &issue, "SYNC").unwrap();
        assert_eq!(create.status.as_deref(), Some("Bogus"));

        let mut storage = crate::storage::manager::Storage::new(&tasks_dir);
        let err = TaskService::create(&mut storage, create)
            .expect_err("status outside project enums must fail");
        match err {
            LoTaRError::ValidationError(msg) => {
                assert!(
                    msg.contains("not enabled") && msg.contains("Todo, Done"),
                    "unexpected message: {msg}"
                );
            }
            other => panic!("expected validation error, got {other:?}"),
        }
        let task_files = std::fs::read_dir(tasks_dir.join("SYNC"))
            .unwrap()
            .filter(|entry| {
                entry
                    .as_ref()
                    .ok()
                    .and_then(|e| {
                        e.file_name()
                            .to_str()
                            .map(|n| n.ends_with(".yml") && n != "config.yml")
                    })
                    .unwrap_or(false)
            })
            .count();
        assert_eq!(
            task_files, 0,
            "failed sync create must not leave a task file"
        );
    }

    #[test]
    fn determine_reference_state_skips_other_provider_refs() {
        let remote = SyncRemoteConfig {
            provider: SyncProvider::Github,
            project: None,
            repo: Some("org/repo".to_string()),
            filter: None,
            auth_profile: None,
            mapping: HashMap::new(),
        };
        let mut task = sample_task();
        task.references.push(ReferenceEntry {
            jira: Some("LTS-1".to_string()),
            ..Default::default()
        });

        match determine_reference_state(&remote, &task) {
            ReferenceState::ProviderOnly(value) => assert_eq!(value, "LTS-1"),
            other => panic!("Expected ProviderOnly, got {other:?}"),
        }
    }
}
