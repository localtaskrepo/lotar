use crate::cli::handlers::CommandHandler;
use crate::cli::{ConfigAction, ConfigNormalizeArgs, ConfigShowArgs, ConfigValidateArgs};
use crate::config::ConfigManager;
use crate::output::OutputRenderer;
use crate::types::{Priority, TaskStatus};
use crate::utils::project::generate_project_prefix;
use crate::utils::project::generate_unique_project_prefix;
use crate::utils::project::resolve_project_input;
use crate::utils::project::validate_explicit_prefix;
use crate::workspace::TasksDirectoryResolver;
use serde_yaml;
use std::fmt::Display;
use std::fs;

fn format_value_list<T: Display>(values: &[T]) -> String {
    let joined = values
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{}]", joined)
}

/// Handler for config commands
pub struct ConfigHandler;

impl CommandHandler for ConfigHandler {
    type Args = ConfigAction;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Self::Result {
        match args {
            ConfigAction::Show(ConfigShowArgs { project, explain }) => {
                Self::handle_config_show(resolver, renderer, project, explain)
            }
            ConfigAction::Set(crate::cli::ConfigSetArgs {
                field,
                value,
                dry_run,
                force,
                global,
            }) => Self::handle_config_set(
                resolver, renderer, field, value, dry_run, force, global, project,
            ),
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
                renderer.emit_success("Available Configuration Templates:");
                renderer.emit_raw_stdout("  • default - Basic task management setup");
                renderer.emit_raw_stdout("  • agile - Agile/Scrum workflow configuration");
                renderer.emit_raw_stdout("  • kanban - Kanban board style setup");
                renderer.emit_info(
                    "Use 'lotar config init --template=<n>' to initialize with a template.",
                );
                Ok(())
            }
            ConfigAction::Normalize(ConfigNormalizeArgs {
                global,
                project,
                write,
            }) => Self::handle_config_normalize(resolver, renderer, global, project, write),
        }
    }
}

impl ConfigHandler {
    fn handle_config_normalize(
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
        global: bool,
        project: Option<String>,
        write: bool,
    ) -> Result<(), String> {
        use crate::utils::paths;
        use std::path::PathBuf;

        let mut targets: Vec<(String, PathBuf)> = Vec::new();

        if global || project.is_none() {
            let path = paths::global_config_path(&resolver.path);
            if path.exists() {
                targets.push(("global".to_string(), path));
            }
        }

        if let Some(proj) = project {
            let path = paths::project_config_path(&resolver.path, &proj);
            if !path.exists() {
                return Err(format!("Project config not found: {}", path.display()));
            }
            targets.push((format!("project:{}", proj), path));
        } else if !global {
            // If neither --global nor --project specified, normalize all project configs
            for (prefix, dir) in crate::utils::filesystem::list_visible_subdirs(&resolver.path) {
                let cfg = dir.join("config.yml");
                if cfg.exists() {
                    targets.push((format!("project:{}", prefix), cfg));
                }
            }
        }

        if targets.is_empty() {
            renderer.emit_info("No configuration files found to normalize");
            return Ok(());
        }

        let mut changed = 0usize;
        for (label, path) in targets {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

            // Attempt normalization via round-trip through our tolerant parsers,
            // then emit in canonical nested style.
            let canonical = if label == "global" {
                let parsed = crate::config::normalization::parse_global_from_yaml_str(&content)
                    .map_err(|e| e.to_string())?;
                crate::config::normalization::to_canonical_global_yaml(&parsed)
            } else {
                // label is project:<prefix>
                let proj = label.split_once(':').map(|(_, p)| p).unwrap_or("");
                let parsed =
                    crate::config::normalization::parse_project_from_yaml_str(proj, &content)
                        .map_err(|e| e.to_string())?;
                crate::config::normalization::to_canonical_project_yaml(&parsed)
            };

            if write {
                std::fs::write(&path, canonical.as_bytes())
                    .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;
                renderer.emit_success(&format!("Normalized {} -> {}", label, path.display()));
                changed += 1;
            } else {
                renderer.emit_info(&format!("Would normalize {} -> {}", label, path.display()));
                renderer.emit_raw_stdout(&canonical);
            }
        }

        if write {
            renderer.emit_success(&format!(
                "Normalization complete ({} file(s) updated)",
                changed
            ));
        }
        Ok(())
    }
    /// Handle config show command with optional project filter
    fn handle_config_show(
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
        project: Option<String>,
        explain: bool,
    ) -> Result<(), String> {
        // Git-like behavior: adopt the parent tasks root if found (read-only is fine to inherit)
        let effective_read_root = resolver.path.clone();

        let config_manager =
            ConfigManager::new_manager_with_tasks_dir_readonly(&effective_read_root)
                .map_err(|e| format!("Failed to load config: {}", e))?;

        // Small helpers to determine provenance for selected fields
        fn env_source_for_key(
            resolved: &crate::config::types::ResolvedConfig,
            key: &str,
        ) -> Option<&'static str> {
            match key {
                "server_port" => std::env::var("LOTAR_PORT")
                    .ok()
                    .and_then(|p| p.parse::<u16>().ok())
                    .filter(|p| *p == resolved.server_port)
                    .map(|_| "env"),
                "default_project" => std::env::var("LOTAR_PROJECT")
                    .ok()
                    .map(|proj| generate_project_prefix(&proj))
                    .filter(|p| p == &resolved.default_prefix)
                    .map(|_| "env"),
                "default_assignee" => std::env::var("LOTAR_DEFAULT_ASSIGNEE")
                    .ok()
                    .filter(|v| resolved.default_assignee.as_deref() == Some(v.as_str()))
                    .map(|_| "env"),
                "default_reporter" => std::env::var("LOTAR_DEFAULT_REPORTER")
                    .ok()
                    .filter(|v| resolved.default_reporter.as_deref() == Some(v.as_str()))
                    .map(|_| "env"),
                _ => None,
            }
        }

