use crate::config::types::{ResolvedConfig, SprintNotificationsConfig};
use crate::output::{OutputFormat, OutputRenderer};
use serde::Serialize;
use std::collections::HashMap;
use std::fmt::Write;

pub(super) struct YamlRenderOptions<'a> {
    pub(super) include_defaults: bool,
    pub(super) include_comments: bool,
    pub(super) colorize_comments: bool,
    pub(super) allowed_sources: Option<&'a [&'a str]>,
}

impl<'a> YamlRenderOptions<'a> {
    pub(super) fn allows(&self, source: Option<&String>) -> bool {
        let label = source.map(|s| s.as_str()).unwrap_or("default");
        if !self.include_defaults && label == "default" {
            return false;
        }
        if let Some(allowed) = self.allowed_sources {
            return allowed.contains(&label);
        }
        true
    }
}

pub(super) fn emit_config_yaml(
    renderer: &OutputRenderer,
    scope: &str,
    label: Option<&str>,
    resolved: &ResolvedConfig,
    sources: &HashMap<String, String>,
    options: &YamlRenderOptions,
    json_meta: Option<serde_json::Value>,
) {
    if matches!(renderer.format, OutputFormat::Json) {
        let sources_payload: HashMap<String, String> = if options.include_defaults {
            sources.clone()
        } else {
            sources
                .iter()
                .filter(|(_, label)| options.allows(Some(*label)))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };
        let mut payload = serde_json::json!({
            "status": "success",
            "scope": scope,
            "include_defaults": options.include_defaults,
            "config": resolved,
            "sources": sources_payload,
        });
        if let Some(lbl) = label.filter(|s| !s.is_empty()) {
            payload["label"] = serde_json::Value::String(lbl.to_string());
        }

        if let Some(meta) = json_meta
            && let Some(meta_map) = meta.as_object()
            && let Some(payload_map) = payload.as_object_mut()
        {
            for (k, v) in meta_map {
                payload_map.insert(k.clone(), v.clone());
            }
        }
        renderer.emit_json(&payload);
        return;
    }

    let scope_title = match scope {
        "global" => "Global",
        "project" => "Project",
        other => other,
    };

    let heading = match (options.include_defaults, label.filter(|s| !s.is_empty())) {
        (true, Some(lbl)) => {
            format!("Effective {scope_title} configuration ({lbl}) – canonical YAML:")
        }
        (true, None) => format!("Effective {scope_title} configuration – canonical YAML:"),
        (false, Some(lbl)) => {
            format!("{scope_title} configuration ({lbl}) – canonical YAML:")
        }
        (false, None) => format!("{scope_title} configuration – canonical YAML:"),
    };
    renderer.emit_info(&heading);
    let yaml = render_resolved_config_yaml(resolved, sources, options);
    renderer.emit_raw_stdout(yaml.trim_end());
}

