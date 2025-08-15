use serde::de::DeserializeOwned;
use serde_yaml::Value;

use crate::config::types::{ConfigError, GlobalConfig, ProjectConfig, StringConfigField};

fn expand_dotted_keys(value: Value) -> Value {
    match value {
        Value::Mapping(map) => {
            let mut root = serde_yaml::Mapping::new();
            for (k, v) in map {
                if let Value::String(key) = k {
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
        cfg.default_prefix = v;
    }
    if let Some(v) = get_path(&data, &["default", "assignee"]).and_then(cast::<String>) {
        cfg.default_assignee = Some(v);
    }
    if let Some(v) = get_path(&data, &["default", "reporter"]).and_then(cast::<String>) {
        cfg.default_reporter = Some(v);
    }
    if let Some(v) = get_path(&data, &["default", "category"]).and_then(cast::<String>) {
        cfg.default_category = Some(v);
    }
    if let Some(v) = get_path(&data, &["default", "tags"]).cloned() {
        if let Ok(list) = serde_yaml::from_value(v) {
            cfg.default_tags = list;
        }
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
        if let Ok(list) = serde_yaml::from_value(v) {
            cfg.issue_states.values = list;
        }
    }
    if let Some(v) = get_path(&data, &["issue", "types"]).cloned() {
        if let Ok(list) = serde_yaml::from_value(v) {
            cfg.issue_types.values = list;
        }
    }
    if let Some(v) = get_path(&data, &["issue", "priorities"]).cloned() {
        if let Ok(list) = serde_yaml::from_value(v) {
            cfg.issue_priorities.values = list;
        }
    }

    // taxonomy.* (legacy) — will be overridden by issue.* if present
    if let Some(v) = get_path(&data, &["taxonomy", "categories"]).cloned() {
        if let Ok(list) = serde_yaml::from_value(v) {
            cfg.categories = StringConfigField { values: list };
        }
    }
    if let Some(v) = get_path(&data, &["taxonomy", "tags"]).cloned() {
        if let Ok(list) = serde_yaml::from_value(v) {
            cfg.tags = StringConfigField { values: list };
        }
    }
    // issue.categories/tags (preferred canonical)
    if let Some(v) = get_path(&data, &["issue", "categories"]).cloned() {
        if let Ok(list) = serde_yaml::from_value(v) {
            cfg.categories = StringConfigField { values: list };
        }
    }
    if let Some(v) = get_path(&data, &["issue", "tags"]).cloned() {
        if let Ok(list) = serde_yaml::from_value(v) {
            cfg.tags = StringConfigField { values: list };
        }
    }

    // custom.fields
    if let Some(v) = get_path(&data, &["custom", "fields"]).cloned() {
        if let Ok(list) = serde_yaml::from_value(v) {
            cfg.custom_fields = StringConfigField { values: list };
        }
    }

    // scan.signal_words
    if let Some(v) = get_path(&data, &["scan", "signal_words"]).cloned() {
        if let Ok(list) = serde_yaml::from_value(v) {
            cfg.scan_signal_words = list;
        }
    }
    // scan.ticket_patterns
    if let Some(v) = get_path(&data, &["scan", "ticket_patterns"]).cloned() {
        if let Ok(list) = serde_yaml::from_value(v) {
            cfg.scan_ticket_patterns = Some(list);
        }
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
    if let Some(v) = get_path(&data, &["auto", "codeowners_assign"]).and_then(cast::<bool>) {
        cfg.auto_codeowners_assign = v;
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

    // project.id
    if let Some(v) = get_path(&data, &["project", "id"]).and_then(cast::<String>) {
        cfg.project_name = v;
    }
    // default.*
    if let Some(v) = get_path(&data, &["default", "reporter"]).and_then(cast::<String>) {
        cfg.default_reporter = Some(v);
    }
    if let Some(v) = get_path(&data, &["default", "category"]).and_then(cast::<String>) {
        cfg.default_category = Some(v);
    }
    if let Some(v) = get_path(&data, &["default", "tags"]).cloned() {
        cfg.default_tags = serde_yaml::from_value(v).ok();
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
    if let Some(v) = get_path(&data, &["default", "category"]).and_then(cast::<String>) {
        cfg.default_category = Some(v);
    }
    if let Some(v) = get_path(&data, &["default", "tags"]).cloned() {
        cfg.default_tags = serde_yaml::from_value(v).ok();
    }
    // issue.*
    if let Some(v) = get_path(&data, &["issue", "states"]).cloned() {
        cfg.issue_states = serde_yaml::from_value(v).ok();
    }
    if let Some(v) = get_path(&data, &["issue", "types"]).cloned() {
        cfg.issue_types = serde_yaml::from_value(v).ok();
    }
    if let Some(v) = get_path(&data, &["issue", "priorities"]).cloned() {
        cfg.issue_priorities = serde_yaml::from_value(v).ok();
    }
    // taxonomy.* (legacy)
    if let Some(v) = get_path(&data, &["taxonomy", "categories"]).cloned() {
        if let Ok(list) = serde_yaml::from_value(v) {
            cfg.categories = Some(StringConfigField { values: list });
        }
    }
    if let Some(v) = get_path(&data, &["taxonomy", "tags"]).cloned() {
        if let Ok(list) = serde_yaml::from_value(v) {
            cfg.tags = Some(StringConfigField { values: list });
        }
    }
    // issue.categories/tags (preferred)
    if let Some(v) = get_path(&data, &["issue", "categories"]).cloned() {
        if let Ok(list) = serde_yaml::from_value(v) {
            cfg.categories = Some(StringConfigField { values: list });
        }
    }
    if let Some(v) = get_path(&data, &["issue", "tags"]).cloned() {
        if let Ok(list) = serde_yaml::from_value(v) {
            cfg.tags = Some(StringConfigField { values: list });
        }
    }
    // scan.signal_words
    if let Some(v) = get_path(&data, &["scan", "signal_words"]).cloned() {
        cfg.scan_signal_words = serde_yaml::from_value(v).ok();
    }
    // scan.ticket_patterns
    if let Some(v) = get_path(&data, &["scan", "ticket_patterns"]).cloned() {
        cfg.scan_ticket_patterns = serde_yaml::from_value(v).ok();
    }

    Ok(cfg)
}

/// Render GlobalConfig into canonical nested YAML form
pub fn to_canonical_global_yaml(cfg: &GlobalConfig) -> String {
    use serde_yaml::Value as Y;
    let mut root = serde_yaml::Mapping::new();

    // server
    let mut server = serde_yaml::Mapping::new();
    server.insert(Y::String("port".into()), Y::Number(cfg.server_port.into()));
    root.insert(Y::String("server".into()), Y::Mapping(server));

    // default
    let mut default = serde_yaml::Mapping::new();
    if !cfg.default_prefix.is_empty() {
        default.insert(
            Y::String("project".into()),
            Y::String(cfg.default_prefix.clone()),
        );
    }
    if let Some(v) = &cfg.default_assignee {
        default.insert(Y::String("assignee".into()), Y::String(v.clone()));
    }
    if let Some(v) = &cfg.default_reporter {
        default.insert(Y::String("reporter".into()), Y::String(v.clone()));
    }
    if let Some(v) = &cfg.default_category {
        default.insert(Y::String("category".into()), Y::String(v.clone()));
    }
    if !cfg.default_tags.is_empty() {
        default.insert(
            Y::String("tags".into()),
            serde_yaml::to_value(&cfg.default_tags).unwrap_or(Y::Null),
        );
    }
    default.insert(
        Y::String("priority".into()),
        serde_yaml::to_value(cfg.default_priority).unwrap_or(Y::Null),
    );
    if let Some(v) = &cfg.default_status {
        default.insert(
            Y::String("status".into()),
            serde_yaml::to_value(v).unwrap_or(Y::Null),
        );
    }
    root.insert(Y::String("default".into()), Y::Mapping(default));

    // issue
    let mut issue = serde_yaml::Mapping::new();
    issue.insert(
        Y::String("states".into()),
        serde_yaml::to_value(&cfg.issue_states.values).unwrap_or(Y::Null),
    );
    issue.insert(
        Y::String("types".into()),
        serde_yaml::to_value(&cfg.issue_types.values).unwrap_or(Y::Null),
    );
    issue.insert(
        Y::String("priorities".into()),
        serde_yaml::to_value(&cfg.issue_priorities.values).unwrap_or(Y::Null),
    );
    root.insert(Y::String("issue".into()), Y::Mapping(issue));

    // categories/tags now live under issue.* in canonical form
    if let Some(mut imap) = root
        .get_mut(Y::String("issue".into()))
        .and_then(|v| v.as_mapping().cloned())
    {
        imap.insert(
            Y::String("categories".into()),
            serde_yaml::to_value(&cfg.categories.values).unwrap_or(Y::Null),
        );
        imap.insert(
            Y::String("tags".into()),
            serde_yaml::to_value(&cfg.tags.values).unwrap_or(Y::Null),
        );
        root.insert(Y::String("issue".into()), Y::Mapping(imap));
    }

    // custom
    let mut custom = serde_yaml::Mapping::new();
    custom.insert(
        Y::String("fields".into()),
        serde_yaml::to_value(&cfg.custom_fields.values).unwrap_or(Y::Null),
    );
    root.insert(Y::String("custom".into()), Y::Mapping(custom));

    // scan
    let mut scan = serde_yaml::Mapping::new();
    scan.insert(
        Y::String("signal_words".into()),
        serde_yaml::to_value(&cfg.scan_signal_words).unwrap_or(Y::Null),
    );
    if let Some(patterns) = crate::config::types::maybe_scan_ticket_patterns(cfg) {
        scan.insert(
            Y::String("ticket_patterns".into()),
            serde_yaml::to_value(patterns).unwrap_or(Y::Null),
        );
    }
    root.insert(Y::String("scan".into()), Y::Mapping(scan));

    // auto
    let mut auto = serde_yaml::Mapping::new();
    auto.insert(Y::String("identity".into()), Y::Bool(cfg.auto_identity));
    auto.insert(
        Y::String("identity_git".into()),
        Y::Bool(cfg.auto_identity_git),
    );
    auto.insert(
        Y::String("set_reporter".into()),
        Y::Bool(cfg.auto_set_reporter),
    );
    auto.insert(
        Y::String("assign_on_status".into()),
        Y::Bool(cfg.auto_assign_on_status),
    );
    auto.insert(
        Y::String("codeowners_assign".into()),
        Y::Bool(cfg.auto_codeowners_assign),
    );
    root.insert(Y::String("auto".into()), Y::Mapping(auto));

    serde_yaml::to_string(&Y::Mapping(root)).unwrap_or_else(|_| "".to_string())
}

/// Render ProjectConfig into canonical nested YAML form
pub fn to_canonical_project_yaml(cfg: &ProjectConfig) -> String {
    use serde_yaml::Value as Y;
    let mut root = serde_yaml::Mapping::new();

    // project
    let mut project = serde_yaml::Mapping::new();
    project.insert(Y::String("id".into()), Y::String(cfg.project_name.clone()));
    root.insert(Y::String("project".into()), Y::Mapping(project));

    // default
    let mut default = serde_yaml::Mapping::new();
    if let Some(v) = &cfg.default_assignee {
        default.insert(Y::String("assignee".into()), Y::String(v.clone()));
    }
    if let Some(v) = &cfg.default_reporter {
        default.insert(Y::String("reporter".into()), Y::String(v.clone()));
    }
    if let Some(v) = &cfg.default_category {
        default.insert(Y::String("category".into()), Y::String(v.clone()));
    }
    if let Some(tags) = &cfg.default_tags {
        if !tags.is_empty() {
            default.insert(
                Y::String("tags".into()),
                serde_yaml::to_value(tags).unwrap_or(Y::Null),
            );
        }
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

    // issue
    let mut issue = serde_yaml::Mapping::new();
    if let Some(v) = &cfg.issue_states {
        issue.insert(
            Y::String("states".into()),
            serde_yaml::to_value(&v.values).unwrap_or(Y::Null),
        );
    }
    if let Some(v) = &cfg.issue_types {
        issue.insert(
            Y::String("types".into()),
            serde_yaml::to_value(&v.values).unwrap_or(Y::Null),
        );
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

    // categories/tags under issue.* in canonical project YAML
    if let Some(v) = &cfg.categories {
        if let Some(issue_map) = root
            .get_mut(Y::String("issue".into()))
            .and_then(|v| v.as_mapping_mut())
        {
            issue_map.insert(
                Y::String("categories".into()),
                serde_yaml::to_value(&v.values).unwrap_or(Y::Null),
            );
        } else {
            let mut im = serde_yaml::Mapping::new();
            im.insert(
                Y::String("categories".into()),
                serde_yaml::to_value(&v.values).unwrap_or(Y::Null),
            );
            root.insert(Y::String("issue".into()), Y::Mapping(im));
        }
    }
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
    if !scan.is_empty() {
        root.insert(Y::String("scan".into()), Y::Mapping(scan));
    }

    serde_yaml::to_string(&Y::Mapping(root)).unwrap_or_else(|_| "".to_string())
}
