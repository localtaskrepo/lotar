use serde::de::DeserializeOwned;
use serde_yaml::Value;

use crate::config::types::{
    ConfigError, GlobalConfig, ProjectConfig, StringConfigField, SyncAuthProfile, SyncConfig,
    SyncRemoteConfig,
};
use crate::types::{Priority, TaskStatus, TaskType};

fn expand_dotted_keys(value: Value) -> Value {
    match value {
        Value::Mapping(map) => {
            let mut root = serde_yaml::Mapping::new();
            for (k, v) in map {
                if let Value::String(key) = k {
                    if key == "auth_profiles" || key == "remotes" {
                        root.insert(Value::String(key), expand_dotted_keys_keep_keys(v));
                        continue;
                    }
                    if key.contains('.') {
                        let mut parts = key.split('.').collect::<Vec<_>>();
                        if parts.is_empty() {
                            continue;
                        }
                        let first = parts.remove(0).to_string();
                        let nested_key = parts.join(".");
                        let nested_value = if nested_key.is_empty() {
                            v
                        } else {
                            let mut nested_map = serde_yaml::Mapping::new();
                            nested_map.insert(Value::String(nested_key), v);
                            Value::Mapping(nested_map)
                        };
                        // Merge into existing child
                        let entry = root
                            .entry(Value::String(first))
                            .or_insert_with(|| Value::Mapping(serde_yaml::Mapping::new()));
                        let merged = merge_values(entry.clone(), nested_value);
                        *entry = merged;
                    } else {
                        root.insert(Value::String(key), expand_dotted_keys(v));
                    }
                } else {
                    root.insert(k, expand_dotted_keys(v));
                }
            }
            Value::Mapping(root)
        }
        Value::Sequence(seq) => Value::Sequence(seq.into_iter().map(expand_dotted_keys).collect()),
        other => other,
    }
}

fn expand_dotted_keys_keep_keys(value: Value) -> Value {
    match value {
        Value::Mapping(map) => {
            let mut root = serde_yaml::Mapping::new();
            for (k, v) in map {
                root.insert(k, expand_dotted_keys(v));
            }
            Value::Mapping(root)
        }
        Value::Sequence(seq) => Value::Sequence(seq.into_iter().map(expand_dotted_keys).collect()),
        other => other,
    }
}

fn merge_values(a: Value, b: Value) -> Value {
    match (a, b) {
        (Value::Mapping(mut m1), Value::Mapping(m2)) => {
            for (k, v2) in m2 {
                if let Some(v1) = m1.get(&k).cloned() {
                    m1.insert(k, merge_values(v1, v2));
                } else {
                    m1.insert(k, v2);
                }
            }
            Value::Mapping(m1)
        }
        // Sequences and scalars: prefer b (last write wins)
        (_, vb) => vb,
    }
}

fn get_path<'a>(root: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut cur = root;
    for key in path {
        match cur {
            Value::Mapping(map) => {
                cur = map.get(Value::String((*key).to_string()))?;
            }
            _ => return None,
        }
    }
    Some(cur)
}

fn cast<T: DeserializeOwned>(v: &Value) -> Option<T> {
    serde_yaml::from_value::<T>(v.clone()).ok()
}

