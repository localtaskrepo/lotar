use crate::config::types::*;
use crate::config::validation::errors::ValidationResult;
use crate::types::{Priority, TaskStatus, TaskType};
use crate::utils::project::{
    RESERVED_PREFIX_MESSAGE, generate_project_prefix, is_reserved_project_prefix,
};
use serde::de::DeserializeOwned;
use serde_yaml_ng;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
/// Parse an explicit `issue.done_states` list: non-empty, duplicate-free
/// (case-insensitive), every entry a valid task status. An empty request is
/// handled by the caller as "unset/inherit", never as an empty list.
fn parse_done_states_value(value: &str) -> Result<Vec<TaskStatus>, ConfigError> {
    let values = parse_token_list::<TaskStatus>(value, "task status")?;
    if values.is_empty() {
        return Err(ConfigError::ParseError(
            "issue_done_states must list at least one terminal status; leave it unset to use the inferred completion policy"
                .to_string(),
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for status in &values {
        if !seen.insert(status.as_str().to_ascii_lowercase()) {
            return Err(ConfigError::ParseError(format!(
                "Duplicate done state '{}' in issue_done_states",
                status.as_str()
            )));
        }
    }
    Ok(values)
}

fn parse_token_list<T>(value: &str, label: &str) -> Result<Vec<T>, ConfigError>
where
    T: std::str::FromStr + DeserializeOwned,
    T::Err: std::fmt::Display,
{
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    if let Ok(list) = serde_yaml_ng::from_str::<Vec<T>>(trimmed) {
        return Ok(list);
    }

    value
        .split(',')
        .map(|part| part.trim())
        .map(|token| {
            if token.is_empty() {
                Err(ConfigError::ParseError(format!(
                    "{} entries cannot be empty.",
                    label
                )))
            } else {
                token.parse::<T>().map_err(|err| {
                    ConfigError::ParseError(format!("Invalid {} '{}': {}", label, token, err))
                })
            }
        })
        .collect()
}

fn parse_sync_remotes(value: &str) -> Result<HashMap<String, SyncRemoteConfig>, ConfigError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(HashMap::new());
    }

    serde_yaml_ng::from_str::<HashMap<String, SyncRemoteConfig>>(trimmed)
        .map_err(|err| ConfigError::ParseError(format!("Invalid remotes value: {}", err)))
}

fn parse_bool_flag(value: &str, field: &str) -> Result<bool, ConfigError> {
    let normalized = value.trim().to_lowercase();
    match normalized.as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(ConfigError::ParseError(format!(
            "{} must be 'true' or 'false', got '{}'",
            field, other
        ))),
    }
}

fn parse_optional_bool_flag(value: &str, field: &str) -> Result<Option<bool>, ConfigError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("inherit") {
        Ok(None)
    } else {
        parse_bool_flag(trimmed, field).map(Some)
    }
}

fn parse_optional_status(value: &str, field: &str) -> Result<Option<TaskStatus>, ConfigError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("inherit") {
        Ok(None)
    } else {
        let status = trimmed.parse::<TaskStatus>().map_err(|err| {
            ConfigError::ParseError(format!("{} must be a valid status: {}", field, err))
        })?;
        Ok(Some(status))
    }
}

fn parse_optional_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("inherit") {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_simple_csv(value: &str) -> Vec<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    if let Ok(mut list) = serde_yaml_ng::from_str::<Vec<String>>(trimmed) {
        for entry in &mut list {
            *entry = entry.trim().to_string();
        }
        list.retain(|entry| !entry.is_empty());
        return list;
    }

    value
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn parse_alias_map<T>(value: &str, label: &str) -> Result<HashMap<String, T>, ConfigError>
where
    T: std::str::FromStr + DeserializeOwned,
    T::Err: std::fmt::Display,
{
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(HashMap::new());
    }

    fn insert_alias<T>(
        target: &mut HashMap<String, T>,
        raw_key: &str,
        value: T,
        label: &str,
    ) -> Result<(), ConfigError> {
        let canonical = raw_key.trim().to_ascii_lowercase();
        if canonical.is_empty() {
            return Err(ConfigError::ParseError(format!(
                "{} alias cannot be empty",
                label
            )));
        }
        if target.contains_key(&canonical) {
            return Err(ConfigError::ParseError(format!(
                "Duplicate {} alias '{}' detected",
                label,
                raw_key.trim()
            )));
        }
        target.insert(canonical, value);
        Ok(())
    }

    if let Ok(map) = serde_yaml_ng::from_str::<HashMap<String, T>>(trimmed) {
        let mut out = HashMap::new();
        for (key, value) in map.into_iter() {
            insert_alias(&mut out, &key, value, label)?;
        }
        return Ok(out);
    }

    if let Ok(map) = serde_yaml_ng::from_str::<HashMap<String, String>>(trimmed) {
        let mut out = HashMap::new();
        for (k, raw) in map.into_iter() {
            let parsed = raw.parse::<T>().map_err(|err| {
                ConfigError::ParseError(format!("Invalid {} '{}': {}", label, raw, err))
            })?;
            insert_alias(&mut out, &k, parsed, label)?;
        }
        return Ok(out);
    }

    let mut out = HashMap::new();
    for entry in trimmed.split([',', ';', '\n']) {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let (alias, target) = entry
            .split_once('=')
            .or_else(|| entry.split_once(':'))
            .ok_or_else(|| {
                ConfigError::ParseError(format!(
                    "Alias entry '{}' must use '=' or ':' to separate alias and value",
                    entry
                ))
            })?;
        let parsed = target.trim().parse::<T>().map_err(|err| {
            ConfigError::ParseError(format!("Invalid {} '{}': {}", label, target.trim(), err))
        })?;
        insert_alias(&mut out, alias, parsed, label)?;
    }

    if out.is_empty() {
        return Err(ConfigError::ParseError(format!(
            "Unable to parse alias map for {}",
            label
        )));
    }

    Ok(out)
}

/// Explicit server-port intent for a global config save.
///
/// The canonical serializer only emits `server.port` when the value differs
/// from the built-in default, which would silently drop an explicit pin for
/// the default port on every rewrite. The intent decides whether the key is
/// kept: pin-preserving by default (unrelated mutations), forced when this
/// save is itself an explicit port request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerPortSaveIntent {
    /// Emit `server.port` iff the existing file on disk already sets it:
    /// unrelated mutations and round-trips preserve an explicit pin, while
    /// fresh/implicit configs never gain the key.
    PreserveExisting,
    /// This save is an explicit port request (e.g. `config set
    /// server.port 8080`): always emit the key, even for the default value.
    ExplicitPin,
}

/// Save global configuration to tasks_dir/config.yml
pub fn save_global_config(tasks_dir: &Path, config: &GlobalConfig) -> Result<(), ConfigError> {
    save_global_config_with_port(tasks_dir, config, ServerPortSaveIntent::PreserveExisting)
}

