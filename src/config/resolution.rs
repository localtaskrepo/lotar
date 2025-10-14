use crate::config::types::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

// Simple in-process cache for resolved configuration keyed by tasks root path.
// This reduces repeated disk IO for common read paths. Invalidate on writes.
static CONFIG_CACHE: OnceLock<RwLock<HashMap<String, ResolvedConfig>>> = OnceLock::new();

fn config_cache() -> &'static RwLock<HashMap<String, ResolvedConfig>> {
    CONFIG_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn cache_key_for(tasks_dir: Option<&Path>) -> String {
    // Normalize to an absolute tasks-root path string for cache key
    let root: PathBuf = match tasks_dir {
        Some(p) => p.to_path_buf(),
        None => crate::utils::paths::tasks_root_from(Path::new(".")),
    };
    let root_str = root
        .canonicalize()
        .unwrap_or(root)
        .to_string_lossy()
        .to_string();
    // Include relevant env overrides in the cache key so runtime env changes create a new entry
    let env_port = std::env::var("LOTAR_PORT").unwrap_or_default();
    let env_proj = std::env::var("LOTAR_PROJECT").unwrap_or_default();
    let env_def_assignee = std::env::var("LOTAR_DEFAULT_ASSIGNEE").unwrap_or_default();
    let env_def_reporter = std::env::var("LOTAR_DEFAULT_REPORTER").unwrap_or_default();
    format!(
        "{}|PORT={}|PROJ={}|DEF_ASG={}|DEF_REP={}",
        root_str, env_port, env_proj, env_def_assignee, env_def_reporter
    )
}

/// Load and merge all configurations with proper priority order
pub fn load_and_merge_configs(tasks_dir: Option<&Path>) -> Result<ResolvedConfig, ConfigError> {
    // Fast path: return from cache if available
    let key = cache_key_for(tasks_dir);
    if let Ok(guard) = config_cache().read() {
        if let Some(cached) = guard.get(&key) {
            return Ok(cached.clone());
        }
    }

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

    let resolved = ResolvedConfig::from_global(config);
    if let Ok(mut guard) = config_cache().write() {
        guard.insert(key, resolved.clone());
    }
    Ok(resolved)
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
    if !override_config.default_tags.is_empty() {
        base.default_tags = override_config.default_tags;
    }
    if override_config.auto_set_reporter != defaults.auto_set_reporter {
        base.auto_set_reporter = override_config.auto_set_reporter;
    }
    if override_config.auto_assign_on_status != defaults.auto_assign_on_status {
        base.auto_assign_on_status = override_config.auto_assign_on_status;
    }
    if override_config.auto_codeowners_assign != defaults.auto_codeowners_assign {
        base.auto_codeowners_assign = override_config.auto_codeowners_assign;
    }
    if override_config.auto_tags_from_path != defaults.auto_tags_from_path {
        base.auto_tags_from_path = override_config.auto_tags_from_path;
    }
    if override_config.auto_branch_infer_type != defaults.auto_branch_infer_type {
        base.auto_branch_infer_type = override_config.auto_branch_infer_type;
    }
    if override_config.auto_branch_infer_status != defaults.auto_branch_infer_status {
        base.auto_branch_infer_status = override_config.auto_branch_infer_status;
    }
    if override_config.auto_branch_infer_priority != defaults.auto_branch_infer_priority {
        base.auto_branch_infer_priority = override_config.auto_branch_infer_priority;
    }
    if override_config.default_priority != defaults.default_priority {
        base.default_priority = override_config.default_priority;
    }
    if override_config.default_status != defaults.default_status {
        base.default_status = override_config.default_status;
    }
    if override_config.custom_fields.values != defaults.custom_fields.values {
        base.custom_fields = override_config.custom_fields;
    }
    if override_config.scan_signal_words != defaults.scan_signal_words {
        base.scan_signal_words = override_config.scan_signal_words;
    }
    if override_config.scan_strip_attributes != defaults.scan_strip_attributes {
        base.scan_strip_attributes = override_config.scan_strip_attributes;
    }
    if override_config.scan_ticket_patterns.is_some() {
        base.scan_ticket_patterns = override_config.scan_ticket_patterns;
    }
    if override_config.scan_enable_ticket_words != defaults.scan_enable_ticket_words {
        base.scan_enable_ticket_words = override_config.scan_enable_ticket_words;
    }
    if override_config.scan_enable_mentions != defaults.scan_enable_mentions {
        base.scan_enable_mentions = override_config.scan_enable_mentions;
    }
    if !override_config.branch_type_aliases.is_empty() {
        base.branch_type_aliases = override_config.branch_type_aliases;
    }
    if !override_config.branch_status_aliases.is_empty() {
        base.branch_status_aliases = override_config.branch_status_aliases;
    }
    if !override_config.branch_priority_aliases.is_empty() {
        base.branch_priority_aliases = override_config.branch_priority_aliases;
    }
    if override_config.auto_identity != defaults.auto_identity {
        base.auto_identity = override_config.auto_identity;
    }
    if override_config.auto_identity_git != defaults.auto_identity_git {
        base.auto_identity_git = override_config.auto_identity_git;
    }
}

