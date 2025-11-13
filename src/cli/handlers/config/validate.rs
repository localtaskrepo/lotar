use super::ConfigHandler;
use crate::output::OutputRenderer;
use crate::workspace::TasksDirectoryResolver;

impl ConfigHandler {
    pub(super) fn handle_config_validate(
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
        project: Option<String>,
        global: bool,
        fix: bool,
        errors_only: bool,
    ) -> Result<(), String> {
        use crate::config::validation::{ConfigValidator, ValidationSeverity};

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

        if global || project.is_none() {
            renderer.emit_info("Validating global configuration");
            let result = validator.validate_global_config(&global_config);

            if result.has_errors() || result.has_warnings() {
                has_errors |= result.has_errors();
                all_results.push(("Global Config".to_string(), result));
            } else {
                renderer.emit_success("Global configuration is valid");
            }
        }

        if let Some(project_name) = project {
            let project_display_name = crate::utils::project::project_display_name_from_config(
                &resolver.path,
                &project_name,
            );
            let project_label = crate::utils::project::format_project_label(
                &project_name,
                project_display_name.as_deref(),
            );
            renderer.emit_info(format_args!(
                "Validating project configuration for '{}'",
                project_label
            ));

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
                                let prefix = &project_name;
                                let conflict_result = validator.check_prefix_conflicts(prefix);

                                let mut combined_result = result;
                                combined_result.merge(conflict_result);

                                if combined_result.has_errors() || combined_result.has_warnings() {
                                    has_errors |= combined_result.has_errors();
                                    all_results.push((
                                        format!("Project Config ({})", project_label),
                                        combined_result,
                                    ));
                                } else {
                                    renderer.emit_success("Project configuration is valid");
                                }
                            }
                            Err(e) => {
                                renderer.emit_error(format_args!(
                                    "Could not parse project config YAML: {}",
                                    e
                                ));
                                has_errors = true;
                            }
                        }
                    }
                    Err(e) => {
                        renderer
                            .emit_error(format_args!("Could not read project config file: {}", e));
                        has_errors = true;
                    }
                }
            } else {
                renderer.emit_error(format_args!(
                    "Project config file not found: {}",
                    project_config_path.display()
                ));
                has_errors = true;
            }
        }

        for (scope, result) in all_results {
            renderer.emit_info(format_args!("{} Validation Results:", scope));

            for error in &result.errors {
                if errors_only && error.severity != ValidationSeverity::Error {
                    continue;
                }
                renderer.emit_raw_stdout(format_args!("{}", error));
            }

            if !errors_only {
                for warning in &result.warnings {
                    renderer.emit_raw_stdout(format_args!("{}", warning));
                }

                for info in &result.info {
                    renderer.emit_raw_stdout(format_args!("{}", info));
                }
            }
        }

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
