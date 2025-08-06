use crate::cli::{ConfigAction, ConfigShowArgs, ScanArgs, ServeArgs, IndexArgs, IndexAction};
use crate::cli::handlers::CommandHandler;
use crate::output::OutputRenderer;
use crate::workspace::TasksDirectoryResolver;
use crate::config::ConfigManager;
use crate::types::{Priority, TaskStatus};
use crate::scanner;
use crate::api_server;
use crate::web_server;
use crate::routes;
use crate::project;
use std::path::PathBuf;
use std::fs;
use serde_yaml;

/// Handler for config commands
pub struct ConfigHandler;

impl CommandHandler for ConfigHandler {
    type Args = ConfigAction;
    type Result = Result<(), String>;
    
    fn execute(args: Self::Args, _project: Option<&str>, resolver: &TasksDirectoryResolver, _renderer: &OutputRenderer) -> Self::Result {
        match args {
            ConfigAction::Show(ConfigShowArgs { project }) => {
                Self::handle_config_show(resolver, project)
            }
            ConfigAction::Set(crate::cli::ConfigSetArgs { 
                field, 
                value, 
                dry_run,
                force,
                global 
            }) => {
                Self::handle_config_set(resolver, field, value, dry_run, force, global)
            }
            ConfigAction::Init(crate::cli::ConfigInitArgs { 
                template,
                prefix,
                project,
                copy_from,
                global,
                dry_run,
                force 
            }) => {
                Self::handle_config_init(resolver, template, prefix, project, copy_from, global, dry_run, force)
            }
            ConfigAction::Templates => {
                println!("📚 Available Configuration Templates:");
                println!("  • default - Basic task management setup");
                println!("  • agile - Agile/Scrum workflow configuration");
                println!("  • kanban - Kanban board style setup");
                println!("  • simple - Minimal configuration");
                println!();
                println!("Use 'lotar config init --template=<name>' to initialize with a template.");
                Ok(())
            }
        }
    }
}

impl ConfigHandler {
    /// Handle config show command with optional project filter
    fn handle_config_show(resolver: &TasksDirectoryResolver, project: Option<String>) -> Result<(), String> {
        let config_manager = ConfigManager::new_manager_with_tasks_dir_readonly(&resolver.path)
            .map_err(|e| format!("Failed to load config: {}", e))?;

        if let Some(project_name) = project {
            // Show project-specific config
            let project_prefix = crate::utils::resolve_project_input(&project_name, &resolver.path);
            let project_config = config_manager.get_project_config(&project_prefix)
                .map_err(|e| format!("Failed to load project config: {}", e))?;
                
            println!("Configuration for project: {}", project_name);
            println!();
            
            // Project Settings section (no server settings for project config)
            println!("Project Settings:");
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
            println!("Issue Priorities: {:?}", project_config.issue_priorities.values);
        } else {
            // Show global config
            let resolved_config = config_manager.get_resolved_config();
            
            println!("Global configuration (showing current effective settings):");
            println!();
            println!(
                "Configuration for project: {}",
                if resolved_config.default_prefix.is_empty() {
                    "(none set - will auto-detect on first task creation)"
                } else {
                    &resolved_config.default_prefix
                }
            );
            println!();

            // Server Settings section
            println!("Server Settings:");
            println!("  Port: {}", resolved_config.server_port);
            println!();

            // Project Settings section
            println!("Project Settings:");
            println!("  Tasks directory: {}", resolver.path.display());
            println!("  Task file extension: yml");
            println!("  Default Project: {}", resolved_config.default_prefix);

            if let Some(assignee) = &resolved_config.default_assignee {
                println!("  Default assignee: {}", assignee);
            }
            println!("  Default Priority: {:?}", resolved_config.default_priority);
            
            // Show default status if configured
            if let Some(status) = &resolved_config.default_status {
                println!("  Default Status: {:?}", status);
            }
        }
        
        Ok(())
    }

