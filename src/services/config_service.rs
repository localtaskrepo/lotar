use crate::api_types::ProjectDTO;
use crate::config::manager::ConfigManager;
use crate::config::source_labels::{
    CONFIG_SOURCE_ENTRIES, build_global_source_labels, build_project_source_labels,
    collapse_label_to_scope,
};
use crate::config::validation::errors::ValidationResult;
use crate::errors::{LoTaRError, LoTaRResult};
use crate::workspace::TasksDirectoryResolver;
use serde_yaml;
use std::collections::BTreeMap;

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
            serde_json::to_value(project_cfg)
                .map_err(|e| LoTaRError::SerializationError(e.to_string()))
        } else {
            serde_json::to_value(mgr.get_resolved_config())
                .map_err(|e| LoTaRError::SerializationError(e.to_string()))
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
        let global_cfg = persistence::load_global_config(Some(&resolver.path)).ok();
        let home_cfg = persistence::load_home_config().ok();
        let global_raw: GlobalConfig = global_cfg.clone().unwrap_or_default();

        let mut project_exists = false;
        let mut project_raw_val = serde_json::json!({});

        let (effective_val, sources_by_path) = if let Some(prefix) = project_prefix {
            let resolved_project = mgr.get_project_config(prefix).map_err(|e| {
                LoTaRError::ValidationError(format!(
                    "Failed to load project config for '{}': {}",
                    prefix, e
                ))
            })?;
            let effective_val = serde_json::to_value(&resolved_project)
                .map_err(|e| LoTaRError::SerializationError(e.to_string()))?;

            let project_cfg =
                persistence::load_project_config_from_dir(prefix, &resolver.path).ok();
            if let Some(cfg) = project_cfg.as_ref() {
                project_exists = true;
                project_raw_val = serde_json::to_value(cfg).unwrap_or(serde_json::json!({}));
            }

            let sources = build_project_source_labels(
                &resolved_project,
                &resolved_global,
                project_cfg.as_ref(),
                &global_cfg,
                &home_cfg,
            );

            (effective_val, sources)
        } else {
            let effective_val = serde_json::to_value(&resolved_global)
                .map_err(|e| LoTaRError::SerializationError(e.to_string()))?;
            let sources = build_global_source_labels(&resolved_global, &global_cfg, &home_cfg);
            (effective_val, sources)
        };

        let mut sources = serde_json::Map::new();
        let is_project_scope = project_prefix.is_some();

        for entry in CONFIG_SOURCE_ENTRIES {
            if is_project_scope && entry.inspect_key == "default_prefix" {
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

        let global_effective_val =
            serde_json::to_value(&resolved_global).unwrap_or(serde_json::json!({}));
        let global_raw_val = serde_json::to_value(&global_raw).unwrap_or(serde_json::json!({}));

        Ok(serde_json::json!({
            "effective": effective_val,
            "global_effective": global_effective_val,
            "global_raw": global_raw_val,
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
            crate::config::operations::validate_field_value("default_prefix", &normalized)
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

    /// Set one or more fields with validation; returns aggregate result information
    pub fn set(
        resolver: &TasksDirectoryResolver,
        values: &std::collections::BTreeMap<String, String>,
        global: bool,
        project: Option<&str>,
    ) -> LoTaRResult<ConfigSetOutcome> {
        let mgr = ConfigManager::new_manager_with_tasks_dir_ensure_config(&resolver.path).map_err(
            |e| LoTaRError::ValidationError(format!("Failed to init config manager: {}", e)),
        )?;

        // Validate keys first
        for (k, v) in values {
            ConfigManager::validate_field_name(k, global).map_err(|e| {
                LoTaRError::ValidationError(format!("Invalid field '{}': {}", k, e))
            })?;
            ConfigManager::validate_field_value(k, v).map_err(|e| {
                LoTaRError::ValidationError(format!("Invalid value for '{}': {}", k, e))
            })?;
        }

        // Determine project prefix if needed
        let target_project = if global {
            None
        } else {
            Some(
                project
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| mgr.get_resolved_config().default_prefix.clone()),
            )
        };

        let mut combined = ValidationResult::new();
        let mut updated = false;

        // When setting project fields, avoid storing duplicates of global values.
        if let Some(proj) = target_project.as_deref() {
            let g = mgr.get_resolved_config();
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
                    serde_yaml::from_str::<std::collections::HashMap<String, String>>(trimmed)
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
                    let (alias, target) =
                        entry.split_once('=').or_else(|| entry.split_once(':'))?;
                    vec.push((alias.trim().to_lowercase(), target.trim().to_string()));
                }
                vec.sort();
                Some(vec)
            };

            for (k, v) in values {
                let v_trim = v.trim();

                // Empty means clear the override for project scope (where applicable)
                if v_trim.is_empty() {
                    crate::config::operations::clear_project_field(&resolver.path, proj, k)
                        .map_err(|e| {
                            LoTaRError::ValidationError(format!("Failed to clear '{}': {}", k, e))
                        })?;
                    updated = true;
                    continue;
                }

                let is_equal_to_global = match k.as_str() {
                    // enum list overrides
                    "issue_states" => {
                        // Normalize to canonical enum strings before comparison
                        let lv: Vec<String> = csv(v)
                            .into_iter()
                            .filter_map(|s| s.parse::<crate::types::TaskStatus>().ok())
                            .map(|e| e.to_string())
                            .collect();
                        let gv: Vec<String> = g
                            .issue_states
                            .values
                            .iter()
                            .map(|x| x.to_string())
                            .collect();
                        lv == gv
                    }
                    "issue_types" => {
                        let lv: Vec<String> = csv(v)
                            .into_iter()
                            .filter_map(|s| s.parse::<crate::types::TaskType>().ok())
                            .map(|e| e.to_string())
                            .collect();
                        let gv: Vec<String> =
                            g.issue_types.values.iter().map(|x| x.to_string()).collect();
                        lv == gv
                    }
                    "issue_priorities" => {
                        let lv: Vec<String> = csv(v)
                            .into_iter()
                            .filter_map(|s| s.parse::<crate::types::Priority>().ok())
                            .map(|e| e.to_string())
                            .collect();
                        let gv: Vec<String> = g
                            .issue_priorities
                            .values
                            .iter()
                            .map(|x| x.to_string())
                            .collect();
                        lv == gv
                    }
                    "tags" => {
                        let gv: Vec<String> = g.tags.values.clone();
                        csv(v) == gv
                    }
                    "custom_fields" => {
                        let gv: Vec<String> = g.custom_fields.values.clone();
                        csv(v) == gv
                    }
                    // scalar defaults
                    "default_priority" => match v_trim.parse::<crate::types::Priority>() {
                        Ok(p) => g.default_priority.to_string() == p.to_string(),
                        Err(_) => false,
                    },
                    "default_status" => match v_trim.parse::<crate::types::TaskStatus>() {
                        Ok(sv) => {
                            g.default_status.as_ref().map(|s| s.to_string()).as_deref()
                                == Some(&sv.to_string())
                        }
                        Err(_) => false,
                    },
                    "default_assignee" => {
                        g.default_assignee
                            .as_ref()
                            .map(|s| s.to_string())
                            .unwrap_or_default()
                            == v_trim
                    }
                    "default_reporter" => {
                        g.default_reporter
                            .as_ref()
                            .map(|s| s.to_string())
                            .unwrap_or_default()
                            == v_trim
                    }
                    "default_tags" => {
                        let gv: String = join(&g.default_tags);
                        let lv = csv(v);
                        join(&lv) == gv
                    }
                    "auto_set_reporter" => parse_bool(v_trim) == Some(g.auto_set_reporter),
                    "auto_assign_on_status" => parse_bool(v_trim) == Some(g.auto_assign_on_status),
                    "scan_signal_words" => csv(v) == g.scan_signal_words,
                    "scan_ticket_patterns" => {
                        let lv = csv(v);
                        let gv = g.scan_ticket_patterns.clone().unwrap_or_else(Vec::new);
                        lv == gv
                    }
                    "scan_enable_ticket_words" => {
                        parse_bool(v_trim) == Some(g.scan_enable_ticket_words)
                    }
                    "scan_enable_mentions" => parse_bool(v_trim) == Some(g.scan_enable_mentions),
                    "scan_strip_attributes" => parse_bool(v_trim) == Some(g.scan_strip_attributes),
                    "branch_type_aliases" => {
                        let lv = parse_alias_pairs(v).unwrap_or_default();
                        let mut gv: Vec<(String, String)> = g
                            .branch_type_aliases
                            .iter()
                            .map(|(k, val)| (k.to_lowercase(), val.to_string()))
                            .collect();
                        gv.sort();
                        lv == gv
                    }
                    "branch_status_aliases" => {
                        let lv = parse_alias_pairs(v).unwrap_or_default();
                        let mut gv: Vec<(String, String)> = g
                            .branch_status_aliases
                            .iter()
                            .map(|(k, val)| (k.to_lowercase(), val.to_string()))
                            .collect();
                        gv.sort();
                        lv == gv
                    }
                    "branch_priority_aliases" => {
                        let lv = parse_alias_pairs(v).unwrap_or_default();
                        let mut gv: Vec<(String, String)> = g
                            .branch_priority_aliases
                            .iter()
                            .map(|(k, val)| (k.to_lowercase(), val.to_string()))
                            .collect();
                        gv.sort();
                        lv == gv
                    }
                    // project_name has no global equivalent
                    "project_name" => false,
                    _ => false,
                };

                if is_equal_to_global {
                    crate::config::operations::clear_project_field(&resolver.path, proj, k)
                        .map_err(|e| {
                            LoTaRError::ValidationError(format!("Failed to clear '{}': {}", k, e))
                        })?;
                    updated = true;
                } else {
                    let validation =
                        ConfigManager::update_config_field(&resolver.path, k, v, Some(proj))
                            .map_err(|e| {
                                LoTaRError::ValidationError(format!(
                                    "Failed to update '{}': {}",
                                    k, e
                                ))
                            })?;
                    combined.merge(validation);
                    updated = true;
                }
            }
        } else {
            // Global scope: apply updates directly
            for (k, v) in values {
                let validation = ConfigManager::update_config_field(&resolver.path, k, v, None)
                    .map_err(|e| {
                        LoTaRError::ValidationError(format!("Failed to update '{}': {}", k, e))
                    })?;
                combined.merge(validation);
                updated = true;
            }
        }

        Ok(ConfigSetOutcome {
            updated,
            validation: combined,
        })
    }
}
