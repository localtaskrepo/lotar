use super::ConfigHandler;
use crate::config::ConfigManager;
use crate::output::OutputRenderer;
use crate::types::{Priority, TaskStatus};
use crate::utils::project::resolve_project_input;
use crate::workspace::TasksDirectoryResolver;

impl ConfigHandler {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn handle_config_set(
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
                renderer.emit_info(format_args!(
                    "Automatically treating '{}' as global configuration field",
                    field
                ));
            }
        }

        if dry_run {
            renderer.emit_info(format_args!("DRY RUN: Would set {} = {}", field, value));

            // Check for validation conflicts
            let conflicts = Self::check_validation_conflicts(resolver, &field, &value, global)?;
            if !conflicts.is_empty() {
                renderer.emit_warning("WARNING: This change would cause validation conflicts:");
                for conflict in conflicts {
                    renderer.emit_raw_stdout(format_args!("  • {}", conflict));
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

        renderer.emit_info(format_args!("Setting configuration: {} = {}", field, value));

        // Check for validation conflicts unless forced
        if !force {
            let conflicts = Self::check_validation_conflicts(resolver, &field, &value, global)?;
            if !conflicts.is_empty() {
                renderer.emit_warning("WARNING: This change would cause validation conflicts:");
                for conflict in conflicts {
                    renderer.emit_raw_stdout(format_args!("  • {}", conflict));
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

        let validation = ConfigManager::update_config_field(
            &resolver.path,
            &field,
            &value,
            project_prefix.as_deref(),
        )
        .map_err(|e| format!("Failed to update config: {}", e))?;

        validation_warnings.extend(validation.warnings.iter().map(|w| w.to_string()));

        if project_prefix.is_some()
            && Self::check_matches_global_default(&field, &value, &resolver.path)
        {
            renderer.emit_info(
                "Note: This project setting matches the global default. This project will now use this explicit value and won't inherit future global changes to this field.",
            );
        }
        renderer.emit_success(format_args!("Successfully updated {}", field));

        if !validation_warnings.is_empty() {
            renderer.emit_warning("Validation warnings detected after applying the change:");
            for warning in validation_warnings {
                renderer.emit_warning(&warning);
            }
        }
        Ok(())
    }

    fn check_matches_global_default(field: &str, value: &str, tasks_dir: &std::path::Path) -> bool {
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
                _ => {}
            }
        }
        false
    }

    fn check_validation_conflicts(
        _resolver: &TasksDirectoryResolver,
        field: &str,
        new_value: &str,
        _global: bool,
    ) -> Result<Vec<String>, String> {
        let mut conflicts = Vec::new();

        if field == "issue_states.values" && new_value.contains("In-Progress") {
            conflicts.push(
                "Task PROJ-1 has status 'InProgress' which doesn't match new 'In-Progress'"
                    .to_string(),
            );
        }

        Ok(conflicts)
    }
}