/// Overlay fields from a GlobalConfig onto a ResolvedConfig using the same
/// non-default override semantics as merge_global_config. This is used when
/// applying higher-priority scopes (home, env) after project-specific values.
pub fn overlay_global_into_resolved(resolved: &mut ResolvedConfig, override_config: GlobalConfig) {
    let defaults = GlobalConfig::default();

    if override_config.server_port != defaults.server_port {
        resolved.server_port = override_config.server_port;
    }
    if override_config.default_prefix != defaults.default_prefix {
        resolved.default_prefix = override_config.default_prefix;
    }

    if override_config.issue_states.values != defaults.issue_states.values {
        resolved.issue_states = override_config.issue_states;
    }
    if override_config.issue_types.values != defaults.issue_types.values {
        resolved.issue_types = override_config.issue_types;
    }
    if override_config.issue_priorities.values != defaults.issue_priorities.values {
        resolved.issue_priorities = override_config.issue_priorities;
    }
    if override_config.tags.values != defaults.tags.values {
        resolved.tags = override_config.tags;
    }

    if override_config.default_assignee.is_some() {
        resolved.default_assignee = override_config.default_assignee;
    }
    if override_config.default_reporter.is_some() {
        resolved.default_reporter = override_config.default_reporter;
    }
    if !override_config.default_tags.is_empty() {
        resolved.default_tags = override_config.default_tags;
    }
    if override_config.auto_set_reporter != defaults.auto_set_reporter {
        resolved.auto_set_reporter = override_config.auto_set_reporter;
    }
    if override_config.auto_assign_on_status != defaults.auto_assign_on_status {
        resolved.auto_assign_on_status = override_config.auto_assign_on_status;
    }
    if override_config.auto_codeowners_assign != defaults.auto_codeowners_assign {
        resolved.auto_codeowners_assign = override_config.auto_codeowners_assign;
    }
    if override_config.auto_tags_from_path != defaults.auto_tags_from_path {
        resolved.auto_tags_from_path = override_config.auto_tags_from_path;
    }
    if override_config.auto_branch_infer_type != defaults.auto_branch_infer_type {
        resolved.auto_branch_infer_type = override_config.auto_branch_infer_type;
    }
    if override_config.auto_branch_infer_status != defaults.auto_branch_infer_status {
        resolved.auto_branch_infer_status = override_config.auto_branch_infer_status;
    }
    if override_config.auto_branch_infer_priority != defaults.auto_branch_infer_priority {
        resolved.auto_branch_infer_priority = override_config.auto_branch_infer_priority;
    }
    if override_config.default_priority != defaults.default_priority {
        resolved.default_priority = override_config.default_priority;
    }
    if override_config.default_status != defaults.default_status {
        resolved.default_status = override_config.default_status;
    }
    if override_config.custom_fields.values != defaults.custom_fields.values {
        resolved.custom_fields = override_config.custom_fields;
    }
    if override_config.scan_signal_words != defaults.scan_signal_words {
        resolved.scan_signal_words = override_config.scan_signal_words;
    }
    if override_config.scan_strip_attributes != defaults.scan_strip_attributes {
        resolved.scan_strip_attributes = override_config.scan_strip_attributes;
    }
    if override_config.scan_ticket_patterns.is_some() {
        resolved.scan_ticket_patterns = override_config.scan_ticket_patterns;
    }
    if override_config.scan_enable_ticket_words != defaults.scan_enable_ticket_words {
        resolved.scan_enable_ticket_words = override_config.scan_enable_ticket_words;
    }
    if override_config.scan_enable_mentions != defaults.scan_enable_mentions {
        resolved.scan_enable_mentions = override_config.scan_enable_mentions;
    }
    if !override_config.branch_type_aliases.is_empty() {
        resolved.branch_type_aliases = override_config.branch_type_aliases;
    }
    if !override_config.branch_status_aliases.is_empty() {
        resolved.branch_status_aliases = override_config.branch_status_aliases;
    }
    if !override_config.branch_priority_aliases.is_empty() {
        resolved.branch_priority_aliases = override_config.branch_priority_aliases;
    }
    if override_config.auto_identity != defaults.auto_identity {
        resolved.auto_identity = override_config.auto_identity;
    }
    if override_config.auto_identity_git != defaults.auto_identity_git {
        resolved.auto_identity_git = override_config.auto_identity_git;
    }
}