fn render_resolved_config_yaml(
    resolved: &ResolvedConfig,
    sources: &HashMap<String, String>,
    options: &YamlRenderOptions,
) -> String {
    let mut sections: Vec<String> = Vec::new();

    let mut server_body = String::new();
    if write_scalar_line(
        &mut server_body,
        2,
        "port",
        &yaml_scalar(&resolved.server_port),
        sources.get("server.port"),
        options,
    ) {
        sections.push(format!("server:\n{}", server_body));
    }

    let mut default_body = String::new();
    let mut default_written = false;
    if !resolved.default_project.is_empty() {
        default_written |= write_scalar_line(
            &mut default_body,
            2,
            "project",
            &yaml_scalar(&resolved.default_project),
            sources.get("default.project"),
            options,
        );
    }
    if let Some(assignee) = &resolved.default_assignee {
        default_written |= write_scalar_line(
            &mut default_body,
            2,
            "assignee",
            &yaml_scalar(assignee),
            sources.get("default.assignee"),
            options,
        );
    }
    if let Some(reporter) = &resolved.default_reporter {
        default_written |= write_scalar_line(
            &mut default_body,
            2,
            "reporter",
            &yaml_scalar(reporter),
            sources.get("default.reporter"),
            options,
        );
    }
    default_written |= write_sequence(
        &mut default_body,
        2,
        "tags",
        &resolved.default_tags,
        sources.get("default.tags"),
        options,
    );
    default_written |= write_scalar_line(
        &mut default_body,
        2,
        "strict-members",
        &yaml_scalar(&resolved.strict_members),
        sources.get("default.strict-members"),
        options,
    );
    default_written |= write_scalar_line(
        &mut default_body,
        2,
        "priority",
        &yaml_scalar(&resolved.default_priority),
        sources.get("default.priority"),
        options,
    );
    if let Some(status) = &resolved.default_status {
        default_written |= write_scalar_line(
            &mut default_body,
            2,
            "status",
            &yaml_scalar(status),
            sources.get("default.status"),
            options,
        );
    }
    if default_written {
        sections.push(format!("default:\n{}", default_body));
    }

    let mut members_body = String::new();
    if write_sequence(
        &mut members_body,
        0,
        "members",
        &resolved.members,
        sources.get("members"),
        options,
    ) {
        sections.push(members_body);
    }

    let mut issue_body = String::new();
    let mut issue_written = false;
    issue_written |= write_sequence(
        &mut issue_body,
        2,
        "states",
        &resolved.issue_states.values,
        sources.get("issue.states"),
        options,
    );
    issue_written |= write_sequence(
        &mut issue_body,
        2,
        "types",
        &resolved.issue_types.values,
        sources.get("issue.types"),
        options,
    );
    issue_written |= write_sequence(
        &mut issue_body,
        2,
        "priorities",
        &resolved.issue_priorities.values,
        sources.get("issue.priorities"),
        options,
    );
    issue_written |= write_sequence(
        &mut issue_body,
        2,
        "tags",
        &resolved.tags.values,
        sources.get("issue.tags"),
        options,
    );
    if issue_written {
        sections.push(format!("issue:\n{}", issue_body));
    }

    let mut custom_body = String::new();
    if write_sequence(
        &mut custom_body,
        2,
        "fields",
        &resolved.custom_fields.values,
        sources.get("custom.fields"),
        options,
    ) {
        sections.push(format!("custom:\n{}", custom_body));
    }

    let mut scan_body = String::new();
    let mut scan_written = false;
    scan_written |= write_sequence(
        &mut scan_body,
        2,
        "signal-words",
        &resolved.scan_signal_words,
        sources.get("scan.signal-words"),
        options,
    );
    if let Some(patterns) = &resolved.scan_ticket_patterns {
        scan_written |= write_sequence(
            &mut scan_body,
            2,
            "ticket-patterns",
            patterns,
            sources.get("scan.ticket-patterns"),
            options,
        );
    } else {
        scan_written |= write_scalar_line(
            &mut scan_body,
            2,
            "ticket-patterns",
            "null",
            sources.get("scan.ticket-patterns"),
            options,
        );
    }
    scan_written |= write_scalar_line(
        &mut scan_body,
        2,
        "enable-ticket-words",
        &yaml_scalar(&resolved.scan_enable_ticket_words),
        sources.get("scan.enable-ticket-words"),
        options,
    );
    scan_written |= write_scalar_line(
        &mut scan_body,
        2,
        "enable-mentions",
        &yaml_scalar(&resolved.scan_enable_mentions),
        sources.get("scan.enable-mentions"),
        options,
    );
    scan_written |= write_scalar_line(
        &mut scan_body,
        2,
        "strip-attributes",
        &yaml_scalar(&resolved.scan_strip_attributes),
        sources.get("scan.strip-attributes"),
        options,
    );
    if scan_written {
        sections.push(format!("scan:\n{}", scan_body));
    }

    let mut auto_body = String::new();
    let mut auto_written = false;
    auto_written |= write_scalar_line(
        &mut auto_body,
        2,
        "populate-members",
        &yaml_scalar(&resolved.auto_populate_members),
        sources.get("auto.populate-members"),
        options,
    );
    auto_written |= write_scalar_line(
        &mut auto_body,
        2,
        "set-reporter",
        &yaml_scalar(&resolved.auto_set_reporter),
        sources.get("auto.set-reporter"),
        options,
    );
    auto_written |= write_scalar_line(
        &mut auto_body,
        2,
        "assign-on-status",
        &yaml_scalar(&resolved.auto_assign_on_status),
        sources.get("auto.assign-on-status"),
        options,
    );
    auto_written |= write_scalar_line(
        &mut auto_body,
        2,
        "codeowners-assign",
        &yaml_scalar(&resolved.auto_codeowners_assign),
        sources.get("auto.codeowners-assign"),
        options,
    );
    auto_written |= write_scalar_line(
        &mut auto_body,
        2,
        "tags-from-path",
        &yaml_scalar(&resolved.auto_tags_from_path),
        sources.get("auto.tags-from-path"),
        options,
    );
    auto_written |= write_scalar_line(
        &mut auto_body,
        2,
        "branch-infer-type",
        &yaml_scalar(&resolved.auto_branch_infer_type),
        sources.get("auto.branch-infer-type"),
        options,
    );
    auto_written |= write_scalar_line(
        &mut auto_body,
        2,
        "branch-infer-status",
        &yaml_scalar(&resolved.auto_branch_infer_status),
        sources.get("auto.branch-infer-status"),
        options,
    );
    auto_written |= write_scalar_line(
        &mut auto_body,
        2,
        "branch-infer-priority",
        &yaml_scalar(&resolved.auto_branch_infer_priority),
        sources.get("auto.branch-infer-priority"),
        options,
    );
    auto_written |= write_scalar_line(
        &mut auto_body,
        2,
        "identity",
        &yaml_scalar(&resolved.auto_identity),
        sources.get("auto.identity"),
        options,
    );
    auto_written |= write_scalar_line(
        &mut auto_body,
        2,
        "identity-git",
        &yaml_scalar(&resolved.auto_identity_git),
        sources.get("auto.identity-git"),
        options,
    );
    if auto_written {
        sections.push(format!("auto:\n{}", auto_body));
    }

    let mut attachments_body = String::new();
    let mut attachments_written = false;
    attachments_written |= write_scalar_line(
        &mut attachments_body,
        2,
        "dir",
        &yaml_scalar(&resolved.attachments_dir),
        sources.get("attachments.dir"),
        options,
    );
    attachments_written |= write_scalar_line(
        &mut attachments_body,
        2,
        "max_upload_mb",
        &yaml_scalar(&resolved.attachments_max_upload_mb),
        sources.get("attachments.max_upload_mb"),
        options,
    );
    if attachments_written {
        sections.push(format!("attachments:\n{}", attachments_body));
    }

    let mut sprint_body = String::new();
    let mut sprint_written = false;
    if resolved.sprint_defaults.capacity_points.is_some()
        || resolved.sprint_defaults.capacity_hours.is_some()
        || resolved.sprint_defaults.length.is_some()
        || resolved.sprint_defaults.overdue_after.is_some()
    {
        let sprint_defaults_source = resolve_section_source(
            sources,
            &[
                "sprints.defaults.capacity_points",
                "sprints.defaults.capacity_hours",
                "sprints.defaults.length",
                "sprints.defaults.overdue_after",
            ],
        );
        sprint_written |= write_scalar_line(
            &mut sprint_body,
            2,
            "defaults",
            &yaml_scalar(&resolved.sprint_defaults),
            sprint_defaults_source,
            options,
        );
    }
    if resolved.sprint_notifications.enabled != SprintNotificationsConfig::default().enabled {
        let sprint_notifications_source =
            resolve_section_source(sources, &["sprints.notifications.enabled"]);
        sprint_written |= write_scalar_line(
            &mut sprint_body,
            2,
            "notifications",
            &yaml_scalar(&resolved.sprint_notifications),
            sprint_notifications_source,
            options,
        );
    }
    if sprint_written {
        sections.push(format!("sprints:\n{}", sprint_body));
    }

    let mut branch_body = String::new();
    let mut branch_written = false;
    branch_written |= write_alias_map(
        &mut branch_body,
        2,
        "type-aliases",
        &resolved.branch_type_aliases,
        sources.get("branch.type-aliases"),
        options,
    );
    branch_written |= write_alias_map(
        &mut branch_body,
        2,
        "status-aliases",
        &resolved.branch_status_aliases,
        sources.get("branch.status-aliases"),
        options,
    );
    branch_written |= write_alias_map(
        &mut branch_body,
        2,
        "priority-aliases",
        &resolved.branch_priority_aliases,
        sources.get("branch.priority-aliases"),
        options,
    );
    if branch_written {
        sections.push(format!("branch:\n{}", branch_body));
    }

    let mut buf = String::from("---\n");
    for (idx, section) in sections.iter().enumerate() {
        if idx > 0 {
            buf.push('\n');
        }
        buf.push_str(section);
    }

    buf
}