// Normalize token strings to a tolerant, comparable form: camelCase/PascalCase -> snake, hyphens/spaces -> underscores, lowercased
fn normalize_token(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let mut prev_is_sep = false;
    for (i, ch) in s.chars().enumerate() {
        if ch == '-' || ch == ' ' || ch == '_' {
            if !prev_is_sep {
                out.push('_');
                prev_is_sep = true;
            }
            continue;
        }
        prev_is_sep = false;
        if ch.is_ascii_uppercase() {
            if i > 0 {
                // insert underscore for camel boundary if previous isn't sep or underscore
                if !out.ends_with('_') {
                    out.push('_');
                }
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    // collapse multiple underscores possibly introduced
    let mut collapsed = String::with_capacity(out.len());
    let mut last_us = false;
    for c in out.chars() {
        if c == '_' {
            if !last_us {
                collapsed.push('_');
                last_us = true;
            }
        } else {
            last_us = false;
            collapsed.push(c);
        }
    }
    collapsed.trim_matches('_').to_string()
}

fn parse_task_status_tolerant(s: &str) -> Option<TaskStatus> {
    let original = s.trim();
    if original.is_empty() {
        return None;
    }
    match normalize_token(s).as_str() {
        "todo" => Some(TaskStatus::from("Todo")),
        "in_progress" | "inprogress" => Some(TaskStatus::from("InProgress")),
        "verify" => Some(TaskStatus::from("Verify")),
        "blocked" => Some(TaskStatus::from("Blocked")),
        "done" => Some(TaskStatus::from("Done")),
        _ => Some(TaskStatus::from(original)),
    }
}

fn parse_priority_tolerant(s: &str) -> Option<Priority> {
    let original = s.trim();
    if original.is_empty() {
        return None;
    }
    match normalize_token(s).as_str() {
        "low" => Some(Priority::from("Low")),
        "medium" => Some(Priority::from("Medium")),
        "high" => Some(Priority::from("High")),
        "critical" => Some(Priority::from("Critical")),
        _ => Some(Priority::from(original)),
    }
}

fn parse_task_type_tolerant(s: &str) -> Option<TaskType> {
    let original = s.trim();
    if original.is_empty() {
        return None;
    }
    match normalize_token(s).as_str() {
        "feature" => Some(TaskType::from("Feature")),
        "bug" => Some(TaskType::from("Bug")),
        "epic" => Some(TaskType::from("Epic")),
        "spike" => Some(TaskType::from("Spike")),
        "chore" => Some(TaskType::from("Chore")),
        _ => Some(TaskType::from(original)),
    }
}

pub fn parse_issue_states_tolerant(
    v: Value,
) -> Option<crate::config::types::ConfigurableField<TaskStatus>> {
    // Try strict first
    if let Ok(cf) =
        serde_yaml::from_value::<crate::config::types::ConfigurableField<TaskStatus>>(v.clone())
    {
        return Some(cf);
    }
    // Fallback vector of strings
    if let Ok(list) = serde_yaml::from_value::<Vec<String>>(v) {
        let mapped: Vec<TaskStatus> = list
            .into_iter()
            .filter_map(|s| parse_task_status_tolerant(&s))
            .collect();
        if !mapped.is_empty() {
            return Some(crate::config::types::ConfigurableField { values: mapped });
        }
    }
    None
}

fn parse_alias_map_tolerant<T: serde::de::DeserializeOwned>(
    v: Value,
    parse: fn(&str) -> Option<T>,
) -> Option<std::collections::HashMap<String, T>> {
    // Try strict first
    if let Ok(mut map) = serde_yaml::from_value::<std::collections::HashMap<String, T>>(v.clone()) {
        let map2 = map.drain().map(|(k, v)| (k.to_lowercase(), v)).collect();
        return Some(map2);
    }
    // Fallback: parse as map of strings
    if let Ok(mut raw) = serde_yaml::from_value::<std::collections::HashMap<String, String>>(v) {
        let mut out = std::collections::HashMap::new();
        for (k, sv) in raw.drain() {
            if let Some(tv) = parse(&sv) {
                out.insert(k.to_lowercase(), tv);
            }
        }
        return Some(out);
    }
    None
}

/// Parse global config supporting both existing flat schema and nested/dotted form
pub fn parse_global_from_yaml_str(content: &str) -> Result<GlobalConfig, ConfigError> {
    let raw: Value = serde_yaml::from_str(content)
        .map_err(|e| ConfigError::ParseError(format!("Failed to parse config: {}", e)))?;
    let data = expand_dotted_keys(raw);

    let mut cfg = GlobalConfig::default();

    // server.port
    if let Some(v) = get_path(&data, &["server", "port"]).and_then(cast::<u16>) {
        cfg.server_port = v;
    }

    // default.*
    if let Some(v) = get_path(&data, &["default", "project"]).and_then(cast::<String>) {
        cfg.default_project = v;
    }
    if let Some(v) = get_path(&data, &["default", "assignee"]).and_then(cast::<String>) {
        cfg.default_assignee = Some(v);
    }
    if let Some(v) = get_path(&data, &["default", "reporter"]).and_then(cast::<String>) {
        cfg.default_reporter = Some(v);
    }
    if let Some(v) = get_path(&data, &["default", "tags"]).cloned()
        && let Ok(list) = serde_yaml::from_value(v)
    {
        cfg.default_tags = list;
    }
    if let Some(v) = get_path(&data, &["members"])
        .cloned()
        .or_else(|| get_path(&data, &["default", "members"]).cloned())
        && let Ok(list) = serde_yaml::from_value::<Vec<String>>(v)
    {
        let members: Vec<String> = list
            .into_iter()
            .map(|entry| entry.trim().to_string())
            .filter(|entry| !entry.is_empty())
            .collect();
        cfg.members = members;
    }
    if let Some(v) = get_path(&data, &["default", "strict_members"]).and_then(cast::<bool>) {
        cfg.strict_members = v;
    }
    if let Some(v) = get_path(&data, &["default", "priority"]).and_then(cast) {
        cfg.default_priority = v;
    }
    if let Some(v) = get_path(&data, &["default", "status"]).and_then(cast) {
        cfg.default_status = Some(v);
    }

    // Note: No legacy flat root keys supported in initial version; use nested/dotted keys.

    // issue.*
    if let Some(v) = get_path(&data, &["issue", "states"]).cloned() {
        // tolerant: accept mixed-case strings like Todo, InProgress, Done
        if let Some(cf) = parse_issue_states_tolerant(v) {
            cfg.issue_states = cf;
        }
    }
    if let Some(v) = get_path(&data, &["issue", "types"]).cloned()
        && let Some(cf) = parse_issue_types_tolerant(v)
    {
        cfg.issue_types = cf;
    }
    if let Some(v) = get_path(&data, &["issue", "priorities"]).cloned()
        && let Ok(list) = serde_yaml::from_value(v)
    {
        cfg.issue_priorities.values = list;
    }

    // taxonomy.* (legacy) — will be overridden by issue.* if present
    if let Some(v) = get_path(&data, &["taxonomy", "tags"]).cloned()
        && let Ok(list) = serde_yaml::from_value(v)
    {
        cfg.tags = StringConfigField { values: list };
    }
    // issue.tags (preferred canonical)
    if let Some(v) = get_path(&data, &["issue", "tags"]).cloned()
        && let Ok(list) = serde_yaml::from_value(v)
    {
        cfg.tags = StringConfigField { values: list };
    }

    // custom.fields
    if let Some(v) = get_path(&data, &["custom", "fields"]).cloned()
        && let Ok(list) = serde_yaml::from_value(v)
    {
        cfg.custom_fields = StringConfigField { values: list };
    }

    // scan.signal_words
    if let Some(v) = get_path(&data, &["scan", "signal_words"]).cloned()
        && let Ok(list) = serde_yaml::from_value(v)
    {
        cfg.scan_signal_words = list;
    }
    // scan.ticket_patterns
    if let Some(v) = get_path(&data, &["scan", "ticket_patterns"]).cloned()
        && let Ok(list) = serde_yaml::from_value(v)
    {
        cfg.scan_ticket_patterns = Some(list);
    }
    // scan.enable_ticket_words
    if let Some(v) = get_path(&data, &["scan", "enable_ticket_words"]).and_then(cast::<bool>) {
        cfg.scan_enable_ticket_words = v;
    }
    // scan.enable_mentions
    if let Some(v) = get_path(&data, &["scan", "enable_mentions"]).and_then(cast::<bool>) {
        cfg.scan_enable_mentions = v;
    }
    // scan.strip_attributes (global)
    if let Some(v) = get_path(&data, &["scan", "strip_attributes"]).and_then(cast::<bool>) {
        cfg.scan_strip_attributes = v;
    }

    // attachments.dir
    if let Some(v) = get_path(&data, &["attachments", "dir"]).and_then(cast::<String>) {
        let trimmed = v.trim().to_string();
        if !trimmed.is_empty() {
            cfg.attachments_dir = trimmed;
        }
    }

    // attachments.max_upload_mb
    if let Some(v) = get_path(&data, &["attachments", "max_upload_mb"]).and_then(cast::<i64>) {
        cfg.attachments_max_upload_mb = v;
    }

    // sync.reports_dir
    if let Some(v) = get_path(&data, &["sync", "reports_dir"]).and_then(cast::<String>) {
        let trimmed = v.trim().to_string();
        if !trimmed.is_empty() {
            cfg.sync_reports_dir = trimmed;
        }
    }

    // sync.write_reports
    if let Some(v) = get_path(&data, &["sync", "write_reports"]).and_then(cast::<bool>) {
        cfg.sync_write_reports = v;
    }

    // sprints.defaults
    if let Some(value) =
        get_path(&data, &["sprints", "defaults", "capacity_points"]).and_then(cast::<u32>)
    {
        cfg.sprints.defaults.capacity_points = Some(value);
    }
    if let Some(value) =
        get_path(&data, &["sprints", "defaults", "capacity_hours"]).and_then(cast::<u32>)
    {
        cfg.sprints.defaults.capacity_hours = Some(value);
    }
    if let Some(value) =
        get_path(&data, &["sprints", "defaults", "length"]).and_then(cast::<String>)
    {
        let trimmed = value.trim().to_string();
        if !trimmed.is_empty() {
            cfg.sprints.defaults.length = Some(trimmed);
        }
    }
    if let Some(value) =
        get_path(&data, &["sprints", "defaults", "overdue_after"]).and_then(cast::<String>)
    {
        let trimmed = value.trim().to_string();
        if !trimmed.is_empty() {
            cfg.sprints.defaults.overdue_after = Some(trimmed);
        }
    }
    if let Some(enabled) =
        get_path(&data, &["sprints", "notifications", "enabled"]).and_then(cast::<bool>)
    {
        cfg.sprints.notifications.enabled = enabled;
    }

    // Back-compat: allow flat keys at root for scan toggles
    if let Some(v) = get_path(&data, &["scan_enable_ticket_words"]).and_then(cast::<bool>) {
        cfg.scan_enable_ticket_words = v;
    }
    if let Some(v) = get_path(&data, &["scan_enable_mentions"]).and_then(cast::<bool>) {
        cfg.scan_enable_mentions = v;
    }

    // auto.*
    if let Some(v) = get_path(&data, &["auto", "identity"]).and_then(cast::<bool>) {
        cfg.auto_identity = v;
    }
    if let Some(v) = get_path(&data, &["auto", "identity_git"]).and_then(cast::<bool>) {
        cfg.auto_identity_git = v;
    }
    if let Some(v) = get_path(&data, &["auto", "set_reporter"]).and_then(cast::<bool>) {
        cfg.auto_set_reporter = v;
    }
    if let Some(v) = get_path(&data, &["auto", "assign_on_status"]).and_then(cast::<bool>) {
        cfg.auto_assign_on_status = v;
    }
    if let Some(v) = get_path(&data, &["auto", "populate_members"]).and_then(cast::<bool>) {
        cfg.auto_populate_members = v;
    }
    if let Some(v) = get_path(&data, &["auto", "codeowners_assign"]).and_then(cast::<bool>) {
        cfg.auto_codeowners_assign = v;
    }
    if let Some(v) = get_path(&data, &["auto", "tags_from_path"]).and_then(cast::<bool>) {
        cfg.auto_tags_from_path = v;
    }
    if let Some(v) = get_path(&data, &["auto", "branch_infer_type"]).and_then(cast::<bool>) {
        cfg.auto_branch_infer_type = v;
    }
    if let Some(v) = get_path(&data, &["auto", "branch_infer_status"]).and_then(cast::<bool>) {
        cfg.auto_branch_infer_status = v;
    }
    if let Some(v) = get_path(&data, &["auto", "branch_infer_priority"]).and_then(cast::<bool>) {
        cfg.auto_branch_infer_priority = v;
    }

    // branch.* alias maps (global)
    if let Some(v) = get_path(&data, &["branch", "type_aliases"]).cloned()
        && let Some(map) = parse_alias_map_tolerant::<TaskType>(v, parse_task_type_tolerant)
    {
        cfg.branch_type_aliases = map;
    }
    if let Some(v) = get_path(&data, &["branch", "status_aliases"]).cloned()
        && let Some(map) = parse_alias_map_tolerant::<TaskStatus>(v, parse_task_status_tolerant)
    {
        cfg.branch_status_aliases = map;
    }
    if let Some(v) = get_path(&data, &["branch", "priority_aliases"]).cloned()
        && let Some(map) = parse_alias_map_tolerant::<Priority>(v, parse_priority_tolerant)
    {
        cfg.branch_priority_aliases = map;
    }

    if let Some(v) = get_path(&data, &["sync"]).cloned()
        && let Ok(sync) = serde_yaml::from_value::<SyncConfig>(v)
    {
        cfg.remotes.extend(sync.remotes);
        cfg.auth_profiles.extend(sync.auth_profiles);
    }
    if let Some(v) = get_path(&data, &["remotes"]).cloned()
        && let Ok(remotes) =
            serde_yaml::from_value::<std::collections::HashMap<String, SyncRemoteConfig>>(v)
    {
        cfg.remotes.extend(remotes);
    }
    if let Some(v) = get_path(&data, &["auth_profiles"]).cloned()
        && let Ok(profiles) =
            serde_yaml::from_value::<std::collections::HashMap<String, SyncAuthProfile>>(v)
    {
        cfg.auth_profiles.extend(profiles);
    }

    Ok(cfg)
}

/// Parse project config supporting nested/dotted keys
pub fn parse_project_from_yaml_str(
    project_name: &str,
    content: &str,
) -> Result<ProjectConfig, ConfigError> {
    let raw: Value = serde_yaml::from_str(content)
        .map_err(|e| ConfigError::ParseError(format!("Failed to parse project config: {}", e)))?;
    let data = expand_dotted_keys(raw);
    let mut cfg = ProjectConfig::new(project_name.to_string());

    // project.name (preferred) / project.id (legacy)
    if let Some(v) = get_path(&data, &["project", "name"]).and_then(cast::<String>) {
        if !v.trim().is_empty() {
            cfg.project_name = v;
        }
    } else if let Some(v) = get_path(&data, &["project", "id"]).and_then(cast::<String>) {
        cfg.project_name = v;
    } else if let Some(v) = get_path(&data, &["config", "project_name"]).and_then(cast::<String>) {
        if !v.trim().is_empty() {
            cfg.project_name = v;
        }
    } else if let Some(v) = get_path(&data, &["project_name"]).and_then(cast::<String>)
        && !v.trim().is_empty()
    {
        cfg.project_name = v;
    }
    // default.*
    if let Some(v) = get_path(&data, &["default", "reporter"]).and_then(cast::<String>) {
        cfg.default_reporter = Some(v);
    }
    if let Some(v) = get_path(&data, &["default", "tags"]).cloned() {
        cfg.default_tags = serde_yaml::from_value(v).ok();
    }
    if let Some(v) = get_path(&data, &["members"])
        .cloned()
        .or_else(|| get_path(&data, &["default", "members"]).cloned())
        && let Ok(list) = serde_yaml::from_value::<Vec<String>>(v)
    {
        let members: Vec<String> = list
            .into_iter()
            .map(|entry| entry.trim().to_string())
            .filter(|entry| !entry.is_empty())
            .collect();
        cfg.members = Some(members);
    }
    if let Some(v) = get_path(&data, &["default", "strict_members"]).and_then(cast::<bool>) {
        cfg.strict_members = Some(v);
    }
    if let Some(v) = get_path(&data, &["default", "assignee"]).and_then(cast::<String>) {
        cfg.default_assignee = Some(v);
    }
    if let Some(v) = get_path(&data, &["default", "priority"]).cloned() {
        cfg.default_priority = serde_yaml::from_value(v).ok();
    }
    if let Some(v) = get_path(&data, &["default", "status"]).cloned() {
        cfg.default_status = serde_yaml::from_value(v).ok();
    }
    if let Some(v) = get_path(&data, &["default", "tags"]).cloned() {
        cfg.default_tags = serde_yaml::from_value(v).ok();
    }
    // auto.*
    if let Some(v) = get_path(&data, &["auto", "populate_members"]).and_then(cast::<bool>) {
        cfg.auto_populate_members = Some(v);
    }
    if let Some(v) = get_path(&data, &["auto", "set_reporter"]).and_then(cast::<bool>) {
        cfg.auto_set_reporter = Some(v);
    }
    if let Some(v) = get_path(&data, &["auto", "assign_on_status"]).and_then(cast::<bool>) {
        cfg.auto_assign_on_status = Some(v);
    }
    if let Some(v) = get_path(&data, &["auto", "codeowners_assign"]).and_then(cast::<bool>) {
        cfg.auto_codeowners_assign = Some(v);
    }
    if let Some(v) = get_path(&data, &["auto", "tags_from_path"]).and_then(cast::<bool>) {
        cfg.auto_tags_from_path = Some(v);
    }
    if let Some(v) = get_path(&data, &["auto", "branch_infer_type"]).and_then(cast::<bool>) {
        cfg.auto_branch_infer_type = Some(v);
    }
    if let Some(v) = get_path(&data, &["auto", "branch_infer_status"]).and_then(cast::<bool>) {
        cfg.auto_branch_infer_status = Some(v);
    }
    if let Some(v) = get_path(&data, &["auto", "branch_infer_priority"]).and_then(cast::<bool>) {
        cfg.auto_branch_infer_priority = Some(v);
    }
    if let Some(v) = get_path(&data, &["auto", "identity"]).and_then(cast::<bool>) {
        cfg.auto_identity = Some(v);
    }
    if let Some(v) = get_path(&data, &["auto", "identity_git"]).and_then(cast::<bool>) {
        cfg.auto_identity_git = Some(v);
    }
    // issue.*
    if let Some(v) = get_path(&data, &["issue", "states"]).cloned() {
        cfg.issue_states = parse_issue_states_tolerant(v);
    }
    if let Some(v) = get_path(&data, &["issue", "types"]).cloned() {
        cfg.issue_types = parse_issue_types_tolerant(v);
    }
    if let Some(v) = get_path(&data, &["issue", "priorities"]).cloned() {
        cfg.issue_priorities = serde_yaml::from_value(v).ok();
    }
    // taxonomy.* (legacy)
    if let Some(v) = get_path(&data, &["taxonomy", "tags"]).cloned()
        && let Ok(list) = serde_yaml::from_value(v)
    {
        cfg.tags = Some(StringConfigField { values: list });
    }
    // issue.tags (preferred)
    if let Some(v) = get_path(&data, &["issue", "tags"]).cloned()
        && let Ok(list) = serde_yaml::from_value(v)
    {
        cfg.tags = Some(StringConfigField { values: list });
    }
    if let Some(v) = get_path(&data, &["custom", "fields"]).cloned()
        && let Ok(list) = serde_yaml::from_value(v)
    {
        cfg.custom_fields = Some(StringConfigField { values: list });
    }
    // scan.signal_words
    if let Some(v) = get_path(&data, &["scan", "signal_words"]).cloned() {
        cfg.scan_signal_words = serde_yaml::from_value(v).ok();
    }
    // scan.ticket_patterns
    if let Some(v) = get_path(&data, &["scan", "ticket_patterns"]).cloned() {
        cfg.scan_ticket_patterns = serde_yaml::from_value(v).ok();
    }
    // scan.enable_ticket_words (project)
    if let Some(v) = get_path(&data, &["scan", "enable_ticket_words"]).and_then(cast::<bool>) {
        cfg.scan_enable_ticket_words = Some(v);
    }
    // scan.enable_mentions (project)
    if let Some(v) = get_path(&data, &["scan", "enable_mentions"]).and_then(cast::<bool>) {
        cfg.scan_enable_mentions = Some(v);
    }
    // scan.strip_attributes (project)
    if let Some(v) = get_path(&data, &["scan", "strip_attributes"]).and_then(cast::<bool>) {
        cfg.scan_strip_attributes = Some(v);
    }

    // Back-compat: support flat keys for project toggles
    if let Some(v) = get_path(&data, &["scan_enable_ticket_words"]).and_then(cast::<bool>) {
        cfg.scan_enable_ticket_words = Some(v);
    }
    if let Some(v) = get_path(&data, &["scan_enable_mentions"]).and_then(cast::<bool>) {
        cfg.scan_enable_mentions = Some(v);
    }

    // attachments.dir (project override)
    if let Some(v) = get_path(&data, &["attachments", "dir"]).and_then(cast::<String>) {
        let trimmed = v.trim().to_string();
        if !trimmed.is_empty() {
            cfg.attachments_dir = Some(trimmed);
        }
    }
    // attachments.max_upload_mb (project override)
    if let Some(v) = get_path(&data, &["attachments", "max_upload_mb"]).and_then(cast::<i64>) {
        cfg.attachments_max_upload_mb = Some(v);
    }

    // sync.reports_dir (project override)
    if let Some(v) = get_path(&data, &["sync", "reports_dir"]).and_then(cast::<String>) {
        let trimmed = v.trim().to_string();
        if !trimmed.is_empty() {
            cfg.sync_reports_dir = Some(trimmed);
        }
    }

    // sync.write_reports (project override)
    if let Some(v) = get_path(&data, &["sync", "write_reports"]).and_then(cast::<bool>) {
        cfg.sync_write_reports = Some(v);
    }

    // branch alias maps (project)
    if let Some(v) = get_path(&data, &["branch", "type_aliases"]).cloned() {
        cfg.branch_type_aliases = parse_alias_map_tolerant::<TaskType>(v, parse_task_type_tolerant);
    }
    if let Some(v) = get_path(&data, &["branch", "status_aliases"]).cloned() {
        cfg.branch_status_aliases =
            parse_alias_map_tolerant::<TaskStatus>(v, parse_task_status_tolerant);
    }
    if let Some(v) = get_path(&data, &["branch", "priority_aliases"]).cloned() {
        cfg.branch_priority_aliases =
            parse_alias_map_tolerant::<Priority>(v, parse_priority_tolerant);
    }

    if let Some(v) = get_path(&data, &["sync"]).cloned()
        && let Ok(sync) = serde_yaml::from_value::<SyncConfig>(v)
    {
        cfg.remotes.extend(sync.remotes);
        cfg.auth_profiles.extend(sync.auth_profiles);
    }
    if let Some(v) = get_path(&data, &["remotes"]).cloned()
        && let Ok(remotes) =
            serde_yaml::from_value::<std::collections::HashMap<String, SyncRemoteConfig>>(v)
    {
        cfg.remotes.extend(remotes);
    }
    if let Some(v) = get_path(&data, &["auth_profiles"]).cloned()
        && let Ok(profiles) =
            serde_yaml::from_value::<std::collections::HashMap<String, SyncAuthProfile>>(v)
    {
        cfg.auth_profiles.extend(profiles);
    }

    Ok(cfg)
}

/// Render GlobalConfig into canonical nested YAML form
pub fn to_canonical_global_yaml(cfg: &GlobalConfig) -> String {
    use serde_yaml::Value as Y;
    let defaults = GlobalConfig::default();
    let mut root = serde_yaml::Mapping::new();

    // server
    if cfg.server_port != defaults.server_port {
        let mut server = serde_yaml::Mapping::new();
        server.insert(Y::String("port".into()), Y::Number(cfg.server_port.into()));
        root.insert(Y::String("server".into()), Y::Mapping(server));
    }

    // default
    let mut default = serde_yaml::Mapping::new();
    if cfg.default_project != defaults.default_project && !cfg.default_project.is_empty() {
        default.insert(
            Y::String("project".into()),
            Y::String(cfg.default_project.clone()),
        );
    }
    if let Some(v) = &cfg.default_assignee {
        default.insert(Y::String("assignee".into()), Y::String(v.clone()));
    }
    if let Some(v) = &cfg.default_reporter {
        default.insert(Y::String("reporter".into()), Y::String(v.clone()));
    }
    if cfg.strict_members {
        default.insert(Y::String("strict_members".into()), Y::Bool(true));
    }
    if !cfg.default_tags.is_empty() {
        default.insert(
            Y::String("tags".into()),
            serde_yaml::to_value(&cfg.default_tags).unwrap_or(Y::Null),
        );
    }
    if cfg.default_priority != defaults.default_priority {
        default.insert(
            Y::String("priority".into()),
            serde_yaml::to_value(&cfg.default_priority).unwrap_or(Y::Null),
        );
    }
    if let Some(v) = &cfg.default_status {
        default.insert(
            Y::String("status".into()),
            serde_yaml::to_value(v).unwrap_or(Y::Null),
        );
    }
    if !default.is_empty() {
        root.insert(Y::String("default".into()), Y::Mapping(default));
    }

    if !cfg.members.is_empty() {
        root.insert(
            Y::String("members".into()),
            serde_yaml::to_value(&cfg.members).unwrap_or(Y::Null),
        );
    }

    // issue
    let mut issue = serde_yaml::Mapping::new();
    if cfg.issue_states.values != defaults.issue_states.values {
        issue.insert(
            Y::String("states".into()),
            serde_yaml::to_value(&cfg.issue_states.values).unwrap_or(Y::Null),
        );
    }
    if cfg.issue_types.values != defaults.issue_types.values {
        issue.insert(
            Y::String("types".into()),
            serde_yaml::to_value(&cfg.issue_types.values).unwrap_or(Y::Null),
        );
    }
    if cfg.issue_priorities.values != defaults.issue_priorities.values {
        issue.insert(
            Y::String("priorities".into()),
            serde_yaml::to_value(&cfg.issue_priorities.values).unwrap_or(Y::Null),
        );
    }
    if cfg.tags.values != defaults.tags.values {
        issue.insert(
            Y::String("tags".into()),
            serde_yaml::to_value(&cfg.tags.values).unwrap_or(Y::Null),
        );
    }
    if !issue.is_empty() {
        root.insert(Y::String("issue".into()), Y::Mapping(issue));
    }

    // custom
    let mut custom = serde_yaml::Mapping::new();
    if cfg.custom_fields.values != defaults.custom_fields.values {
        custom.insert(
            Y::String("fields".into()),
            serde_yaml::to_value(&cfg.custom_fields.values).unwrap_or(Y::Null),
        );
    }
    if !custom.is_empty() {
        root.insert(Y::String("custom".into()), Y::Mapping(custom));
    }

    // scan
    let mut scan = serde_yaml::Mapping::new();
    if cfg.scan_signal_words != defaults.scan_signal_words {
        scan.insert(
            Y::String("signal_words".into()),
            serde_yaml::to_value(&cfg.scan_signal_words).unwrap_or(Y::Null),
        );
    }
    if let Some(patterns) = crate::config::types::maybe_scan_ticket_patterns(cfg) {
        scan.insert(
            Y::String("ticket_patterns".into()),
            serde_yaml::to_value(patterns).unwrap_or(Y::Null),
        );
    }
    if cfg.scan_enable_ticket_words != defaults.scan_enable_ticket_words {
        scan.insert(
            Y::String("enable_ticket_words".into()),
            Y::Bool(cfg.scan_enable_ticket_words),
        );
    }
    if cfg.scan_enable_mentions != defaults.scan_enable_mentions {
        scan.insert(
            Y::String("enable_mentions".into()),
            Y::Bool(cfg.scan_enable_mentions),
        );
    }
    // include scan.strip_attributes only if false to avoid redundant true defaults
    if !cfg.scan_strip_attributes {
        scan.insert(
            Y::String("strip_attributes".into()),
            Y::Bool(cfg.scan_strip_attributes),
        );
    }
    if !scan.is_empty() {
        root.insert(Y::String("scan".into()), Y::Mapping(scan));
    }

    // attachments
    if (cfg.attachments_dir != defaults.attachments_dir && !cfg.attachments_dir.is_empty())
        || cfg.attachments_max_upload_mb != defaults.attachments_max_upload_mb
    {
        let mut attachments = serde_yaml::Mapping::new();
        if cfg.attachments_dir != defaults.attachments_dir && !cfg.attachments_dir.is_empty() {
            attachments.insert(
                Y::String("dir".into()),
                Y::String(cfg.attachments_dir.clone()),
            );
        }
        if cfg.attachments_max_upload_mb != defaults.attachments_max_upload_mb {
            attachments.insert(
                Y::String("max_upload_mb".into()),
                Y::Number(cfg.attachments_max_upload_mb.into()),
            );
        }
        if !attachments.is_empty() {
            root.insert(Y::String("attachments".into()), Y::Mapping(attachments));
        }
    }

    // sync reports
    let mut sync = serde_yaml::Mapping::new();
    if cfg.sync_reports_dir != defaults.sync_reports_dir && !cfg.sync_reports_dir.is_empty() {
        sync.insert(
            Y::String("reports_dir".into()),
            Y::String(cfg.sync_reports_dir.clone()),
        );
    }
    if cfg.sync_write_reports != defaults.sync_write_reports {
        sync.insert(
            Y::String("write_reports".into()),
            Y::Bool(cfg.sync_write_reports),
        );
    }
    if !sync.is_empty() {
        root.insert(Y::String("sync".into()), Y::Mapping(sync));
    }

    // sprints
    let mut sprints = serde_yaml::Mapping::new();
    let mut sprint_defaults = serde_yaml::Mapping::new();
    if let Some(points) = cfg.sprints.defaults.capacity_points {
        sprint_defaults.insert(
            Y::String("capacity_points".into()),
            Y::Number(points.into()),
        );
    }
    if let Some(hours) = cfg.sprints.defaults.capacity_hours {
        sprint_defaults.insert(Y::String("capacity_hours".into()), Y::Number(hours.into()));
    }
    if let Some(length) = &cfg.sprints.defaults.length {
        sprint_defaults.insert(Y::String("length".into()), Y::String(length.clone()));
    }
    if let Some(overdue) = &cfg.sprints.defaults.overdue_after {
        sprint_defaults.insert(
            Y::String("overdue_after".into()),
            Y::String(overdue.clone()),
        );
    }
    if !sprint_defaults.is_empty() {
        sprints.insert(Y::String("defaults".into()), Y::Mapping(sprint_defaults));
    }

    if cfg.sprints.notifications.enabled != defaults.sprints.notifications.enabled {
        let mut notifications = serde_yaml::Mapping::new();
        notifications.insert(
            Y::String("enabled".into()),
            Y::Bool(cfg.sprints.notifications.enabled),
        );
        sprints.insert(Y::String("notifications".into()), Y::Mapping(notifications));
    }

    if !sprints.is_empty() {
        root.insert(Y::String("sprints".into()), Y::Mapping(sprints));
    }

    // auto
    let mut auto = serde_yaml::Mapping::new();
    if cfg.auto_identity != defaults.auto_identity {
        auto.insert(Y::String("identity".into()), Y::Bool(cfg.auto_identity));
    }
    if cfg.auto_identity_git != defaults.auto_identity_git {
        auto.insert(
            Y::String("identity_git".into()),
            Y::Bool(cfg.auto_identity_git),
        );
    }
    if cfg.auto_set_reporter != defaults.auto_set_reporter {
        auto.insert(
            Y::String("set_reporter".into()),
            Y::Bool(cfg.auto_set_reporter),
        );
    }
    if cfg.auto_assign_on_status != defaults.auto_assign_on_status {
        auto.insert(
            Y::String("assign_on_status".into()),
            Y::Bool(cfg.auto_assign_on_status),
        );
    }
    if cfg.auto_populate_members != defaults.auto_populate_members {
        auto.insert(
            Y::String("populate_members".into()),
            Y::Bool(cfg.auto_populate_members),
        );
    }
    if cfg.auto_codeowners_assign != defaults.auto_codeowners_assign {
        auto.insert(
            Y::String("codeowners_assign".into()),
            Y::Bool(cfg.auto_codeowners_assign),
        );
    }
    if cfg.auto_tags_from_path != defaults.auto_tags_from_path {
        auto.insert(
            Y::String("tags_from_path".into()),
            Y::Bool(cfg.auto_tags_from_path),
        );
    }
    if cfg.auto_branch_infer_type != defaults.auto_branch_infer_type {
        auto.insert(
            Y::String("branch_infer_type".into()),
            Y::Bool(cfg.auto_branch_infer_type),
        );
    }
    if cfg.auto_branch_infer_status != defaults.auto_branch_infer_status {
        auto.insert(
            Y::String("branch_infer_status".into()),
            Y::Bool(cfg.auto_branch_infer_status),
        );
    }
    if cfg.auto_branch_infer_priority != defaults.auto_branch_infer_priority {
        auto.insert(
            Y::String("branch_infer_priority".into()),
            Y::Bool(cfg.auto_branch_infer_priority),
        );
    }
    if !auto.is_empty() {
        root.insert(Y::String("auto".into()), Y::Mapping(auto));
    }

    // branch alias maps (canonical)
    if !cfg.branch_type_aliases.is_empty()
        || !cfg.branch_status_aliases.is_empty()
        || !cfg.branch_priority_aliases.is_empty()
    {
        let mut branch = serde_yaml::Mapping::new();
        if !cfg.branch_type_aliases.is_empty() {
            branch.insert(
                Y::String("type_aliases".into()),
                serde_yaml::to_value(&cfg.branch_type_aliases).unwrap_or(Y::Null),
            );
        }
        if !cfg.branch_status_aliases.is_empty() {
            branch.insert(
                Y::String("status_aliases".into()),
                serde_yaml::to_value(&cfg.branch_status_aliases).unwrap_or(Y::Null),
            );
        }
        if !cfg.branch_priority_aliases.is_empty() {
            branch.insert(
                Y::String("priority_aliases".into()),
                serde_yaml::to_value(&cfg.branch_priority_aliases).unwrap_or(Y::Null),
            );
        }
        root.insert(Y::String("branch".into()), Y::Mapping(branch));
    }

    if !cfg.remotes.is_empty() {
        root.insert(
            Y::String("remotes".into()),
            serde_yaml::to_value(&cfg.remotes).unwrap_or(Y::Null),
        );
    }
    if !cfg.auth_profiles.is_empty() {
        root.insert(
            Y::String("auth_profiles".into()),
            serde_yaml::to_value(&cfg.auth_profiles).unwrap_or(Y::Null),
        );
    }

    if root.is_empty() {
        return "# Global configuration uses built-in defaults.\n# See docs/help/config.md for available settings.\n"
            .to_string();
    }

    serde_yaml::to_string(&Y::Mapping(root)).unwrap_or_else(|_| "".to_string())
}

/// Render ProjectConfig into canonical nested YAML form
pub fn to_canonical_project_yaml(cfg: &ProjectConfig) -> String {
    use serde_yaml::Value as Y;
    let mut root = serde_yaml::Mapping::new();

    // project
    if !cfg.project_name.trim().is_empty() {
        let mut project = serde_yaml::Mapping::new();
        project.insert(
            Y::String("name".into()),
            Y::String(cfg.project_name.clone()),
        );
        root.insert(Y::String("project".into()), Y::Mapping(project));
    }

    // default
    let mut default = serde_yaml::Mapping::new();
    if let Some(v) = &cfg.default_assignee {
        default.insert(Y::String("assignee".into()), Y::String(v.clone()));
    }
    if let Some(v) = &cfg.default_reporter {
        default.insert(Y::String("reporter".into()), Y::String(v.clone()));
    }
    if let Some(strict) = cfg.strict_members {
        default.insert(Y::String("strict_members".into()), Y::Bool(strict));
    }
    if let Some(tags) = &cfg.default_tags
        && !tags.is_empty()
    {
        default.insert(
            Y::String("tags".into()),
            serde_yaml::to_value(tags).unwrap_or(Y::Null),
        );
    }
    if let Some(v) = &cfg.default_priority {
        default.insert(
            Y::String("priority".into()),
            serde_yaml::to_value(v).unwrap_or(Y::Null),
        );
    }
    if let Some(v) = &cfg.default_status {
        default.insert(
            Y::String("status".into()),
            serde_yaml::to_value(v).unwrap_or(Y::Null),
        );
    }
    if !default.is_empty() {
        root.insert(Y::String("default".into()), Y::Mapping(default));
    }

    if let Some(members) = &cfg.members {
        root.insert(
            Y::String("members".into()),
            serde_yaml::to_value(members).unwrap_or(Y::Null),
        );
    }

    let mut auto = serde_yaml::Mapping::new();
    if let Some(v) = cfg.auto_populate_members {
        auto.insert(Y::String("populate_members".into()), Y::Bool(v));
    }
    if let Some(v) = cfg.auto_set_reporter {
        auto.insert(Y::String("set_reporter".into()), Y::Bool(v));
    }
    if let Some(v) = cfg.auto_assign_on_status {
        auto.insert(Y::String("assign_on_status".into()), Y::Bool(v));
    }
    if let Some(v) = cfg.auto_codeowners_assign {
        auto.insert(Y::String("codeowners_assign".into()), Y::Bool(v));
    }
    if let Some(v) = cfg.auto_tags_from_path {
        auto.insert(Y::String("tags_from_path".into()), Y::Bool(v));
    }
    if let Some(v) = cfg.auto_branch_infer_type {
        auto.insert(Y::String("branch_infer_type".into()), Y::Bool(v));
    }
    if let Some(v) = cfg.auto_branch_infer_status {
        auto.insert(Y::String("branch_infer_status".into()), Y::Bool(v));
    }
    if let Some(v) = cfg.auto_branch_infer_priority {
        auto.insert(Y::String("branch_infer_priority".into()), Y::Bool(v));
    }
    if let Some(v) = cfg.auto_identity {
        auto.insert(Y::String("identity".into()), Y::Bool(v));
    }
    if let Some(v) = cfg.auto_identity_git {
        auto.insert(Y::String("identity_git".into()), Y::Bool(v));
    }
    if !auto.is_empty() {
        root.insert(Y::String("auto".into()), Y::Mapping(auto));
    }

    // issue
    let mut issue = serde_yaml::Mapping::new();
    if let Some(v) = &cfg.issue_states {
        let vals: Vec<Y> = v
            .values
            .iter()
            .map(|s| Y::String(s.as_str().to_string()))
            .collect();
        issue.insert(Y::String("states".into()), Y::Sequence(vals));
    }
    if let Some(v) = &cfg.issue_types {
        let vals: Vec<Y> = v
            .values
            .iter()
            .map(|t| Y::String(t.as_str().to_string()))
            .collect();
        issue.insert(Y::String("types".into()), Y::Sequence(vals));
    }
    if let Some(v) = &cfg.issue_priorities {
        issue.insert(
            Y::String("priorities".into()),
            serde_yaml::to_value(&v.values).unwrap_or(Y::Null),
        );
    }
    if !issue.is_empty() {
        root.insert(Y::String("issue".into()), Y::Mapping(issue));
    }

    // tags under issue.* in canonical project YAML
    if let Some(v) = &cfg.tags {
        if let Some(issue_map) = root
            .get_mut(Y::String("issue".into()))
            .and_then(|v| v.as_mapping_mut())
        {
            issue_map.insert(
                Y::String("tags".into()),
                serde_yaml::to_value(&v.values).unwrap_or(Y::Null),
            );
        } else {
            let mut im = serde_yaml::Mapping::new();
            im.insert(
                Y::String("tags".into()),
                serde_yaml::to_value(&v.values).unwrap_or(Y::Null),
            );
            root.insert(Y::String("issue".into()), Y::Mapping(im));
        }
    }

    if let Some(fields) = &cfg.custom_fields {
        let mut custom = serde_yaml::Mapping::new();
        custom.insert(
            Y::String("fields".into()),
            serde_yaml::to_value(&fields.values).unwrap_or(Y::Null),
        );
        root.insert(Y::String("custom".into()), Y::Mapping(custom));
    }

    // scan
    let mut scan = serde_yaml::Mapping::new();
    if let Some(v) = &cfg.scan_signal_words {
        scan.insert(
            Y::String("signal_words".into()),
            serde_yaml::to_value(v).unwrap_or(Y::Null),
        );
    }
    if let Some(patterns) = crate::config::types::maybe_project_scan_ticket_patterns(cfg) {
        scan.insert(
            Y::String("ticket_patterns".into()),
            serde_yaml::to_value(patterns).unwrap_or(Y::Null),
        );
    }
    if let Some(enabled) = cfg.scan_enable_ticket_words {
        scan.insert(Y::String("enable_ticket_words".into()), Y::Bool(enabled));
    }
    if let Some(enabled) = cfg.scan_enable_mentions {
        scan.insert(Y::String("enable_mentions".into()), Y::Bool(enabled));
    }
    if let Some(b) = &cfg.scan_strip_attributes {
        scan.insert(Y::String("strip_attributes".into()), Y::Bool(*b));
    }
    if !scan.is_empty() {
        root.insert(Y::String("scan".into()), Y::Mapping(scan));
    }

    // attachments
    let has_attachments = cfg
        .attachments_dir
        .as_ref()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
        || cfg.attachments_max_upload_mb.is_some();
    if has_attachments {
        let mut attachments = serde_yaml::Mapping::new();
        if let Some(dir) = &cfg.attachments_dir
            && !dir.trim().is_empty()
        {
            attachments.insert(Y::String("dir".into()), Y::String(dir.clone()));
        }
        if let Some(max_mb) = cfg.attachments_max_upload_mb {
            attachments.insert(Y::String("max_upload_mb".into()), Y::Number(max_mb.into()));
        }
        if !attachments.is_empty() {
            root.insert(Y::String("attachments".into()), Y::Mapping(attachments));
        }
    }

    // sync reports
    let has_sync_reports = cfg
        .sync_reports_dir
        .as_ref()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
        || cfg.sync_write_reports.is_some();
    if has_sync_reports {
        let mut sync = serde_yaml::Mapping::new();
        if let Some(dir) = &cfg.sync_reports_dir
            && !dir.trim().is_empty()
        {
            sync.insert(Y::String("reports_dir".into()), Y::String(dir.clone()));
        }
        if let Some(enabled) = cfg.sync_write_reports {
            sync.insert(Y::String("write_reports".into()), Y::Bool(enabled));
        }
        if !sync.is_empty() {
            root.insert(Y::String("sync".into()), Y::Mapping(sync));
        }
    }

    // branch alias maps in project canonical YAML
    let has_branch = cfg
        .branch_type_aliases
        .as_ref()
        .map(|m| !m.is_empty())
        .unwrap_or(false)
        || cfg
            .branch_status_aliases
            .as_ref()
            .map(|m| !m.is_empty())
            .unwrap_or(false)
        || cfg
            .branch_priority_aliases
            .as_ref()
            .map(|m| !m.is_empty())
            .unwrap_or(false);
    if has_branch {
        let mut branch = serde_yaml::Mapping::new();
        if let Some(m) = &cfg.branch_type_aliases
            && !m.is_empty()
        {
            branch.insert(
                Y::String("type_aliases".into()),
                serde_yaml::to_value(m).unwrap_or(Y::Null),
            );
        }
        if let Some(m) = &cfg.branch_status_aliases
            && !m.is_empty()
        {
            branch.insert(
                Y::String("status_aliases".into()),
                serde_yaml::to_value(m).unwrap_or(Y::Null),
            );
        }
        if let Some(m) = &cfg.branch_priority_aliases
            && !m.is_empty()
        {
            branch.insert(
                Y::String("priority_aliases".into()),
                serde_yaml::to_value(m).unwrap_or(Y::Null),
            );
        }
        root.insert(Y::String("branch".into()), Y::Mapping(branch));
    }

    if !cfg.remotes.is_empty() {
        root.insert(
            Y::String("remotes".into()),
            serde_yaml::to_value(&cfg.remotes).unwrap_or(Y::Null),
        );
    }
    if !cfg.auth_profiles.is_empty() {
        root.insert(
            Y::String("auth_profiles".into()),
            serde_yaml::to_value(&cfg.auth_profiles).unwrap_or(Y::Null),
        );
    }

    serde_yaml::to_string(&Y::Mapping(root)).unwrap_or_else(|_| "".to_string())
}

// Helper: tolerant parser for issue.types accepting mixed-case strings mapping to TaskType
fn parse_issue_types_tolerant(
    v: serde_yaml::Value,
) -> Option<crate::config::types::ConfigurableField<crate::types::TaskType>> {
    use crate::config::types::ConfigurableField;
    use crate::types::TaskType;
    use std::str::FromStr;

    if let Ok(cf) = serde_yaml::from_value::<ConfigurableField<TaskType>>(v.clone()) {
        return Some(cf);
    }
    if let Ok(list) = serde_yaml::from_value::<Vec<String>>(v.clone()) {
        let mut out: Vec<TaskType> = Vec::new();
        for s in list {
            if let Ok(tt) = TaskType::from_str(&s) {
                out.push(tt);
            }
        }
        return Some(ConfigurableField { values: out });
    }
    None
}