        fn source_label_for_global(
            resolved: &crate::config::types::ResolvedConfig,
            global_cfg: &Option<crate::config::types::GlobalConfig>,
            home_cfg: &Option<crate::config::types::GlobalConfig>,
            key: &str,
        ) -> &'static str {
            if let Some("env") = env_source_for_key(resolved, key) {
                return "env";
            }
            // compare against home then global, else default
            match key {
                "server_port" => {
                    if home_cfg
                        .as_ref()
                        .is_some_and(|home| home.server_port == resolved.server_port)
                    {
                        return "home";
                    }
                    if global_cfg
                        .as_ref()
                        .is_some_and(|glob| glob.server_port == resolved.server_port)
                    {
                        return "global";
                    }
                    "default"
                }
                "default_project" => {
                    if home_cfg
                        .as_ref()
                        .is_some_and(|home| home.default_prefix == resolved.default_prefix)
                    {
                        return "home";
                    }
                    if global_cfg
                        .as_ref()
                        .is_some_and(|glob| glob.default_prefix == resolved.default_prefix)
                    {
                        return "global";
                    }
                    "default"
                }
                "default_assignee" => {
                    if home_cfg
                        .as_ref()
                        .is_some_and(|home| home.default_assignee == resolved.default_assignee)
                    {
                        return "home";
                    }
                    if global_cfg
                        .as_ref()
                        .is_some_and(|glob| glob.default_assignee == resolved.default_assignee)
                    {
                        return "global";
                    }
                    "default"
                }
                "default_reporter" => {
                    if home_cfg
                        .as_ref()
                        .is_some_and(|home| home.default_reporter == resolved.default_reporter)
                    {
                        return "home";
                    }
                    if global_cfg
                        .as_ref()
                        .is_some_and(|glob| glob.default_reporter == resolved.default_reporter)
                    {
                        return "global";
                    }
                    "default"
                }
                "default_priority" => {
                    if home_cfg
                        .as_ref()
                        .is_some_and(|home| home.default_priority == resolved.default_priority)
                    {
                        return "home";
                    }
                    if global_cfg
                        .as_ref()
                        .is_some_and(|glob| glob.default_priority == resolved.default_priority)
                    {
                        return "global";
                    }
                    "default"
                }
                "default_status" => {
                    if home_cfg
                        .as_ref()
                        .is_some_and(|home| home.default_status == resolved.default_status)
                    {
                        return "home";
                    }
                    if global_cfg
                        .as_ref()
                        .is_some_and(|glob| glob.default_status == resolved.default_status)
                    {
                        return "global";
                    }
                    "default"
                }
                "issue_states" => {
                    if home_cfg.as_ref().is_some_and(|home| {
                        home.issue_states.values == resolved.issue_states.values
                    }) {
                        return "home";
                    }
                    if global_cfg.as_ref().is_some_and(|glob| {
                        glob.issue_states.values == resolved.issue_states.values
                    }) {
                        return "global";
                    }
                    "default"
                }
                "issue_types" => {
                    if home_cfg
                        .as_ref()
                        .is_some_and(|home| home.issue_types.values == resolved.issue_types.values)
                    {
                        return "home";
                    }
                    if global_cfg
                        .as_ref()
                        .is_some_and(|glob| glob.issue_types.values == resolved.issue_types.values)
                    {
                        return "global";
                    }
                    "default"
                }
                "issue_priorities" => {
                    if home_cfg.as_ref().is_some_and(|home| {
                        home.issue_priorities.values == resolved.issue_priorities.values
                    }) {
                        return "home";
                    }
                    if global_cfg.as_ref().is_some_and(|glob| {
                        glob.issue_priorities.values == resolved.issue_priorities.values
                    }) {
                        return "global";
                    }
                    "default"
                }
                _ => "default",
            }
        }

        if let Some(project_name) = project {
            // Show project-specific config
            let project_prefix = resolve_project_input(&project_name, resolver.path.as_path());
            let resolved_project = config_manager
                .get_project_config(&project_prefix)
                .map_err(|e| format!("Failed to load project config: {}", e))?;

            let project_cfg_raw = crate::config::persistence::load_project_config_from_dir(
                &project_prefix,
                &effective_read_root,
            )
            .ok();
            let project_label = crate::utils::project::format_project_label(
                &project_prefix,
                project_cfg_raw.as_ref().and_then(|cfg| {
                    let name = cfg.project_name.trim();
                    if name.is_empty() { None } else { Some(name) }
                }),
            );

            renderer.emit_info(&format!("Configuration for project: {}", project_label));

            // Project Settings section (no server settings for project config)
            renderer.emit_info("Project Settings:");
            renderer.emit_raw_stdout(&format!(
                "  Tasks directory: {}",
                effective_read_root.display()
            ));
            renderer.emit_raw_stdout("  Task file extension: yml");
            renderer.emit_raw_stdout(&format!(
                "  Project prefix: {}",
                resolved_project.default_prefix
            ));

            if let Some(assignee) = &resolved_project.default_assignee {
                renderer.emit_raw_stdout(&format!("  Default assignee: {}", assignee));
            }
            renderer.emit_raw_stdout(&format!(
                "  Default Priority: {}",
                resolved_project.default_priority
            ));

            // Show default status if configured
            if let Some(status) = &resolved_project.default_status {
                renderer.emit_raw_stdout(&format!("  Default Status: {}", status));
            }
            renderer.emit_raw_stdout("");

            // Issue Types, States, and Priorities
            renderer.emit_raw_stdout(&format!(
                "Issue States: {}",
                format_value_list(&resolved_project.issue_states.values)
            ));
            renderer.emit_raw_stdout(&format!(
                "Issue Types: {}",
                format_value_list(&resolved_project.issue_types.values)
            ));
            renderer.emit_raw_stdout(&format!(
                "Issue Priorities: {}",
                format_value_list(&resolved_project.issue_priorities.values)
            ));

            if explain {
                renderer.emit_info("Value sources:");
                let base = config_manager.get_resolved_config();

                // Compare against base (global+home+env merged) to infer project overrides
                if resolved_project.default_assignee != base.default_assignee {
                    renderer.emit_raw_stdout("  default_assignee: project");
                }
                if resolved_project.default_reporter != base.default_reporter {
                    renderer.emit_raw_stdout("  default_reporter: project");
                }
                if resolved_project.default_priority != base.default_priority {
                    renderer.emit_raw_stdout("  default_priority: project");
                }
                if resolved_project.default_status != base.default_status {
                    renderer.emit_raw_stdout("  default_status: project");
                }
                if resolved_project.issue_states.values != base.issue_states.values {
                    renderer.emit_raw_stdout("  issue_states: project");
                }
                if resolved_project.issue_types.values != base.issue_types.values {
                    renderer.emit_raw_stdout("  issue_types: project");
                }
                if resolved_project.issue_priorities.values != base.issue_priorities.values {
                    renderer.emit_raw_stdout("  issue_priorities: project");
                }

                // If JSON format, emit a structured explanation block
                if matches!(renderer.format, crate::output::OutputFormat::Json) {
                    // Determine per-field source using project config file presence
                    let global_cfg =
                        crate::config::persistence::load_global_config(Some(&effective_read_root))
                            .ok();
                    let home_cfg = crate::config::persistence::load_home_config().ok();

                    let src = |key: &str| -> &'static str {
                        // Project file explicit setting wins
                        match (key, &project_cfg_raw) {
                            ("default_assignee", Some(pc)) if pc.default_assignee.is_some() => {
                                "project"
                            }
                            ("default_reporter", Some(pc)) if pc.default_reporter.is_some() => {
                                "project"
                            }
                            ("default_priority", Some(pc)) if pc.default_priority.is_some() => {
                                "project"
                            }
                            ("default_status", Some(pc)) if pc.default_status.is_some() => {
                                "project"
                            }
                            ("issue_states", Some(pc)) if pc.issue_states.is_some() => "project",
                            ("issue_types", Some(pc)) if pc.issue_types.is_some() => "project",
                            ("issue_priorities", Some(pc)) if pc.issue_priorities.is_some() => {
                                "project"
                            }
                            _ => source_label_for_global(
                                &resolved_project,
                                &global_cfg,
                                &home_cfg,
                                key,
                            ),
                        }
                    };

                    let explanation = serde_json::json!({
                        "status": "success",
                        "scope": "project",
                        "project": project_name,
                        "project_prefix": project_prefix,
                        "project_label": project_label,
                        "config": resolved_project,
                        "sources": {
                            "default_assignee": src("default_assignee"),
                            "default_reporter": src("default_reporter"),
                            "default_priority": src("default_priority"),
                            "default_status": src("default_status"),
                            "issue_states": src("issue_states"),
                            "issue_types": src("issue_types"),
                            "issue_priorities": src("issue_priorities")
                        }
                    });
                    renderer.emit_raw_stdout(&explanation.to_string());
                }
            }
        } else {
            let resolved_config = config_manager.get_resolved_config();
            renderer.emit_info(&format!(
                "Configuration for project: {}",
                if resolved_config.default_prefix.is_empty() {
                    "(none set - will auto-detect on first task creation)"
                } else {
                    &resolved_config.default_prefix
                }
            ));
            renderer.emit_info("Project Settings:");
            renderer.emit_raw_stdout(&format!(
                "  Tasks directory: {}",
                effective_read_root.display()
            ));
            renderer.emit_raw_stdout("  Task file extension: yml");
            renderer.emit_raw_stdout(&format!(
                "  Project prefix: {}",
                resolved_config.default_prefix
            ));
            renderer.emit_raw_stdout(&format!("  Port: {}", resolved_config.server_port));
            renderer.emit_raw_stdout(&format!(
                "  Default Project: {}",
                if resolved_config.default_prefix.is_empty() {
                    "(none set - will auto-detect on first task creation)"
                } else {
                    &resolved_config.default_prefix
                }
            ));

            if explain {
                renderer.emit_info("Value sources:");
                let global_cfg =
                    crate::config::persistence::load_global_config(Some(&effective_read_root)).ok();
                let home_cfg = crate::config::persistence::load_home_config().ok();

                let sp =
                    source_label_for_global(resolved_config, &global_cfg, &home_cfg, "server_port");
                let dp = source_label_for_global(
                    resolved_config,
                    &global_cfg,
                    &home_cfg,
                    "default_project",
                );
                let da = source_label_for_global(
                    resolved_config,
                    &global_cfg,
                    &home_cfg,
                    "default_assignee",
                );
                let dr = source_label_for_global(
                    resolved_config,
                    &global_cfg,
                    &home_cfg,
                    "default_reporter",
                );
                let pri = source_label_for_global(
                    resolved_config,
                    &global_cfg,
                    &home_cfg,
                    "default_priority",
                );
                let ds = source_label_for_global(
                    resolved_config,
                    &global_cfg,
                    &home_cfg,
                    "default_status",
                );
                let iss = source_label_for_global(
                    resolved_config,
                    &global_cfg,
                    &home_cfg,
                    "issue_states",
                );
                let ity =
                    source_label_for_global(resolved_config, &global_cfg, &home_cfg, "issue_types");
                let ipr = source_label_for_global(
                    resolved_config,
                    &global_cfg,
                    &home_cfg,
                    "issue_priorities",
                );

                renderer.emit_raw_stdout(&format!("  server_port: {}", sp));
                renderer.emit_raw_stdout(&format!("  default_project: {}", dp));
                renderer.emit_raw_stdout(&format!("  default_assignee: {}", da));
                renderer.emit_raw_stdout(&format!("  default_reporter: {}", dr));
                renderer.emit_raw_stdout(&format!("  default_priority: {}", pri));
                renderer.emit_raw_stdout(&format!("  default_status: {}", ds));
                renderer.emit_raw_stdout(&format!("  issue_states: {}", iss));
                renderer.emit_raw_stdout(&format!("  issue_types: {}", ity));
                renderer.emit_raw_stdout(&format!("  issue_priorities: {}", ipr));

                // JSON structured explanation if requested in JSON format
                if matches!(renderer.format, crate::output::OutputFormat::Json) {
                    let explanation = serde_json::json!({
                        "status": "success",
                        "scope": "global",
                        "config": resolved_config,
                        "sources": {
                            "server_port": sp,
                            "default_project": dp,
                            "default_assignee": da,
                            "default_reporter": dr,
                            "default_priority": pri,
                            "default_status": ds,
                            "issue_states": iss,
                            "issue_types": ity,
                            "issue_priorities": ipr
                        }
                    });
                    renderer.emit_raw_stdout(&explanation.to_string());
                }
            }
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_config_set(
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
                renderer.emit_info(&format!(
                    "Automatically treating '{}' as global configuration field",
                    field
                ));
            }
        }

        if dry_run {
            renderer.emit_info(&format!("DRY RUN: Would set {} = {}", field, value));

            // Check for validation conflicts
            let conflicts = Self::check_validation_conflicts(resolver, &field, &value, global)?;
            if !conflicts.is_empty() {
                renderer.emit_warning("WARNING: This change would cause validation conflicts:");
                for conflict in conflicts {
                    renderer.emit_raw_stdout(&format!("  • {}", conflict));
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

        renderer.emit_info(&format!("Setting configuration: {} = {}", field, value));

        // Check for validation conflicts unless forced
        if !force {
            let conflicts = Self::check_validation_conflicts(resolver, &field, &value, global)?;
            if !conflicts.is_empty() {
                renderer.emit_warning("WARNING: This change would cause validation conflicts:");
                for conflict in conflicts {
                    renderer.emit_raw_stdout(&format!("  • {}", conflict));
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
                renderer.emit_info(
                    "Note: This project setting matches the global default. This project will now use this explicit value and won't inherit future global changes to this field.",
                );
            }
        }
        renderer.emit_success(&format!("Successfully updated {}", field));
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
            renderer.emit_info(&format!(
                "DRY RUN: Would initialize config with template '{}'",
                template
            ));
            if let Some(ref prefix) = prefix {
                renderer.emit_raw_stdout(&format!("  • Project prefix: {}", prefix));
                // Validate explicit prefix
                if let Some(ref project_name) = project {
                    if let Err(conflict) =
                        validate_explicit_prefix(prefix, project_name, &resolver.path)
                    {
                        renderer.emit_raw_stdout(&format!("  ❌ Conflict detected: {}", conflict));
                        return Err(conflict);
                    }
                    renderer.emit_raw_stdout(&format!("  ✅ Prefix '{}' is available", prefix));
                }
            }
            if let Some(ref project) = project {
                renderer.emit_raw_stdout(&format!("  • Project name: {}", project));
                // Show what prefix would be generated and check for conflicts
                if prefix.is_none() {
                    match generate_unique_project_prefix(project, &resolver.path) {
                        Ok(generated_prefix) => {
                            renderer.emit_raw_stdout(&format!(
                                "  • Generated prefix: {} ✅",
                                generated_prefix
                            ));
                        }
                        Err(conflict) => {
                            renderer.emit_raw_stdout(&format!(
                                "  • Generated prefix: {} ❌",
                                generate_project_prefix(project)
                            ));
                            renderer
                                .emit_raw_stdout(&format!("  ❌ Conflict detected: {}", conflict));
                            return Err(conflict);
                        }
                    }
                }
            }
            if let Some(ref copy_from) = copy_from {
                renderer.emit_raw_stdout(&format!("  • Copy settings from: {}", copy_from));
            }
            if global {
                renderer.emit_raw_stdout("  • Target: Global configuration (.tasks/config.yml)");
            } else {
                let project_name = project.as_deref().unwrap_or("DEFAULT");
                let project_prefix = if let Some(ref prefix) = prefix {
                    prefix.clone()
                } else {
                    match generate_unique_project_prefix(project_name, &resolver.path) {
                        Ok(prefix) => prefix,
                        Err(_) => generate_project_prefix(project_name), // For display purposes
                    }
                };
                renderer.emit_raw_stdout(&format!(
                    "  • Target: Project configuration (.tasks/{}/config.yml)",
                    project_prefix
                ));
            }
            renderer.emit_success(
                "Dry run completed. Use the same command without --dry-run to apply.",
            );
            return Ok(());
        }

        // Standardized info message for initialization
        renderer.emit_info(&format!(
            "Initializing configuration with template '{}'",
            template
        ));

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

        // TODO (LOTA-4): Implement actual conflict detection
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
        // TODO (LOTA-5): Load actual template from embedded files or resources
        let template_content = match template_name {
            "default" => include_str!("../../config/templates/default.yml"),
            "agile" => include_str!("../../config/templates/agile.yml"),
            "kanban" => include_str!("../../config/templates/kanban.yml"),
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

        if let Some(config_map) = config.as_mapping_mut() {
            Self::normalize_template_config(config_map);
        }

        if let Some(config_map) = config.as_mapping_mut() {
            if let Some(source_project) = copy_from.as_deref() {
                Self::merge_config_from_project(config_map, resolver, renderer, source_project)?;
                // Re-normalize after merge to ensure canonical structure
                Self::normalize_template_config(config_map);
            }

            if let Some(project_name) = project.as_ref() {
                Self::set_nested_value(
                    config_map,
                    &["project", "name"],
                    serde_yaml::Value::String(project_name.clone()),
                );
            }
        }

        // Determine target path
        let config_path = if global {
            // Ensure tasks directory exists for global config
            fs::create_dir_all(&resolver.path)
                .map_err(|e| format!("Failed to create tasks directory: {}", e))?;
            crate::utils::paths::global_config_path(&resolver.path)
        } else {
            let detected_project_name = Self::extract_project_name(&config);
            let project_name_owned = project
                .as_deref()
                .map(|s| s.to_string())
                .or_else(|| detected_project_name.clone())
                .unwrap_or_else(|| "DEFAULT".to_string());

            // Generate prefix from project name with conflict detection
            let project_prefix = if let Some(explicit_prefix) = &prefix {
                // User provided explicit prefix, validate it doesn't conflict
                validate_explicit_prefix(explicit_prefix, &project_name_owned, &resolver.path)?;
                explicit_prefix.clone()
            } else {
                // Generate prefix with conflict detection
                generate_unique_project_prefix(&project_name_owned, &resolver.path)?
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

        // Write configuration using canonical nested format
        let tmp_yaml = serde_yaml::to_string(&config)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;
        let canonical = if config_path.parent() == Some(&resolver.path) {
            // Global config
            let parsed = crate::config::normalization::parse_global_from_yaml_str(&tmp_yaml)
                .map_err(|e| format!("Failed to parse config for canonicalization: {}", e))?;
            Self::validate_generated_global_config(resolver, renderer, &parsed)?;
            crate::config::normalization::to_canonical_global_yaml(&parsed)
        } else {
            // Project config; derive prefix from parent dir name
            let prefix = config_path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                .unwrap_or("");
            // Use the human-readable project name for storage metadata if provided
            let detected_project_name = Self::extract_project_name(&config);
            let project_name_value = project
                .as_deref()
                .or(detected_project_name.as_deref())
                .unwrap_or(prefix);
            let parsed = crate::config::normalization::parse_project_from_yaml_str(
                project_name_value,
                &tmp_yaml,
            )
            .map_err(|e| format!("Failed to parse project config for canonicalization: {}", e))?;
            Self::validate_generated_project_config(resolver, renderer, &parsed)?;
            crate::config::normalization::to_canonical_project_yaml(&parsed)
        };

        fs::write(&config_path, canonical)
            .map_err(|e| format!("Failed to write config file: {}", e))?;

        renderer.emit_success(&format!(
            "Configuration initialized at: {}",
            config_path.display()
        ));
        Ok(())
    }

    /// Merge configuration from another project
    fn merge_config_from_project(
        target_config: &mut serde_yaml::Mapping,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
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
            // Copy relevant fields (excluding identity-specific keys like project name/prefix)
            for (key, value) in source_map {
                if let Some(key_str) = key.as_str()
                    && key_str != "project_name"
                    && key_str != "prefix"
                    && key_str != "project"
                {
                    target_config.insert(key.clone(), value.clone());
                }
            }
        }

        renderer.emit_info(&format!(
            "Copied settings from project '{}'",
            source_project
        ));
        Ok(())
    }

    fn normalize_template_config(config_map: &mut serde_yaml::Mapping) {
        use serde_yaml::Value as Y;

        if let Some(project_name_value) = config_map.remove(Y::String("project_name".into())) {
            Self::set_nested_value(config_map, &["project", "name"], project_name_value);
        }

        // "prefix" is only used for CLI arguments; canonical templates shouldn't store it
        config_map.remove(Y::String("prefix".into()));

        if let Some(value) = config_map.remove(Y::String("issue_states".into())) {
            let normalized = Self::unwrap_template_values(value);
            Self::set_nested_value(config_map, &["issue", "states"], normalized);
        }
        if let Some(value) = config_map.remove(Y::String("issue_types".into())) {
            let normalized = Self::unwrap_template_values(value);
            Self::set_nested_value(config_map, &["issue", "types"], normalized);
        }
        if let Some(value) = config_map.remove(Y::String("issue_priorities".into())) {
            let normalized = Self::unwrap_template_values(value);
            Self::set_nested_value(config_map, &["issue", "priorities"], normalized);
        }
        if let Some(value) = config_map.remove(Y::String("tags".into())) {
            let normalized = Self::unwrap_template_values(value);
            Self::set_nested_value(config_map, &["issue", "tags"], normalized);
        }
        if let Some(value) = config_map.remove(Y::String("categories".into())) {
            let normalized = Self::unwrap_template_values(value);
            Self::set_nested_value(config_map, &["issue", "categories"], normalized);
        }
        if let Some(value) = config_map.remove(Y::String("custom_fields".into())) {
            let normalized = Self::unwrap_template_values(value);
            Self::set_nested_value(config_map, &["custom", "fields"], normalized);
        }
    }

    fn unwrap_template_values(value: serde_yaml::Value) -> serde_yaml::Value {
        use serde_yaml::Value as Y;
        match value {
            Y::Mapping(mut map) => {
                let values_key = Y::String("values".into());
                if let Some(inner) = map.remove(&values_key) {
                    return inner;
                }
                let primitive_key = Y::String("primitive".into());
                if let Some(inner) = map.remove(&primitive_key) {
                    return inner;
                }
                Y::Mapping(map)
            }
            other => other,
        }
    }

    fn set_nested_value(map: &mut serde_yaml::Mapping, path: &[&str], value: serde_yaml::Value) {
        use serde_yaml::Value as Y;

        if path.is_empty() {
            return;
        }

        let key = Y::String(path[0].to_string());
        if path.len() == 1 {
            map.insert(key, value);
            return;
        }

        let mut child = match map.remove(&key) {
            Some(Y::Mapping(existing)) => existing,
            _ => serde_yaml::Mapping::new(),
        };

        Self::set_nested_value(&mut child, &path[1..], value);
        map.insert(key, Y::Mapping(child));
    }

    fn extract_project_name(config: &serde_yaml::Value) -> Option<String> {
        use serde_yaml::Value as Y;

        let map = config.as_mapping()?;
        if let Some(project_value) = map.get(Y::String("project".into())) {
            if let Some(project_map) = project_value.as_mapping() {
                if let Some(name_value) = project_map.get(Y::String("name".into())) {
                    if let Some(name_str) = name_value.as_str() {
                        let trimmed = name_str.trim();
                        if trimmed.is_empty() || trimmed.contains("{{") {
                            return None;
                        }
                        return Some(trimmed.to_string());
                    }
                }
            }
        }

        if let Some(legacy_name) = map.get(Y::String("project_name".into())) {
            if let Some(name_str) = legacy_name.as_str() {
                let trimmed = name_str.trim();
                if trimmed.is_empty() || trimmed.contains("{{") {
                    return None;
                }
                return Some(trimmed.to_string());
            }
        }

        None
    }

    fn validate_generated_project_config(
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
        config: &crate::config::types::ProjectConfig,
    ) -> Result<(), String> {
        let validator = crate::config::validation::ConfigValidator::new(&resolver.path);
        let result = validator.validate_project_config(config);

        for warning in &result.warnings {
            renderer.emit_warning(&warning.to_string());
        }

        if result.has_errors() {
            for error in &result.errors {
                renderer.emit_error(&error.to_string());
            }
            return Err("Generated project configuration failed validation".to_string());
        }

        Ok(())
    }

    fn validate_generated_global_config(
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
        config: &crate::config::types::GlobalConfig,
    ) -> Result<(), String> {
        let validator = crate::config::validation::ConfigValidator::new(&resolver.path);
        let result = validator.validate_global_config(config);

        for warning in &result.warnings {
            renderer.emit_warning(&warning.to_string());
        }

        if result.has_errors() {
            for error in &result.errors {
                renderer.emit_error(&error.to_string());
            }
            return Err("Generated global configuration failed validation".to_string());
        }

        Ok(())
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

        // Load global config from tasks directory (normalization-aware)
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

        // Validate global config if requested or no specific scope given
        if global || project.is_none() {
            renderer.emit_info("Validating global configuration");
            let result = validator.validate_global_config(&global_config);

            if result.has_errors() || result.has_warnings() {
                has_errors |= result.has_errors(); // Only actual errors affect exit code
                all_results.push(("Global Config".to_string(), result));
            } else {
                renderer.emit_success("Global configuration is valid");
            }
        }

        // Validate project config if requested or available
        if let Some(project_name) = project {
            let project_display_name = crate::utils::project::project_display_name_from_config(
                &resolver.path,
                &project_name,
            );
            let project_label = crate::utils::project::format_project_label(
                &project_name,
                project_display_name.as_deref(),
            );
            renderer.emit_info(&format!(
                "Validating project configuration for '{}'",
                project_label
            ));

            // Load project config directly from file
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

                                // For prefix conflicts, we need to determine the actual prefix used
                                // This would typically come from the project directory name or config
                                let prefix = &project_name; // Simple fallback
                                let conflict_result = validator.check_prefix_conflicts(prefix);

                                let mut combined_result = result;
                                combined_result.merge(conflict_result);

                                if combined_result.has_errors() || combined_result.has_warnings() {
                                    has_errors |= combined_result.has_errors(); // Only actual errors affect exit code
                                    all_results.push((
                                        format!("Project Config ({})", project_label),
                                        combined_result,
                                    ));
                                } else {
                                    renderer.emit_success("Project configuration is valid");
                                }
                            }
                            Err(e) => {
                                renderer.emit_error(&format!(
                                    "Could not parse project config YAML: {}",
                                    e
                                ));
                                has_errors = true;
                            }
                        }
                    }
                    Err(e) => {
                        renderer.emit_error(&format!("Could not read project config file: {}", e));
                        has_errors = true;
                    }
                }
            } else {
                renderer.emit_error(&format!(
                    "Project config file not found: {}",
                    project_config_path.display()
                ));
                has_errors = true;
            }
        }

        // Display results
        for (scope, result) in all_results {
            renderer.emit_info(&format!("{} Validation Results:", scope));

            // Display errors
            for error in &result.errors {
                if errors_only && error.severity != ValidationSeverity::Error {
                    continue;
                }
                renderer.emit_raw_stdout(&format!("{}", error));
            }

            // Display warnings (unless errors_only is set)
            if !errors_only {
                for warning in &result.warnings {
                    renderer.emit_raw_stdout(&format!("{}", warning));
                }

                // Display info messages
                for info in &result.info {
                    renderer.emit_raw_stdout(&format!("{}", info));
                }
            }
        }

        // Handle validation outcome
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
