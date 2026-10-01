use crate::api_types::ProjectDTO;
use crate::config::manager::ConfigManager;
use crate::config::source_labels::{
    CONFIG_SOURCE_ENTRIES, build_global_source_labels_with_port,
    build_project_source_labels_with_port, collapse_label_to_scope,
};
use crate::config::validation::errors::ValidationResult;
use crate::errors::{LoTaRError, LoTaRResult};
use crate::workspace::TasksDirectoryResolver;
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct ConfigSetOutcome {
    pub updated: bool,
    pub validation: ValidationResult,
}

pub struct ConfigService;

impl ConfigService {
    pub fn show(
        resolver: &TasksDirectoryResolver,
        project_prefix: Option<&str>,
    ) -> LoTaRResult<serde_json::Value> {
        let mgr = ConfigManager::new_manager_with_tasks_dir_readonly(&resolver.path)
            .map_err(|e| LoTaRError::ValidationError(format!("Failed to load config: {}", e)))?;

        if let Some(prefix) = project_prefix {
            let project_cfg = mgr.get_project_config(prefix).map_err(|e| {
                LoTaRError::ValidationError(format!(
                    "Failed to load project config for '{}': {}",
                    prefix, e
                ))
            })?;
            let mut value = serde_json::to_value(project_cfg)
                .map_err(|e| LoTaRError::SerializationError(e.to_string()))?;
            redact_config(&mut value);
            Ok(value)
        } else {
            let mut value = serde_json::to_value(mgr.get_resolved_config())
                .map_err(|e| LoTaRError::SerializationError(e.to_string()))?;
            redact_config(&mut value);
            Ok(value)
        }
    }