/// Save global configuration with explicit server-port intent.
///
/// Callers are expected to have loaded (and, in the candidate pipeline,
/// validated) the existing configuration before writing: a present but
/// malformed config file blocks those paths before this save runs, so the
/// `PreserveExisting` presence read below only ever sees a file that
/// parse-verified callers already accepted.
pub fn save_global_config_with_port(
    tasks_dir: &Path,
    config: &GlobalConfig,
    port_intent: ServerPortSaveIntent,
) -> Result<(), ConfigError> {
    let config_path = crate::utils::paths::global_config_path(tasks_dir);

    // Ensure the tasks directory exists
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            ConfigError::IoError(format!("Failed to create tasks directory: {}", e))
        })?;
    }

    let explicit_server_port = match port_intent {
        ServerPortSaveIntent::ExplicitPin => true,
        ServerPortSaveIntent::PreserveExisting => {
            crate::config::persistence::config_file_server_port(&config_path, "global config")
                .ok()
                .flatten()
                .is_some()
        }
    };

    // Serialize in canonical nested format
    let config_yaml = crate::config::normalization::to_canonical_global_yaml_with(
        config,
        crate::config::normalization::GlobalYamlOptions {
            explicit_server_port,
        },
    );

    crate::storage::safety::atomic_write_file(&config_path, &config_yaml)
        .map_err(|e| ConfigError::IoError(format!("Failed to write global config: {}", e)))?;

    // Invalidate cache for this tasks_dir
    crate::config::resolution::invalidate_config_cache_for(Some(tasks_dir));
    crate::utils::identity::invalidate_identity_cache(Some(tasks_dir));

    Ok(())
}

/// Save updated project configuration to tasks_dir/{project}/config.yml
pub fn save_project_config(
    tasks_dir: &Path,
    project_prefix: &str,
    config: &ProjectConfig,
) -> Result<(), ConfigError> {
    if let Err(e) = crate::storage::safety::validate_project_prefix(project_prefix) {
        return Err(ConfigError::IoError(e));
    }
    let project_dir = crate::utils::paths::project_dir(tasks_dir, project_prefix);
    let config_path = crate::utils::paths::project_config_path(tasks_dir, project_prefix);

    // Ensure the project directory exists
    fs::create_dir_all(&project_dir)
        .map_err(|e| ConfigError::IoError(format!("Failed to create project directory: {}", e)))?;

    // Serialize in canonical nested format
    let config_yaml = crate::config::normalization::to_canonical_project_yaml(config);

    crate::storage::safety::atomic_write_file(&config_path, &config_yaml)
        .map_err(|e| ConfigError::IoError(format!("Failed to write project config: {}", e)))?;

    // Invalidate cache for this tasks_dir
    crate::config::resolution::invalidate_config_cache_for(Some(tasks_dir));
    crate::utils::identity::invalidate_identity_cache(Some(tasks_dir));

    Ok(())
}

/// Ensure the provided members exist in the project's configuration, creating the project
/// config if necessary. Returns the updated member list when changes were applied.
pub fn auto_populate_project_members(
    tasks_dir: &Path,
    project_prefix: &str,
    base_members: &[String],
    new_members: &[String],
) -> Result<Option<Vec<String>>, ConfigError> {
    match plan_auto_populated_project_config(tasks_dir, project_prefix, base_members, new_members)?
    {
        Some((project_config, effective)) => {
            save_project_config(tasks_dir, project_prefix, &project_config)?;
            Ok(Some(effective))
        }
        None => Ok(None),
    }
}

/// Compute the project config that auto-population would write, without
/// writing it. Returns `None` when no member changes are needed; otherwise the
/// updated config (for the caller to persist or stage transactionally, DEV-55)
/// and the effective member list.
pub fn plan_auto_populated_project_config(
    tasks_dir: &Path,
    project_prefix: &str,
    base_members: &[String],
    new_members: &[String],
) -> Result<Option<(crate::config::types::ProjectConfig, Vec<String>)>, ConfigError> {
    if new_members.is_empty() {
        return Ok(None);
    }

    let mut project_config =
        crate::config::persistence::load_project_config_from_dir(project_prefix, tasks_dir)?;

    let mut effective: Vec<String> = if let Some(existing) = project_config.members.clone() {
        existing
    } else {
        base_members
            .iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect()
    };

    let mut changed = false;
    for candidate in new_members {
        let trimmed = candidate.trim();
        if trimmed.is_empty() {
            continue;
        }
        let already_present = effective
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(trimmed));
        if !already_present {
            effective.push(trimmed.to_string());
            changed = true;
        }
    }

    if !changed {
        return Ok(None);
    }

    effective.sort_by_key(|a| a.to_ascii_lowercase());
    project_config.members = Some(effective.clone());
    Ok(Some((project_config, effective)))
}

/// Update a specific field in global or project configuration.
///
/// Compatibility wrapper: builds one validated candidate through the shared
/// DEV-71 pipeline (schema, config, resolved, and real-task validation before
/// a single atomic write). Dotted canonical field names are accepted. See
/// [`crate::config::candidate`].
pub fn update_config_field(
    tasks_dir: &Path,
    field: &str,
    value: &str,
    project_prefix: Option<&str>,
) -> Result<ValidationResult, ConfigError> {
    let scope = match project_prefix {
        Some(prefix) => crate::config::candidate::ConfigScope::Project(prefix.to_string()),
        None => crate::config::candidate::ConfigScope::Global,
    };
    let result = crate::config::candidate::apply_config_set(
        tasks_dir,
        &scope,
        &crate::config::candidate::ConfigSetRequest {
            entries: vec![(field.to_string(), value.to_string())],
            force: false,
            dry_run: false,
        },
    )?;
    Ok(result.validation)
}

