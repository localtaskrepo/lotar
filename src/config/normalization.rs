use serde::de::DeserializeOwned;
use serde_yaml_ng::Value;

use crate::config::types::{ConfigError, GlobalConfig, ProjectConfig};
use crate::types::TaskStatus;

fn expand_dotted_keys(value: Value) -> Result<Value, ConfigError> {
    match value {
        Value::Mapping(map) => {
            let mut root = Value::Mapping(serde_yaml_ng::Mapping::new());
            for (k, v) in map {
                let Value::String(key) = k else {
                    return Err(ConfigError::ParseError(
                        "Config keys must be strings".into(),
                    ));
                };
                let value = if matches!(
                    key.rsplit('.').next().unwrap_or_default(),
                    "auth_profiles"
                        | "remotes"
                        | "agents"
                        | "env"
                        | "type_aliases"
                        | "status_aliases"
                        | "priority_aliases"
                        | "branch_type_aliases"
                        | "branch_status_aliases"
                        | "branch_priority_aliases"
                ) {
                    // These maps contain user-defined names, not config paths.
                    v
                } else {
                    expand_dotted_keys(v)?
                };
                let nested = key.rsplit('.').fold(value, |value, part| {
                    let mut map = serde_yaml_ng::Mapping::new();
                    map.insert(Value::String(part.into()), value);
                    Value::Mapping(map)
                });
                root = merge_values(root, nested)?;
            }
            Ok(root)
        }
        Value::Sequence(seq) => seq
            .into_iter()
            .map(expand_dotted_keys)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Sequence),
        other => Ok(other),
    }
}