    /// Handle config set command
    fn handle_config_set(
        resolver: &TasksDirectoryResolver,
        field: String,
        value: String,
        dry_run: bool,
        force: bool,
        mut global: bool,
    ) -> Result<(), String> {
        // Auto-detect global-only fields
        let global_only_fields = vec!["server_port", "default_prefix", "default_project"];
        if global_only_fields.contains(&field.as_str()) && !global {
            global = true;
            if !dry_run {
                println!("ℹ️  Automatically treating '{}' as global configuration field", field);
            }
        }
        
        if dry_run {
            println!("🔍 DRY RUN: Would set {} = {}", field, value);
            
            // Check for validation conflicts
            let conflicts = Self::check_validation_conflicts(resolver, &field, &value, global)?;
            if !conflicts.is_empty() {
                println!("⚠️  WARNING: This change would cause validation conflicts:");
                for conflict in conflicts {
                    println!("  • {}", conflict);
                }
                if !force {
                    println!();
                    println!("Use --force to apply anyway, or fix conflicting values first.");
                    return Ok(());
                }
            }
            
            println!("✅ Dry run completed. Use the same command without --dry-run to apply.");
            return Ok(());
        }
        
        println!("🔧 Setting configuration: {} = {}", field, value);
        
        // Check for validation conflicts unless forced
        if !force {
            let conflicts = Self::check_validation_conflicts(resolver, &field, &value, global)?;
            if !conflicts.is_empty() {
                println!("⚠️  WARNING: This change would cause validation conflicts:");
                for conflict in conflicts {
                    println!("  • {}", conflict);
                }
                println!();
                println!("Use --dry-run to see what would change, or --force to apply anyway.");
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
            let config_manager = ConfigManager::new_manager_with_tasks_dir_ensure_config(&resolver.path)
                .map_err(|e| format!("Failed to load config: {}", e))?;
            let default_prefix = config_manager.get_resolved_config().default_prefix.clone();
            
            if !default_prefix.is_empty() {
                Some(default_prefix)
            } else {
                return Err("No default project set. Use --global flag or set a default project first.".to_string());
            }
        };
        
        // Update the configuration
        ConfigManager::update_config_field(&resolver.path, &field, &value, project_prefix.as_deref())
            .map_err(|e| format!("Failed to update config: {}", e))?;
        
        // Show helpful information about project-specific config
        if project_prefix.is_some() {
            // Check if the value matches the global default and inform the user
            if Self::check_matches_global_default(&field, &value, &resolver.path) {
                println!("ℹ️  Note: This project setting matches the global default. This project will now use this explicit value and won't inherit future global changes to this field.");
            }
        }

        println!("✅ Successfully updated {}", field);
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
    fn handle_config_init(
        resolver: &TasksDirectoryResolver,
        template: String,
        prefix: Option<String>,
        project: Option<String>,
        copy_from: Option<String>,
        global: bool,
        dry_run: bool,
        force: bool,
    ) -> Result<(), String> {
        if dry_run {
            println!("🔍 DRY RUN: Would initialize config with template '{}'", template);
            if let Some(ref prefix) = prefix {
                println!("  • Project prefix: {}", prefix);
                // Validate explicit prefix
                if let Some(ref project_name) = project {
                    if let Err(conflict) = crate::utils::validate_explicit_prefix(prefix, project_name, &resolver.path) {
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
                            println!("  • Generated prefix: {} ❌", crate::utils::generate_project_prefix(project));
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
                    match crate::utils::generate_unique_project_prefix(project_name, &resolver.path) {
                        Ok(prefix) => prefix,
                        Err(_) => crate::utils::generate_project_prefix(project_name), // For display purposes
                    }
                };
                println!("  • Target: Project configuration (.tasks/{}/config.yml)", project_prefix);
            }
            println!("✅ Dry run completed. Use the same command without --dry-run to apply.");
            return Ok(());
        }
        
        println!("🚀 Initializing configuration with template '{}'", template);
        
        // Load template
        let template_config = Self::load_template(&template)?;
        
        // Apply template with customizations
        Self::apply_template_config(resolver, template_config, prefix, project, copy_from, global, force)
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
            conflicts.push("Task PROJ-1 has status 'InProgress' which doesn't match new 'In-Progress'".to_string());
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
    fn apply_template_config(
        resolver: &TasksDirectoryResolver,
        template: serde_yaml::Value,
        prefix: Option<String>,
        project: Option<String>,
        copy_from: Option<String>,
        global: bool,
        force: bool,
    ) -> Result<(), String> {
        // Extract config from template
        let template_map = template.as_mapping()
            .ok_or("Invalid template format")?;
        
        let config_section = template_map.get(&serde_yaml::Value::String("config".to_string()))
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
            resolver.path.join("config.yml")
        } else {
            let project_name = project.as_deref()
                .or_else(|| {
                    config.get("project_name")
                        .and_then(|v| v.as_str())
                })
                .unwrap_or("DEFAULT");
            
            // Generate prefix from project name with conflict detection
            let project_prefix = if let Some(explicit_prefix) = prefix {
                // User provided explicit prefix, validate it doesn't conflict
                crate::utils::validate_explicit_prefix(&explicit_prefix, project_name, &resolver.path)?;
                explicit_prefix
            } else {
                // Generate prefix with conflict detection
                crate::utils::generate_unique_project_prefix(project_name, &resolver.path)?
            };
            
            let project_dir = resolver.path.join(&project_prefix);
            fs::create_dir_all(&project_dir)
                .map_err(|e| format!("Failed to create project directory: {}", e))?;
            project_dir.join("config.yml")
        };
        
        // Check if config already exists
        if config_path.exists() && !force {
            return Err(format!("Configuration already exists at {}. Use --force to overwrite.", config_path.display()));
        }
        
        // Write configuration
        let config_yaml = serde_yaml::to_string(&config)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;
        
        fs::write(&config_path, config_yaml)
            .map_err(|e| format!("Failed to write config file: {}", e))?;
        
        println!("✅ Configuration initialized at: {}", config_path.display());
        Ok(())
    }
    
    /// Merge configuration from another project
    fn merge_config_from_project(
        target_config: &mut serde_yaml::Mapping,
        resolver: &TasksDirectoryResolver,
        source_project: &str,
    ) -> Result<(), String> {
        let source_config_path = resolver.path.join(source_project).join("config.yml");
        
        if !source_config_path.exists() {
            return Err(format!("Source project '{}' does not exist", source_project));
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
        
        println!("📋 Copied settings from project '{}'", source_project);
        Ok(())
    }

    /// Flatten template "values" structure to direct arrays for config fields
    fn flatten_template_values(config_map: &mut serde_yaml::Mapping) {
        let fields_to_flatten = vec![
            "issue_states", "issue_types", "issue_priorities", 
            "categories", "tags", "custom_fields"
        ];
        
        for field_name in fields_to_flatten {
            let field_key = serde_yaml::Value::String(field_name.to_string());
            if let Some(field_value) = config_map.get_mut(&field_key) {
                if let Some(field_map) = field_value.as_mapping() {
                    if let Some(values) = field_map.get(&serde_yaml::Value::String("values".to_string())) {
                        // Replace the nested structure with just the values array
                        *field_value = values.clone();
                    }
                }
            }
        }
    }
}

/// Handler for scan command
pub struct ScanHandler;

impl CommandHandler for ScanHandler {
    type Args = ScanArgs;
    type Result = Result<(), String>;
    
    fn execute(args: Self::Args, _project: Option<&str>, _resolver: &TasksDirectoryResolver, _renderer: &OutputRenderer) -> Self::Result {
        let path = if let Some(scan_path) = args.path {
            PathBuf::from(scan_path)
        } else {
            project::get_project_path().unwrap_or_else(|| {
                println!("No path specified. Using current directory.");
                PathBuf::from(".")
            })
        };

        if !path.exists() {
            return Err(format!("Path '{}' does not exist", path.display()));
        }

        println!("🔍 Scanning {} for TODO comments...", path.display());
        
        let mut scanner = scanner::Scanner::new(path);
        let results = scanner.scan();
        
        if results.is_empty() {
            println!("✅ No TODO comments found.");
        } else {
            println!("📝 Found {} TODO comment(s):", results.len());
            for entry in results {
                if args.detailed {
                    println!("  📄 {}", entry.file_path.display());
                    println!("    Line {}: {}", entry.line_number, entry.title.trim());
                    if !entry.annotation.is_empty() {
                        println!("    Note: {}", entry.annotation);
                    }
                    println!();
                } else {
                    println!("  {}:{} - {}", 
                        entry.file_path.display(), 
                        entry.line_number, 
                        entry.title.trim()
                    );
                }
            }
        }
        
        Ok(())
    }
}

/// Handler for serve command
pub struct ServeHandler;

impl CommandHandler for ServeHandler {
    type Args = ServeArgs;
    type Result = Result<(), String>;
    
    fn execute(args: Self::Args, _project: Option<&str>, _resolver: &TasksDirectoryResolver, _renderer: &OutputRenderer) -> Self::Result {
        let port = args.port.unwrap_or(8080);
        let host = args.host;
        
        println!("🚀 Starting LoTaR web server...");
        println!("   Host: {}", host);
        println!("   Port: {}", port);
        println!("   URL: http://{}:{}", host, port);
        
        if args.open {
            // Open browser automatically
            let url = format!("http://{}:{}", host, port);
            if let Err(e) = open_browser(&url) {
                println!("⚠️  Failed to open browser: {}", e);
                println!("   Please navigate to {} manually", url);
            }
        }
        
        println!("Press Ctrl+C to stop the server");
        
        let mut api_server = api_server::ApiServer::new();
        routes::initialize(&mut api_server);
        web_server::serve(&api_server, port);
        
        Ok(())
    }
}

/// Handler for index commands
pub struct IndexHandler;

impl CommandHandler for IndexHandler {
    type Args = IndexArgs;
    type Result = Result<(), String>;
    
    fn execute(args: Self::Args, _project: Option<&str>, _resolver: &TasksDirectoryResolver, _renderer: &OutputRenderer) -> Self::Result {
        match args.action {
            IndexAction::Rebuild => {
                println!("🔄 Index functionality has been simplified - no rebuild needed");
                println!("✅ All task filtering is now done directly on files");
                Ok(())
            }
        }
    }
}

/// Helper function to open browser (cross-platform)
fn open_browser(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(&["/c", "start", url])
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    
    Ok(())
}