/// Apply a field update to GlobalConfig
pub fn apply_field_to_global_config(
    config: &mut GlobalConfig,
    field: &str,
    value: &str,
) -> Result<(), ConfigError> {
    match field {
        "server_port" => {
            config.server_port = value
                .parse::<u16>()
                .map_err(|_| ConfigError::ParseError(format!("Invalid port number: {}", value)))?;
        }
        "default_project" => {
            // Normalize to storage prefix, accepting either full project name or prefix
            config.default_project = generate_project_prefix(value);
        }
        "default_assignee" => {
            config.default_assignee = if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            };
        }
        "default_reporter" => {
            config.default_reporter = if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            };
        }
        "default_priority" => {
            config.default_priority = value.parse::<Priority>().map_err(|err| {
                ConfigError::ParseError(format!("Invalid priority '{}': {}", value, err))
            })?;
        }
        "default_status" => {
            if value.trim().is_empty() {
                config.default_status = None;
                return Ok(());
            }
            let status = value.parse::<TaskStatus>().map_err(|err| {
                ConfigError::ParseError(format!("Invalid status '{}': {}", value, err))
            })?;
            config.default_status = Some(status);
        }
        "issue_states" => {
            let states = parse_token_list::<TaskStatus>(value, "task status")?;
            config.issue_states = ConfigurableField { values: states };
        }
        "issue_done_states" => {
            // An empty value unsets the explicit global list and restores the
            // legacy inferred completion policy; an explicit list must be
            // non-empty and duplicate-free.
            if value.trim().is_empty() {
                config.issue_done_states = None;
                return Ok(());
            }
            let done = parse_done_states_value(value)?;
            config.issue_done_states = Some(ConfigurableField { values: done });
        }
        "issue_types" => {
            let types = parse_token_list::<TaskType>(value, "task type")?;
            config.issue_types = ConfigurableField { values: types };
        }
        "issue_priorities" => {
            let priorities = parse_token_list::<Priority>(value, "priority")?;
            config.issue_priorities = ConfigurableField { values: priorities };
        }
        "tags" | "custom_fields" => {
            let values = parse_simple_csv(value);
            if field == "custom_fields" {
                for v in &values {
                    if let Some(canonical) = crate::utils::fields::is_reserved_field(v) {
                        return Err(ConfigError::ParseError(format!(
                            "Custom field '{}' collides with built-in field '{}'. Choose a different name.",
                            v, canonical
                        )));
                    }
                }
            }
            let field_config = StringConfigField { values };

            match field {
                "tags" => config.tags = field_config,
                "custom_fields" => config.custom_fields = field_config,
                _ => unreachable!(),
            }
        }
        "default_tags" => {
            config.default_tags = parse_simple_csv(value);
        }
        "members" => {
            config.members = parse_simple_csv(value);
        }
        "strict_members" => {
            config.strict_members = parse_bool_flag(value, field)?;
        }
        "auto_populate_members" => {
            config.auto_populate_members = parse_bool_flag(value, field)?;
        }
        "auto_set_reporter" => {
            config.auto_set_reporter = parse_bool_flag(value, field)?;
        }
        "auto_assign_on_status" => {
            config.auto_assign_on_status = parse_bool_flag(value, field)?;
        }
        "auto_codeowners_assign" => {
            config.auto_codeowners_assign = parse_bool_flag(value, field)?;
        }
        "auto_tags_from_path" => {
            config.auto_tags_from_path = parse_bool_flag(value, field)?;
        }
        "auto_branch_infer_type" => {
            config.auto_branch_infer_type = parse_bool_flag(value, field)?;
        }
        "auto_branch_infer_status" => {
            config.auto_branch_infer_status = parse_bool_flag(value, field)?;
        }
        "auto_branch_infer_priority" => {
            config.auto_branch_infer_priority = parse_bool_flag(value, field)?;
        }
        "auto_identity" => {
            config.auto_identity = parse_bool_flag(value, field)?;
        }
        "auto_identity_git" => {
            config.auto_identity_git = parse_bool_flag(value, field)?;
        }
        "attachments_dir" => {
            config.attachments_dir = value.trim().to_string();
        }
        "attachments_max_upload_mb" => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                config.attachments_max_upload_mb =
                    GlobalConfig::default().attachments_max_upload_mb;
            } else {
                let parsed = trimmed.parse::<i64>().map_err(|_| {
                    ConfigError::ParseError(format!(
                        "{} must be an integer (MiB), got '{}'",
                        field, trimmed
                    ))
                })?;
                config.attachments_max_upload_mb = parsed;
            }
        }
        "sync_reports_dir" => {
            config.sync_reports_dir = value.trim().to_string();
        }
        "sync_write_reports" => {
            config.sync_write_reports = parse_bool_flag(value, field)?;
        }
        "agent_context_enabled" => {
            config.agent_context_enabled = parse_bool_flag(value, field)?;
        }
        "agent_context_extension" => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return Err(ConfigError::ParseError(
                    "agent_context_extension cannot be empty".to_string(),
                ));
            }
            config.agent_context_extension = trimmed.to_string();
        }
        "agent_logs_dir" => {
            let trimmed = value.trim();
            config.agent_logs_dir = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        "agent_instructions" => {
            let trimmed = value.trim();
            config.agent_instructions = if trimmed.is_empty() {
                None
            } else {
                Some(AgentInstructionsConfig::Inline(trimmed.to_string()))
            };
        }
        "agent_instructions_file" => {
            let trimmed = value.trim();
            config.agent_instructions = if trimmed.is_empty() {
                None
            } else {
                Some(AgentInstructionsConfig::File {
                    file: trimmed.to_string(),
                })
            };
        }
        "agent_on_start_status" => {
            config.agent_automation.on_start.set_status = parse_optional_status(value, field)?;
        }
        "agent_on_success_status" => {
            config.agent_automation.on_success.set_status = parse_optional_status(value, field)?;
        }
        "agent_on_failure_status" => {
            config.agent_automation.on_failure.set_status = parse_optional_status(value, field)?;
        }
        "agent_on_cancel_status" => {
            config.agent_automation.on_cancel.set_status = parse_optional_status(value, field)?;
        }
        "agent_on_start_reassign_to" => {
            config.agent_automation.on_start.reassign_to = parse_optional_string(value);
        }
        "agent_on_success_reassign_to" => {
            config.agent_automation.on_success.reassign_to = parse_optional_string(value);
        }
        "agent_on_failure_reassign_to" => {
            config.agent_automation.on_failure.reassign_to = parse_optional_string(value);
        }
        "agent_on_cancel_reassign_to" => {
            config.agent_automation.on_cancel.reassign_to = parse_optional_string(value);
        }
        "agent_worktree_enabled" => {
            config.agent_worktree.enabled = parse_bool_flag(value, field)?;
        }
        "agent_worktree_dir" => {
            let trimmed = value.trim();
            config.agent_worktree.dir = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        "agent_worktree_branch_prefix" => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return Err(ConfigError::ParseError(
                    "agent_worktree_branch_prefix cannot be empty".to_string(),
                ));
            }
            config.agent_worktree.branch_prefix = trimmed.to_string();
        }
        "remotes" => {
            config.remotes = parse_sync_remotes(value)?;
        }
        "scan_signal_words" => {
            config.scan_signal_words = parse_simple_csv(value);
        }
        "scan_ticket_patterns" => {
            let entries = parse_simple_csv(value);
            if entries.is_empty() {
                config.scan_ticket_patterns = None;
            } else {
                config.scan_ticket_patterns = Some(entries);
            }
        }
        "scan_enable_ticket_words" => {
            config.scan_enable_ticket_words = parse_bool_flag(value, field)?;
        }
        "scan_enable_mentions" => {
            config.scan_enable_mentions = parse_bool_flag(value, field)?;
        }
        "scan_strip_attributes" => {
            config.scan_strip_attributes = parse_bool_flag(value, field)?;
        }
        "branch_type_aliases" => {
            let map = parse_alias_map::<TaskType>(value, "branch type alias")?;
            config.branch_type_aliases = map;
        }
        "branch_status_aliases" => {
            let map = parse_alias_map::<TaskStatus>(value, "branch status alias")?;
            config.branch_status_aliases = map;
        }
        "branch_priority_aliases" => {
            let map = parse_alias_map::<Priority>(value, "branch priority alias")?;
            config.branch_priority_aliases = map;
        }
        "sprints_defaults_capacity_points" => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                config.sprints.defaults.capacity_points = None;
            } else {
                let parsed = trimmed.parse::<u32>().map_err(|err| {
                    ConfigError::ParseError(format!(
                        "Invalid sprint capacity points '{}': {}",
                        value, err
                    ))
                })?;
                config.sprints.defaults.capacity_points = Some(parsed);
            }
        }
        "sprints_defaults_capacity_hours" => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                config.sprints.defaults.capacity_hours = None;
            } else {
                let parsed = trimmed.parse::<u32>().map_err(|err| {
                    ConfigError::ParseError(format!(
                        "Invalid sprint capacity hours '{}': {}",
                        value, err
                    ))
                })?;
                config.sprints.defaults.capacity_hours = Some(parsed);
            }
        }
        "sprints_defaults_length" => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                config.sprints.defaults.length = None;
            } else {
                config.sprints.defaults.length = Some(trimmed.to_string());
            }
        }
        "sprints_defaults_overdue_after" => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                config.sprints.defaults.overdue_after = None;
            } else {
                config.sprints.defaults.overdue_after = Some(trimmed.to_string());
            }
        }
        "sprints_notifications_enabled" => {
            config.sprints.notifications.enabled = parse_bool_flag(value, field)?;
        }
        "web_ui_path" => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                config.web_ui_path = None;
            } else {
                config.web_ui_path = Some(trimmed.to_string());
            }
        }
        _ => {
            return Err(ConfigError::ParseError(format!(
                "Unknown global config field: {}",
                field
            )));
        }
    }
    Ok(())
}

