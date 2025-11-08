use crate::cli::handlers::CommandHandler;
use crate::cli::{ConfigAction, ConfigNormalizeArgs, ConfigShowArgs, ConfigValidateArgs};
use crate::config::ConfigManager;
use crate::output::OutputRenderer;
use crate::types::{Priority, TaskStatus};
use crate::utils::project::generate_project_prefix;
use crate::utils::project::generate_unique_project_prefix;
use crate::utils::project::resolve_project_input;
use crate::utils::project::validate_explicit_prefix;
use crate::workspace::TasksDirectoryResolver;
use serde_yaml;
use std::collections::HashMap;
use std::fs;
use std::io::IsTerminal;

struct YamlRenderOptions<'a> {
    include_defaults: bool,
    include_comments: bool,
    colorize_comments: bool,
    allowed_sources: Option<&'a [&'a str]>,
}

impl<'a> YamlRenderOptions<'a> {
    fn allows(&self, source: Option<&String>) -> bool {
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

/// Handler for config commands
pub struct ConfigHandler;

impl CommandHandler for ConfigHandler {
    type Args = ConfigAction;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Self::Result {
        match args {
            ConfigAction::Show(ConfigShowArgs {
                project,
                explain,
                full,
            }) => Self::handle_config_show(resolver, renderer, project, explain, full),
            ConfigAction::Set(crate::cli::ConfigSetArgs {
                field,
                value,
                dry_run,
                force,
                global,
            }) => Self::handle_config_set(
                resolver, renderer, field, value, dry_run, force, global, project,
            ),
            ConfigAction::Init(crate::cli::ConfigInitArgs {
                template,
                prefix,
                project,
                copy_from,
                global,
                dry_run,
                force,
            }) => Self::handle_config_init(
                resolver, renderer, template, prefix, project, copy_from, global, dry_run, force,
            ),
            ConfigAction::Validate(ConfigValidateArgs {
                project,
                global,
                fix,
                errors_only,
            }) => {
                Self::handle_config_validate(resolver, renderer, project, global, fix, errors_only)
            }
            ConfigAction::Templates => {
                renderer.emit_success("Available Configuration Templates:");
                renderer.emit_raw_stdout("  • default - Basic task management setup");
                renderer.emit_raw_stdout("  • agile - Agile/Scrum workflow configuration");
                renderer.emit_raw_stdout("  • kanban - Kanban board style setup");
                renderer.emit_info(
                    "Use 'lotar config init --template=<n>' to initialize with a template.",
                );
                Ok(())
            }
            ConfigAction::Normalize(ConfigNormalizeArgs {
                global,
                project,
                write,
            }) => Self::handle_config_normalize(resolver, renderer, global, project, write),
        }
    }
}

impl ConfigHandler {
    fn emit_config_yaml(
        renderer: &OutputRenderer,
        scope: &str,
        label: Option<&str>,
        resolved: &crate::config::types::ResolvedConfig,
        sources: &HashMap<String, String>,
        options: &YamlRenderOptions,
    ) {
        if matches!(renderer.format, crate::output::OutputFormat::Json) {
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
            renderer.emit_raw_stdout(&payload.to_string());
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
            (false, Some(lbl)) => format!("{scope_title} configuration ({lbl}) – canonical YAML:"),
            (false, None) => format!("{scope_title} configuration – canonical YAML:"),
        };
        renderer.emit_info(&heading);
        let yaml = Self::render_resolved_config_yaml(resolved, sources, options);
        renderer.emit_raw_stdout(yaml.trim_end());
    }

    fn render_resolved_config_yaml(
        resolved: &crate::config::types::ResolvedConfig,
        sources: &HashMap<String, String>,
        options: &YamlRenderOptions,
    ) -> String {
        let mut sections: Vec<String> = Vec::new();

        // Server section
        let mut server_body = String::new();
        if Self::write_scalar_line(
            &mut server_body,
            2,
            "port",
            &Self::yaml_scalar(&resolved.server_port),
            sources.get("server.port"),
            options,
        ) {
            sections.push(format!("server:\n{}", server_body));
        }

        // Default section
        let mut default_body = String::new();
        let mut default_written = false;
        if !resolved.default_prefix.is_empty() {
            default_written |= Self::write_scalar_line(
                &mut default_body,
                2,
                "project",
                &Self::yaml_scalar(&resolved.default_prefix),
                sources.get("default.project"),
                options,
            );
        }
        if let Some(assignee) = &resolved.default_assignee {
            default_written |= Self::write_scalar_line(
                &mut default_body,
                2,
                "assignee",
                &Self::yaml_scalar(assignee),
                sources.get("default.assignee"),
                options,
            );
        }
        if let Some(reporter) = &resolved.default_reporter {
            default_written |= Self::write_scalar_line(
                &mut default_body,
                2,
                "reporter",
                &Self::yaml_scalar(reporter),
                sources.get("default.reporter"),
                options,
            );
        }
        default_written |= Self::write_sequence(
            &mut default_body,
            2,
            "tags",
            &resolved.default_tags,
            sources.get("default.tags"),
            options,
        );
        default_written |= Self::write_scalar_line(
            &mut default_body,
            2,
            "strict-members",
            &Self::yaml_scalar(&resolved.strict_members),
            sources.get("default.strict-members"),
            options,
        );
        default_written |= Self::write_scalar_line(
            &mut default_body,
            2,
            "priority",
            &Self::yaml_scalar(&resolved.default_priority),
            sources.get("default.priority"),
            options,
        );
        if let Some(status) = &resolved.default_status {
            default_written |= Self::write_scalar_line(
                &mut default_body,
                2,
                "status",
                &Self::yaml_scalar(status),
                sources.get("default.status"),
                options,
            );
        }
        if default_written {
            sections.push(format!("default:\n{}", default_body));
        }

        let mut members_body = String::new();
        let members_written = Self::write_sequence(
            &mut members_body,
            0,
            "members",
            &resolved.members,
            sources.get("members"),
            options,
        );
        if members_written {
            sections.push(members_body);
        }

        // Issue section
        let mut issue_body = String::new();
        let mut issue_written = false;
        issue_written |= Self::write_sequence(
            &mut issue_body,
            2,
            "states",
            &resolved.issue_states.values,
            sources.get("issue.states"),
            options,
        );
        issue_written |= Self::write_sequence(
            &mut issue_body,
            2,
            "types",
            &resolved.issue_types.values,
            sources.get("issue.types"),
            options,
        );
        issue_written |= Self::write_sequence(
            &mut issue_body,
            2,
            "priorities",
            &resolved.issue_priorities.values,
            sources.get("issue.priorities"),
            options,
        );
        issue_written |= Self::write_sequence(
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

        // Custom fields section
        let mut custom_body = String::new();
        if Self::write_sequence(
            &mut custom_body,
            2,
            "fields",
            &resolved.custom_fields.values,
            sources.get("custom.fields"),
            options,
        ) {
            sections.push(format!("custom:\n{}", custom_body));
        }

        // Scan section
        let mut scan_body = String::new();
        let mut scan_written = false;
        scan_written |= Self::write_sequence(
            &mut scan_body,
            2,
            "signal-words",
            &resolved.scan_signal_words,
            sources.get("scan.signal-words"),
            options,
        );
        if let Some(patterns) = &resolved.scan_ticket_patterns {
            scan_written |= Self::write_sequence(
                &mut scan_body,
                2,
                "ticket-patterns",
                patterns,
                sources.get("scan.ticket-patterns"),
                options,
            );
        } else {
            scan_written |= Self::write_scalar_line(
                &mut scan_body,
                2,
                "ticket-patterns",
                "null",
                sources.get("scan.ticket-patterns"),
                options,
            );
        }
        scan_written |= Self::write_scalar_line(
            &mut scan_body,
            2,
            "enable-ticket-words",
            &Self::yaml_scalar(&resolved.scan_enable_ticket_words),
            sources.get("scan.enable-ticket-words"),
            options,
        );
        scan_written |= Self::write_scalar_line(
            &mut scan_body,
            2,
            "enable-mentions",
            &Self::yaml_scalar(&resolved.scan_enable_mentions),
            sources.get("scan.enable-mentions"),
            options,
        );
        scan_written |= Self::write_scalar_line(
            &mut scan_body,
            2,
            "strip-attributes",
            &Self::yaml_scalar(&resolved.scan_strip_attributes),
            sources.get("scan.strip-attributes"),
            options,
        );
        if scan_written {
            sections.push(format!("scan:\n{}", scan_body));
        }

        // Automation section
        let mut auto_body = String::new();
        let mut auto_written = false;
        auto_written |= Self::write_scalar_line(
            &mut auto_body,
            2,
            "populate-members",
            &Self::yaml_scalar(&resolved.auto_populate_members),
            sources.get("auto.populate-members"),
            options,
        );
        auto_written |= Self::write_scalar_line(
            &mut auto_body,
            2,
            "set-reporter",
            &Self::yaml_scalar(&resolved.auto_set_reporter),
            sources.get("auto.set-reporter"),
            options,
        );
        auto_written |= Self::write_scalar_line(
            &mut auto_body,
            2,
            "assign-on-status",
            &Self::yaml_scalar(&resolved.auto_assign_on_status),
            sources.get("auto.assign-on-status"),
            options,
        );
        auto_written |= Self::write_scalar_line(
            &mut auto_body,
            2,
            "codeowners-assign",
            &Self::yaml_scalar(&resolved.auto_codeowners_assign),
            sources.get("auto.codeowners-assign"),
            options,
        );
        auto_written |= Self::write_scalar_line(
            &mut auto_body,
            2,
            "tags-from-path",
            &Self::yaml_scalar(&resolved.auto_tags_from_path),
            sources.get("auto.tags-from-path"),
            options,
        );
        auto_written |= Self::write_scalar_line(
            &mut auto_body,
            2,
            "branch-infer-type",
            &Self::yaml_scalar(&resolved.auto_branch_infer_type),
            sources.get("auto.branch-infer-type"),
            options,
        );
        auto_written |= Self::write_scalar_line(
            &mut auto_body,
            2,
            "branch-infer-status",
            &Self::yaml_scalar(&resolved.auto_branch_infer_status),
            sources.get("auto.branch-infer-status"),
            options,
        );
        auto_written |= Self::write_scalar_line(
            &mut auto_body,
            2,
            "branch-infer-priority",
            &Self::yaml_scalar(&resolved.auto_branch_infer_priority),
            sources.get("auto.branch-infer-priority"),
            options,
        );
        auto_written |= Self::write_scalar_line(
            &mut auto_body,
            2,
            "identity",
            &Self::yaml_scalar(&resolved.auto_identity),
            sources.get("auto.identity"),
            options,
        );
        auto_written |= Self::write_scalar_line(
            &mut auto_body,
            2,
            "identity-git",
            &Self::yaml_scalar(&resolved.auto_identity_git),
            sources.get("auto.identity-git"),
            options,
        );
        if auto_written {
            sections.push(format!("auto:\n{}", auto_body));
        }

        // Branch section
        let mut branch_body = String::new();
        let mut branch_written = false;
        if !resolved.branch_type_aliases.is_empty() {
            let mut entries: Vec<_> = resolved.branch_type_aliases.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            let formatted: Vec<(String, String)> = entries
                .into_iter()
                .map(|(k, v)| (k.clone(), Self::yaml_scalar(v)))
                .collect();
            branch_written |= Self::write_mapping_entries(
                &mut branch_body,
                2,
                "type-aliases",
                &formatted,
                sources.get("branch.type-aliases"),
                options,
            );
        }
        if !resolved.branch_status_aliases.is_empty() {
            let mut entries: Vec<_> = resolved.branch_status_aliases.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            let formatted: Vec<(String, String)> = entries
                .into_iter()
                .map(|(k, v)| (k.clone(), Self::yaml_scalar(v)))
                .collect();
            branch_written |= Self::write_mapping_entries(
                &mut branch_body,
                2,
                "status-aliases",
                &formatted,
                sources.get("branch.status-aliases"),
                options,
            );
        }
        if !resolved.branch_priority_aliases.is_empty() {
            let mut entries: Vec<_> = resolved.branch_priority_aliases.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            let formatted: Vec<(String, String)> = entries
                .into_iter()
                .map(|(k, v)| (k.clone(), Self::yaml_scalar(v)))
                .collect();
            branch_written |= Self::write_mapping_entries(
                &mut branch_body,
                2,
                "priority-aliases",
                &formatted,
                sources.get("branch.priority-aliases"),
                options,
            );
        }
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

    fn yaml_scalar<T: serde::Serialize>(value: &T) -> String {
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
        let comment = Self::yaml_comment(source, options);
        buf.push_str(&format!("{indent_str}{key}: {value}{comment}\n"));
        true
    }

    fn write_sequence<T: serde::Serialize>(
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
        let comment = Self::yaml_comment(source, options);
        if values.is_empty() {
            buf.push_str(&format!("{indent_str}{key}: []{comment}\n"));
            return true;
        }
        buf.push_str(&format!("{indent_str}{key}:{comment}\n"));
        let item_indent = " ".repeat(indent + 2);
        for value in values {
            let formatted = Self::yaml_scalar(value);
            buf.push_str(&format!("{item_indent}- {formatted}\n"));
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
        let comment = Self::yaml_comment(source, options);
        buf.push_str(&format!("{indent_str}{key}:{comment}\n"));
        let entry_indent = " ".repeat(indent + 2);
        for (name, value) in entries {
            buf.push_str(&format!("{entry_indent}{name}: {value}\n"));
        }
        true
    }

    fn env_source_for_key(
        resolved: &crate::config::types::ResolvedConfig,
        key: &str,
    ) -> Option<&'static str> {
        match key {
            "server_port" => std::env::var("LOTAR_PORT")
                .ok()
                .and_then(|p| p.parse::<u16>().ok())
                .filter(|p| *p == resolved.server_port)
                .map(|_| "env"),
            "default_project" => std::env::var("LOTAR_PROJECT")
                .ok()
                .map(|proj| generate_project_prefix(&proj))
                .filter(|p| p == &resolved.default_prefix)
                .map(|_| "env"),
            "default_assignee" => std::env::var("LOTAR_DEFAULT_ASSIGNEE")
                .ok()
                .filter(|v| resolved.default_assignee.as_deref() == Some(v.as_str()))
                .map(|_| "env"),
            "default_reporter" => std::env::var("LOTAR_DEFAULT_REPORTER")
                .ok()
                .filter(|v| resolved.default_reporter.as_deref() == Some(v.as_str()))
                .map(|_| "env"),
            _ => None,
        }
    }

    fn source_label_for_global(
        resolved: &crate::config::types::ResolvedConfig,
        global_cfg: &Option<crate::config::types::GlobalConfig>,
        home_cfg: &Option<crate::config::types::GlobalConfig>,
        key: &str,
    ) -> &'static str {
        if let Some("env") = Self::env_source_for_key(resolved, key) {
            return "env";
        }

        let defaults = crate::config::types::GlobalConfig::default();
        let default_server_port = defaults.server_port;
        let default_prefix = defaults.default_prefix.clone();
        let default_assignee = defaults.default_assignee.clone();
        let default_reporter = defaults.default_reporter.clone();
        let default_priority = defaults.default_priority.clone();
        let default_status = defaults.default_status.clone();
        let default_issue_states = defaults.issue_states.values.clone();
        let default_issue_types = defaults.issue_types.values.clone();
        let default_issue_priorities = defaults.issue_priorities.values.clone();
        let default_issue_tags = defaults.tags.values.clone();
        let default_default_tags = defaults.default_tags.clone();
        let default_members = defaults.members.clone();
        let default_strict_members = defaults.strict_members;
        let default_custom_fields = defaults.custom_fields.values.clone();
        let default_scan_signal_words = defaults.scan_signal_words.clone();
        let default_scan_ticket_patterns = defaults.scan_ticket_patterns.clone();
        let default_scan_enable_ticket_words = defaults.scan_enable_ticket_words;
        let default_scan_enable_mentions = defaults.scan_enable_mentions;
        let default_scan_strip_attributes = defaults.scan_strip_attributes;
        let default_auto_set_reporter = defaults.auto_set_reporter;
        let default_auto_assign_on_status = defaults.auto_assign_on_status;
        let default_auto_codeowners_assign = defaults.auto_codeowners_assign;
        let default_auto_tags_from_path = defaults.auto_tags_from_path;
        let default_auto_branch_infer_type = defaults.auto_branch_infer_type;
        let default_auto_branch_infer_status = defaults.auto_branch_infer_status;
        let default_auto_branch_infer_priority = defaults.auto_branch_infer_priority;
        let default_auto_populate_members = defaults.auto_populate_members;
        let default_auto_identity = defaults.auto_identity;
        let default_auto_identity_git = defaults.auto_identity_git;
        let default_branch_type_aliases = defaults.branch_type_aliases.clone();
        let default_branch_status_aliases = defaults.branch_status_aliases.clone();
        let default_branch_priority_aliases = defaults.branch_priority_aliases.clone();

        let home = home_cfg.as_ref();
        let global = global_cfg.as_ref();

        match key {
            "server_port" => {
                if home.is_some_and(|home| home.server_port == resolved.server_port) {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.server_port == resolved.server_port
                        && glob.server_port != default_server_port
                }) {
                    return "global";
                }
                "default"
            }
            "default_project" => {
                if home.is_some_and(|home| home.default_prefix == resolved.default_prefix) {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.default_prefix == resolved.default_prefix
                        && glob.default_prefix != default_prefix
                }) {
                    return "global";
                }
                "default"
            }
            "default_assignee" => {
                if home.is_some_and(|home| home.default_assignee == resolved.default_assignee) {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.default_assignee == resolved.default_assignee
                        && glob.default_assignee != default_assignee
                }) {
                    return "global";
                }
                "default"
            }
            "default_reporter" => {
                if home.is_some_and(|home| home.default_reporter == resolved.default_reporter) {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.default_reporter == resolved.default_reporter
                        && glob.default_reporter != default_reporter
                }) {
                    return "global";
                }
                "default"
            }
            "default_priority" => {
                if home.is_some_and(|home| home.default_priority == resolved.default_priority) {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.default_priority == resolved.default_priority
                        && glob.default_priority != default_priority
                }) {
                    return "global";
                }
                "default"
            }
            "default_status" => {
                if home.is_some_and(|home| home.default_status == resolved.default_status) {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.default_status == resolved.default_status
                        && glob.default_status != default_status
                }) {
                    return "global";
                }
                "default"
            }
            "issue_states" => {
                if home.is_some_and(|home| home.issue_states.values == resolved.issue_states.values)
                {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.issue_states.values == resolved.issue_states.values
                        && glob.issue_states.values.as_slice() != default_issue_states.as_slice()
                }) {
                    return "global";
                }
                "default"
            }
            "issue_types" => {
                if home.is_some_and(|home| home.issue_types.values == resolved.issue_types.values) {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.issue_types.values == resolved.issue_types.values
                        && glob.issue_types.values.as_slice() != default_issue_types.as_slice()
                }) {
                    return "global";
                }
                "default"
            }
            "issue_priorities" => {
                if home.is_some_and(|home| {
                    home.issue_priorities.values == resolved.issue_priorities.values
                }) {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.issue_priorities.values == resolved.issue_priorities.values
                        && glob.issue_priorities.values.as_slice()
                            != default_issue_priorities.as_slice()
                }) {
                    return "global";
                }
                "default"
            }
            "default_tags" => {
                if home.is_some_and(|home| home.default_tags == resolved.default_tags) {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.default_tags == resolved.default_tags
                        && glob.default_tags.as_slice() != default_default_tags.as_slice()
                }) {
                    return "global";
                }
                "default"
            }
            "members" => {
                if home.is_some_and(|home| home.members == resolved.members) {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.members == resolved.members
                        && glob.members.as_slice() != default_members.as_slice()
                }) {
                    return "global";
                }
                "default"
            }
            "strict_members" => {
                if home.is_some_and(|home| home.strict_members == resolved.strict_members) {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.strict_members == resolved.strict_members
                        && glob.strict_members != default_strict_members
                }) {
                    return "global";
                }
                "default"
            }
            "auto_populate_members" => {
                if home.is_some_and(|home| {
                    home.auto_populate_members == resolved.auto_populate_members
                }) {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.auto_populate_members == resolved.auto_populate_members
                        && glob.auto_populate_members != default_auto_populate_members
                }) {
                    return "global";
                }
                "default"
            }
            "custom_fields" => {
                if home
                    .is_some_and(|home| home.custom_fields.values == resolved.custom_fields.values)
                {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.custom_fields.values == resolved.custom_fields.values
                        && glob.custom_fields.values.as_slice() != default_custom_fields.as_slice()
                }) {
                    return "global";
                }
                "default"
            }
            "issue_tags" => {
                if home.is_some_and(|home| home.tags.values == resolved.tags.values) {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.tags.values == resolved.tags.values
                        && glob.tags.values.as_slice() != default_issue_tags.as_slice()
                }) {
                    return "global";
                }
                "default"
            }
            "scan_signal_words" => {
                if home.is_some_and(|home| home.scan_signal_words == resolved.scan_signal_words) {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.scan_signal_words == resolved.scan_signal_words
                        && glob.scan_signal_words.as_slice() != default_scan_signal_words.as_slice()
                }) {
                    return "global";
                }
                "default"
            }
            "scan_ticket_patterns" => {
                if home
                    .and_then(|home| home.scan_ticket_patterns.as_ref())
                    .is_some_and(|patterns| {
                        Some(patterns) == resolved.scan_ticket_patterns.as_ref()
                    })
                {
                    return "home";
                }
                if global
                    .and_then(|glob| glob.scan_ticket_patterns.as_ref())
                    .is_some_and(|patterns| {
                        Some(patterns) == resolved.scan_ticket_patterns.as_ref()
                            && Some(patterns) != default_scan_ticket_patterns.as_ref()
                    })
                {
                    return "global";
                }
                "default"
            }
            "scan_enable_ticket_words" => {
                if home.is_some_and(|home| {
                    home.scan_enable_ticket_words == resolved.scan_enable_ticket_words
                }) {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.scan_enable_ticket_words == resolved.scan_enable_ticket_words
                        && glob.scan_enable_ticket_words != default_scan_enable_ticket_words
                }) {
                    return "global";
                }
                "default"
            }
            "scan_enable_mentions" => {
                if home
                    .is_some_and(|home| home.scan_enable_mentions == resolved.scan_enable_mentions)
                {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.scan_enable_mentions == resolved.scan_enable_mentions
                        && glob.scan_enable_mentions != default_scan_enable_mentions
                }) {
                    return "global";
                }
                "default"
            }
            "scan_strip_attributes" => {
                if home.is_some_and(|home| {
                    home.scan_strip_attributes == resolved.scan_strip_attributes
                }) {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.scan_strip_attributes == resolved.scan_strip_attributes
                        && glob.scan_strip_attributes != default_scan_strip_attributes
                }) {
                    return "global";
                }
                "default"
            }
            "auto_set_reporter" => {
                if home.is_some_and(|home| home.auto_set_reporter == resolved.auto_set_reporter) {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.auto_set_reporter == resolved.auto_set_reporter
                        && glob.auto_set_reporter != default_auto_set_reporter
                }) {
                    return "global";
                }
                "default"
            }
            "auto_assign_on_status" => {
                if home.is_some_and(|home| {
                    home.auto_assign_on_status == resolved.auto_assign_on_status
                }) {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.auto_assign_on_status == resolved.auto_assign_on_status
                        && glob.auto_assign_on_status != default_auto_assign_on_status
                }) {
                    return "global";
                }
                "default"
            }
            "auto_codeowners_assign" => {
                if home.is_some_and(|home| {
                    home.auto_codeowners_assign == resolved.auto_codeowners_assign
                }) {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.auto_codeowners_assign == resolved.auto_codeowners_assign
                        && glob.auto_codeowners_assign != default_auto_codeowners_assign
                }) {
                    return "global";
                }
                "default"
            }
            "auto_tags_from_path" => {
                if home.is_some_and(|home| home.auto_tags_from_path == resolved.auto_tags_from_path)
                {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.auto_tags_from_path == resolved.auto_tags_from_path
                        && glob.auto_tags_from_path != default_auto_tags_from_path
                }) {
                    return "global";
                }
                "default"
            }
            "auto_branch_infer_type" => {
                if home.is_some_and(|home| {
                    home.auto_branch_infer_type == resolved.auto_branch_infer_type
                }) {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.auto_branch_infer_type == resolved.auto_branch_infer_type
                        && glob.auto_branch_infer_type != default_auto_branch_infer_type
                }) {
                    return "global";
                }
                "default"
            }
            "auto_branch_infer_status" => {
                if home.is_some_and(|home| {
                    home.auto_branch_infer_status == resolved.auto_branch_infer_status
                }) {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.auto_branch_infer_status == resolved.auto_branch_infer_status
                        && glob.auto_branch_infer_status != default_auto_branch_infer_status
                }) {
                    return "global";
                }
                "default"
            }
            "auto_branch_infer_priority" => {
                if home.is_some_and(|home| {
                    home.auto_branch_infer_priority == resolved.auto_branch_infer_priority
                }) {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.auto_branch_infer_priority == resolved.auto_branch_infer_priority
                        && glob.auto_branch_infer_priority != default_auto_branch_infer_priority
                }) {
                    return "global";
                }
                "default"
            }
            "auto_identity" => {
                if home.is_some_and(|home| home.auto_identity == resolved.auto_identity) {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.auto_identity == resolved.auto_identity
                        && glob.auto_identity != default_auto_identity
                }) {
                    return "global";
                }
                "default"
            }
            "auto_identity_git" => {
                if home.is_some_and(|home| home.auto_identity_git == resolved.auto_identity_git) {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.auto_identity_git == resolved.auto_identity_git
                        && glob.auto_identity_git != default_auto_identity_git
                }) {
                    return "global";
                }
                "default"
            }
            "branch_type_aliases" => {
                if home.is_some_and(|home| home.branch_type_aliases == resolved.branch_type_aliases)
                {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.branch_type_aliases == resolved.branch_type_aliases
                        && glob.branch_type_aliases != default_branch_type_aliases
                }) {
                    return "global";
                }
                "default"
            }
            "branch_status_aliases" => {
                if home.is_some_and(|home| {
                    home.branch_status_aliases == resolved.branch_status_aliases
                }) {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.branch_status_aliases == resolved.branch_status_aliases
                        && glob.branch_status_aliases != default_branch_status_aliases
                }) {
                    return "global";
                }
                "default"
            }
            "branch_priority_aliases" => {
                if home.is_some_and(|home| {
                    home.branch_priority_aliases == resolved.branch_priority_aliases
                }) {
                    return "home";
                }
                if global.is_some_and(|glob| {
                    glob.branch_priority_aliases == resolved.branch_priority_aliases
                        && glob.branch_priority_aliases != default_branch_priority_aliases
                }) {
                    return "global";
                }
                "default"
            }
            _ => "default",
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn source_label_for_project(
        key: &str,
        resolved_project: &crate::config::types::ResolvedConfig,
        base_config: &crate::config::types::ResolvedConfig,
        project_cfg: Option<&crate::config::types::ProjectConfig>,
        global_cfg: &Option<crate::config::types::GlobalConfig>,
        home_cfg: &Option<crate::config::types::GlobalConfig>,
    ) -> String {
        match key {
            "default_project" => "project".to_string(),
            "default_assignee" => {
                if project_cfg
                    .and_then(|pc| pc.default_assignee.as_ref())
                    .is_some()
                    || resolved_project.default_assignee != base_config.default_assignee
                {
                    "project".to_string()
                } else {
                    Self::source_label_for_global(base_config, global_cfg, home_cfg, key)
                        .to_string()
                }
            }
            "default_reporter" => {
                if project_cfg
                    .and_then(|pc| pc.default_reporter.as_ref())
                    .is_some()
                    || resolved_project.default_reporter != base_config.default_reporter
                {
                    "project".to_string()
                } else {
                    Self::source_label_for_global(base_config, global_cfg, home_cfg, key)
                        .to_string()
                }
            }
            "default_priority" => {
                if project_cfg
                    .and_then(|pc| pc.default_priority.as_ref())
                    .is_some()
                    || resolved_project.default_priority != base_config.default_priority
                {
                    "project".to_string()
                } else {
                    Self::source_label_for_global(base_config, global_cfg, home_cfg, key)
                        .to_string()
                }
            }
            "default_status" => {
                if project_cfg
                    .and_then(|pc| pc.default_status.as_ref())
                    .is_some()
                    || resolved_project.default_status != base_config.default_status
                {
                    "project".to_string()
                } else {
                    Self::source_label_for_global(base_config, global_cfg, home_cfg, key)
                        .to_string()
                }
            }
            "default_tags" => {
                if project_cfg
                    .and_then(|pc| pc.default_tags.as_ref())
                    .is_some()
                    || resolved_project.default_tags != base_config.default_tags
                {
                    "project".to_string()
                } else {
                    Self::source_label_for_global(base_config, global_cfg, home_cfg, key)
                        .to_string()
                }
            }
            "members" => {
                if project_cfg.and_then(|pc| pc.members.as_ref()).is_some()
                    || resolved_project.members != base_config.members
                {
                    "project".to_string()
                } else {
                    Self::source_label_for_global(base_config, global_cfg, home_cfg, key)
                        .to_string()
                }
            }
            "strict_members" => {
                if project_cfg.and_then(|pc| pc.strict_members).is_some()
                    || resolved_project.strict_members != base_config.strict_members
                {
                    "project".to_string()
                } else {
                    Self::source_label_for_global(base_config, global_cfg, home_cfg, key)
                        .to_string()
                }
            }
            "issue_states" => {
                if project_cfg
                    .and_then(|pc| pc.issue_states.as_ref())
                    .is_some()
                    || resolved_project.issue_states.values != base_config.issue_states.values
                {
                    "project".to_string()
                } else {
                    Self::source_label_for_global(base_config, global_cfg, home_cfg, key)
                        .to_string()
                }
            }
            "issue_types" => {
                if project_cfg.and_then(|pc| pc.issue_types.as_ref()).is_some()
                    || resolved_project.issue_types.values != base_config.issue_types.values
                {
                    "project".to_string()
                } else {
                    Self::source_label_for_global(base_config, global_cfg, home_cfg, key)
                        .to_string()
                }
            }
            "issue_priorities" => {
                if project_cfg
                    .and_then(|pc| pc.issue_priorities.as_ref())
                    .is_some()
                    || resolved_project.issue_priorities.values
                        != base_config.issue_priorities.values
                {
                    "project".to_string()
                } else {
                    Self::source_label_for_global(base_config, global_cfg, home_cfg, key)
                        .to_string()
                }
            }
            "issue_tags" => {
                if project_cfg
                    .and_then(|pc| pc.tags.as_ref())
                    .map(|tags| tags.values == resolved_project.tags.values)
                    .unwrap_or(false)
                    || resolved_project.tags.values != base_config.tags.values
                {
                    "project".to_string()
                } else {
                    Self::source_label_for_global(base_config, global_cfg, home_cfg, key)
                        .to_string()
                }
            }
            "custom_fields" => {
                if project_cfg
                    .and_then(|pc| pc.custom_fields.as_ref())
                    .map(|fields| fields.values == resolved_project.custom_fields.values)
                    .unwrap_or(false)
                    || resolved_project.custom_fields.values != base_config.custom_fields.values
                {
                    "project".to_string()
                } else {
                    Self::source_label_for_global(base_config, global_cfg, home_cfg, key)
                        .to_string()
                }
            }
            "scan_signal_words" => {
                if project_cfg
                    .and_then(|pc| pc.scan_signal_words.as_ref())
                    .is_some()
                    || resolved_project.scan_signal_words != base_config.scan_signal_words
                {
                    "project".to_string()
                } else {
                    Self::source_label_for_global(base_config, global_cfg, home_cfg, key)
                        .to_string()
                }
            }
            "scan_ticket_patterns" => {
                if project_cfg
                    .and_then(|pc| pc.scan_ticket_patterns.as_ref())
                    .is_some()
                    || resolved_project.scan_ticket_patterns != base_config.scan_ticket_patterns
                {
                    "project".to_string()
                } else {
                    Self::source_label_for_global(base_config, global_cfg, home_cfg, key)
                        .to_string()
                }
            }
            "scan_enable_ticket_words" => {
                if project_cfg
                    .and_then(|pc| pc.scan_enable_ticket_words)
                    .is_some()
                    || resolved_project.scan_enable_ticket_words
                        != base_config.scan_enable_ticket_words
                {
                    "project".to_string()
                } else {
                    Self::source_label_for_global(base_config, global_cfg, home_cfg, key)
                        .to_string()
                }
            }
            "scan_enable_mentions" => {
                if project_cfg.and_then(|pc| pc.scan_enable_mentions).is_some()
                    || resolved_project.scan_enable_mentions != base_config.scan_enable_mentions
                {
                    "project".to_string()
                } else {
                    Self::source_label_for_global(base_config, global_cfg, home_cfg, key)
                        .to_string()
                }
            }
            "scan_strip_attributes" => {
                if project_cfg
                    .and_then(|pc| pc.scan_strip_attributes)
                    .is_some()
                    || resolved_project.scan_strip_attributes != base_config.scan_strip_attributes
                {
                    "project".to_string()
                } else {
                    Self::source_label_for_global(base_config, global_cfg, home_cfg, key)
                        .to_string()
                }
            }
            "auto_populate_members" => {
                if project_cfg
                    .and_then(|pc| pc.auto_populate_members)
                    .is_some()
                    || resolved_project.auto_populate_members != base_config.auto_populate_members
                {
                    "project".to_string()
                } else {
                    Self::source_label_for_global(base_config, global_cfg, home_cfg, key)
                        .to_string()
                }
            }
            "auto_set_reporter" => {
                if project_cfg.and_then(|pc| pc.auto_set_reporter).is_some()
                    || resolved_project.auto_set_reporter != base_config.auto_set_reporter
                {
                    "project".to_string()
                } else {
                    Self::source_label_for_global(base_config, global_cfg, home_cfg, key)
                        .to_string()
                }
            }
            "auto_assign_on_status" => {
                if project_cfg
                    .and_then(|pc| pc.auto_assign_on_status)
                    .is_some()
                    || resolved_project.auto_assign_on_status != base_config.auto_assign_on_status
                {
                    "project".to_string()
                } else {
                    Self::source_label_for_global(base_config, global_cfg, home_cfg, key)
                        .to_string()
                }
            }
            "auto_codeowners_assign"
            | "auto_tags_from_path"
            | "auto_branch_infer_type"
            | "auto_branch_infer_status"
            | "auto_branch_infer_priority"
            | "auto_identity"
            | "auto_identity_git" => {
                Self::source_label_for_global(base_config, global_cfg, home_cfg, key).to_string()
            }
            "branch_type_aliases" => {
                if project_cfg
                    .and_then(|pc| pc.branch_type_aliases.as_ref())
                    .is_some()
                    || resolved_project.branch_type_aliases != base_config.branch_type_aliases
                {
                    "project".to_string()
                } else {
                    Self::source_label_for_global(base_config, global_cfg, home_cfg, key)
                        .to_string()
                }
            }
            "branch_status_aliases" => {
                if project_cfg
                    .and_then(|pc| pc.branch_status_aliases.as_ref())
                    .is_some()
                    || resolved_project.branch_status_aliases != base_config.branch_status_aliases
                {
                    "project".to_string()
                } else {
                    Self::source_label_for_global(base_config, global_cfg, home_cfg, key)
                        .to_string()
                }
            }
            "branch_priority_aliases" => {
                if project_cfg
                    .and_then(|pc| pc.branch_priority_aliases.as_ref())
                    .is_some()
                    || resolved_project.branch_priority_aliases
                        != base_config.branch_priority_aliases
                {
                    "project".to_string()
                } else {
                    Self::source_label_for_global(base_config, global_cfg, home_cfg, key)
                        .to_string()
                }
            }
            "server_port" => {
                Self::source_label_for_global(base_config, global_cfg, home_cfg, key).to_string()
            }
            _ => Self::source_label_for_global(base_config, global_cfg, home_cfg, key).to_string(),
        }
    }
    fn handle_config_normalize(
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
        global: bool,
        project: Option<String>,
        write: bool,
    ) -> Result<(), String> {
        use crate::utils::paths;
        use std::path::PathBuf;

        let mut targets: Vec<(String, PathBuf)> = Vec::new();

        if global || project.is_none() {
            let path = paths::global_config_path(&resolver.path);
            if path.exists() {
                targets.push(("global".to_string(), path));
            }
        }

        if let Some(proj) = project {
            let path = paths::project_config_path(&resolver.path, &proj);
            if !path.exists() {
                return Err(format!("Project config not found: {}", path.display()));
            }
            targets.push((format!("project:{}", proj), path));
        } else if !global {
            // If neither --global nor --project specified, normalize all project configs
            for (prefix, dir) in crate::utils::filesystem::list_visible_subdirs(&resolver.path) {
                let cfg = dir.join("config.yml");
                if cfg.exists() {
                    targets.push((format!("project:{}", prefix), cfg));
                }
            }
        }

        if targets.is_empty() {
            renderer.emit_info("No configuration files found to normalize");
            return Ok(());
        }

        let mut changed = 0usize;
        for (label, path) in targets {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

            // Attempt normalization via round-trip through our tolerant parsers,
            // then emit in canonical nested style.
            let canonical = if label == "global" {
                let parsed = crate::config::normalization::parse_global_from_yaml_str(&content)
                    .map_err(|e| e.to_string())?;
                crate::config::normalization::to_canonical_global_yaml(&parsed)
            } else {
                // label is project:<prefix>
                let proj = label.split_once(':').map(|(_, p)| p).unwrap_or("");
                let parsed =
                    crate::config::normalization::parse_project_from_yaml_str(proj, &content)
                        .map_err(|e| e.to_string())?;
                crate::config::normalization::to_canonical_project_yaml(&parsed)
            };

            if write {
                std::fs::write(&path, canonical.as_bytes())
                    .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;
                renderer.emit_success(&format!("Normalized {} -> {}", label, path.display()));
                changed += 1;
            } else {
                renderer.emit_info(&format!("Would normalize {} -> {}", label, path.display()));
                renderer.emit_raw_stdout(&canonical);
            }
        }

        if write {
            renderer.emit_success(&format!(
                "Normalization complete ({} file(s) updated)",
                changed
            ));
        }
        Ok(())
    }
    /// Handle config show command with optional project filter
    fn handle_config_show(
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
        project: Option<String>,
        explain: bool,
        full: bool,
    ) -> Result<(), String> {
        // Git-like behavior: adopt the parent tasks root if found (read-only is fine to inherit)
        let effective_read_root = resolver.path.clone();

        let config_manager =
            ConfigManager::new_manager_with_tasks_dir_readonly(&effective_read_root)
                .map_err(|e| format!("Failed to load config: {}", e))?;

        let colorize_comments =
            std::env::var("NO_COLOR").is_err() && std::io::stdout().is_terminal();

        if !matches!(renderer.format, crate::output::OutputFormat::Json) {
            let mut message = format!("Tasks directory: {}", effective_read_root.display());
            if effective_read_root.is_relative() {
                if let Ok(resolved) = std::fs::canonicalize(&effective_read_root) {
                    message.push_str(&format!(" (resolved: {})", resolved.display()));
                }
            }
            renderer.emit_info(&message);
        }

        if let Some(project_name) = project {
            let project_prefix = resolve_project_input(&project_name, resolver.path.as_path());
            let resolved_project = config_manager
                .get_project_config(&project_prefix)
                .map_err(|e| format!("Failed to load project config: {}", e))?;

            let project_cfg_raw = crate::config::persistence::load_project_config_from_dir(
                &project_prefix,
                &effective_read_root,
            )
            .ok();
            let project_label = crate::utils::project::format_project_label(
                &project_prefix,
                project_cfg_raw.as_ref().and_then(|cfg| {
                    let name = cfg.project_name.trim();
                    if name.is_empty() { None } else { Some(name) }
                }),
            );
            let global_cfg =
                crate::config::persistence::load_global_config(Some(&effective_read_root)).ok();
            let home_cfg = crate::config::persistence::load_home_config().ok();
            let base_config = config_manager.get_resolved_config().clone();
            let project_sources = Self::build_project_source_labels(
                &resolved_project,
                &base_config,
                project_cfg_raw.as_ref(),
                &global_cfg,
                &home_cfg,
            );

            const PROJECT_ONLY_SOURCES: &[&str] = &["project"];
            let options = YamlRenderOptions {
                include_defaults: full,
                include_comments: explain,
                colorize_comments,
                allowed_sources: if full {
                    None
                } else {
                    Some(PROJECT_ONLY_SOURCES)
                },
            };

            Self::emit_config_yaml(
                renderer,
                "project",
                Some(project_label.as_str()),
                &resolved_project,
                &project_sources,
                &options,
            );
        } else {
            let resolved_config = config_manager.get_resolved_config();
            let global_cfg =
                crate::config::persistence::load_global_config(Some(&effective_read_root)).ok();
            let home_cfg = crate::config::persistence::load_home_config().ok();
            let sources = Self::build_global_source_labels(resolved_config, &global_cfg, &home_cfg);

            const GLOBAL_SOURCES: &[&str] = &["env", "home", "global"];
            let options = YamlRenderOptions {
                include_defaults: full,
                include_comments: explain,
                colorize_comments,
                allowed_sources: if full { None } else { Some(GLOBAL_SOURCES) },
            };

            Self::emit_config_yaml(
                renderer,
                "global",
                None,
                resolved_config,
                &sources,
                &options,
            );
        }

        Ok(())
    }

    fn build_global_source_labels(
        resolved: &crate::config::types::ResolvedConfig,
        global_cfg: &Option<crate::config::types::GlobalConfig>,
        home_cfg: &Option<crate::config::types::GlobalConfig>,
    ) -> HashMap<String, String> {
        let mut labels = HashMap::new();
        let mut insert = |path: &str, key: &str| {
            let label = Self::source_label_for_global(resolved, global_cfg, home_cfg, key);
            labels.insert(path.to_string(), label.to_string());
        };

        insert("server.port", "server_port");
        insert("default.project", "default_project");
        insert("default.assignee", "default_assignee");
        insert("default.reporter", "default_reporter");
        insert("default.tags", "default_tags");
        insert("members", "members");
        insert("default.strict-members", "strict_members");
        insert("default.priority", "default_priority");
        insert("default.status", "default_status");
        insert("issue.states", "issue_states");
        insert("issue.types", "issue_types");
        insert("issue.priorities", "issue_priorities");
        insert("issue.tags", "issue_tags");
        insert("custom.fields", "custom_fields");
        insert("scan.signal-words", "scan_signal_words");
        insert("scan.ticket-patterns", "scan_ticket_patterns");
        insert("scan.enable-ticket-words", "scan_enable_ticket_words");
        insert("scan.enable-mentions", "scan_enable_mentions");
        insert("scan.strip-attributes", "scan_strip_attributes");
        insert("auto.populate-members", "auto_populate_members");
        insert("auto.set-reporter", "auto_set_reporter");
        insert("auto.assign-on-status", "auto_assign_on_status");
        insert("auto.codeowners-assign", "auto_codeowners_assign");
        insert("auto.tags-from-path", "auto_tags_from_path");
        insert("auto.branch-infer-type", "auto_branch_infer_type");
        insert("auto.branch-infer-status", "auto_branch_infer_status");
        insert("auto.branch-infer-priority", "auto_branch_infer_priority");
        insert("auto.identity", "auto_identity");
        insert("auto.identity-git", "auto_identity_git");
        insert("branch.type-aliases", "branch_type_aliases");
        insert("branch.status-aliases", "branch_status_aliases");
        insert("branch.priority-aliases", "branch_priority_aliases");

        labels
    }

    #[allow(clippy::too_many_arguments)]
    fn build_project_source_labels(
        resolved_project: &crate::config::types::ResolvedConfig,
        base_config: &crate::config::types::ResolvedConfig,
        project_cfg: Option<&crate::config::types::ProjectConfig>,
        global_cfg: &Option<crate::config::types::GlobalConfig>,
        home_cfg: &Option<crate::config::types::GlobalConfig>,
    ) -> HashMap<String, String> {
        let mut labels = HashMap::new();
        let mut insert = |path: &str, key: &str| {
            let label = Self::source_label_for_project(
                key,
                resolved_project,
                base_config,
                project_cfg,
                global_cfg,
                home_cfg,
            );
            labels.insert(path.to_string(), label);
        };

        insert("server.port", "server_port");
        insert("default.project", "default_project");
        insert("default.assignee", "default_assignee");
        insert("default.reporter", "default_reporter");
        insert("default.tags", "default_tags");
        insert("members", "members");
        insert("default.strict-members", "strict_members");
        insert("default.priority", "default_priority");
        insert("default.status", "default_status");
        insert("issue.states", "issue_states");
        insert("issue.types", "issue_types");
        insert("issue.priorities", "issue_priorities");
        insert("issue.tags", "issue_tags");
        insert("custom.fields", "custom_fields");
        insert("scan.signal-words", "scan_signal_words");
        insert("scan.ticket-patterns", "scan_ticket_patterns");
        insert("scan.enable-ticket-words", "scan_enable_ticket_words");
        insert("scan.enable-mentions", "scan_enable_mentions");
        insert("scan.strip-attributes", "scan_strip_attributes");
        insert("auto.populate-members", "auto_populate_members");
        insert("auto.set-reporter", "auto_set_reporter");
        insert("auto.assign-on-status", "auto_assign_on_status");
        insert("auto.codeowners-assign", "auto_codeowners_assign");
        insert("auto.tags-from-path", "auto_tags_from_path");
        insert("auto.branch-infer-type", "auto_branch_infer_type");
        insert("auto.branch-infer-status", "auto_branch_infer_status");
        insert("auto.branch-infer-priority", "auto_branch_infer_priority");
        insert("auto.identity", "auto_identity");
        insert("auto.identity-git", "auto_identity_git");
        insert("branch.type-aliases", "branch_type_aliases");
        insert("branch.status-aliases", "branch_status_aliases");
        insert("branch.priority-aliases", "branch_priority_aliases");

        labels
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_config_set(
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
        field: String,
        value: String,
        dry_run: bool,
        force: bool,
        mut global: bool,
        project: Option<&str>,
    ) -> Result<(), String> {
        // Auto-detect global-only fields
        let global_only_fields = ["server_port", "default_prefix", "default_project"];
        if global_only_fields.contains(&field.as_str()) && !global {
            global = true;
            if !dry_run {
                renderer.emit_info(&format!(
                    "Automatically treating '{}' as global configuration field",
                    field
                ));
            }
        }

        if dry_run {
            renderer.emit_info(&format!("DRY RUN: Would set {} = {}", field, value));

            // Check for validation conflicts
            let conflicts = Self::check_validation_conflicts(resolver, &field, &value, global)?;
            if !conflicts.is_empty() {
                renderer.emit_warning("WARNING: This change would cause validation conflicts:");
                for conflict in conflicts {
                    renderer.emit_raw_stdout(&format!("  • {}", conflict));
                }
                if !force {
                    renderer
                        .emit_info("Use --force to apply anyway, or fix conflicting values first.");
                    return Ok(());
                }
            }

            renderer.emit_success(
                "Dry run completed. Use the same command without --dry-run to apply.",
            );
            return Ok(());
        }

        renderer.emit_info(&format!("Setting configuration: {} = {}", field, value));

        // Check for validation conflicts unless forced
        if !force {
            let conflicts = Self::check_validation_conflicts(resolver, &field, &value, global)?;
            if !conflicts.is_empty() {
                renderer.emit_warning("WARNING: This change would cause validation conflicts:");
                for conflict in conflicts {
                    renderer.emit_raw_stdout(&format!("  • {}", conflict));
                }
                renderer.emit_info(
                    "Use --dry-run to see what would change, or --force to apply anyway.",
                );
                return Err("Configuration change blocked due to validation conflicts".to_string());
            }
        }

        // Validate field name and value
        ConfigManager::validate_field_name(&field, global)
            .map_err(|e| format!("Validation error: {}", e))?;
        ConfigManager::validate_field_value(&field, &value)
            .map_err(|e| format!("Validation error: {}", e))?;

        // Determine project prefix if not global
        let project_prefix = if global {
            None
        } else if let Some(explicit_project) = project.and_then(|p| {
            let trimmed = p.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(resolve_project_input(trimmed, &resolver.path))
            }
        }) {
            Some(explicit_project)
        } else {
            // For project-specific config, we need to determine the project
            // This could be explicitly provided or auto-detected from current context
            // For now, let's use the default project if available
            let config_manager =
                ConfigManager::new_manager_with_tasks_dir_ensure_config(&resolver.path)
                    .map_err(|e| format!("Failed to load config: {}", e))?;
            let default_prefix = config_manager.get_resolved_config().default_prefix.clone();

            if !default_prefix.is_empty() {
                Some(default_prefix)
            } else {
                return Err(
                    "No default project set. Use --global flag or set a default project first."
                        .to_string(),
                );
            }
        };

        let mut validation_warnings: Vec<String> = Vec::new();

        // Update the configuration
        let validation = ConfigManager::update_config_field(
            &resolver.path,
            &field,
            &value,
            project_prefix.as_deref(),
        )
        .map_err(|e| format!("Failed to update config: {}", e))?;

        validation_warnings.extend(validation.warnings.iter().map(|w| w.to_string()));

        // Show helpful information about project-specific config
        if project_prefix.is_some() {
            // Check if the value matches the global default and inform the user
            if Self::check_matches_global_default(&field, &value, &resolver.path) {
                renderer.emit_info(
                    "Note: This project setting matches the global default. This project will now use this explicit value and won't inherit future global changes to this field.",
                );
            }
        }
        renderer.emit_success(&format!("Successfully updated {}", field));

        if !validation_warnings.is_empty() {
            renderer.emit_warning("Validation warnings detected after applying the change:");
            for warning in validation_warnings {
                renderer.emit_warning(&warning);
            }
        }
        Ok(())
    }

    /// Check if a field value matches the global default
    fn check_matches_global_default(field: &str, value: &str, tasks_dir: &std::path::Path) -> bool {
        // Load global config to compare
        if let Ok(config_manager) = ConfigManager::new_manager_with_tasks_dir_readonly(tasks_dir) {
            let global_config = config_manager.get_resolved_config();

            match field {
                "default_priority" => {
                    if let Ok(priority) = value.parse::<Priority>() {
                        return priority == global_config.default_priority;
                    }
                }
                "default_status" => {
                    if let Ok(status) = value.parse::<TaskStatus>() {
                        return global_config.default_status.as_ref() == Some(&status);
                    }
                }
                "default_assignee" => {
                    return global_config.default_assignee.as_deref() == Some(value);
                }
                // For other fields, we'd need to compare arrays which is more complex
                // For now, just handle the simple cases
                _ => {}
            }
        }
        false
    }

    /// Handle config init command
    #[allow(clippy::too_many_arguments)]
    fn handle_config_init(
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
        template: String,
        prefix: Option<String>,
        project: Option<String>,
        copy_from: Option<String>,
        global: bool,
        dry_run: bool,
        force: bool,
    ) -> Result<(), String> {
        if dry_run {
            renderer.emit_info(&format!(
                "DRY RUN: Would initialize config with template '{}'",
                template
            ));
            if let Some(ref prefix) = prefix {
                renderer.emit_raw_stdout(&format!("  • Project prefix: {}", prefix));
                // Validate explicit prefix
                if let Some(ref project_name) = project {
                    if let Err(conflict) =
                        validate_explicit_prefix(prefix, project_name, &resolver.path)
                    {
                        renderer.emit_raw_stdout(&format!("  ❌ Conflict detected: {}", conflict));
                        return Err(conflict);
                    }
                    renderer.emit_raw_stdout(&format!("  ✅ Prefix '{}' is available", prefix));
                }
            }
            if let Some(ref project) = project {
                renderer.emit_raw_stdout(&format!("  • Project name: {}", project));
                // Show what prefix would be generated and check for conflicts
                if prefix.is_none() {
                    match generate_unique_project_prefix(project, &resolver.path) {
                        Ok(generated_prefix) => {
                            renderer.emit_raw_stdout(&format!(
                                "  • Generated prefix: {} ✅",
                                generated_prefix
                            ));
                        }
                        Err(conflict) => {
                            renderer.emit_raw_stdout(&format!(
                                "  • Generated prefix: {} ❌",
                                generate_project_prefix(project)
                            ));
                            renderer
                                .emit_raw_stdout(&format!("  ❌ Conflict detected: {}", conflict));
                            return Err(conflict);
                        }
                    }
                }
            }
            if let Some(ref copy_from) = copy_from {
                renderer.emit_raw_stdout(&format!("  • Copy settings from: {}", copy_from));
            }
            if global {
                renderer.emit_raw_stdout("  • Target: Global configuration (.tasks/config.yml)");
            } else {
                let project_name = project.as_deref().unwrap_or("DEFAULT");
                let project_prefix = if let Some(ref prefix) = prefix {
                    prefix.clone()
                } else {
                    match generate_unique_project_prefix(project_name, &resolver.path) {
                        Ok(prefix) => prefix,
                        Err(_) => generate_project_prefix(project_name), // For display purposes
                    }
                };
                renderer.emit_raw_stdout(&format!(
                    "  • Target: Project configuration (.tasks/{}/config.yml)",
                    project_prefix
                ));
            }
            renderer.emit_success(
                "Dry run completed. Use the same command without --dry-run to apply.",
            );
            return Ok(());
        }

        // Standardized info message for initialization
        renderer.emit_info(&format!(
            "Initializing configuration with template '{}'",
            template
        ));

        // Load template
        let template_config = Self::load_template(&template)?;

        // Apply template with customizations
        Self::apply_template_config(
            resolver,
            renderer,
            template_config,
            prefix,
            project,
            copy_from,
            global,
            force,
        )
    }

    /// Check for validation conflicts when changing config
    fn check_validation_conflicts(
        _resolver: &TasksDirectoryResolver,
        field: &str,
        new_value: &str,
        _global: bool,
    ) -> Result<Vec<String>, String> {
        let mut conflicts = Vec::new();

        // TODO (LOTA-4): Implement actual conflict detection
        // This would:
        // 1. Load existing tasks
        // 2. Check if any task values would become invalid with new config
        // 3. Return list of conflicting tasks/values

        // For now, just simulate some example conflicts
        if field == "issue_states.values" && new_value.contains("In-Progress") {
            conflicts.push(
                "Task PROJ-1 has status 'InProgress' which doesn't match new 'In-Progress'"
                    .to_string(),
            );
        }

        Ok(conflicts)
    }

    /// Load a configuration template
    fn load_template(template_name: &str) -> Result<serde_yaml::Value, String> {
        // For now, return a basic template structure
        // TODO (LOTA-5): Load actual template from embedded files or resources
        let template_content = match template_name {
            "default" => include_str!("../../config/templates/default.yml"),
            "agile" => include_str!("../../config/templates/agile.yml"),
            "kanban" => include_str!("../../config/templates/kanban.yml"),
            _ => return Err(format!("Unknown template: {}", template_name)),
        };

        serde_yaml::from_str(template_content)
            .map_err(|e| format!("Failed to parse template '{}': {}", template_name, e))
    }

    /// Apply template configuration
    #[allow(clippy::too_many_arguments)]
    fn apply_template_config(
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
        template: serde_yaml::Value,
        prefix: Option<String>,
        project: Option<String>,
        copy_from: Option<String>,
        global: bool,
        force: bool,
    ) -> Result<(), String> {
        // Extract config from template
        let template_map = template.as_mapping().ok_or("Invalid template format")?;

        let config_section = template_map
            .get(serde_yaml::Value::String("config".to_string()))
            .ok_or("Template missing 'config' section")?;

        let mut config = config_section.clone();

        if let Some(config_map) = config.as_mapping_mut() {
            Self::normalize_template_config(config_map);
        }

        if let Some(config_map) = config.as_mapping_mut() {
            if let Some(source_project) = copy_from.as_deref() {
                Self::merge_config_from_project(config_map, resolver, renderer, source_project)?;
                // Re-normalize after merge to ensure canonical structure
                Self::normalize_template_config(config_map);
            }

            if let Some(project_name) = project.as_ref() {
                Self::set_nested_value(
                    config_map,
                    &["project", "name"],
                    serde_yaml::Value::String(project_name.clone()),
                );
            }
        }

        // Determine target path
        let config_path = if global {
            // Ensure tasks directory exists for global config
            fs::create_dir_all(&resolver.path)
                .map_err(|e| format!("Failed to create tasks directory: {}", e))?;
            crate::utils::paths::global_config_path(&resolver.path)
        } else {
            let detected_project_name = Self::extract_project_name(&config);
            let project_name_owned = project
                .as_deref()
                .map(|s| s.to_string())
                .or_else(|| detected_project_name.clone())
                .unwrap_or_else(|| "DEFAULT".to_string());

            // Generate prefix from project name with conflict detection
            let project_prefix = if let Some(explicit_prefix) = &prefix {
                // User provided explicit prefix, validate it doesn't conflict
                validate_explicit_prefix(explicit_prefix, &project_name_owned, &resolver.path)?;
                explicit_prefix.clone()
            } else {
                // Generate prefix with conflict detection
                generate_unique_project_prefix(&project_name_owned, &resolver.path)?
            };

            let project_dir = crate::utils::paths::project_dir(&resolver.path, &project_prefix);
            fs::create_dir_all(&project_dir)
                .map_err(|e| format!("Failed to create project directory: {}", e))?;
            crate::utils::paths::project_config_path(&resolver.path, &project_prefix)
        };

        // Check if config already exists
        if config_path.exists() && !force {
            return Err(format!(
                "Configuration already exists at {}. Use --force to overwrite.",
                config_path.display()
            ));
        }

        // Write configuration using canonical nested format
        let tmp_yaml = serde_yaml::to_string(&config)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;
        let canonical = if config_path.parent() == Some(&resolver.path) {
            // Global config
            let parsed = crate::config::normalization::parse_global_from_yaml_str(&tmp_yaml)
                .map_err(|e| format!("Failed to parse config for canonicalization: {}", e))?;
            Self::validate_generated_global_config(resolver, renderer, &parsed)?;
            crate::config::normalization::to_canonical_global_yaml(&parsed)
        } else {
            // Project config; derive prefix from parent dir name
            let prefix = config_path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                .unwrap_or("");
            // Use the human-readable project name for storage metadata if provided
            let detected_project_name = Self::extract_project_name(&config);
            let project_name_value = project
                .as_deref()
                .or(detected_project_name.as_deref())
                .unwrap_or(prefix);
            let parsed = crate::config::normalization::parse_project_from_yaml_str(
                project_name_value,
                &tmp_yaml,
            )
            .map_err(|e| format!("Failed to parse project config for canonicalization: {}", e))?;
            Self::validate_generated_project_config(resolver, renderer, &parsed)?;
            crate::config::normalization::to_canonical_project_yaml(&parsed)
        };

        fs::write(&config_path, canonical)
            .map_err(|e| format!("Failed to write config file: {}", e))?;

        renderer.emit_success(&format!(
            "Configuration initialized at: {}",
            config_path.display()
        ));
        Ok(())
    }

    /// Merge configuration from another project
    fn merge_config_from_project(
        target_config: &mut serde_yaml::Mapping,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
        source_project: &str,
    ) -> Result<(), String> {
        let source_config_path =
            crate::utils::paths::project_config_path(&resolver.path, source_project);

        if !source_config_path.exists() {
            return Err(format!(
                "Source project '{}' does not exist",
                source_project
            ));
        }

        let source_content = fs::read_to_string(&source_config_path)
            .map_err(|e| format!("Failed to read source config: {}", e))?;

        let source_config: serde_yaml::Value = serde_yaml::from_str(&source_content)
            .map_err(|e| format!("Failed to parse source config: {}", e))?;

        if let Some(source_map) = source_config.as_mapping() {
            // Copy relevant fields (excluding identity-specific keys like project name/prefix)
            for (key, value) in source_map {
                if let Some(key_str) = key.as_str()
                    && key_str != "project_name"
                    && key_str != "prefix"
                    && key_str != "project"
                {
                    target_config.insert(key.clone(), value.clone());
                }
            }
        }

        renderer.emit_info(&format!(
            "Copied settings from project '{}'",
            source_project
        ));
        Ok(())
    }

    fn normalize_template_config(config_map: &mut serde_yaml::Mapping) {
        use serde_yaml::Value as Y;

        if let Some(project_name_value) = config_map.remove(Y::String("project_name".into())) {
            Self::set_nested_value(config_map, &["project", "name"], project_name_value);
        }

        // "prefix" is only used for CLI arguments; canonical templates shouldn't store it
        config_map.remove(Y::String("prefix".into()));

        if let Some(value) = config_map.remove(Y::String("issue_states".into())) {
            let normalized = Self::unwrap_template_values(value);
            Self::set_nested_value(config_map, &["issue", "states"], normalized);
        }
        if let Some(value) = config_map.remove(Y::String("issue_types".into())) {
            let normalized = Self::unwrap_template_values(value);
            Self::set_nested_value(config_map, &["issue", "types"], normalized);
        }
        if let Some(value) = config_map.remove(Y::String("issue_priorities".into())) {
            let normalized = Self::unwrap_template_values(value);
            Self::set_nested_value(config_map, &["issue", "priorities"], normalized);
        }
        if let Some(value) = config_map.remove(Y::String("tags".into())) {
            let normalized = Self::unwrap_template_values(value);
            Self::set_nested_value(config_map, &["issue", "tags"], normalized);
        }
        if let Some(value) = config_map.remove(Y::String("categories".into())) {
            let normalized = Self::unwrap_template_values(value);
            Self::set_nested_value(config_map, &["issue", "categories"], normalized);
        }
        if let Some(value) = config_map.remove(Y::String("custom_fields".into())) {
            let normalized = Self::unwrap_template_values(value);
            Self::set_nested_value(config_map, &["custom", "fields"], normalized);
        }
    }

    fn unwrap_template_values(value: serde_yaml::Value) -> serde_yaml::Value {
        use serde_yaml::Value as Y;
        match value {
            Y::Mapping(mut map) => {
                let values_key = Y::String("values".into());
                if let Some(inner) = map.remove(&values_key) {
                    return inner;
                }
                let primitive_key = Y::String("primitive".into());
                if let Some(inner) = map.remove(&primitive_key) {
                    return inner;
                }
                Y::Mapping(map)
            }
            other => other,
        }
    }

    fn set_nested_value(map: &mut serde_yaml::Mapping, path: &[&str], value: serde_yaml::Value) {
        use serde_yaml::Value as Y;

        if path.is_empty() {
            return;
        }

        let key = Y::String(path[0].to_string());
        if path.len() == 1 {
            map.insert(key, value);
            return;
        }

        let mut child = match map.remove(&key) {
            Some(Y::Mapping(existing)) => existing,
            _ => serde_yaml::Mapping::new(),
        };

        Self::set_nested_value(&mut child, &path[1..], value);
        map.insert(key, Y::Mapping(child));
    }

    fn extract_project_name(config: &serde_yaml::Value) -> Option<String> {
        use serde_yaml::Value as Y;

        let map = config.as_mapping()?;
        if let Some(project_value) = map.get(Y::String("project".into())) {
            if let Some(project_map) = project_value.as_mapping() {
                if let Some(name_value) = project_map.get(Y::String("name".into())) {
                    if let Some(name_str) = name_value.as_str() {
                        let trimmed = name_str.trim();
                        if trimmed.is_empty() || trimmed.contains("{{") {
                            return None;
                        }
                        return Some(trimmed.to_string());
                    }
                }
            }
        }

        if let Some(legacy_name) = map.get(Y::String("project_name".into())) {
            if let Some(name_str) = legacy_name.as_str() {
                let trimmed = name_str.trim();
                if trimmed.is_empty() || trimmed.contains("{{") {
                    return None;
                }
                return Some(trimmed.to_string());
            }
        }

        None
    }

    fn validate_generated_project_config(
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
        config: &crate::config::types::ProjectConfig,
    ) -> Result<(), String> {
        let validator = crate::config::validation::ConfigValidator::new(&resolver.path);
        let result = validator.validate_project_config(config);

        for warning in &result.warnings {
            renderer.emit_warning(&warning.to_string());
        }

        if result.has_errors() {
            for error in &result.errors {
                renderer.emit_error(&error.to_string());
            }
            return Err("Generated project configuration failed validation".to_string());
        }

        Ok(())
    }

    fn validate_generated_global_config(
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
        config: &crate::config::types::GlobalConfig,
    ) -> Result<(), String> {
        let validator = crate::config::validation::ConfigValidator::new(&resolver.path);
        let result = validator.validate_global_config(config);

        for warning in &result.warnings {
            renderer.emit_warning(&warning.to_string());
        }

        if result.has_errors() {
            for error in &result.errors {
                renderer.emit_error(&error.to_string());
            }
            return Err("Generated global configuration failed validation".to_string());
        }

        Ok(())
    }

    fn handle_config_validate(
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
        project: Option<String>,
        global: bool,
        fix: bool,
        errors_only: bool,
    ) -> Result<(), String> {
        use crate::config::validation::{ConfigValidator, ValidationSeverity};

        // Load global config from tasks directory (normalization-aware)
        let global_config_path = crate::utils::paths::global_config_path(&resolver.path);
        let global_config = if global_config_path.exists() {
            match std::fs::read_to_string(&global_config_path) {
                Ok(content) => crate::config::normalization::parse_global_from_yaml_str(&content)
                    .map_err(|e| format!("Failed to parse global config: {}", e))?,
                Err(e) => {
                    return Err(format!("Failed to read global config file: {}", e));
                }
            }
        } else {
            crate::config::types::GlobalConfig::default()
        };

        let validator = ConfigValidator::new(&resolver.path);
        let mut all_results = Vec::new();
        let mut has_errors = false;

        // Validate global config if requested or no specific scope given
        if global || project.is_none() {
            renderer.emit_info("Validating global configuration");
            let result = validator.validate_global_config(&global_config);

            if result.has_errors() || result.has_warnings() {
                has_errors |= result.has_errors(); // Only actual errors affect exit code
                all_results.push(("Global Config".to_string(), result));
            } else {
                renderer.emit_success("Global configuration is valid");
            }
        }

        // Validate project config if requested or available
        if let Some(project_name) = project {
            let project_display_name = crate::utils::project::project_display_name_from_config(
                &resolver.path,
                &project_name,
            );
            let project_label = crate::utils::project::format_project_label(
                &project_name,
                project_display_name.as_deref(),
            );
            renderer.emit_info(&format!(
                "Validating project configuration for '{}'",
                project_label
            ));

            // Load project config directly from file
            let project_config_path =
                crate::utils::paths::project_config_path(&resolver.path, &project_name);

            if project_config_path.exists() {
                match std::fs::read_to_string(&project_config_path) {
                    Ok(config_content) => {
                        match crate::config::normalization::parse_project_from_yaml_str(
                            &project_name,
                            &config_content,
                        ) {
                            Ok(project_config) => {
                                let result = validator.validate_project_config(&project_config);

                                // For prefix conflicts, we need to determine the actual prefix used
                                // This would typically come from the project directory name or config
                                let prefix = &project_name; // Simple fallback
                                let conflict_result = validator.check_prefix_conflicts(prefix);

                                let mut combined_result = result;
                                combined_result.merge(conflict_result);

                                if combined_result.has_errors() || combined_result.has_warnings() {
                                    has_errors |= combined_result.has_errors(); // Only actual errors affect exit code
                                    all_results.push((
                                        format!("Project Config ({})", project_label),
                                        combined_result,
                                    ));
                                } else {
                                    renderer.emit_success("Project configuration is valid");
                                }
                            }
                            Err(e) => {
                                renderer.emit_error(&format!(
                                    "Could not parse project config YAML: {}",
                                    e
                                ));
                                has_errors = true;
                            }
                        }
                    }
                    Err(e) => {
                        renderer.emit_error(&format!("Could not read project config file: {}", e));
                        has_errors = true;
                    }
                }
            } else {
                renderer.emit_error(&format!(
                    "Project config file not found: {}",
                    project_config_path.display()
                ));
                has_errors = true;
            }
        }

        // Display results
        for (scope, result) in all_results {
            renderer.emit_info(&format!("{} Validation Results:", scope));

            // Display errors
            for error in &result.errors {
                if errors_only && error.severity != ValidationSeverity::Error {
                    continue;
                }
                renderer.emit_raw_stdout(&format!("{}", error));
            }

            // Display warnings (unless errors_only is set)
            if !errors_only {
                for warning in &result.warnings {
                    renderer.emit_raw_stdout(&format!("{}", warning));
                }

                // Display info messages
                for info in &result.info {
                    renderer.emit_raw_stdout(&format!("{}", info));
                }
            }
        }

        // Handle validation outcome
        if has_errors {
            renderer.emit_error("Configuration validation failed with errors");

            if fix {
                renderer.emit_warning("Auto-fix functionality not yet implemented");
                renderer
                    .emit_info("Please review the suggestions above and make manual corrections");
            }

            return Err("Configuration validation failed".to_string());
        } else {
            renderer.emit_success("All configurations are valid!");
        }

        Ok(())
    }
}
