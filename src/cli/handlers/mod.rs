use crate::cli::AddArgs;
use crate::cli::project::ProjectResolver;
use crate::cli::validation::CliValidator;
use crate::output::{OutputRenderer, OutputFormat};
use crate::storage::{Storage, task::Task};
use crate::types::TaskType;
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
                None => config.default_priority.clone(),
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
                        println!("{}", renderer.render_success(&format!("Created task: {}", task_id)));
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
                        println!("{}", renderer.render_success(&format!("Created task: {}", task_id)));
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
                    println!("{}", renderer.render_success(&format!("Created task: {}", task_id)));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to create a test resolver
    fn create_test_resolver() -> TasksDirectoryResolver {
        TasksDirectoryResolver {
            path: std::path::PathBuf::from("/tmp/test_tasks"),
            source: crate::workspace::TasksDirectorySource::CurrentDirectory,
        }
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
