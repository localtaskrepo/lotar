//! Field mapping rules between local tasks and remote issues, in both
//! directions.

use super::*;

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
            archived: false,
            moved_from: Vec::new(),
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