/// Get project-specific configuration by merging with global config
pub fn get_project_config(
    _resolved_config: &ResolvedConfig,
    project_name: &str,
    tasks_dir: &std::path::Path,
) -> Result<ResolvedConfig, ConfigError> {
    // Desired precedence: CLI > env > home > project > global > defaults
    // We don't re-handle CLI here (handled by command handlers). Implement the
    // remainder by building a fresh chain to ensure home/env can override project.

    // 1) Start from defaults -> global
    let mut base_global = GlobalConfig::default();
    if let Ok(global_config) = crate::config::persistence::load_global_config(Some(tasks_dir)) {
        merge_global_config(&mut base_global, global_config);
    }
    // Convert to resolved baseline
    let mut resolved = ResolvedConfig::from_global(base_global.clone());

    // 2) Overlay project-specific config
    let project_config =
        crate::config::persistence::load_project_config_from_dir(project_name, tasks_dir)?;
    if let Some(states) = project_config.issue_states {
        resolved.issue_states = states;
    }
    if let Some(types) = project_config.issue_types {
        resolved.issue_types = types;
    }
    if let Some(priorities) = project_config.issue_priorities {
        resolved.issue_priorities = priorities;
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
    if let Some(auto) = project_config.auto_set_reporter {
        resolved.auto_set_reporter = auto;
    }
    if let Some(auto) = project_config.auto_assign_on_status {
        resolved.auto_assign_on_status = auto;
    }
    if let Some(priority) = project_config.default_priority {
        resolved.default_priority = priority;
    }
    if let Some(status) = project_config.default_status {
        resolved.default_status = Some(status);
    }
    if let Some(custom_fields) = project_config.custom_fields {
        resolved.custom_fields = custom_fields;
    }
    if let Some(scan_words) = project_config.scan_signal_words {
        resolved.scan_signal_words = scan_words;
    }
    if let Some(patterns) = project_config.scan_ticket_patterns {
        resolved.scan_ticket_patterns = Some(patterns);
    }
    if let Some(enable) = project_config.scan_enable_ticket_words {
        resolved.scan_enable_ticket_words = enable;
    }
    if let Some(enable) = project_config.scan_enable_mentions {
        resolved.scan_enable_mentions = enable;
    }
    // project-level scan.strip_attributes override
    if let Some(strip) = project_config.scan_strip_attributes {
        resolved.scan_strip_attributes = strip;
    }
    // Overlay project-level branch alias maps (if provided)
    if let Some(m) = project_config.branch_type_aliases {
        if !m.is_empty() {
            // normalize keys to lowercase at use-time to be safe
            resolved.branch_type_aliases =
                m.into_iter().map(|(k, v)| (k.to_lowercase(), v)).collect();
        }
    }
    if let Some(m) = project_config.branch_status_aliases {
        if !m.is_empty() {
            resolved.branch_status_aliases =
                m.into_iter().map(|(k, v)| (k.to_lowercase(), v)).collect();
        }
    }
    if let Some(m) = project_config.branch_priority_aliases {
        if !m.is_empty() {
            resolved.branch_priority_aliases =
                m.into_iter().map(|(k, v)| (k.to_lowercase(), v)).collect();
        }
    }
    // Smart toggles are currently only global/home/env scoped; project-level toggles could be added later

    // 3) Overlay home config (higher priority than project)
    if let Ok(home_config) = crate::config::persistence::load_home_config() {
        overlay_global_into_resolved(&mut resolved, home_config);
    }

    // 4) Overlay environment variables (highest of config sources)
    let mut env_cfg = GlobalConfig::default();
    crate::config::persistence::apply_env_overrides(&mut env_cfg);
    overlay_global_into_resolved(&mut resolved, env_cfg);

    // Preserve any already-resolved fields from the provided resolved_config that
    // are not impacted by project/home/env precedence (e.g., server_port from
    // tasks-dir scoped config). For now, prefer the rebuilt values to ensure
    // strict precedence as requested.
    Ok(resolved)
}

/// Invalidate the cached resolved configuration for a specific tasks_dir
pub fn invalidate_config_cache_for(tasks_dir: Option<&Path>) {
    let key = cache_key_for(tasks_dir);
    if let Ok(mut guard) = config_cache().write() {
        guard.remove(&key);
    }
}

impl ResolvedConfig {
    pub fn from_global(global: GlobalConfig) -> Self {
        Self {
            server_port: global.server_port,
            default_prefix: global.default_prefix,
            issue_states: global.issue_states,
            issue_types: global.issue_types,
            issue_priorities: global.issue_priorities,
            tags: global.tags,
            default_assignee: global.default_assignee,
            default_reporter: global.default_reporter,
            default_tags: global.default_tags,
            auto_set_reporter: global.auto_set_reporter,
            auto_assign_on_status: global.auto_assign_on_status,
            auto_codeowners_assign: global.auto_codeowners_assign,
            auto_tags_from_path: global.auto_tags_from_path,
            auto_branch_infer_type: global.auto_branch_infer_type,
            auto_branch_infer_status: global.auto_branch_infer_status,
            auto_branch_infer_priority: global.auto_branch_infer_priority,
            default_priority: global.default_priority,
            default_status: global.default_status,
            custom_fields: global.custom_fields,
            scan_signal_words: global.scan_signal_words,
            scan_strip_attributes: global.scan_strip_attributes,
            scan_ticket_patterns: global.scan_ticket_patterns,
            scan_enable_ticket_words: global.scan_enable_ticket_words,
            scan_enable_mentions: global.scan_enable_mentions,
            auto_identity: global.auto_identity,
            auto_identity_git: global.auto_identity_git,
            branch_type_aliases: global.branch_type_aliases,
            branch_status_aliases: global.branch_status_aliases,
            branch_priority_aliases: global.branch_priority_aliases,
        }
    }
}