fn yaml_scalar<T: Serialize>(value: &T) -> String {
    match serde_yaml::to_string(value) {
        Ok(mut text) => {
            if let Some(stripped) = text.strip_prefix("---\n") {
                text = stripped.to_string();
            }
            if let Some(stripped) = text.strip_suffix("\n...") {
                text = stripped.to_string();
            }
            text.trim().to_string()
        }
        Err(_) => "\"<invalid>\"".to_string(),
    }
}

fn yaml_comment(source: Option<&String>, options: &YamlRenderOptions) -> String {
    if !options.include_comments {
        return String::new();
    }

    let Some(source) = source.filter(|s| !s.is_empty()) else {
        return String::new();
    };

    if !options.colorize_comments {
        return format!(" # ({source})");
    }

    const COMMENT_COLOR: &str = "\u{001b}[38;5;110m";
    const RESET: &str = "\u{001b}[0m";
    format!(" {COMMENT_COLOR}# ({source}){RESET}")
}

fn write_scalar_line(
    buf: &mut String,
    indent: usize,
    key: &str,
    value: &str,
    source: Option<&String>,
    options: &YamlRenderOptions,
) -> bool {
    if !options.allows(source) {
        return false;
    }
    let indent_str = " ".repeat(indent);
    let comment = yaml_comment(source, options);
    let _ = writeln!(buf, "{indent_str}{key}: {value}{comment}");
    true
}