fn merge_values(a: Value, b: Value) -> Result<Value, ConfigError> {
    match (a, b) {
        (Value::Mapping(mut m1), Value::Mapping(m2)) => {
            for (k, v2) in m2 {
                if let Some(v1) = m1.get(&k).cloned() {
                    m1.insert(k, merge_values(v1, v2)?);
                } else {
                    m1.insert(k, v2);
                }
            }
            Ok(Value::Mapping(m1))
        }
        (a, b) if a == b => Ok(b),
        // Do not silently discard a malformed or conflicting representation.
        _ => Err(ConfigError::ParseError(
            "Conflicting dotted and nested config keys".into(),
        )),
    }
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

pub fn parse_issue_states_tolerant(
    v: Value,
) -> Option<crate::config::types::ConfigurableField<TaskStatus>> {
    // Try strict first
    if let Ok(cf) =
        serde_yaml_ng::from_value::<crate::config::types::ConfigurableField<TaskStatus>>(v.clone())
    {
        return Some(cf);
    }
    // Fallback vector of strings
    if let Ok(list) = serde_yaml_ng::from_value::<Vec<String>>(v) {
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

// Aliases are applied from legacy to canonical. Flat fields are the typed
// schema itself, so newly shipped fields cannot silently disappear here.
const CONFIG_PATHS: &[(&str, &str)] = &[
    ("config.project_name", "project_name"),
    ("project.id", "project_name"),
    ("project.name", "project_name"),
    ("server.port", "server_port"),
    ("default.project", "default_project"),
    ("default.assignee", "default_assignee"),
    ("default.reporter", "default_reporter"),
    ("default.tags", "default_tags"),
    ("default.members", "members"),
    ("members", "members"),
    ("default.strict_members", "strict_members"),
    ("default.priority", "default_priority"),
    ("default.status", "default_status"),
    ("taxonomy.tags", "tags"),
    ("issue.tags", "tags"),
    ("issue.states", "issue_states"),
    ("issue.types", "issue_types"),
    ("issue.priorities", "issue_priorities"),
    ("custom.fields", "custom_fields"),
    ("scan.signal_words", "scan_signal_words"),
    ("scan.ticket_patterns", "scan_ticket_patterns"),
    ("scan.enable_ticket_words", "scan_enable_ticket_words"),
    ("scan.enable_mentions", "scan_enable_mentions"),
    ("scan.strip_attributes", "scan_strip_attributes"),
    ("auto.identity", "auto_identity"),
    ("auto.identity_git", "auto_identity_git"),
    ("auto.populate_members", "auto_populate_members"),
    ("auto.set_reporter", "auto_set_reporter"),
    ("auto.assign_on_status", "auto_assign_on_status"),
    ("auto.codeowners_assign", "auto_codeowners_assign"),
    ("auto.tags_from_path", "auto_tags_from_path"),
    ("auto.branch_infer_type", "auto_branch_infer_type"),
    ("auto.branch_infer_status", "auto_branch_infer_status"),
    ("auto.branch_infer_priority", "auto_branch_infer_priority"),
    ("branch.type_aliases", "branch_type_aliases"),
    ("branch.status_aliases", "branch_status_aliases"),
    ("branch.priority_aliases", "branch_priority_aliases"),
    ("attachments.dir", "attachments_dir"),
    ("attachments.max_upload_mb", "attachments_max_upload_mb"),
    ("sync.reports_dir", "sync_reports_dir"),
    ("sync.write_reports", "sync_write_reports"),
    ("agent.context_enabled", "agent_context_enabled"),
    ("agent.context_extension", "agent_context_extension"),
    ("agent.logs_dir", "agent_logs_dir"),
    ("agent.instructions", "agent_instructions"),
    ("agent.automation", "agent_automation"),
    ("agent.worktree", "agent_worktree"),
    ("sync.remotes", "remotes"),
    ("sync.auth_profiles", "auth_profiles"),
];

fn parse_config<T: DeserializeOwned>(content: &str, defaults: Value) -> Result<T, ConfigError> {
    let raw: Value = serde_yaml_ng::from_str(content)
        .map_err(|_| ConfigError::ParseError("Invalid config YAML".into()))?;
    let raw = if raw.is_null()
        && content.lines().all(|line| {
            let line = line.trim();
            line.is_empty() || line.starts_with('#') || line == "---" || line == "..."
        }) {
        Value::Mapping(serde_yaml_ng::Mapping::new())
    } else {
        raw
    };
    let data = expand_dotted_keys(raw)?;
    let input = data
        .as_mapping()
        .ok_or_else(|| ConfigError::ParseError("Config root must be a mapping".into()))?;
    let Value::Mapping(mut flat) = defaults else {
        return Err(ConfigError::ParseError(
            "Config defaults must be a mapping".into(),
        ));
    };
    flat.extend(input.clone());
    // Validate flat fields even when a canonical alias will override them.
    // Do not expose deserializer messages: they may include credential values.
    serde_yaml_ng::from_value::<T>(Value::Mapping(flat.clone()))
        .map_err(|_| ConfigError::ParseError("Invalid type in flat config fields".into()))?;
    for &(path, field) in CONFIG_PATHS {
        let mut value = &data;
        let mut present = true;
        for part in path.split('.') {
            let map = value.as_mapping().ok_or_else(|| {
                ConfigError::ParseError(format!("Config section for '{path}' must be a mapping"))
            })?;
            let Some(child) = map.get(Value::String(part.into())) else {
                present = false;
                break;
            };
            value = child;
        }
        if !present {
            continue;
        }
        let key = Value::String(field.into());
        let mut candidate = flat.clone();
        candidate.insert(key.clone(), value.clone());
        serde_yaml_ng::from_value::<T>(Value::Mapping(candidate)).map_err(|_| {
            ConfigError::ParseError(format!("Invalid type for config field '{path}'"))
        })?;
        let mut value = value.clone();
        if matches!(field, "remotes" | "auth_profiles") {
            // Root profile maps are canonical; keep disjoint legacy entries,
            // but replace whole profiles rather than merging credentials.
            if let (Value::Mapping(legacy), Some(Value::Mapping(root))) =
                (&mut value, data.get(field))
            {
                legacy.extend(root.clone());
            }
        }
        flat.insert(key, value);
    }
    for field in [
        "branch_type_aliases",
        "branch_status_aliases",
        "branch_priority_aliases",
    ] {
        if let Some(Value::Mapping(map)) = flat.get_mut(Value::String(field.into())) {
            *map = std::mem::take(map)
                .into_iter()
                .map(|(key, value)| {
                    (
                        match key {
                            Value::String(s) => Value::String(s.to_lowercase()),
                            other => other,
                        },
                        value,
                    )
                })
                .collect();
        }
    }
    serde_yaml_ng::from_value(Value::Mapping(flat))
        .map_err(|_| ConfigError::ParseError("Invalid config field types".into()))
}

/// Parse all shipped flat fields and canonical nested/dotted aliases.
pub fn parse_global_from_yaml_str(content: &str) -> Result<GlobalConfig, ConfigError> {
    parse_config(
        content,
        serde_yaml_ng::to_value(GlobalConfig::default())
            .map_err(|_| ConfigError::ParseError("Failed to serialize global defaults".into()))?,
    )
}

/// Parse project overrides with the same alias and validation rules as global config.
pub fn parse_project_from_yaml_str(
    project_name: &str,
    content: &str,
) -> Result<ProjectConfig, ConfigError> {
    parse_config(
        content,
        serde_yaml_ng::to_value(ProjectConfig::new(project_name.into()))
            .map_err(|_| ConfigError::ParseError("Failed to serialize project defaults".into()))?,
    )
}

/// Render GlobalConfig into canonical nested YAML form
pub fn to_canonical_global_yaml(cfg: &GlobalConfig) -> String {
    use serde_yaml_ng::Value as Y;
    let defaults = GlobalConfig::default();
    let mut root = serde_yaml_ng::Mapping::new();

    // server
    if cfg.server_port != defaults.server_port {
        let mut server = serde_yaml_ng::Mapping::new();
        server.insert(Y::String("port".into()), Y::Number(cfg.server_port.into()));
        root.insert(Y::String("server".into()), Y::Mapping(server));
    }
    if let Some(path) = &cfg.web_ui_path {
        root.insert(Y::String("web_ui_path".into()), Y::String(path.clone()));
    }

    // default
    let mut default = serde_yaml_ng::Mapping::new();
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
            serde_yaml_ng::to_value(&cfg.default_tags).unwrap_or(Y::Null),
        );
    }
    if cfg.default_priority != defaults.default_priority {
        default.insert(
            Y::String("priority".into()),
            serde_yaml_ng::to_value(&cfg.default_priority).unwrap_or(Y::Null),
        );
    }
    if let Some(v) = &cfg.default_status {
        default.insert(
            Y::String("status".into()),
            serde_yaml_ng::to_value(v).unwrap_or(Y::Null),
        );
    }
    if !default.is_empty() {
        root.insert(Y::String("default".into()), Y::Mapping(default));
    }

    if !cfg.members.is_empty() {
        root.insert(
            Y::String("members".into()),
            serde_yaml_ng::to_value(&cfg.members).unwrap_or(Y::Null),
        );
    }

    // issue
    let mut issue = serde_yaml_ng::Mapping::new();
    if cfg.issue_states.values != defaults.issue_states.values {
        issue.insert(
            Y::String("states".into()),
            serde_yaml_ng::to_value(&cfg.issue_states.values).unwrap_or(Y::Null),
        );
    }
    if cfg.issue_types.values != defaults.issue_types.values {
        issue.insert(
            Y::String("types".into()),
            serde_yaml_ng::to_value(&cfg.issue_types.values).unwrap_or(Y::Null),
        );
    }
    if cfg.issue_priorities.values != defaults.issue_priorities.values {
        issue.insert(
            Y::String("priorities".into()),
            serde_yaml_ng::to_value(&cfg.issue_priorities.values).unwrap_or(Y::Null),
        );
    }
    if cfg.tags.values != defaults.tags.values {
        issue.insert(
            Y::String("tags".into()),
            serde_yaml_ng::to_value(&cfg.tags.values).unwrap_or(Y::Null),
        );
    }
    if !issue.is_empty() {
        root.insert(Y::String("issue".into()), Y::Mapping(issue));
    }

    // custom
    let mut custom = serde_yaml_ng::Mapping::new();
    if cfg.custom_fields.values != defaults.custom_fields.values {
        custom.insert(
            Y::String("fields".into()),
            serde_yaml_ng::to_value(&cfg.custom_fields.values).unwrap_or(Y::Null),
        );
    }
    if !custom.is_empty() {
        root.insert(Y::String("custom".into()), Y::Mapping(custom));
    }

    // scan
    let mut scan = serde_yaml_ng::Mapping::new();
    if cfg.scan_signal_words != defaults.scan_signal_words {
        scan.insert(
            Y::String("signal_words".into()),
            serde_yaml_ng::to_value(&cfg.scan_signal_words).unwrap_or(Y::Null),
        );
    }
    if let Some(patterns) = crate::config::types::maybe_scan_ticket_patterns(cfg) {
        scan.insert(
            Y::String("ticket_patterns".into()),
            serde_yaml_ng::to_value(patterns).unwrap_or(Y::Null),
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
    if cfg.attachments_dir != defaults.attachments_dir
        || cfg.attachments_max_upload_mb != defaults.attachments_max_upload_mb
    {
        let mut attachments = serde_yaml_ng::Mapping::new();
        if cfg.attachments_dir != defaults.attachments_dir {
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
    let mut sync = serde_yaml_ng::Mapping::new();
    if cfg.sync_reports_dir != defaults.sync_reports_dir {
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

    // agent
    let mut agent = serde_yaml_ng::Mapping::new();
    if cfg.agent_context_extension != defaults.agent_context_extension {
        agent.insert(
            Y::String("context_extension".into()),
            Y::String(cfg.agent_context_extension.clone()),
        );
    }
    if let Some(dir) = &cfg.agent_logs_dir {
        agent.insert(Y::String("logs_dir".into()), Y::String(dir.clone()));
    }
    if cfg.agent_context_enabled != defaults.agent_context_enabled {
        agent.insert(
            Y::String("context_enabled".into()),
            Y::Bool(cfg.agent_context_enabled),
        );
    }
    if let Some(instructions) = &cfg.agent_instructions {
        agent.insert(
            Y::String("instructions".into()),
            serde_yaml_ng::to_value(instructions).unwrap_or(Y::Null),
        );
    }
    if cfg.agent_automation != defaults.agent_automation {
        agent.insert(
            Y::String("automation".into()),
            serde_yaml_ng::to_value(&cfg.agent_automation).unwrap_or(Y::Null),
        );
    }
    if cfg.agent_worktree != defaults.agent_worktree {
        agent.insert(
            Y::String("worktree".into()),
            serde_yaml_ng::to_value(&cfg.agent_worktree).unwrap_or(Y::Null),
        );
    }
    if !agent.is_empty() {
        root.insert(Y::String("agent".into()), Y::Mapping(agent));
    }
    if !cfg.agents.is_empty() {
        root.insert(
            Y::String("agents".into()),
            serde_yaml_ng::to_value(&cfg.agents).unwrap_or(Y::Null),
        );
    }

    // sprints
    let mut sprints = serde_yaml_ng::Mapping::new();
    let mut sprint_defaults = serde_yaml_ng::Mapping::new();
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
        let mut notifications = serde_yaml_ng::Mapping::new();
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
    let mut auto = serde_yaml_ng::Mapping::new();
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
        let mut branch = serde_yaml_ng::Mapping::new();
        if !cfg.branch_type_aliases.is_empty() {
            branch.insert(
                Y::String("type_aliases".into()),
                serde_yaml_ng::to_value(&cfg.branch_type_aliases).unwrap_or(Y::Null),
            );
        }
        if !cfg.branch_status_aliases.is_empty() {
            branch.insert(
                Y::String("status_aliases".into()),
                serde_yaml_ng::to_value(&cfg.branch_status_aliases).unwrap_or(Y::Null),
            );
        }
        if !cfg.branch_priority_aliases.is_empty() {
            branch.insert(
                Y::String("priority_aliases".into()),
                serde_yaml_ng::to_value(&cfg.branch_priority_aliases).unwrap_or(Y::Null),
            );
        }
        root.insert(Y::String("branch".into()), Y::Mapping(branch));
    }

    if !cfg.remotes.is_empty() {
        root.insert(
            Y::String("remotes".into()),
            serde_yaml_ng::to_value(&cfg.remotes).unwrap_or(Y::Null),
        );
    }
    if !cfg.auth_profiles.is_empty() {
        root.insert(
            Y::String("auth_profiles".into()),
            serde_yaml_ng::to_value(&cfg.auth_profiles).unwrap_or(Y::Null),
        );
    }

    if root.is_empty() {
        return "# Global configuration uses built-in defaults.\n# See docs/help/config.md for available settings.\n"
            .to_string();
    }

    serde_yaml_ng::to_string(&Y::Mapping(root)).unwrap_or_default()
}

/// Render ProjectConfig into canonical nested YAML form
pub fn to_canonical_project_yaml(cfg: &ProjectConfig) -> String {
    use serde_yaml_ng::Value as Y;
    let mut root = serde_yaml_ng::Mapping::new();

    // project
    if !cfg.project_name.trim().is_empty() {
        let mut project = serde_yaml_ng::Mapping::new();
        project.insert(
            Y::String("name".into()),
            Y::String(cfg.project_name.clone()),
        );
        root.insert(Y::String("project".into()), Y::Mapping(project));
    }

    // default
    let mut default = serde_yaml_ng::Mapping::new();
    if let Some(v) = &cfg.default_assignee {
        default.insert(Y::String("assignee".into()), Y::String(v.clone()));
    }
    if let Some(v) = &cfg.default_reporter {
        default.insert(Y::String("reporter".into()), Y::String(v.clone()));
    }
    if let Some(strict) = cfg.strict_members {
        default.insert(Y::String("strict_members".into()), Y::Bool(strict));
    }
    if let Some(tags) = &cfg.default_tags {
        default.insert(
            Y::String("tags".into()),
            serde_yaml_ng::to_value(tags).unwrap_or(Y::Null),
        );
    }
    if let Some(v) = &cfg.default_priority {
        default.insert(
            Y::String("priority".into()),
            serde_yaml_ng::to_value(v).unwrap_or(Y::Null),
        );
    }
    if let Some(v) = &cfg.default_status {
        default.insert(
            Y::String("status".into()),
            serde_yaml_ng::to_value(v).unwrap_or(Y::Null),
        );
    }
    if !default.is_empty() {
        root.insert(Y::String("default".into()), Y::Mapping(default));
    }

    if let Some(members) = &cfg.members {
        root.insert(
            Y::String("members".into()),
            serde_yaml_ng::to_value(members).unwrap_or(Y::Null),
        );
    }

    let mut auto = serde_yaml_ng::Mapping::new();
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
    let mut issue = serde_yaml_ng::Mapping::new();
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
            serde_yaml_ng::to_value(&v.values).unwrap_or(Y::Null),
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
                serde_yaml_ng::to_value(&v.values).unwrap_or(Y::Null),
            );
        } else {
            let mut im = serde_yaml_ng::Mapping::new();
            im.insert(
                Y::String("tags".into()),
                serde_yaml_ng::to_value(&v.values).unwrap_or(Y::Null),
            );
            root.insert(Y::String("issue".into()), Y::Mapping(im));
        }
    }

    if let Some(fields) = &cfg.custom_fields {
        let mut custom = serde_yaml_ng::Mapping::new();
        custom.insert(
            Y::String("fields".into()),
            serde_yaml_ng::to_value(&fields.values).unwrap_or(Y::Null),
        );
        root.insert(Y::String("custom".into()), Y::Mapping(custom));
    }

    // scan
    let mut scan = serde_yaml_ng::Mapping::new();
    if let Some(v) = &cfg.scan_signal_words {
        scan.insert(
            Y::String("signal_words".into()),
            serde_yaml_ng::to_value(v).unwrap_or(Y::Null),
        );
    }
    if let Some(patterns) = crate::config::types::maybe_project_scan_ticket_patterns(cfg) {
        scan.insert(
            Y::String("ticket_patterns".into()),
            serde_yaml_ng::to_value(patterns).unwrap_or(Y::Null),
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
    let has_attachments = cfg.attachments_dir.is_some() || cfg.attachments_max_upload_mb.is_some();
    if has_attachments {
        let mut attachments = serde_yaml_ng::Mapping::new();
        if let Some(dir) = &cfg.attachments_dir {
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
    let has_sync_reports = cfg.sync_reports_dir.is_some() || cfg.sync_write_reports.is_some();
    if has_sync_reports {
        let mut sync = serde_yaml_ng::Mapping::new();
        if let Some(dir) = &cfg.sync_reports_dir {
            sync.insert(Y::String("reports_dir".into()), Y::String(dir.clone()));
        }
        if let Some(enabled) = cfg.sync_write_reports {
            sync.insert(Y::String("write_reports".into()), Y::Bool(enabled));
        }
        if !sync.is_empty() {
            root.insert(Y::String("sync".into()), Y::Mapping(sync));
        }
    }

    // agent
    for (key, value) in [
        ("context_extension", &cfg.agent_context_extension),
        ("logs_dir", &cfg.agent_logs_dir),
    ] {
        if let Some(value) = value {
            let entry = root
                .entry(Y::String("agent".into()))
                .or_insert_with(|| Y::Mapping(serde_yaml_ng::Mapping::new()));
            if let Y::Mapping(map) = entry {
                map.insert(Y::String(key.into()), Y::String(value.clone()));
            }
        }
    }
    if let Some(enabled) = cfg.agent_context_enabled {
        let entry = root
            .entry(Y::String("agent".into()))
            .or_insert_with(|| Y::Mapping(serde_yaml_ng::Mapping::new()));
        if let Y::Mapping(map) = entry {
            map.insert(Y::String("context_enabled".into()), Y::Bool(enabled));
        }
    }
    if let Some(instructions) = &cfg.agent_instructions {
        let entry = root
            .entry(Y::String("agent".into()))
            .or_insert_with(|| Y::Mapping(serde_yaml_ng::Mapping::new()));
        if let Y::Mapping(map) = entry {
            map.insert(
                Y::String("instructions".into()),
                serde_yaml_ng::to_value(instructions).unwrap_or(Y::Null),
            );
        }
    }
    if let Some(automation) = &cfg.agent_automation {
        let entry = root
            .entry(Y::String("agent".into()))
            .or_insert_with(|| Y::Mapping(serde_yaml_ng::Mapping::new()));
        if let Y::Mapping(map) = entry {
            map.insert(
                Y::String("automation".into()),
                serde_yaml_ng::to_value(automation).unwrap_or(Y::Null),
            );
        }
    }
    if let Some(worktree) = &cfg.agent_worktree {
        let entry = root
            .entry(Y::String("agent".into()))
            .or_insert_with(|| Y::Mapping(serde_yaml_ng::Mapping::new()));
        if let Y::Mapping(map) = entry {
            map.insert(
                Y::String("worktree".into()),
                serde_yaml_ng::to_value(worktree).unwrap_or(Y::Null),
            );
        }
    }
    if let Some(profiles) = &cfg.agents {
        root.insert(
            Y::String("agents".into()),
            serde_yaml_ng::to_value(profiles).unwrap_or(Y::Null),
        );
    }

    // branch alias maps in project canonical YAML
    let has_branch = cfg.branch_type_aliases.is_some()
        || cfg.branch_status_aliases.is_some()
        || cfg.branch_priority_aliases.is_some();
    if has_branch {
        let mut branch = serde_yaml_ng::Mapping::new();
        if let Some(m) = &cfg.branch_type_aliases {
            branch.insert(
                Y::String("type_aliases".into()),
                serde_yaml_ng::to_value(m).unwrap_or(Y::Null),
            );
        }
        if let Some(m) = &cfg.branch_status_aliases {
            branch.insert(
                Y::String("status_aliases".into()),
                serde_yaml_ng::to_value(m).unwrap_or(Y::Null),
            );
        }
        if let Some(m) = &cfg.branch_priority_aliases {
            branch.insert(
                Y::String("priority_aliases".into()),
                serde_yaml_ng::to_value(m).unwrap_or(Y::Null),
            );
        }
        root.insert(Y::String("branch".into()), Y::Mapping(branch));
    }

    if !cfg.remotes.is_empty() {
        root.insert(
            Y::String("remotes".into()),
            serde_yaml_ng::to_value(&cfg.remotes).unwrap_or(Y::Null),
        );
    }
    if !cfg.auth_profiles.is_empty() {
        root.insert(
            Y::String("auth_profiles".into()),
            serde_yaml_ng::to_value(&cfg.auth_profiles).unwrap_or(Y::Null),
        );
    }

    serde_yaml_ng::to_string(&Y::Mapping(root)).unwrap_or_default()
}
