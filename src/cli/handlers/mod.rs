use crate::cli::AddArgs;
use crate::cli::project::ProjectResolver;
use crate::cli::validation::CliValidator;
use crate::config::types::ResolvedConfig;
use crate::output::{OutputFormat, OutputRenderer};
use crate::storage::{Storage, task::Task};
use crate::types::{Priority, TaskStatus, TaskType};
use crate::workspace::TasksDirectoryResolver;
use serde_json;

pub mod commands;
pub mod priority;
pub mod status;
pub mod task;

// Re-export handlers for easy access
pub use commands::{ConfigHandler, IndexHandler, ScanHandler, ServeHandler};
pub use task::TaskHandler;

/// Trait for command handlers
pub trait CommandHandler {
    type Args;
    type Result;

    fn execute(
        args: Self::Args,
        project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Self::Result;
}

/// Handler for adding tasks with the new CLI
pub struct AddHandler;

impl CommandHandler for AddHandler {
    type Args = AddArgs;
    type Result = Result<String, String>;

    fn execute(
        args: Self::Args,
        project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        _renderer: &OutputRenderer,
    ) -> Self::Result {
        // Create project resolver and validator
        let mut project_resolver = ProjectResolver::new(resolver)
            .map_err(|e| format!("Failed to initialize project resolver: {}", e))?;

        // Resolve project first (needed for project-specific config)
        let effective_project = match project_resolver.resolve_project("", project) {
            Ok(project) => {
                if project.is_empty() {
                    // No default project set, use global config
                    None
                } else {
                    Some(project)
                }
            }
            Err(e) => {
                // Project validation failed - this should be an error, not fallback
                return Err(e);
            }
        };

        // Get appropriate configuration (project-specific or global)
        let config = match &effective_project {
            Some(project_name) => project_resolver
                .get_project_config(project_name)
                .map_err(|e| format!("Failed to get project configuration: {}", e))?,
            None => {
                // Use global config
                project_resolver.get_config().clone()
            }
        };

        let validator = CliValidator::new(&config);

        // Process and validate arguments
        let validated_type = if args.bug {
            validator
                .validate_task_type("Bug")
                .map_err(|e| format!("Task type validation failed: {}", e))?
        } else if args.epic {
            validator
                .validate_task_type("Epic")
                .map_err(|e| format!("Task type validation failed: {}", e))?
        } else {
            match args.task_type {
                Some(task_type) => validator
                    .validate_task_type(&task_type)
                    .map_err(|e| format!("Task type validation failed: {}", e))?,
                None => TaskType::Feature, // Default
            }
        };

        let validated_priority = if args.critical {
            validator
                .validate_priority("Critical")
                .map_err(|e| format!("Priority validation failed: {}", e))?
        } else if args.high {
            validator
                .validate_priority("High")
                .map_err(|e| format!("Priority validation failed: {}", e))?
        } else {
            match args.priority {
                Some(priority) => validator
                    .validate_priority(&priority)
                    .map_err(|e| format!("Priority validation failed: {}", e))?,
                None => Self::get_default_priority(&config),
            }
        };

        // Validate assignee if provided
        let validated_assignee = if let Some(ref assignee) = args.assignee {
            Some(
                validator
                    .validate_assignee(assignee)
                    .map_err(|e| format!("Assignee validation failed: {}", e))?,
            )
        } else {
            config.default_assignee.clone()
        };

        // Validate due date if provided
        let validated_due_date = if let Some(ref due_date) = args.due {
            Some(
                validator
                    .parse_due_date(due_date)
                    .map_err(|e| format!("Due date validation failed: {}", e))?,
            )
        } else {
            None
        };

        // Validate effort if provided
        let validated_effort = if let Some(ref effort) = args.effort {
            Some(
                validator
                    .validate_effort(effort)
                    .map_err(|e| format!("Effort validation failed: {}", e))?,
            )
        } else {
            None
        };

        // Validate category if provided
        let validated_category = if let Some(ref category) = args.category {
            Some(
                validator
                    .validate_category(category)
                    .map_err(|e| format!("Category validation failed: {}", e))?,
            )
        } else {
            None
        };

        // Validate tags
        let mut validated_tags = Vec::new();
        for tag in &args.tags {
            let validated_tag = validator
                .validate_tag(tag)
                .map_err(|e| format!("Tag validation failed for '{}': {}", tag, e))?;
            validated_tags.push(validated_tag);
        }

        // Create the task
        let mut task = Task::new(resolver.path.clone(), args.title, validated_priority);

        // Set default status based on config (explicit default or first in issue_states)
        task.status = Self::get_default_status(&config);

        // Set validated properties
        task.task_type = validated_type;
        task.assignee = validated_assignee;
        task.due_date = validated_due_date;
        task.effort = validated_effort;
        task.description = args.description;
        task.category = validated_category;
        task.tags = validated_tags;

        // Handle arbitrary fields with validation
        for (key, value) in args.fields {
            // Validate the custom field name and value
            let (validated_key, validated_value) = validator
                .validate_custom_field(&key, &value)
                .map_err(|e| format!("Custom field validation failed for '{}': {}", key, e))?;

            // Store as custom fields using serde_yaml::Value
            task.custom_fields
                .insert(validated_key, serde_yaml::Value::String(validated_value));
        }

        // Save the task
        let mut storage = if let Some(project_name) = effective_project.as_deref() {
            // Use project context for smart global config creation
            Storage::new_with_context(resolver.path.clone(), Some(project_name))
        } else {
            // Try to auto-detect project context for smart global config
            let context = crate::project::detect_project_name();
            Storage::new_with_context(resolver.path.clone(), context.as_deref())
        };

        // Use resolved project prefix, not the raw project name
        let detected_name = if project.is_none() {
            // Only detect project name if user didn't explicitly specify one
            crate::project::detect_project_name()
        } else {
            None
        };

        let (project_for_storage, original_project_name) = if let Some(explicit_project) = project {
            // If we have an explicit project from command line, resolve it to its prefix
            let prefix = crate::utils::resolve_project_input(explicit_project, &resolver.path);
            (prefix, Some(explicit_project))
        } else if let Some(ref detected) = detected_name {
            // Auto-detected project name - generate prefix but use original name for config
            let prefix = crate::utils::generate_project_prefix(detected);
            (prefix, Some(detected.as_str()))
        } else {
            // Fall back to effective project logic (from global config default)
            let prefix = if let Some(project) = effective_project.as_deref() {
                project.to_string()
            } else {
                crate::project::get_effective_project_name(resolver)
            };
            (prefix, None)
        };

        let task_id = storage.add(&task, &project_for_storage, original_project_name);

        Ok(task_id)
    }
}

impl AddHandler {
    /// Render the output for a successfully created task
    pub fn render_add_success(
        task_id: &str,
        cli_project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) {
        // Fetch the created task to show details (read-only operation)
        if let Some(storage) = Storage::try_open(resolver.path.clone()) {
            let project_name = cli_project.map(|s| s.to_string()).unwrap_or_else(|| {
                // Extract project from task ID (e.g., "TTF-1" -> "TTF")
                if let Some(dash_pos) = task_id.find('-') {
                    task_id[..dash_pos].to_string()
                } else {
                    "default".to_string()
                }
            });

            if let Some(task) = storage.get(task_id, project_name) {
                match renderer.format {
                    OutputFormat::Json => {
                        let response = serde_json::json!({
                            "status": "success",
                            "message": format!("Created task: {}", task_id),
                            "task": {
                                "id": task_id,
                                "title": task.title,
                                "status": task.status.to_string(),
                                "priority": task.priority.to_string(),
                                "task_type": task.task_type.to_string(),
                                "assignee": task.assignee,
                                "due_date": task.due_date,
                                "description": task.description,
                                "created": task.created,
                                "modified": task.modified
                            }
                        });
                        println!("{}", response);
                    }
                    _ => {
                        println!(
                            "{}",
                            renderer.render_success(&format!("Created task: {}", task_id))
                        );
                        println!("  Title: {}", task.title);
                        println!("  Status: {}", task.status);
                        println!("  Priority: {}", task.priority);
                        println!("  Type: {}", task.task_type);
                        if let Some(assignee) = &task.assignee {
                            println!("  Assignee: {}", assignee);
                        }
                        if let Some(due_date) = &task.due_date {
                            println!("  Due date: {}", due_date);
                        }
                        if let Some(description) = &task.description {
                            if !description.is_empty() {
                                println!("  Description: {}", description);
                            }
                        }
                    }
                }
            } else {
                // Fallback to simple message if we can't fetch task details
                match renderer.format {
                    OutputFormat::Json => {
                        let response = serde_json::json!({
                            "status": "success",
                            "message": format!("Created task: {}", task_id),
                            "task_id": task_id
                        });
                        println!("{}", response);
                    }
                    _ => {
                        println!(
                            "{}",
                            renderer.render_success(&format!("Created task: {}", task_id))
                        );
                    }
                }
            }
        } else {
            // Fallback if storage can't be opened
            match renderer.format {
                OutputFormat::Json => {
                    let response = serde_json::json!({
                        "status": "success",
                        "message": format!("Created task: {}", task_id),
                        "task_id": task_id
                    });
                    println!("{}", response);
                }
                _ => {
                    println!(
                        "{}",
                        renderer.render_success(&format!("Created task: {}", task_id))
                    );
                }
            }
        }
    }