fn write_sequence<T: Serialize>(
    buf: &mut String,
    indent: usize,
    key: &str,
    values: &[T],
    source: Option<&String>,
    options: &YamlRenderOptions,
) -> bool {
    if !options.allows(source) {
        return false;
    }
    let indent_str = " ".repeat(indent);
    let comment = yaml_comment(source, options);
    if values.is_empty() {
        let _ = writeln!(buf, "{indent_str}{key}: []{comment}");
        return true;
    }
    let _ = writeln!(buf, "{indent_str}{key}:{comment}");
    let item_indent = " ".repeat(indent + 2);
    for value in values {
        let formatted = yaml_scalar(value);
        let _ = writeln!(buf, "{item_indent}- {formatted}");
    }
    true
}

fn write_mapping_entries(
    buf: &mut String,
    indent: usize,
    key: &str,
    entries: &[(String, String)],
    source: Option<&String>,
    options: &YamlRenderOptions,
) -> bool {
    if entries.is_empty() || !options.allows(source) {
        return false;
    }
    let indent_str = " ".repeat(indent);
    let comment = yaml_comment(source, options);
    let _ = writeln!(buf, "{indent_str}{key}:{comment}");
    let entry_indent = " ".repeat(indent + 2);
    for (name, value) in entries {
        let _ = writeln!(buf, "{entry_indent}{name}: {value}");
    }
    true
}

fn write_alias_map<T: Serialize>(
    buf: &mut String,
    indent: usize,
    key: &str,
    aliases: &HashMap<String, T>,
    source: Option<&String>,
    options: &YamlRenderOptions,
) -> bool {
    if aliases.is_empty() {
        return false;
    }
    let mut entries: Vec<_> = aliases.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    let formatted: Vec<(String, String)> = entries
        .into_iter()
        .map(|(name, value)| (name.clone(), yaml_scalar(value)))
        .collect();
    write_mapping_entries(buf, indent, key, &formatted, source, options)
}

fn resolve_section_source<'a>(
    sources: &'a HashMap<String, String>,
    keys: &[&str],
) -> Option<&'a String> {
    let mut best: Option<&String> = None;
    let mut best_rank = -1;
    for key in keys {
        if let Some(candidate) = sources.get(*key) {
            let rank = scope_rank(candidate);
            if rank > best_rank {
                best = Some(candidate);
                best_rank = rank;
            }
        }
    }
    best
}

fn scope_rank(label: &str) -> i32 {
    match label {
        "project" => 3,
        "global" | "home" | "env" => 2,
        "default" | "built_in" => 1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn write_alias_map_renders_sorted_entries_and_respects_filters() {
        let mut aliases = HashMap::new();
        aliases.insert("beta".to_string(), "second".to_string());
        aliases.insert("alpha".to_string(), "first".to_string());

        let mut buf = String::new();
        let source_label = "global".to_string();
        let options = YamlRenderOptions {
            include_defaults: true,
            include_comments: true,
            colorize_comments: false,
            allowed_sources: None,
        };

        let rendered = write_alias_map(
            &mut buf,
            2,
            "aliases",
            &aliases,
            Some(&source_label),
            &options,
        );

        assert!(rendered);
        assert_eq!(
            buf,
            "  aliases: # (global)\n    alpha: first\n    beta: second\n"
        );

        let mut filtered_buf = String::new();
        let allowed_only_project = ["project"];
        let filtered_options = YamlRenderOptions {
            allowed_sources: Some(&allowed_only_project),
            ..options
        };

        let filtered = write_alias_map(
            &mut filtered_buf,
            2,
            "aliases",
            &aliases,
            Some(&source_label),
            &filtered_options,
        );

        assert!(!filtered);
        assert!(filtered_buf.is_empty());
    }
}