    /// Inspect effective config and field provenance
    pub fn inspect(
        resolver: &TasksDirectoryResolver,
        project_prefix: Option<&str>,
    ) -> LoTaRResult<serde_json::Value> {
        use crate::config::persistence;
        use crate::config::types::GlobalConfig;
        use crate::utils::paths;

        let mgr = ConfigManager::new_manager_with_tasks_dir_readonly(&resolver.path)
            .map_err(|e| LoTaRError::ValidationError(format!("Failed to load config: {}", e)))?;

        let resolved_global = mgr.get_resolved_config().clone();

        let global_path = paths::global_config_path(&resolver.path);
        let has_global_file = global_path.exists();
        let global_port_explicit = persistence::global_config_sets_server_port(&resolver.path);
        let global_cfg = persistence::load_global_config(Some(&resolver.path)).ok();
        let home_cfg = persistence::load_home_config().ok();
        let global_raw: GlobalConfig = global_cfg.clone().unwrap_or_default();

        let mut project_exists = false;
        let mut project_raw_val = serde_json::json!({});
        let mut project_cfg = None;

        let (mut effective_val, sources_by_path) = if let Some(prefix) = project_prefix {
            let resolved_project = mgr.get_project_config(prefix).map_err(|e| {
                LoTaRError::ValidationError(format!(
                    "Failed to load project config for '{}': {}",
                    prefix, e
                ))
            })?;
            let effective_val = serde_json::to_value(&resolved_project)
                .map_err(|e| LoTaRError::SerializationError(e.to_string()))?;

            project_cfg = persistence::load_project_config_from_dir(prefix, &resolver.path).ok();
            if let Some(cfg) = project_cfg.as_ref() {
                project_exists = true;
                project_raw_val = serde_json::to_value(cfg).unwrap_or(serde_json::json!({}));
            }

            let sources = build_project_source_labels_with_port(
                &resolved_project,
                &resolved_global,
                project_cfg.as_ref(),
                &global_cfg,
                &home_cfg,
                global_port_explicit,
            );

            (effective_val, sources)
        } else {
            let effective_val = serde_json::to_value(&resolved_global)
                .map_err(|e| LoTaRError::SerializationError(e.to_string()))?;
            let sources = build_global_source_labels_with_port(
                &resolved_global,
                &global_cfg,
                &home_cfg,
                global_port_explicit,
            );
            (effective_val, sources)
        };

        let mut sources = serde_json::Map::new();
        let is_project_scope = project_prefix.is_some();

        for entry in CONFIG_SOURCE_ENTRIES {
            if is_project_scope && entry.inspect_key == "default_project" {
                let scope = if has_global_file {
                    "global"
                } else {
                    "built_in"
                };
                sources.insert(
                    entry.inspect_key.to_string(),
                    serde_json::Value::String(scope.to_string()),
                );
                continue;
            }

            if let Some(label) = sources_by_path.get(entry.path) {
                let collapsed = collapse_label_to_scope(label);
                sources.insert(
                    entry.inspect_key.to_string(),
                    serde_json::Value::String(collapsed.to_string()),
                );
            }
        }

        let mut global_effective_val =
            serde_json::to_value(&resolved_global).unwrap_or(serde_json::json!({}));
        let mut global_raw_val = serde_json::to_value(&global_raw).unwrap_or(serde_json::json!({}));
        for value in [
            &mut effective_val,
            &mut global_effective_val,
            &mut global_raw_val,
            &mut project_raw_val,
        ] {
            redact_config(value);
        }

        let mut auth_profiles = global_raw.auth_profiles.clone();
        if let Some(home) = home_cfg.as_ref() {
            for (key, profile) in &home.auth_profiles {
                auth_profiles.insert(key.clone(), profile.clone());
            }
        }
        if let Some(cfg) = project_cfg.as_ref() {
            for (key, profile) in &cfg.auth_profiles {
                auth_profiles.insert(key.clone(), profile.clone());
            }
        }
        let mut auth_profiles_val =
            serde_json::to_value(&auth_profiles).unwrap_or(serde_json::json!({}));
        strip_auth_profiles(&mut auth_profiles_val);

        Ok(serde_json::json!({
            "effective": effective_val,
            "global_effective": global_effective_val,
            "global_raw": global_raw_val,
            "auth_profiles": auth_profiles_val,
            "sources": serde_json::Value::Object(sources),
            "has_global_file": has_global_file,
            "project_exists": project_exists,
            "project_raw": project_raw_val,
        }))
    }