/// Apply a field update to ProjectConfig
pub fn apply_field_to_project_config(
    config: &mut ProjectConfig,
    field: &str,
    value: &str,
) -> Result<(), ConfigError> {
    match field {
        "project_name" => {
            config.project_name = value.to_string();
        }
        "default_assignee" => {
            config.default_assignee = if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            };
        }
        "default_reporter" => {
            config.default_reporter = if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            };
        }
        "default_priority" => {
            if value.trim().is_empty() {
                config.default_priority = None;
                return Ok(());
            }
            let priority = value.parse::<Priority>().map_err(|err| {
                ConfigError::ParseError(format!("Invalid priority '{}': {}", value, err))
            })?;
            config.default_priority = Some(priority);
        }
        "default_status" => {
            if value.trim().is_empty() {
                config.default_status = None;
                return Ok(());
            }
            let status = value.parse::<TaskStatus>().map_err(|err| {
                ConfigError::ParseError(format!("Invalid status '{}': {}", value, err))
            })?;
            config.default_status = Some(status);
        }
        "default_tags" => {
            let values = parse_simple_csv(value);
            config.default_tags = if values.is_empty() {
                None
            } else {
                Some(values)
            };
        }
        "members" => {
            let members = parse_simple_csv(value);
            config.members = if members.is_empty() {
                None
            } else {
                Some(members)
            };
        }
        "strict_members" => {
            config.strict_members = parse_optional_bool_flag(value, field)?;
        }
        "auto_populate_members" => {
            config.auto_populate_members = parse_optional_bool_flag(value, field)?;
        }
        "issue_states" => {
            let states = parse_token_list::<TaskStatus>(value, "task status")?;
            config.issue_states = Some(ConfigurableField { values: states });
        }
        "issue_done_states" => {
            // Empty value clears the override (inherit the effective global
            // list or the inferred policy); an explicit list must be
            // non-empty and duplicate-free.
            if value.trim().is_empty() {
                config.issue_done_states = None;
                return Ok(());
            }
            let done = parse_done_states_value(value)?;
            config.issue_done_states = Some(ConfigurableField { values: done });
        }
        "issue_types" => {
            let types = parse_token_list::<TaskType>(value, "task type")?;
            config.issue_types = Some(ConfigurableField { values: types });
        }
        "issue_priorities" => {
            let priorities = parse_token_list::<Priority>(value, "priority")?;
            config.issue_priorities = Some(ConfigurableField { values: priorities });
        }
        "tags" | "custom_fields" => {
            let values = parse_simple_csv(value);
            if field == "custom_fields" {
                for v in &values {
                    if let Some(canonical) = crate::utils::fields::is_reserved_field(v) {
                        return Err(ConfigError::ParseError(format!(
                            "Custom field '{}' collides with built-in field '{}'. Choose a different name.",
                            v, canonical
                        )));
                    }
                }
            }
            let field_config = StringConfigField { values };

            match field {
                "tags" => config.tags = Some(field_config),
                "custom_fields" => config.custom_fields = Some(field_config),
                _ => unreachable!(),
            }
        }
        "auto_set_reporter" => {
            config.auto_set_reporter = parse_optional_bool_flag(value, field)?;
        }
        "auto_assign_on_status" => {
            config.auto_assign_on_status = parse_optional_bool_flag(value, field)?;
        }
        "auto_codeowners_assign" => {
            config.auto_codeowners_assign = parse_optional_bool_flag(value, field)?;
        }
        "auto_tags_from_path" => {
            config.auto_tags_from_path = parse_optional_bool_flag(value, field)?;
        }
        "auto_branch_infer_type" => {
            config.auto_branch_infer_type = parse_optional_bool_flag(value, field)?;
        }
        "auto_branch_infer_status" => {
            config.auto_branch_infer_status = parse_optional_bool_flag(value, field)?;
        }
        "auto_branch_infer_priority" => {
            config.auto_branch_infer_priority = parse_optional_bool_flag(value, field)?;
        }
        "auto_identity" => {
            config.auto_identity = parse_optional_bool_flag(value, field)?;
        }
        "auto_identity_git" => {
            config.auto_identity_git = parse_optional_bool_flag(value, field)?;
        }
        "scan_signal_words" => {
            let entries = parse_simple_csv(value);
            config.scan_signal_words = if entries.is_empty() {
                None
            } else {
                Some(entries)
            };
        }
        "scan_ticket_patterns" => {
            let entries = parse_simple_csv(value);
            config.scan_ticket_patterns = if entries.is_empty() {
                None
            } else {
                Some(entries)
            };
        }
        "scan_enable_ticket_words" => {
            config.scan_enable_ticket_words = parse_optional_bool_flag(value, field)?;
        }
        "scan_enable_mentions" => {
            config.scan_enable_mentions = parse_optional_bool_flag(value, field)?;
        }
        "scan_strip_attributes" => {
            config.scan_strip_attributes = parse_optional_bool_flag(value, field)?;
        }
        "attachments_dir" => {
            let trimmed = value.trim();
            config.attachments_dir = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        "attachments_max_upload_mb" => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                config.attachments_max_upload_mb = None;
            } else {
                let parsed = trimmed.parse::<i64>().map_err(|_| {
                    ConfigError::ParseError(format!(
                        "{} must be an integer (MiB), got '{}'",
                        field, trimmed
                    ))
                })?;
                config.attachments_max_upload_mb = Some(parsed);
            }
        }
        "sync_reports_dir" => {
            let trimmed = value.trim();
            config.sync_reports_dir = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        "sync_write_reports" => {
            config.sync_write_reports = parse_optional_bool_flag(value, field)?;
        }
        "agent_context_enabled" => {
            config.agent_context_enabled = parse_optional_bool_flag(value, field)?;
        }
        "agent_context_extension" => {
            let trimmed = value.trim();
            config.agent_context_extension =
                if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("inherit") {
                    None
                } else {
                    Some(trimmed.to_string())
                };
        }
        "agent_logs_dir" => {
            let trimmed = value.trim();
            config.agent_logs_dir = if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("inherit")
            {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        "agent_instructions" => {
            let trimmed = value.trim();
            config.agent_instructions =
                if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("inherit") {
                    None
                } else {
                    Some(AgentInstructionsConfig::Inline(trimmed.to_string()))
                };
        }
        "agent_instructions_file" => {
            let trimmed = value.trim();
            config.agent_instructions =
                if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("inherit") {
                    None
                } else {
                    Some(AgentInstructionsConfig::File {
                        file: trimmed.to_string(),
                    })
                };
        }
        "agent_on_start_status" => {
            let automation = config.agent_automation.get_or_insert_with(Default::default);
            let action = automation.on_start.get_or_insert_with(Default::default);
            action.set_status = parse_optional_status(value, field)?;
        }
        "agent_on_success_status" => {
            let automation = config.agent_automation.get_or_insert_with(Default::default);
            let action = automation.on_success.get_or_insert_with(Default::default);
            action.set_status = parse_optional_status(value, field)?;
        }
        "agent_on_failure_status" => {
            let automation = config.agent_automation.get_or_insert_with(Default::default);
            let action = automation.on_failure.get_or_insert_with(Default::default);
            action.set_status = parse_optional_status(value, field)?;
        }
        "agent_on_cancel_status" => {
            let automation = config.agent_automation.get_or_insert_with(Default::default);
            let action = automation.on_cancel.get_or_insert_with(Default::default);
            action.set_status = parse_optional_status(value, field)?;
        }
        "agent_on_start_reassign_to" => {
            let automation = config.agent_automation.get_or_insert_with(Default::default);
            let action = automation.on_start.get_or_insert_with(Default::default);
            action.reassign_to = parse_optional_string(value);
        }
        "agent_on_success_reassign_to" => {
            let automation = config.agent_automation.get_or_insert_with(Default::default);
            let action = automation.on_success.get_or_insert_with(Default::default);
            action.reassign_to = parse_optional_string(value);
        }
        "agent_on_failure_reassign_to" => {
            let automation = config.agent_automation.get_or_insert_with(Default::default);
            let action = automation.on_failure.get_or_insert_with(Default::default);
            action.reassign_to = parse_optional_string(value);
        }
        "agent_on_cancel_reassign_to" => {
            let automation = config.agent_automation.get_or_insert_with(Default::default);
            let action = automation.on_cancel.get_or_insert_with(Default::default);
            action.reassign_to = parse_optional_string(value);
        }
        "agent_worktree_enabled" => {
            let worktree = config.agent_worktree.get_or_insert_with(Default::default);
            worktree.enabled = parse_optional_bool_flag(value, field)?;
        }
        "agent_worktree_dir" => {
            let worktree = config.agent_worktree.get_or_insert_with(Default::default);
            worktree.dir = parse_optional_string(value);
        }
        "agent_worktree_branch_prefix" => {
            let worktree = config.agent_worktree.get_or_insert_with(Default::default);
            worktree.branch_prefix = parse_optional_string(value);
        }
        "remotes" => {
            config.remotes = parse_sync_remotes(value)?;
        }
        "branch_type_aliases" => {
            if value.trim().is_empty() {
                config.branch_type_aliases = None;
            } else {
                let map = parse_alias_map::<TaskType>(value, "branch type alias")?;
                config.branch_type_aliases = Some(map);
            }
        }
        "branch_status_aliases" => {
            if value.trim().is_empty() {
                config.branch_status_aliases = None;
            } else {
                let map = parse_alias_map::<TaskStatus>(value, "branch status alias")?;
                config.branch_status_aliases = Some(map);
            }
        }
        "branch_priority_aliases" => {
            if value.trim().is_empty() {
                config.branch_priority_aliases = None;
            } else {
                let map = parse_alias_map::<Priority>(value, "branch priority alias")?;
                config.branch_priority_aliases = Some(map);
            }
        }
        _ => {
            return Err(ConfigError::ParseError(format!(
                "Unknown project config field: {}",
                field
            )));
        }
    }
    Ok(())
}

/// Fields settable in the global configuration (canonical flat names).
pub const GLOBAL_CONFIG_FIELDS: &[&str] = &[
    "server_port",
    "default_project",
    "default_assignee",
    "default_reporter",
    "default_tags",
    "members",
    "strict_members",
    "auto_populate_members",
    "default_priority",
    "default_status",
    "tags",
    "custom_fields",
    "issue_states",
    "issue_done_states",
    "issue_types",
    "issue_priorities",
    "auto_set_reporter",
    "auto_assign_on_status",
    "auto_codeowners_assign",
    "auto_tags_from_path",
    "auto_branch_infer_type",
    "auto_branch_infer_status",
    "auto_branch_infer_priority",
    "auto_identity",
    "auto_identity_git",
    "scan_signal_words",
    "scan_ticket_patterns",
    "scan_enable_ticket_words",
    "scan_enable_mentions",
    "scan_strip_attributes",
    "branch_type_aliases",
    "branch_status_aliases",
    "branch_priority_aliases",
    "attachments_dir",
    "attachments_max_upload_mb",
    "sync_reports_dir",
    "sync_write_reports",
    "agent_context_enabled",
    "agent_context_extension",
    "agent_logs_dir",
    "agent_instructions",
    "agent_instructions_file",
    "agent_on_start_status",
    "agent_on_success_status",
    "agent_on_failure_status",
    "agent_on_cancel_status",
    "agent_on_start_reassign_to",
    "agent_on_success_reassign_to",
    "agent_on_failure_reassign_to",
    "agent_on_cancel_reassign_to",
    "agent_worktree_enabled",
    "agent_worktree_dir",
    "agent_worktree_branch_prefix",
    "remotes",
    "sprints_defaults_capacity_points",
    "sprints_defaults_capacity_hours",
    "sprints_defaults_length",
    "sprints_defaults_overdue_after",
    "sprints_notifications_enabled",
    "web_ui_path",
];

/// Fields settable in a project configuration override (canonical flat names).
pub const PROJECT_CONFIG_FIELDS: &[&str] = &[
    "project_name",
    "default_assignee",
    "default_reporter",
    "default_tags",
    "members",
    "strict_members",
    "auto_populate_members",
    "default_priority",
    "default_status",
    "issue_states",
    "issue_done_states",
    "issue_types",
    "issue_priorities",
    "tags",
    "custom_fields",
    "auto_set_reporter",
    "auto_assign_on_status",
    "auto_codeowners_assign",
    "auto_tags_from_path",
    "auto_branch_infer_type",
    "auto_branch_infer_status",
    "auto_branch_infer_priority",
    "auto_identity",
    "auto_identity_git",
    "scan_signal_words",
    "scan_ticket_patterns",
    "scan_enable_ticket_words",
    "scan_enable_mentions",
    "scan_strip_attributes",
    "branch_type_aliases",
    "branch_status_aliases",
    "branch_priority_aliases",
    "attachments_dir",
    "attachments_max_upload_mb",
    "sync_reports_dir",
    "sync_write_reports",
    "agent_context_enabled",
    "agent_context_extension",
    "agent_logs_dir",
    "agent_instructions",
    "agent_instructions_file",
    "agent_on_start_status",
    "agent_on_success_status",
    "agent_on_failure_status",
    "agent_on_cancel_status",
    "agent_on_start_reassign_to",
    "agent_on_success_reassign_to",
    "agent_on_failure_reassign_to",
    "agent_on_cancel_reassign_to",
    "agent_worktree_enabled",
    "agent_worktree_dir",
    "agent_worktree_branch_prefix",
    "remotes",
];

/// Valid canonical field names for the given scope.
pub fn valid_fields(is_global: bool) -> &'static [&'static str] {
    if is_global {
        GLOBAL_CONFIG_FIELDS
    } else {
        PROJECT_CONFIG_FIELDS
    }
}

