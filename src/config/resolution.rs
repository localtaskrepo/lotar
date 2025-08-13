use crate::config::types::*;
use std::path::Path;

/// Load and merge all configurations with proper priority order
pub fn load_and_merge_configs(tasks_dir: Option<&Path>) -> Result<ResolvedConfig, ConfigError> {
    // Start with built-in defaults
    let mut config = GlobalConfig::default();

    // Don't ensure global config exists for read-only operations
    // Only create it when actually needed for task operations

    // 4. Global config (.tasks/config.yml or custom dir) - lowest priority (after defaults)
    let tasks_dir_buf = tasks_dir.map(|p| p.to_path_buf());
    if let Ok(global_config) =
        crate::config::persistence::load_global_config(tasks_dir_buf.as_deref())
    {
        merge_global_config(&mut config, global_config);
    }

    // 3. Project config (.tasks/{project}/config.yml) - will be handled per-project
    // For now, we'll use global as base

    // 2. Home config (~/.lotar) - higher priority
    if let Ok(home_config) = crate::config::persistence::load_home_config() {
        merge_global_config(&mut config, home_config);
    }

    // 1. Environment variables (highest priority)
    crate::config::persistence::apply_env_overrides(&mut config);

    Ok(ResolvedConfig::from_global(config))
}

/// Improved merging that only overrides non-default values
pub fn merge_global_config(base: &mut GlobalConfig, override_config: GlobalConfig) {
    // Only override fields that are different from defaults
    let defaults = GlobalConfig::default();

    if override_config.server_port != defaults.server_port {
        base.server_port = override_config.server_port;
    }
    if override_config.default_prefix != defaults.default_prefix {
        base.default_prefix = override_config.default_prefix;
    }

    // For configurable fields, we do full replacement if they differ
    if override_config.issue_states.values != defaults.issue_states.values {
        base.issue_states = override_config.issue_states;
    }
    if override_config.issue_types.values != defaults.issue_types.values {
        base.issue_types = override_config.issue_types;
    }
    if override_config.issue_priorities.values != defaults.issue_priorities.values {
        base.issue_priorities = override_config.issue_priorities;
    }
    if override_config.categories.values != defaults.categories.values {
        base.categories = override_config.categories;
    }
    if override_config.tags.values != defaults.tags.values {
        base.tags = override_config.tags;
    }

    // Optional fields
    if override_config.default_assignee.is_some() {
        base.default_assignee = override_config.default_assignee;
    }
    if override_config.default_reporter.is_some() {
        base.default_reporter = override_config.default_reporter;
    }
    if override_config.auto_set_reporter != defaults.auto_set_reporter {
        base.auto_set_reporter = override_config.auto_set_reporter;
    }
    if override_config.auto_assign_on_status != defaults.auto_assign_on_status {
        base.auto_assign_on_status = override_config.auto_assign_on_status;
    }
    if override_config.default_priority != defaults.default_priority {
        base.default_priority = override_config.default_priority;
    }
}

/// Get project-specific configuration by merging with global config
pub fn get_project_config(
    resolved_config: &ResolvedConfig,
    project_name: &str,
) -> Result<ResolvedConfig, ConfigError> {
    // Load project-specific config and merge with global
    let project_config = crate::config::persistence::load_project_config(project_name)?;
    let mut resolved = resolved_config.clone();

    // Apply project-specific overrides
    if let Some(states) = project_config.issue_states {
        resolved.issue_states = states;
    }
    if let Some(types) = project_config.issue_types {
        resolved.issue_types = types;
    }
    if let Some(priorities) = project_config.issue_priorities {
        resolved.issue_priorities = priorities;
    }
    if let Some(categories) = project_config.categories {
        resolved.categories = categories;
    }
    if let Some(tags) = project_config.tags {
        resolved.tags = tags;
    }
    if let Some(assignee) = project_config.default_assignee {
        resolved.default_assignee = Some(assignee);
    }
    if let Some(reporter) = project_config.default_reporter {
        resolved.default_reporter = Some(reporter);
    }
    // Note: project-level toggles for automation not currently supported in ProjectConfig; could be added later
    if let Some(priority) = project_config.default_priority {
        resolved.default_priority = priority;
    }
    if let Some(status) = project_config.default_status {
        resolved.default_status = Some(status);
    }
    if let Some(custom_fields) = project_config.custom_fields {
        resolved.custom_fields = custom_fields;
    }

    Ok(resolved)
}

impl ResolvedConfig {
    pub fn from_global(global: GlobalConfig) -> Self {
        Self {
            server_port: global.server_port,
            default_prefix: global.default_prefix,
            issue_states: global.issue_states,
            issue_types: global.issue_types,
            issue_priorities: global.issue_priorities,
            categories: global.categories,
            tags: global.tags,
            default_assignee: global.default_assignee,
            default_reporter: global.default_reporter,
            auto_set_reporter: global.auto_set_reporter,
            auto_assign_on_status: global.auto_assign_on_status,
            default_priority: global.default_priority,
            default_status: global.default_status,
            custom_fields: global.custom_fields,
        }
    }
}