    /// Create a new project configuration with optional overrides.
    pub fn create_project(
        resolver: &TasksDirectoryResolver,
        name: &str,
        explicit_prefix: Option<&str>,
        values: Option<&BTreeMap<String, String>>,
    ) -> LoTaRResult<ProjectDTO> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(LoTaRError::ValidationError(
                "Project name is required".to_string(),
            ));
        }

        let tasks_dir = &resolver.path;

        // Ensure the project name does not collide with existing prefixes or names.
        for (prefix, _) in crate::utils::filesystem::list_visible_subdirs(tasks_dir) {
            if prefix.eq_ignore_ascii_case(trimmed) {
                return Err(LoTaRError::ValidationError(format!(
                    "Project name '{}' conflicts with existing prefix '{}'. Choose a different name.",
                    trimmed, prefix
                )));
            }

            if let Ok(cfg) =
                crate::config::persistence::load_project_config_from_dir(&prefix, tasks_dir)
                && cfg.project_name.eq_ignore_ascii_case(trimmed)
            {
                return Err(LoTaRError::ValidationError(format!(
                    "Project '{}' already exists.",
                    cfg.project_name
                )));
            }
        }

        let prefix = if let Some(raw) = explicit_prefix {
            let normalized = raw.trim().to_uppercase();
            if normalized.is_empty() {
                return Err(LoTaRError::ValidationError(
                    "Project prefix cannot be empty".into(),
                ));
            }
            crate::config::operations::validate_field_value("default_project", &normalized)
                .map_err(|e| LoTaRError::ValidationError(e.to_string()))?;
            crate::utils::project::validate_explicit_prefix(
                &normalized,
                trimmed,
                tasks_dir.as_path(),
            )
            .map_err(LoTaRError::ValidationError)?;
            normalized
        } else {
            crate::utils::project::generate_unique_project_prefix(trimmed, tasks_dir.as_path())
                .map_err(LoTaRError::ValidationError)?
        };

        let project_dir = crate::utils::paths::project_dir(tasks_dir, &prefix);
        if project_dir.exists() {
            return Err(LoTaRError::ValidationError(format!(
                "Project prefix '{}' already exists.",
                prefix
            )));
        }

        let mut updates = values.cloned().unwrap_or_default();
        updates.insert("project_name".to_string(), trimmed.to_string());

        let _ = Self::set(resolver, &updates, false, Some(&prefix))?;

        Ok(ProjectDTO {
            name: trimmed.to_string(),
            prefix,
        })
    }

    /// Set one or more fields with validation; returns aggregate result
    /// information.
    ///
    /// Builds ONE candidate through the shared DEV-71 pipeline: names are
    /// canonicalized (flat and dotted), schema/config/resolved/real-task
    /// validation runs against a precedence-faithful snapshot, and a single
    /// atomic write persists all entries together — all-or-nothing even when
    /// some entries are valid and a later one is rejected. No configuration
    /// artifacts are created before validation succeeds. REST and MCP callers
    /// have no force escape: task conflicts reject the request.
    pub fn set(
        resolver: &TasksDirectoryResolver,
        values: &std::collections::BTreeMap<String, String>,
        global: bool,
        project: Option<&str>,
    ) -> LoTaRResult<ConfigSetOutcome> {
        use crate::config::candidate::{self, ConfigScope, ConfigSetRequest};

        let tasks_dir = resolver.path.as_path();

        let scope = if global {
            ConfigScope::Global
        } else {
            let explicit = project
                .map(str::trim)
                .filter(|trimmed| !trimmed.is_empty())
                .map(str::to_string);
            let prefix = explicit.unwrap_or_else(|| {
                // Read-only default-project resolution: no config bootstrap
                // before validation, matching the CLI behavior.
                ConfigManager::new_manager_with_tasks_dir_readonly(tasks_dir)
                    .ok()
                    .and_then(|mgr| {
                        let default = mgr.get_resolved_config().default_project.clone();
                        if default.is_empty() {
                            None
                        } else {
                            Some(default)
                        }
                    })
                    .or_else(|| crate::config::persistence::auto_detect_prefix(tasks_dir))
                    .unwrap_or_default()
            });
            if prefix.is_empty() {
                return Err(LoTaRError::ValidationError(
                    "No default project set. Provide 'project' in the request or set a default project first."
                        .to_string(),
                ));
            }
            ConfigScope::Project(prefix)
        };

        // Canonicalize and schema-validate the RAW values before any rewrite —
        // including the dedup-to-clear conversion below — so REST/MCP share
        // the CLI's exact invalid-value contract: a malformed CSV such as
        // "Todo,,Done" can never normalize its way into an accepted clear.
        let is_global = matches!(scope, ConfigScope::Global);
        let canonical_entries = candidate::canonicalize_entries(
            &values
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<Vec<_>>(),
            is_global,
        )
        .map_err(|e| LoTaRError::ValidationError(e.to_string()))?;
        candidate::validate_raw_entries(&canonical_entries, is_global)
            .map_err(|e| LoTaRError::ValidationError(e.to_string()))?;

        let mut entries: Vec<(String, String)> = Vec::with_capacity(canonical_entries.len());
        if let ConfigScope::Project(_) = &scope {
            // When setting project fields, avoid storing duplicates of global
            // values: an empty entry clears the override instead.
            let resolved_global = ConfigManager::new_manager_with_tasks_dir_readonly(tasks_dir)
                .map_err(|e| LoTaRError::ValidationError(format!("Failed to load config: {}", e)))?
                .get_resolved_config()
                .clone();
            for (field, value) in &canonical_entries {
                if value.trim().is_empty()
                    || Self::value_matches_global(&resolved_global, field, value)
                {
                    entries.push((field.clone(), String::new()));
                } else {
                    entries.push((field.clone(), value.clone()));
                }
            }
        } else {
            entries = canonical_entries;
        }

        let result = candidate::apply_config_set(
            tasks_dir,
            &scope,
            &ConfigSetRequest {
                entries,
                force: false,
                dry_run: false,
            },
        )
        .map_err(|e| LoTaRError::ValidationError(e.to_string()))?;

        Ok(ConfigSetOutcome {
            updated: result.updated,
            validation: result.validation,
        })
    }

    /// Whether a project-scope value equals the resolved global value for the
    /// same canonical field (so storing it would be a redundant duplicate
    /// override). Callers must have canonicalized and schema-validated the
    /// raw entry first; this comparison must never be the acceptance path for
    /// a malformed value.
    fn value_matches_global(
        global: &crate::config::types::ResolvedConfig,
        key: &str,
        raw_value: &str,
    ) -> bool {
        let value = raw_value;
        let value_trim = value.trim();
        let csv = |s: &str| -> Vec<String> {
            s.split(',')
                .map(|p| p.trim())
                .filter(|p| !p.is_empty())
                .map(|p| p.to_string())
                .collect()
        };
        let join = |v: &Vec<String>| -> String { v.join(",") };
        let parse_bool = |raw: &str| -> Option<bool> {
            match raw.trim().to_lowercase().as_str() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            }
        };
        let parse_alias_pairs = |raw: &str| -> Option<Vec<(String, String)>> {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Some(Vec::new());
            }
            if let Ok(map) =
                serde_yaml_ng::from_str::<std::collections::HashMap<String, String>>(trimmed)
            {
                let mut vec: Vec<(String, String)> = map
                    .into_iter()
                    .map(|(k, v)| (k.to_lowercase(), v.trim().to_string()))
                    .collect();
                vec.sort();
                return Some(vec);
            }
            let mut vec: Vec<(String, String)> = Vec::new();
            for entry in trimmed.split([',', ';', '\n']) {
                let entry = entry.trim();
                if entry.is_empty() {
                    continue;
                }
                let (alias, target) = entry.split_once('=').or_else(|| entry.split_once(':'))?;
                vec.push((alias.trim().to_lowercase(), target.trim().to_string()));
            }
            vec.sort();
            Some(vec)
        };

        match key {
            // enum list overrides
            "issue_states" => {
                let local: Vec<String> = csv(value)
                    .into_iter()
                    .filter_map(|s| s.parse::<crate::types::TaskStatus>().ok())
                    .map(|e| e.to_string())
                    .collect();
                let global_values: Vec<String> = global
                    .issue_states
                    .values
                    .iter()
                    .map(|x| x.to_string())
                    .collect();
                local == global_values
            }
            "issue_types" => {
                let local: Vec<String> = csv(value)
                    .into_iter()
                    .filter_map(|s| s.parse::<crate::types::TaskType>().ok())
                    .map(|e| e.to_string())
                    .collect();
                let global_values: Vec<String> = global
                    .issue_types
                    .values
                    .iter()
                    .map(|x| x.to_string())
                    .collect();
                local == global_values
            }
            "issue_priorities" => {
                let local: Vec<String> = csv(value)
                    .into_iter()
                    .filter_map(|s| s.parse::<crate::types::Priority>().ok())
                    .map(|e| e.to_string())
                    .collect();
                let global_values: Vec<String> = global
                    .issue_priorities
                    .values
                    .iter()
                    .map(|x| x.to_string())
                    .collect();
                local == global_values
            }
            "tags" => csv(value) == global.tags.values,
            "custom_fields" => csv(value) == global.custom_fields.values,
            // scalar defaults
            "default_priority" => match value_trim.parse::<crate::types::Priority>() {
                Ok(p) => global.default_priority.to_string() == p.to_string(),
                Err(_) => false,
            },
            "default_status" => match value_trim.parse::<crate::types::TaskStatus>() {
                Ok(sv) => {
                    global
                        .default_status
                        .as_ref()
                        .map(|s| s.to_string())
                        .as_deref()
                        == Some(&sv.to_string())
                }
                Err(_) => false,
            },
            "default_assignee" => {
                global
                    .default_assignee
                    .as_ref()
                    .map(|s| s.to_string())
                    .unwrap_or_default()
                    == value_trim
            }
            "default_reporter" => {
                global
                    .default_reporter
                    .as_ref()
                    .map(|s| s.to_string())
                    .unwrap_or_default()
                    == value_trim
            }
            "default_tags" => {
                let global_value: String = join(&global.default_tags);
                let local = csv(value);
                join(&local) == global_value
            }
            "auto_set_reporter" => parse_bool(value_trim) == Some(global.auto_set_reporter),
            "auto_assign_on_status" => parse_bool(value_trim) == Some(global.auto_assign_on_status),
            "scan_signal_words" => csv(value) == global.scan_signal_words,
            "scan_ticket_patterns" => {
                let local = csv(value);
                let global_value = global.scan_ticket_patterns.clone().unwrap_or_default();
                local == global_value
            }
            "scan_enable_ticket_words" => {
                parse_bool(value_trim) == Some(global.scan_enable_ticket_words)
            }
            "scan_enable_mentions" => parse_bool(value_trim) == Some(global.scan_enable_mentions),
            "scan_strip_attributes" => parse_bool(value_trim) == Some(global.scan_strip_attributes),
            "branch_type_aliases" => {
                let local = parse_alias_pairs(value).unwrap_or_default();
                let mut global_value: Vec<(String, String)> = global
                    .branch_type_aliases
                    .iter()
                    .map(|(k, v)| (k.to_lowercase(), v.to_string()))
                    .collect();
                global_value.sort();
                local == global_value
            }
            "branch_status_aliases" => {
                let local = parse_alias_pairs(value).unwrap_or_default();
                let mut global_value: Vec<(String, String)> = global
                    .branch_status_aliases
                    .iter()
                    .map(|(k, v)| (k.to_lowercase(), v.to_string()))
                    .collect();
                global_value.sort();
                local == global_value
            }
            "branch_priority_aliases" => {
                let local = parse_alias_pairs(value).unwrap_or_default();
                let mut global_value: Vec<(String, String)> = global
                    .branch_priority_aliases
                    .iter()
                    .map(|(k, v)| (k.to_lowercase(), v.to_string()))
                    .collect();
                global_value.sort();
                local == global_value
            }
            // project_name has no global equivalent
            _ => false,
        }
    }
}

fn strip_auth_profiles(value: &mut serde_json::Value) {
    let serde_json::Value::Object(profiles) = value else {
        return;
    };
    for profile in profiles.values_mut() {
        if let serde_json::Value::Object(profile_map) = profile {
            profile_map.remove("token_env");
            profile_map.remove("email_env");
        }
    }
}

fn redact_config(value: &mut serde_json::Value) {
    let serde_json::Value::Object(map) = value else {
        return;
    };
    let keys = ["agents", "agent_profiles"];
    if let Some(profiles) = map.get_mut("auth_profiles") {
        strip_auth_profiles(profiles);
    }
    if let Some(sync) = map.get_mut("sync") {
        redact_config(sync);
    }
    for key in keys {
        let Some(serde_json::Value::Object(profiles)) = map.get_mut(key) else {
            continue;
        };
        for profile in profiles.values_mut() {
            if let serde_json::Value::Object(profile_map) = profile {
                profile_map.remove("env");
            }
        }
    }
}