/// Validate that a (canonical flat) field name is valid for the given scope.
pub fn validate_field_name(field: &str, is_global: bool) -> Result<(), ConfigError> {
    let valid_fields = valid_fields(is_global);
    if !valid_fields.contains(&field) {
        let scope = if is_global { "global" } else { "project" };
        return Err(ConfigError::ParseError(format!(
            "Invalid {} config field: '{}'. Valid fields: {}",
            scope,
            field,
            valid_fields.join(", ")
        )));
    }
    Ok(())
}

/// Canonicalize a config-set field name for the given scope (DEV-71).
///
/// Shipped flat names pass through unchanged. Documented dotted canonical
/// names resolve through the authoritative [`CONFIG_PATHS`] table, including
/// the exceptions (`default.strict_members` -> `strict_members`,
/// `issue.tags` -> `tags`). Unknown names and aliases whose target is not
/// settable in the requested scope are rejected.
pub fn canonicalize_field_name(field: &str, is_global: bool) -> Result<String, ConfigError> {
    let trimmed = field.trim();
    if trimmed.is_empty() {
        return Err(ConfigError::ParseError(
            "Config field name cannot be empty".to_string(),
        ));
    }
    let valid_fields = valid_fields(is_global);
    if valid_fields.contains(&trimmed) {
        return Ok(trimmed.to_string());
    }
    if let Some(&(_, flat)) = crate::config::normalization::CONFIG_PATHS
        .iter()
        .find(|&&(dotted, _)| dotted == trimmed)
    {
        if valid_fields.contains(&flat) {
            return Ok(flat.to_string());
        }
        let scope = if is_global { "global" } else { "project" };
        return Err(ConfigError::ParseError(format!(
            "Canonical field '{}' maps to '{}' which is not settable for {} configurations",
            trimmed, flat, scope
        )));
    }
    let scope = if is_global { "global" } else { "project" };
    Err(ConfigError::ParseError(format!(
        "Invalid {} config field: '{}'. Valid flat fields: {} (dotted canonical aliases such as 'default.priority' or 'issue.states' are also accepted)",
        scope,
        trimmed,
        valid_fields.join(", ")
    )))
}

