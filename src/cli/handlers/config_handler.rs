use crate::cli::handlers::CommandHandler;
use crate::cli::{ConfigAction, ConfigShowArgs, ConfigValidateArgs};
use crate::config::ConfigManager;
use crate::output::OutputRenderer;
use crate::types::{Priority, TaskStatus};
use crate::workspace::TasksDirectoryResolver;
use serde_yaml;
use std::fs;

/// Handler for config commands
pub struct ConfigHandler;

impl CommandHandler for ConfigHandler {
    type Args = ConfigAction;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        _project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Self::Result {
        match args {
            ConfigAction::Show(ConfigShowArgs { project }) => {
                Self::handle_config_show(resolver, project)
            }
            ConfigAction::Set(crate::cli::ConfigSetArgs {
                field,
                value,
                dry_run,
                force,
                global,
            }) => Self::handle_config_set(resolver, renderer, field, value, dry_run, force, global),
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
                println!(
                    "{}",
                    renderer.render_success("Available Configuration Templates:")
                );
                println!("  • default - Basic task management setup");
                println!("  • agile - Agile/Scrum workflow configuration");
                println!("  • kanban - Kanban board style setup");
                println!("  • simple - Minimal configuration");
                println!(
                    "{}",
                    renderer.render_info(
                        "Use 'lotar config init --template=<n>' to initialize with a template."
                    )
                );
                Ok(())
            }
        }
    }
}

impl ConfigHandler {
    /// Handle config show command with optional project filter
    fn handle_config_show(
        resolver: &TasksDirectoryResolver,
        project: Option<String>,
    ) -> Result<(), String> {
        let config_manager = ConfigManager::new_manager_with_tasks_dir_readonly(&resolver.path)
            .map_err(|e| format!("Failed to load config: {}", e))?;

        if let Some(project_name) = project {
            // Show project-specific config
            let project_prefix =
                crate::utils::resolve_project_input(&project_name, resolver.path.as_path());
            let project_config = config_manager
                .get_project_config(&project_prefix)
                .map_err(|e| format!("Failed to load project config: {}", e))?;

            println!(
                "{}",
                OutputRenderer::new(crate::output::OutputFormat::Text, false)
                    .render_info(&format!("Configuration for project: {}", project_name))
            );

            // Project Settings section (no server settings for project config)
            println!(
                "{}",
                OutputRenderer::new(crate::output::OutputFormat::Text, false)
                    .render_info("Project Settings:")
            );
            println!("  Tasks directory: {}", resolver.path.display());
            println!("  Task file extension: yml");
            println!("  Project prefix: {}", project_config.default_prefix);

            if let Some(assignee) = &project_config.default_assignee {
                println!("  Default assignee: {}", assignee);
            }
            println!("  Default Priority: {:?}", project_config.default_priority);

            // Show default status if configured
            if let Some(status) = &project_config.default_status {
                println!("  Default Status: {:?}", status);
            }
            println!();

            // Issue Types, States, and Priorities
            println!("Issue States: {:?}", project_config.issue_states.values);
            println!("Issue Types: {:?}", project_config.issue_types.values);
            println!(
                "Issue Priorities: {:?}",
                project_config.issue_priorities.values
            );
        } else {
            let resolved_config = config_manager.get_resolved_config();
            println!(
                "{}",
                OutputRenderer::new(crate::output::OutputFormat::Text, false).render_info(
                    &format!(
                        "Configuration for project: {}",
                        if resolved_config.default_prefix.is_empty() {
                            "(none set - will auto-detect on first task creation)"
                        } else {
                            &resolved_config.default_prefix
                        }
                    )
                )
            );
            println!(
                "{}",
                OutputRenderer::new(crate::output::OutputFormat::Text, false)
                    .render_info("Project Settings:")
            );
            println!("  Tasks directory: {}", resolver.path.display());
            println!("  Task file extension: yml");
            println!("  Project prefix: {}", resolved_config.default_prefix);
            println!("  Port: {}", resolved_config.server_port);
            println!(
                "  Default Project: {}",
                if resolved_config.default_prefix.is_empty() {
                    "(none set - will auto-detect on first task creation)"
                } else {
                    &resolved_config.default_prefix
                }
            );
        }

        Ok(())
    }

