use super::ConfigHandler;
use crate::config::ConfigManager;
use crate::config::candidate::{self, ConfigScope, ConfigSetRequest};
use crate::config::operations::canonicalize_field_name;
use crate::output::OutputRenderer;
use crate::types::{Priority, TaskStatus};
use crate::utils::project::resolve_project_input;
use crate::workspace::TasksDirectoryResolver;

impl ConfigHandler {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn handle_config_set(
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
        field: &str,
        value: &str,
        dry_run: bool,
        force: bool,
        mut global: bool,
        project: Option<&str>,
    ) -> Result<(), String> {
        let trimmed_field = field.trim();

        // Fields that exist only in the global configuration — including
        // their dotted canonical aliases such as `server.port` — are applied
        // to the global scope automatically.
        if !global
            && let Some(canonical) =
                crate::config::operations::is_global_only_field_name(trimmed_field)
        {
            global = true;
            if !dry_run {
                renderer.emit_info(format_args!(
                    "Automatically treating '{}' (-> {}) as global configuration field",
                    trimmed_field, canonical
                ));
            }
        }

        // Validate the name for the resolved scope up front so an unknown
        // field never falls back to default-project resolution or writes
        // anything, in dry-run and forced runs alike.
        let canonical = canonicalize_field_name(trimmed_field, global)
            .map_err(|e| format!("Validation error: {}", e))?;

        // Determine the project scope without bootstrapping configuration:
        // read-only resolution plus a read-only prefix auto-detection pass.
        let scope = if global {
            ConfigScope::Global
        } else {
            let explicit = project.and_then(|p| {
                let trimmed = p.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(resolve_project_input(trimmed, &resolver.path))
                }
            });
            let prefix = match explicit {
                Some(prefix) => prefix,
                None => {
                    let readonly =
                        ConfigManager::new_manager_with_tasks_dir_readonly(&resolver.path)
                            .map_err(|e| format!("Failed to load config: {}", e))?;
                    let default_project = readonly.get_resolved_config().default_project.clone();
                    if !default_project.is_empty() {
                        default_project
                    } else if let Some(detected) =
                        crate::config::persistence::auto_detect_prefix(&resolver.path)
                    {
                        detected
                    } else {
                        return Err(
                            "No default project set. Use --global flag or set a default project first."
                                .to_string(),
                        );
                    }
                }
            };
            ConfigScope::Project(prefix)
        };

        // One candidate: schema, config, resolved, and real-task validation
        // before a single atomic write. Dry runs omit only the write.
        let result = candidate::apply_config_set(
            &resolver.path,
            &scope,
            &ConfigSetRequest {
                entries: vec![(trimmed_field.to_string(), value.to_string())],
                force,
                dry_run,
            },
        )
        .map_err(|e| format!("Configuration change rejected: {}", e))?;

        let warnings: Vec<String> = result
            .validation
            .warnings
            .iter()
            .map(|w| w.to_string())
            .collect();

        if dry_run {
            renderer.emit_info(format_args!("DRY RUN: Would set {} = {}", canonical, value));
            if !warnings.is_empty() {
                renderer.emit_warning("Validation warnings this change would carry:");
                for warning in &warnings {
                    renderer.emit_warning(warning);
                }
            }
            renderer.emit_success(
                "Dry run completed. Use the same command without --dry-run to apply.",
            );
            return Ok(());
        }

        if matches!(scope, ConfigScope::Project(_))
            && Self::check_matches_global_default(&canonical, value, &resolver.path)
        {
            renderer.emit_info(
                "Note: This project setting matches the global default. This project will now use this explicit value and won't inherit future global changes to this field.",
            );
        }
        renderer.emit_success(format_args!("Successfully updated {}", canonical));

        if !warnings.is_empty() {
            renderer.emit_warning("Validation warnings detected after applying the change:");
            for warning in warnings {
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
}