/// If `field` (flat or dotted) canonicalizes to a global-only field, return
/// its canonical name. Used to auto-promote `config set server.port 9000` and
/// friends to the global scope without requiring `--global`.
pub fn is_global_only_field_name(field: &str) -> Option<String> {
    let canonical = canonicalize_field_name(field, true).ok()?;
    if PROJECT_CONFIG_FIELDS.contains(&canonical.as_str()) {
        None
    } else {
        Some(canonical)
    }
}

/// Validate that a field value is valid for the given field
pub fn validate_field_value(field: &str, value: &str) -> Result<(), ConfigError> {
    match field {
        "server_port" => {
            let port = value
                .parse::<u16>()
                .map_err(|_| ConfigError::ParseError(format!("Invalid port number: {}", value)))?;
            if port == 0 {
                return Err(ConfigError::ParseError(
                    "Port number must be between 1 and 65535".to_string(),
                ));
            }
        }
        "default_priority" => {
            if value.trim().is_empty() {
                return Ok(());
            }
            value.parse::<Priority>().map_err(|err| {
                ConfigError::ParseError(format!("Invalid priority '{}': {}", value, err))
            })?;
        }
        "default_status" => {
            if value.trim().is_empty() {
                return Ok(());
            }
            value.parse::<TaskStatus>().map_err(|err| {
                ConfigError::ParseError(format!("Invalid status '{}': {}", value, err))
            })?;
        }
        "default_project" => {
            if is_reserved_project_prefix(value) {
                return Err(ConfigError::ParseError(RESERVED_PREFIX_MESSAGE.to_string()));
            }
            if !value
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
            {
                return Err(ConfigError::ParseError(
                    "Project prefix can only contain alphanumeric characters, hyphens, and underscores".to_string()
                ));
            }
            if value.len() > 20 {
                return Err(ConfigError::ParseError(
                    "Project prefix cannot be longer than 20 characters".to_string(),
                ));
            }
        }
        "default_assignee" | "default_reporter" | "project_name"
            // Basic validation - not empty and reasonable length
            if value.len() > 100 => {
                return Err(ConfigError::ParseError(format!(
                    "{} cannot be longer than 100 characters",
                    field
                )));
            }
        "default_tags" | "tags" | "custom_fields" => {
            // Basic validation: allow comma-separated values, each up to 50 chars
            for part in value.split(',') {
                let p = part.trim();
                if p.len() > 50 {
                    return Err(ConfigError::ParseError(format!(
                        "Value '{}' is too long (max 50 chars)",
                        p
                    )));
                }
            }
        }
        "members" => {
            for part in value.split(',') {
                let token = part.trim();
                if token.len() > 100 {
                    return Err(ConfigError::ParseError(format!(
                        "Member '{}' is too long (max 100 chars)",
                        token
                    )));
                }
            }
        }
        "attachments_dir" => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return Err(ConfigError::ParseError(
                    "attachments_dir cannot be empty".to_string(),
                ));
            }
            if trimmed.len() > 512 {
                return Err(ConfigError::ParseError(
                    "attachments_dir is too long".to_string(),
                ));
            }
        }
        "sync_reports_dir" => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return Err(ConfigError::ParseError(
                    "sync_reports_dir cannot be empty".to_string(),
                ));
            }
            if trimmed.len() > 200 {
                return Err(ConfigError::ParseError(
                    "sync_reports_dir is too long".to_string(),
                ));
            }
        }
        "attachments_max_upload_mb" => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return Ok(());
            }
            let parsed = trimmed.parse::<i64>().map_err(|_| {
                ConfigError::ParseError(format!(
                    "attachments_max_upload_mb must be an integer (MiB), got '{}'",
                    trimmed
                ))
            })?;
            if parsed < -1 {
                return Err(ConfigError::ParseError(
                    "attachments_max_upload_mb must be -1 (unlimited), 0 (disabled), or a positive integer".to_string(),
                ));
            }
        }
        "sync_write_reports" => {
            parse_bool_flag(value, field)?;
        }
        "agent_context_enabled" => {
            parse_bool_flag(value, field)?;
        }
        "agent_context_extension"
            if value.trim().is_empty() => {
                return Err(ConfigError::ParseError(
                    "agent_context_extension cannot be empty".to_string(),
                ));
            }
        "agent_logs_dir" => {
            // Accept any string; empty clears the setting.
        }
        "agent_instructions" | "agent_instructions_file" => {
            // Accept any string; empty values clear the override.
        }
        "agent_on_start_status"
        | "agent_on_success_status"
        | "agent_on_failure_status"
        | "agent_on_cancel_status" => {
            let _ = parse_optional_status(value, field)?;
        }
        "agent_on_start_reassign_to"
        | "agent_on_success_reassign_to"
        | "agent_on_failure_reassign_to"
        | "agent_on_cancel_reassign_to" => {
            let _ = parse_optional_string(value);
        }
        "agent_worktree_enabled" => {
            let _ = parse_optional_bool_flag(value, field)?;
        }
        "agent_worktree_dir" => {
            let _ = parse_optional_string(value);
        }
        "agent_worktree_branch_prefix" => {
            let _ = parse_optional_string(value);
        }
        "remotes" => {
            let _ = parse_sync_remotes(value)?;
        }
        "strict_members"
            if !value.trim().is_empty() => {
                parse_bool_flag(value, field)?;
            }
        "auto_set_reporter"
        | "auto_populate_members"
        | "auto_assign_on_status"
        | "auto_codeowners_assign"
        | "auto_tags_from_path"
        | "auto_branch_infer_type"
        | "auto_branch_infer_status"
        | "auto_branch_infer_priority"
        | "auto_identity"
        | "auto_identity_git"
        | "scan_enable_ticket_words"
        | "scan_enable_mentions"
        | "scan_strip_attributes"
            if !value.trim().is_empty() => {
                parse_bool_flag(value, field)?;
            }
        "scan_signal_words" | "scan_ticket_patterns" => {
            for part in value.split(',') {
                let token = part.trim();
                if token.len() > 100 {
                    return Err(ConfigError::ParseError(format!(
                        "Value '{}' is too long (max 100 chars)",
                        token
                    )));
                }
            }
        }
        "branch_type_aliases"
            if !value.trim().is_empty() => {
                parse_alias_map::<TaskType>(value, "branch type alias")?;
            }
        "branch_status_aliases"
            if !value.trim().is_empty() => {
                parse_alias_map::<TaskStatus>(value, "branch status alias")?;
            }
        "branch_priority_aliases"
            if !value.trim().is_empty() => {
                parse_alias_map::<Priority>(value, "branch priority alias")?;
            }
        "issue_states" => {
            parse_token_list::<TaskStatus>(value, "task status")?;
        }
        "issue_done_states" => {
            // Non-empty requests must parse to a non-empty, duplicate-free
            // list; empty requests mean "unset/inherit" and are valid.
            if !value.trim().is_empty() {
                parse_done_states_value(value)?;
            }
        }
        "issue_types" => {
            parse_token_list::<TaskType>(value, "task type")?;
        }
        "issue_priorities" => {
            parse_token_list::<Priority>(value, "priority")?;
        }
        "sprints_defaults_capacity_points" | "sprints_defaults_capacity_hours" => {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                trimmed.parse::<u32>().map_err(|err| {
                    ConfigError::ParseError(format!(
                        "Invalid sprint capacity value '{}': {}",
                        value, err
                    ))
                })?;
            }
        }
        "sprints_defaults_length" | "sprints_defaults_overdue_after" => {
            // No additional validation beyond trimming; empty clears the override
        }
        "sprints_notifications_enabled"
            if !value.trim().is_empty() => {
                parse_bool_flag(value, field)?;
            }
        _ => {} // Other fields don't need specific validation
    }
    Ok(())
}