    fn handle_config_set(
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
        field: String,
        value: String,
        dry_run: bool,
        force: bool,
        mut global: bool,
    ) -> Result<(), String> {
        // Auto-detect global-only fields
        let global_only_fields = ["server_port", "default_prefix", "default_project"];
        if global_only_fields.contains(&field.as_str()) && !global {
            global = true;
            if !dry_run {
                println!(
                    "{}",
                    renderer.render_info(&format!(
                        "Automatically treating '{}' as global configuration field",
                        field
                    ))
                );
            }
        }

        if dry_run {
            println!(
                "{}",
                renderer.render_info(&format!("DRY RUN: Would set {} = {}", field, value))
            );

            // Check for validation conflicts
            let conflicts = Self::check_validation_conflicts(resolver, &field, &value, global)?;
            if !conflicts.is_empty() {
                println!(
                    "{}",
                    renderer
                        .render_warning("WARNING: This change would cause validation conflicts:")
                );
                for conflict in conflicts {
                    println!("  • {}", conflict);
                }
                if !force {
                    println!(
                        "{}",
                        renderer.render_info(
                            "Use --force to apply anyway, or fix conflicting values first."
                        )
                    );
                    return Ok(());
                }
            }

            println!(
                "{}",
                renderer.render_success(
                    "Dry run completed. Use the same command without --dry-run to apply."
                )
            );
            return Ok(());
        }

        println!(
            "{}",
            renderer.render_info(&format!("Setting configuration: {} = {}", field, value))
        );

        // Check for validation conflicts unless forced
        if !force {
            let conflicts = Self::check_validation_conflicts(resolver, &field, &value, global)?;
            if !conflicts.is_empty() {
                println!(
                    "{}",
                    renderer
                        .render_warning("WARNING: This change would cause validation conflicts:")
                );
                for conflict in conflicts {
                    println!("  • {}", conflict);
                }
                println!(
                    "{}",
                    renderer.render_info(
                        "Use --dry-run to see what would change, or --force to apply anyway."
                    )
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

        // Update the configuration
        ConfigManager::update_config_field(
            &resolver.path,
            &field,
            &value,
            project_prefix.as_deref(),
        )
        .map_err(|e| format!("Failed to update config: {}", e))?;

        // Show helpful information about project-specific config
        if project_prefix.is_some() {
            // Check if the value matches the global default and inform the user
            if Self::check_matches_global_default(&field, &value, &resolver.path) {
                println!(
                    "{}",
                    renderer.render_info(
                        "Note: This project setting matches the global default. This project will now use this explicit value and won't inherit future global changes to this field."
                    )
                );
            }
        }
        println!(
            "{}",
            renderer.render_success(&format!("Successfully updated {}", field))
        );
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
            println!(
                "{}",
                renderer.render_info(&format!(
                    "DRY RUN: Would initialize config with template '{}'",
                    template
                ))
            );
            if let Some(ref prefix) = prefix {
                println!("  • Project prefix: {}", prefix);
                // Validate explicit prefix
                if let Some(ref project_name) = project {
                    if let Err(conflict) =
                        crate::utils::validate_explicit_prefix(prefix, project_name, &resolver.path)
                    {
                        println!("  ❌ Conflict detected: {}", conflict);
                        return Err(conflict);
                    }
                    println!("  ✅ Prefix '{}' is available", prefix);
                }
            }
            if let Some(ref project) = project {
                println!("  • Project name: {}", project);
                // Show what prefix would be generated and check for conflicts
                if prefix.is_none() {
                    match crate::utils::generate_unique_project_prefix(project, &resolver.path) {
                        Ok(generated_prefix) => {
                            println!("  • Generated prefix: {} ✅", generated_prefix);
                        }
                        Err(conflict) => {
                            println!(
                                "  • Generated prefix: {} ❌",
                                crate::utils::generate_project_prefix(project)
                            );
                            println!("  ❌ Conflict detected: {}", conflict);
                            return Err(conflict);
                        }
                    }
                }
            }
            if let Some(ref copy_from) = copy_from {
                println!("  • Copy settings from: {}", copy_from);
            }
            if global {
                println!("  • Target: Global configuration (.tasks/config.yml)");
            } else {
                let project_name = project.as_deref().unwrap_or("DEFAULT");
                let project_prefix = if let Some(ref prefix) = prefix {
                    prefix.clone()
                } else {
                    match crate::utils::generate_unique_project_prefix(project_name, &resolver.path)
                    {
                        Ok(prefix) => prefix,
                        Err(_) => crate::utils::generate_project_prefix(project_name), // For display purposes
                    }
                };
                println!(
                    "  • Target: Project configuration (.tasks/{}/config.yml)",
                    project_prefix
                );
            }
            println!(
                "{}",
                renderer.render_success(
                    "Dry run completed. Use the same command without --dry-run to apply."
                )
            );
            return Ok(());
        }

        // Standardized info message for initialization
        println!(
            "{}",
            renderer.render_info(&format!(
                "Initializing configuration with template '{}'",
                template
            ))
        );

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

        // TODO: Implement actual conflict detection
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
        // TODO: Load actual template from embedded files or resources
        let template_content = match template_name {
            "default" => include_str!("../../config/templates/default.yml"),
            "agile" => include_str!("../../config/templates/agile.yml"),
            "kanban" => include_str!("../../config/templates/kanban.yml"),
            "simple" => include_str!("../../config/templates/simple.yml"),
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

        // Transform template format to config format by flattening "values" fields
        if let Some(config_map) = config.as_mapping_mut() {
            Self::flatten_template_values(config_map);
        }

        // Apply customizations
        if let Some(config_map) = config.as_mapping_mut() {
            // Set project prefix if provided
            if let Some(ref prefix) = prefix {
                config_map.insert(
                    serde_yaml::Value::String("prefix".to_string()),
                    serde_yaml::Value::String(prefix.clone()),
                );
            }

            // Set project name if provided
            if let Some(ref project_name) = project {
                config_map.insert(
                    serde_yaml::Value::String("project_name".to_string()),
                    serde_yaml::Value::String(project_name.clone()),
                );
            }

            // Copy settings from another project if specified
            if let Some(source_project) = copy_from {
                Self::merge_config_from_project(config_map, resolver, &source_project)?;
            }
        }

        // Determine target path
        let config_path = if global {
            // Ensure tasks directory exists for global config
            fs::create_dir_all(&resolver.path)
                .map_err(|e| format!("Failed to create tasks directory: {}", e))?;
            crate::utils::paths::global_config_path(&resolver.path)
        } else {
            let project_name = project
                .as_deref()
                .or_else(|| config.get("project_name").and_then(|v| v.as_str()))
                .unwrap_or("DEFAULT");

            // Generate prefix from project name with conflict detection
            let project_prefix = if let Some(explicit_prefix) = prefix {
                // User provided explicit prefix, validate it doesn't conflict
                crate::utils::validate_explicit_prefix(
                    &explicit_prefix,
                    project_name,
                    &resolver.path,
                )?;
                explicit_prefix
            } else {
                // Generate prefix with conflict detection
                crate::utils::generate_unique_project_prefix(project_name, &resolver.path)?
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

        // Write configuration
        let config_yaml = serde_yaml::to_string(&config)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;

        fs::write(&config_path, config_yaml)
            .map_err(|e| format!("Failed to write config file: {}", e))?;

        println!(
            "{}",
            renderer.render_success(&format!(
                "Configuration initialized at: {}",
                config_path.display()
            ))
        );
        Ok(())
    }

    /// Merge configuration from another project
    fn merge_config_from_project(
        target_config: &mut serde_yaml::Mapping,
        resolver: &TasksDirectoryResolver,
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
            // Copy relevant fields (excluding project_name which should be unique)
            for (key, value) in source_map {
                if let Some(key_str) = key.as_str() {
                    if key_str != "project_name" && key_str != "prefix" {
                        target_config.insert(key.clone(), value.clone());
                    }
                }
            }
        }

        println!(
            "{}",
            OutputRenderer::new(crate::output::OutputFormat::Text, false).render_info(&format!(
                "Copied settings from project '{}'",
                source_project
            ))
        );
        Ok(())
    }

    /// Flatten template "values" structure to direct arrays for config fields
    fn flatten_template_values(config_map: &mut serde_yaml::Mapping) {
        let fields_to_flatten = vec![
            "issue_states",
            "issue_types",
            "issue_priorities",
            "categories",
            "tags",
            "custom_fields",
        ];

        for field_name in fields_to_flatten {
            let field_key = serde_yaml::Value::String(field_name.to_string());
            if let Some(field_value) = config_map.get_mut(&field_key) {
                if let Some(field_map) = field_value.as_mapping() {
                    if let Some(values) =
                        field_map.get(serde_yaml::Value::String("values".to_string()))
                    {
                        // Replace the nested structure with just the values array
                        *field_value = values.clone();
                    }
                }
            }
        }
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

        // Load global config from tasks directory
        let global_config_path = crate::utils::paths::global_config_path(&resolver.path);
        let global_config = if global_config_path.exists() {
            match std::fs::read_to_string(&global_config_path) {
                Ok(content) => serde_yaml::from_str::<crate::config::types::GlobalConfig>(&content)
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
            println!(
                "{}",
                renderer.render_info("Validating global configuration")
            );
            let result = validator.validate_global_config(&global_config);

            if result.has_errors() || result.has_warnings() {
                has_errors |= result.has_errors(); // Only actual errors affect exit code
                all_results.push(("Global Config".to_string(), result));
            } else {
                println!(
                    "{}",
                    renderer.render_success("Global configuration is valid")
                );
            }
        }

        // Validate project config if requested or available
        if let Some(project_name) = project {
            println!(
                "{}",
                renderer.render_info(&format!(
                    "Validating project configuration for '{}'",
                    project_name
                ))
            );

            // Load project config directly from file
            let project_config_path =
                crate::utils::paths::project_config_path(&resolver.path, &project_name);

            if project_config_path.exists() {
                match std::fs::read_to_string(&project_config_path) {
                    Ok(config_content) => {
                        match serde_yaml::from_str::<crate::config::types::ProjectConfig>(
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
                                        format!("Project Config ({})", project_name),
                                        combined_result,
                                    ));
                                } else {
                                    println!(
                                        "{}",
                                        renderer.render_success("Project configuration is valid")
                                    );
                                }
                            }
                            Err(e) => {
                                eprintln!(
                                    "{}",
                                    renderer.render_error(&format!(
                                        "Could not parse project config YAML: {}",
                                        e
                                    ))
                                );
                                has_errors = true;
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "{}",
                            renderer.render_error(&format!(
                                "Could not read project config file: {}",
                                e
                            ))
                        );
                        has_errors = true;
                    }
                }
            } else {
                eprintln!(
                    "{}",
                    renderer.render_error(&format!(
                        "Project config file not found: {}",
                        project_config_path.display()
                    ))
                );
                has_errors = true;
            }
        }

        // Display results
        for (scope, result) in all_results {
            println!(
                "{}",
                renderer.render_info(&format!("{} Validation Results:", scope))
            );

            // Display errors
            for error in &result.errors {
                if errors_only && error.severity != ValidationSeverity::Error {
                    continue;
                }
                println!("{}", error);
            }

            // Display warnings (unless errors_only is set)
            if !errors_only {
                for warning in &result.warnings {
                    println!("{}", warning);
                }

                // Display info messages
                for info in &result.info {
                    println!("{}", info);
                }
            }
        }

        // Handle validation outcome
        if has_errors {
            println!(
                "{}",
                renderer.render_error("Configuration validation failed with errors")
            );

            if fix {
                println!(
                    "{}",
                    renderer.render_warning("Auto-fix functionality not yet implemented")
                );
                println!(
                    "{}",
                    renderer.render_info(
                        "Please review the suggestions above and make manual corrections"
                    )
                );
            }

            return Err("Configuration validation failed".to_string());
        } else {
            println!(
                "{}",
                renderer.render_success("All configurations are valid!")
            );
        }

        Ok(())
    }
}