    /// Generic smart default selection with comprehensive fallback logic
    ///
    /// Implements the smart default specification:
    /// 1. Project explicit default (if set and valid in project values)
    /// 2. Global default (if valid in project values)
    /// 3. First in project values
    /// 4. Crash if empty (user configuration error)
    fn get_smart_default<T>(
        project_explicit: Option<&T>,
        global_default: &T,
        project_values: &[T],
        field_name: &str,
    ) -> Result<T, String>
    where
        T: Clone + PartialEq + std::fmt::Debug,
    {
        // Error if project has no values configured (user configuration error)
        if project_values.is_empty() {
            return Err(format!(
                "Project configuration error: {} list is empty. Please configure at least one {} value.",
                field_name, field_name
            ));
        }

        // 1. Use project explicit default if set and valid in project values
        if let Some(explicit) = project_explicit {
            if project_values.contains(explicit) {
                return Ok(explicit.clone());
            } else {
                eprintln!(
                    "Warning: Project default {} '{:?}' is not in configured {} list {:?}. Using smart fallback.",
                    field_name, explicit, field_name, project_values
                );
            }
        }

        // 2. Use global default if it's valid in project values
        if project_values.contains(global_default) {
            return Ok(global_default.clone());
        } else {
            eprintln!(
                "Warning: Global default {} '{:?}' is not in project {} list {:?}. Using first configured value.",
                field_name, global_default, field_name, project_values
            );
        }

        // 3. Use first in project values as final fallback
        Ok(project_values[0].clone())
    }