/// Clear a specific field in a project configuration (remove the override).
///
/// Field-precise: clearing a nested agent setting (for example
/// `agent_on_start_status`) clears only that subfield and preserves sibling
/// settings; emptied sections are pruned instead of leaving empty shells.
/// Unknown fields are rejected.
pub fn clear_project_override_field(
    config: &mut ProjectConfig,
    field: &str,
) -> Result<(), ConfigError> {
    match field {
        // Scalar option fields
        "default_assignee" => config.default_assignee = None,
        "default_reporter" => config.default_reporter = None,
        "default_priority" => config.default_priority = None,
        "default_status" => config.default_status = None,
        // List option fields
        "default_tags" => config.default_tags = None,
        "members" => config.members = None,
        "strict_members" => config.strict_members = None,
        // Enum list overrides
        "issue_states" => config.issue_states = None,
        "issue_done_states" => config.issue_done_states = None,
        "issue_types" => config.issue_types = None,
        "issue_priorities" => config.issue_priorities = None,
        // String-config lists
        "tags" => config.tags = None,
        "custom_fields" => config.custom_fields = None,
        "auto_set_reporter" => config.auto_set_reporter = None,
        "auto_assign_on_status" => config.auto_assign_on_status = None,
        "auto_populate_members" => config.auto_populate_members = None,
        "auto_codeowners_assign" => config.auto_codeowners_assign = None,
        "auto_tags_from_path" => config.auto_tags_from_path = None,
        "auto_branch_infer_type" => config.auto_branch_infer_type = None,
        "auto_branch_infer_status" => config.auto_branch_infer_status = None,
        "auto_branch_infer_priority" => config.auto_branch_infer_priority = None,
        "auto_identity" => config.auto_identity = None,
        "auto_identity_git" => config.auto_identity_git = None,
        "scan_signal_words" => config.scan_signal_words = None,
        "scan_ticket_patterns" => config.scan_ticket_patterns = None,
        "scan_enable_ticket_words" => config.scan_enable_ticket_words = None,
        "scan_enable_mentions" => config.scan_enable_mentions = None,
        "scan_strip_attributes" => config.scan_strip_attributes = None,
        "branch_type_aliases" => config.branch_type_aliases = None,
        "branch_status_aliases" => config.branch_status_aliases = None,
        "branch_priority_aliases" => config.branch_priority_aliases = None,
        "agent_context_enabled" => config.agent_context_enabled = None,
        "agent_context_extension" => config.agent_context_extension = None,
        "agent_logs_dir" => config.agent_logs_dir = None,
        "agent_instructions" | "agent_instructions_file" => {
            config.agent_instructions = None;
        }
        "agent_on_start_status" => {
            clear_agent_action_field(config, AgentActionField::Start, ActionSubfield::SetStatus)
        }
        "agent_on_success_status" => {
            clear_agent_action_field(config, AgentActionField::Success, ActionSubfield::SetStatus)
        }
        "agent_on_failure_status" => {
            clear_agent_action_field(config, AgentActionField::Failure, ActionSubfield::SetStatus)
        }
        "agent_on_cancel_status" => {
            clear_agent_action_field(config, AgentActionField::Cancel, ActionSubfield::SetStatus)
        }
        "agent_on_start_reassign_to" => {
            clear_agent_action_field(config, AgentActionField::Start, ActionSubfield::ReassignTo)
        }
        "agent_on_success_reassign_to" => clear_agent_action_field(
            config,
            AgentActionField::Success,
            ActionSubfield::ReassignTo,
        ),
        "agent_on_failure_reassign_to" => clear_agent_action_field(
            config,
            AgentActionField::Failure,
            ActionSubfield::ReassignTo,
        ),
        "agent_on_cancel_reassign_to" => {
            clear_agent_action_field(config, AgentActionField::Cancel, ActionSubfield::ReassignTo)
        }
        "agent_worktree_enabled" => clear_agent_worktree_field(config, WorktreeSubfield::Enabled),
        "agent_worktree_dir" => clear_agent_worktree_field(config, WorktreeSubfield::Dir),
        "agent_worktree_branch_prefix" => {
            clear_agent_worktree_field(config, WorktreeSubfield::BranchPrefix);
        }
        "attachments_dir" => config.attachments_dir = None,
        "attachments_max_upload_mb" => config.attachments_max_upload_mb = None,
        "sync_reports_dir" => config.sync_reports_dir = None,
        "sync_write_reports" => config.sync_write_reports = None,
        // Project name is not optional; do not clear it silently
        "project_name" => { /* no-op: cannot clear non-optional */ }
        other => {
            return Err(ConfigError::ParseError(format!(
                "Unknown project config field: {}",
                other
            )));
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum AgentActionField {
    Start,
    Success,
    Failure,
    Cancel,
}

#[derive(Clone, Copy)]
enum ActionSubfield {
    SetStatus,
    ReassignTo,
}

#[derive(Clone, Copy)]
enum WorktreeSubfield {
    Enabled,
    Dir,
    BranchPrefix,
}

fn action_override_is_empty(action: &crate::config::types::AgentAutomationActionOverride) -> bool {
    action.set_status.is_none() && action.reassign_to.is_none()
}

fn clear_agent_action_field(
    config: &mut ProjectConfig,
    action_field: AgentActionField,
    subfield: ActionSubfield,
) {
    let Some(automation) = config.agent_automation.as_mut() else {
        return;
    };
    let action = match action_field {
        AgentActionField::Start => automation.on_start.as_mut(),
        AgentActionField::Success => automation.on_success.as_mut(),
        AgentActionField::Failure => automation.on_failure.as_mut(),
        AgentActionField::Cancel => automation.on_cancel.as_mut(),
    };
    let Some(action) = action else {
        return;
    };
    match subfield {
        ActionSubfield::SetStatus => action.set_status = None,
        ActionSubfield::ReassignTo => action.reassign_to = None,
    }
    // Prune emptied action overrides and, when every action is gone, the
    // whole section — sibling settings survive, empty shells do not.
    let automation = config.agent_automation.as_ref().unwrap();
    let all_empty = [
        automation.on_start.as_ref(),
        automation.on_success.as_ref(),
        automation.on_failure.as_ref(),
        automation.on_cancel.as_ref(),
    ]
    .iter()
    .all(|action| action.is_none_or(action_override_is_empty));
    if all_empty {
        config.agent_automation = None;
    } else {
        let automation = config.agent_automation.as_mut().unwrap();
        if automation
            .on_start
            .as_ref()
            .is_some_and(action_override_is_empty)
        {
            automation.on_start = None;
        }
        if automation
            .on_success
            .as_ref()
            .is_some_and(action_override_is_empty)
        {
            automation.on_success = None;
        }
        if automation
            .on_failure
            .as_ref()
            .is_some_and(action_override_is_empty)
        {
            automation.on_failure = None;
        }
        if automation
            .on_cancel
            .as_ref()
            .is_some_and(action_override_is_empty)
        {
            automation.on_cancel = None;
        }
    }
}

fn clear_agent_worktree_field(config: &mut ProjectConfig, subfield: WorktreeSubfield) {
    let Some(worktree) = config.agent_worktree.as_mut() else {
        return;
    };
    match subfield {
        WorktreeSubfield::Enabled => worktree.enabled = None,
        WorktreeSubfield::Dir => worktree.dir = None,
        WorktreeSubfield::BranchPrefix => worktree.branch_prefix = None,
    }
    let worktree = config.agent_worktree.as_ref().unwrap();
    let empty = worktree.enabled.is_none()
        && worktree.dir.is_none()
        && worktree.branch_prefix.is_none()
        && worktree.max_parallel_jobs.is_none()
        && worktree.cleanup_on_done.is_none()
        && worktree.cleanup_on_failure.is_none()
        && worktree.cleanup_on_cancel.is_none()
        && worktree.cleanup_delete_branches.is_none();
    if empty {
        config.agent_worktree = None;
    }
}

/// Clear a specific override in a project configuration and save it.
///
/// Compatibility wrapper routed through the shared DEV-71 candidate pipeline
/// (validation before a single atomic write).
pub fn clear_project_field(
    tasks_dir: &Path,
    project_prefix: &str,
    field: &str,
) -> Result<(), ConfigError> {
    let scope = crate::config::candidate::ConfigScope::Project(project_prefix.to_string());
    crate::config::candidate::apply_config_set(
        tasks_dir,
        &scope,
        &crate::config::candidate::ConfigSetRequest {
            entries: vec![(field.to_string(), String::new())],
            force: false,
            dry_run: false,
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clearing_default_priority_uses_none() {
        let mut config = ProjectConfig::new("demo".to_string());
        config.default_priority = Some(Priority::from("Existing"));

        apply_field_to_project_config(&mut config, "default_priority", " ")
            .expect("should allow clearing values");

        assert!(config.default_priority.is_none());
    }

    #[test]
    fn setting_custom_default_priority_preserves_token() {
        let mut config = ProjectConfig::new("demo".to_string());

        apply_field_to_project_config(&mut config, "default_priority", "🔥 Hot")
            .expect("custom priority should be accepted");

        let stored = config
            .default_priority
            .expect("priority should be set")
            .as_str()
            .to_string();
        assert_eq!(stored, "🔥 Hot");
    }

    #[test]
    fn clearing_default_status_uses_none() {
        let mut config = ProjectConfig::new("demo".to_string());
        config.default_status = Some(TaskStatus::from("Todo"));

        apply_field_to_project_config(&mut config, "default_status", "")
            .expect("should allow clearing default status");

        assert!(config.default_status.is_none());
    }

    #[test]
    fn parse_alias_map_accepts_mixed_delimiters() {
        let raw = "feat=Feature; bug : Bug\nchore=Chore";
        let parsed =
            parse_alias_map::<TaskType>(raw, "branch type alias").expect("alias map should parse");
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed.get("feat").unwrap().to_string(), "Feature");
        assert_eq!(parsed.get("bug").unwrap().to_string(), "Bug");
        assert_eq!(parsed.get("chore").unwrap().to_string(), "Chore");
    }

    #[test]
    fn parse_alias_map_normalizes_keys_to_lowercase() {
        let raw_yaml = r#"{ Feat: Feature, BUGFIX: Bug }"#;
        let parsed = parse_alias_map::<TaskType>(raw_yaml, "branch type alias")
            .expect("alias map should parse");
        assert!(parsed.contains_key("feat"));
        assert!(parsed.contains_key("bugfix"));
    }

    #[test]
    fn parse_alias_map_rejects_duplicate_aliases() {
        let raw = "feat=Feature,FEAT=Bug";
        let err = parse_alias_map::<TaskType>(raw, "branch type alias")
            .expect_err("duplicate aliases should error");
        assert!(format!("{}", err).contains("Duplicate branch type alias"));
    }

    #[test]
    fn parse_alias_map_rejects_empty_alias_keys() {
        let err = parse_alias_map::<TaskType>("=Feature", "branch type alias")
            .expect_err("empty alias should error");
        assert!(format!("{}", err).contains("alias cannot be empty"));
    }

    #[test]
    fn custom_issue_states_are_trimmed_and_preserved() {
        let mut config = ProjectConfig::new("demo".to_string());

        apply_field_to_project_config(&mut config, "issue_states", "Queued, In QA")
            .expect("custom states should be accepted");

        let states = config
            .issue_states
            .expect("states should be set")
            .values
            .into_iter()
            .map(|status| status.as_str().to_string())
            .collect::<Vec<_>>();
        assert_eq!(states, vec!["Queued", "In QA"]);
    }

    #[test]
    fn issue_states_empty_entries_error() {
        let mut config = ProjectConfig::new("demo".to_string());

        let err = apply_field_to_project_config(&mut config, "issue_states", "Todo, , Done")
            .expect_err("should reject empty entries");

        assert!(format!("{}", err).contains("entries cannot be empty"));
    }

    #[test]
    fn validation_allows_clearing_optional_fields() {
        assert!(validate_field_value("default_priority", "").is_ok());
        assert!(validate_field_value("default_status", "").is_ok());
    }
}
