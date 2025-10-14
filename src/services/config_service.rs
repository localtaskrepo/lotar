use crate::config::manager::ConfigManager;
use crate::errors::{LoTaRError, LoTaRResult};
use crate::workspace::TasksDirectoryResolver;
use serde_yaml;

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
        use crate::config::types::{GlobalConfig, ProjectConfig};
        use crate::utils::paths;

        let mgr = ConfigManager::new_manager_with_tasks_dir_readonly(&resolver.path)
            .map_err(|e| LoTaRError::ValidationError(format!("Failed to load config: {}", e)))?;

        // Effective config for requested scope
        let effective_val = if let Some(prefix) = project_prefix {
            let project_cfg = mgr.get_project_config(prefix).map_err(|e| {
                LoTaRError::ValidationError(format!(
                    "Failed to load project config for '{}': {}",
                    prefix, e
                ))
            })?;
            serde_json::to_value(project_cfg)
                .map_err(|e| LoTaRError::SerializationError(e.to_string()))?
        } else {
            serde_json::to_value(mgr.get_resolved_config())
                .map_err(|e| LoTaRError::SerializationError(e.to_string()))?
        };

        // Raw global (file or default) and existence flag
        let global_path = paths::global_config_path(&resolver.path);
        let has_global_file = global_path.exists();
        let global_raw: GlobalConfig =
            persistence::load_global_config(Some(&resolver.path)).unwrap_or_default();

        // Raw project if requested
        let mut project_exists = false;
        let project_raw_val = if let Some(prefix) = project_prefix {
            match persistence::load_project_config_from_dir(prefix, &resolver.path) {
                Ok(pc) => {
                    project_exists = true;
                    serde_json::to_value(pc).unwrap_or(serde_json::json!({}))
                }
                Err(_) => serde_json::json!({}),
            }
        } else {
            serde_json::json!({})
        };

        // Compute simple provenance per field
        let mut sources = serde_json::Map::new();
        let is_project_scope = project_prefix.is_some();

        // Helper to mark a field source
        let mut mark = |field: &str, src: &str| {
            sources.insert(
                field.to_string(),
                serde_json::Value::String(src.to_string()),
            );
        };

        if is_project_scope {
            // Parse project_raw to detect explicit overrides
            let project_raw: ProjectConfig = serde_json::from_value(project_raw_val.clone())
                .unwrap_or(ProjectConfig::new(project_prefix.unwrap_or("").to_string()));
            // Fields that are Option in ProjectConfig indicate overrides if Some
            if project_raw.issue_states.is_some() {
                mark("issue_states", "project");
            } else {
                mark(
                    "issue_states",
                    if has_global_file {
                        "global"
                    } else {
                        "built_in"
                    },
                );
            }
            if project_raw.issue_types.is_some() {
                mark("issue_types", "project");
            } else {
                mark(
                    "issue_types",
                    if has_global_file {
                        "global"
                    } else {
                        "built_in"
                    },
                );
            }
            if project_raw.issue_priorities.is_some() {
                mark("issue_priorities", "project");
            } else {
                mark(
                    "issue_priorities",
                    if has_global_file {
                        "global"
                    } else {
                        "built_in"
                    },
                );
            }
            if project_raw.tags.is_some() {
                mark("tags", "project");
            } else {
                mark(
                    "tags",
                    if has_global_file {
                        "global"
                    } else {
                        "built_in"
                    },
                );
            }
            if project_raw.custom_fields.is_some() {
                mark("custom_fields", "project");
            } else {
                mark(
                    "custom_fields",
                    if has_global_file {
                        "global"
                    } else {
                        "built_in"
                    },
                );
            }
            if project_raw.default_assignee.is_some() {
                mark("default_assignee", "project");
            } else {
                mark(
                    "default_assignee",
                    if has_global_file {
                        "global"
                    } else {
                        "built_in"
                    },
                );
            }
            if project_raw.default_reporter.is_some() {
                mark("default_reporter", "project");
            } else {
                mark(
                    "default_reporter",
                    if has_global_file {
                        "global"
                    } else {
                        "built_in"
                    },
                );
            }
            if project_raw.default_tags.is_some() {
                mark("default_tags", "project");
            } else {
                mark(
                    "default_tags",
                    if has_global_file {
                        "global"
                    } else {
                        "built_in"
                    },
                );
            }
            if project_raw.default_priority.is_some() {
                mark("default_priority", "project");
            } else {
                mark(
                    "default_priority",
                    if has_global_file {
                        "global"
                    } else {
                        "built_in"
                    },
                );
            }
            if project_raw.default_status.is_some() {
                mark("default_status", "project");
            } else {
                mark(
                    "default_status",
                    if has_global_file {
                        "global"
                    } else {
                        "built_in"
                    },
                );
            }
            if project_raw.auto_set_reporter.is_some() {
                mark("auto_set_reporter", "project");
            } else {
                mark(
                    "auto_set_reporter",
                    if has_global_file {
                        "global"
                    } else {
                        "built_in"
                    },
                );
            }
            if project_raw.auto_assign_on_status.is_some() {
                mark("auto_assign_on_status", "project");
            } else {
                mark(
                    "auto_assign_on_status",
                    if has_global_file {
                        "global"
                    } else {
                        "built_in"
                    },
                );
            }
            if project_raw.scan_signal_words.is_some() {
                mark("scan_signal_words", "project");
            } else {
                mark(
                    "scan_signal_words",
                    if has_global_file {
                        "global"
                    } else {
                        "built_in"
                    },
                );
            }
            if project_raw.scan_ticket_patterns.is_some() {
                mark("scan_ticket_patterns", "project");
            } else {
                mark(
                    "scan_ticket_patterns",
                    if has_global_file {
                        "global"
                    } else {
                        "built_in"
                    },
                );
            }
            if project_raw.scan_enable_ticket_words.is_some() {
                mark("scan_enable_ticket_words", "project");
            } else {
                mark(
                    "scan_enable_ticket_words",
                    if has_global_file {
                        "global"
                    } else {
                        "built_in"
                    },
                );
            }
            if project_raw.scan_enable_mentions.is_some() {
                mark("scan_enable_mentions", "project");
            } else {
                mark(
                    "scan_enable_mentions",
                    if has_global_file {
                        "global"
                    } else {
                        "built_in"
                    },
                );
            }
            if project_raw.scan_strip_attributes.is_some() {
                mark("scan_strip_attributes", "project");
            } else {
                mark(
                    "scan_strip_attributes",
                    if has_global_file {
                        "global"
                    } else {
                        "built_in"
                    },
                );
            }
            if project_raw.branch_type_aliases.is_some() {
                mark("branch_type_aliases", "project");
            } else {
                mark(
                    "branch_type_aliases",
                    if has_global_file {
                        "global"
                    } else {
                        "built_in"
                    },
                );
            }
            if project_raw.branch_status_aliases.is_some() {
                mark("branch_status_aliases", "project");
            } else {
                mark(
                    "branch_status_aliases",
                    if has_global_file {
                        "global"
                    } else {
                        "built_in"
                    },
                );
            }
            if project_raw.branch_priority_aliases.is_some() {
                mark("branch_priority_aliases", "project");
            } else {
                mark(
                    "branch_priority_aliases",
                    if has_global_file {
                        "global"
                    } else {
                        "built_in"
                    },
                );
            }
            // Fields only in global
            mark(
                "default_prefix",
                if has_global_file {
                    "global"
                } else {
                    "built_in"
                },
            );
        } else {
            // Global scope: everything comes from global or built-in
            let global_or_default = if has_global_file {
                "global"
            } else {
                "built_in"
            };
            for field in [
                "server_port",
                "default_prefix",
                "default_assignee",
                "default_reporter",
                "default_tags",
                "default_priority",
                "default_status",
                "tags",
                "custom_fields",
                "issue_states",
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
            ] {
                mark(field, global_or_default);
            }
        }

        // Global effective and raw for reference in UIs
        let global_effective_val =
            serde_json::to_value(mgr.get_resolved_config()).unwrap_or(serde_json::json!({}));
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

    /// Set one or more fields with validation; returns true if updated
    pub fn set(
        resolver: &TasksDirectoryResolver,
        values: &std::collections::BTreeMap<String, String>,
        global: bool,
        project: Option<&str>,
    ) -> LoTaRResult<bool> {
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
                    // best-effort clear; ignore errors for non-clearable fields
                    let _ = crate::config::operations::clear_project_field(&resolver.path, proj, k);
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
                        join(&csv(v)) == gv
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
                    // Clear override instead of writing duplicate
                    let _ = crate::config::operations::clear_project_field(&resolver.path, proj, k);
                } else {
                    ConfigManager::update_config_field(&resolver.path, k, v, Some(proj)).map_err(
                        |e| LoTaRError::ValidationError(format!("Failed to update '{}': {}", k, e)),
                    )?;
                }
            }
        } else {
            // Global scope: apply updates directly
            for (k, v) in values {
                ConfigManager::update_config_field(&resolver.path, k, v, None).map_err(|e| {
                    LoTaRError::ValidationError(format!("Failed to update '{}': {}", k, e))
                })?;
            }
        }

        Ok(true)
    }
}