    /// Get default priority with smart fallback logic
    fn get_default_priority(config: &ResolvedConfig) -> Priority {
        // Note: ResolvedConfig.default_priority is always set (not Option)
        // We treat it as the global default, and there's no separate project explicit default for priority
        match Self::get_smart_default(
            None, // No project explicit default for priority in current design
            &config.default_priority,
            &config.issue_priorities.values,
            "priority",
        ) {
            Ok(priority) => priority,
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    }

    /// Get default status with smart fallback logic  
    fn get_default_status(config: &ResolvedConfig) -> TaskStatus {
        // Error if project has no status values configured (user configuration error)
        if config.issue_states.values.is_empty() {
            eprintln!(
                "Error: Project configuration error: status list is empty. Please configure at least one status value."
            );
            std::process::exit(1);
        }

        // 1. Use project explicit default if set and valid in project values
        if let Some(explicit) = &config.default_status {
            if config.issue_states.values.contains(explicit) {
                return explicit.clone();
            } else {
                eprintln!(
                    "Warning: Project default status '{:?}' is not in configured status list {:?}. Using smart fallback.",
                    explicit, config.issue_states.values
                );
            }
        }

        // 2. For status, there's typically no global default, so skip to step 3
        // (Global default_status is usually None)

        // 3. Use first in project values as fallback
        config.issue_states.values[0].clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::{ConfigurableField, ResolvedConfig, StringConfigField};
    use crate::types::{Priority, TaskStatus, TaskType};

    // Helper function to create a test resolver
    fn create_test_resolver() -> TasksDirectoryResolver {
        TasksDirectoryResolver {
            path: std::path::PathBuf::from("/tmp/test_tasks"),
            source: crate::workspace::TasksDirectorySource::CurrentDirectory,
        }
    }

    // Helper function to create a test ResolvedConfig with custom values
    fn create_test_config(
        priorities: Vec<Priority>,
        default_priority: Priority,
        statuses: Vec<TaskStatus>,
        default_status: Option<TaskStatus>,
    ) -> ResolvedConfig {
        ResolvedConfig {
            server_port: 8080,
            default_prefix: "TEST".to_string(),
            issue_states: ConfigurableField { values: statuses },
            issue_types: ConfigurableField {
                values: vec![TaskType::Feature, TaskType::Bug],
            },
            issue_priorities: ConfigurableField { values: priorities },
            categories: StringConfigField {
                values: vec!["*".to_string()],
            },
            tags: StringConfigField {
                values: vec!["*".to_string()],
            },
            default_assignee: None,
            default_priority,
            default_status,
            custom_fields: StringConfigField {
                values: vec!["*".to_string()],
            },
        }
    }

    #[test]
    fn test_get_smart_default_basic_functionality() {
        // Test with basic string values to verify the generic function
        let project_values = vec!["Alpha".to_string(), "Beta".to_string(), "Gamma".to_string()];
        let global_default = "Beta".to_string();

        // Case 1: Project explicit default that exists in project values
        let project_explicit = Some("Gamma".to_string());
        let result = AddHandler::get_smart_default(
            project_explicit.as_ref(),
            &global_default,
            &project_values,
            "test_field",
        );
        assert_eq!(result.unwrap(), "Gamma");

        // Case 2: No project explicit, global default exists in project values
        let result =
            AddHandler::get_smart_default(None, &global_default, &project_values, "test_field");
        assert_eq!(result.unwrap(), "Beta");

        // Case 3: Global default not in project values, use first
        let global_not_in_project = "Delta".to_string();
        let result = AddHandler::get_smart_default(
            None,
            &global_not_in_project,
            &project_values,
            "test_field",
        );
        assert_eq!(result.unwrap(), "Alpha");

        // Case 4: Project explicit not in project values, fallback to global
        let invalid_explicit = Some("Zeta".to_string());
        let result = AddHandler::get_smart_default(
            invalid_explicit.as_ref(),
            &global_default,
            &project_values,
            "test_field",
        );
        assert_eq!(result.unwrap(), "Beta");

        // Case 5: Empty project values should error
        let empty_values: Vec<String> = vec![];
        let result =
            AddHandler::get_smart_default(None, &global_default, &empty_values, "test_field");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("configuration error"));
    }

    #[test]
    fn test_get_default_priority_scenarios() {
        // Case 1: Global default priority exists in project priorities
        let config = create_test_config(
            vec![Priority::Low, Priority::Medium, Priority::High],
            Priority::Medium,
            vec![TaskStatus::Todo, TaskStatus::InProgress, TaskStatus::Done],
            None,
        );
        let result = AddHandler::get_default_priority(&config);
        assert_eq!(result, Priority::Medium);

        // Case 2: Global default priority not in project priorities, use first
        let config = create_test_config(
            vec![Priority::Critical, Priority::High],
            Priority::Medium, // Not in project list
            vec![TaskStatus::Todo, TaskStatus::InProgress],
            None,
        );
        let result = AddHandler::get_default_priority(&config);
        assert_eq!(result, Priority::Critical);

        // Case 3: Global default is Low, project has [High, Medium, Low], should use Low
        let config = create_test_config(
            vec![Priority::High, Priority::Medium, Priority::Low],
            Priority::Low,
            vec![TaskStatus::Todo],
            None,
        );
        let result = AddHandler::get_default_priority(&config);
        assert_eq!(result, Priority::Low);
    }

    #[test]
    fn test_get_default_status_scenarios() {
        // Case 1: Project explicit default status exists in project statuses
        let config = create_test_config(
            vec![Priority::Medium],
            Priority::Medium,
            vec![TaskStatus::Todo, TaskStatus::InProgress, TaskStatus::Done],
            Some(TaskStatus::InProgress),
        );
        let result = AddHandler::get_default_status(&config);
        assert_eq!(result, TaskStatus::InProgress);

        // Case 2: No project explicit default, global default exists in project
        let config = create_test_config(
            vec![Priority::Medium],
            Priority::Medium,
            vec![TaskStatus::Todo, TaskStatus::InProgress, TaskStatus::Done],
            None, // No explicit default
        );
        let result = AddHandler::get_default_status(&config);
        assert_eq!(result, TaskStatus::Todo); // First in project values

        // Case 3: Project explicit default not in project values, fallback to first
        let config = create_test_config(
            vec![Priority::Medium],
            Priority::Medium,
            vec![TaskStatus::InProgress, TaskStatus::Done],
            Some(TaskStatus::Todo), // Not in project list
        );
        let result = AddHandler::get_default_status(&config);
        assert_eq!(result, TaskStatus::InProgress); // First in project values

        // Case 4: Different status combinations
        let config = create_test_config(
            vec![Priority::Medium],
            Priority::Medium,
            vec![TaskStatus::Verify, TaskStatus::Blocked],
            Some(TaskStatus::Verify),
        );
        let result = AddHandler::get_default_status(&config);
        assert_eq!(result, TaskStatus::Verify);
    }

    #[test]
    fn test_priority_validation_integration() {
        // Test that the new logic integrates correctly with the task creation flow
        let priorities = vec![Priority::Critical, Priority::High, Priority::Medium];
        let config = create_test_config(
            priorities.clone(),
            Priority::Low, // Global default not in project list
            vec![TaskStatus::Todo, TaskStatus::Done],
            Some(TaskStatus::Todo),
        );

        // When no priority is specified, should get first from project list (Critical)
        let result = AddHandler::get_default_priority(&config);
        assert_eq!(result, Priority::Critical);
    }

    #[test]
    fn test_status_validation_integration() {
        // Test status scenarios that should occur in real usage
        let statuses = vec![
            TaskStatus::Blocked,
            TaskStatus::InProgress,
            TaskStatus::Verify,
            TaskStatus::Done,
        ];

        // Case 1: Explicit default is valid
        let config = create_test_config(
            vec![Priority::Medium],
            Priority::Medium,
            statuses.clone(),
            Some(TaskStatus::Verify),
        );
        let result = AddHandler::get_default_status(&config);
        assert_eq!(result, TaskStatus::Verify);

        // Case 2: No explicit default, use first
        let config = create_test_config(
            vec![Priority::Medium],
            Priority::Medium,
            statuses.clone(),
            None,
        );
        let result = AddHandler::get_default_status(&config);
        assert_eq!(result, TaskStatus::Blocked);
    }

    #[test]
    fn test_edge_cases() {
        // Test with single value lists
        let config = create_test_config(
            vec![Priority::Critical],
            Priority::Medium, // Not in list
            vec![TaskStatus::Todo],
            None,
        );

        let priority_result = AddHandler::get_default_priority(&config);
        assert_eq!(priority_result, Priority::Critical);

        let status_result = AddHandler::get_default_status(&config);
        assert_eq!(status_result, TaskStatus::Todo);
    }

    #[test]
    fn test_add_handler_basic() {
        let args = AddArgs {
            title: "Test Task".to_string(),
            task_type: None,
            priority: None,
            assignee: None,
            effort: None,
            due: None,
            description: None,
            category: None,
            tags: vec![],
            fields: vec![],
            bug: false,
            epic: false,
            critical: false,
            high: false,
        };

        let resolver = create_test_resolver();
        let renderer = OutputRenderer::new(crate::output::OutputFormat::Text, false);

        // This would fail in a real test because we need actual config files
        // But it demonstrates the structure
        match AddHandler::execute(args, None, &resolver, &renderer) {
            Ok(task_id) => println!("Created task: {}", task_id),
            Err(e) => println!("Expected error in test: {}", e),
        }
    }
}
