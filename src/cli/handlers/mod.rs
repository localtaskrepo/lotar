use crate::cli::AddArgs;
use crate::cli::project::ProjectResolver;
use crate::cli::validation::CliValidator;
use crate::config::types::ResolvedConfig;
use crate::output::{LogLevel, OutputFormat, OutputRenderer};
use crate::storage::{manager::Storage, task::Task};
use crate::types::{Priority, TaskStatus, TaskType};
use crate::workspace::TasksDirectoryResolver;
use serde_json;

pub mod config_handler;
pub mod priority;
pub mod scan_handler;
pub mod serve_handler;
pub mod status;
pub mod task;

// Re-export handlers for easy access
pub use config_handler::ConfigHandler;
pub use scan_handler::ScanHandler;
pub use serve_handler::ServeHandler;
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
        renderer: &OutputRenderer,
    ) -> Self::Result {
        renderer.log_info("add: begin validation and project resolution");
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
        renderer.log_debug("add: arguments validated and normalized");

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
            // No default due date
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

        // Validate category if provided; otherwise use default.category if configured
        let validated_category = if let Some(ref category) = args.category {
            Some(
                validator
                    .validate_category(category)
                    .map_err(|e| format!("Category validation failed: {}", e))?,
            )
        } else {
            match &config.default_category {
                Some(default_cat) => Some(
                    validator
                        .validate_category(default_cat)
                        .map_err(|e| format!("Category validation failed: {}", e))?,
                ),
                None => None,
            }
        };

        // Validate tags
        let mut validated_tags = Vec::new();
        let base_tags = if args.tags.is_empty() {
            config.default_tags.clone()
        } else {
            vec![]
        };
        for tag in args.tags.iter().chain(base_tags.iter()) {
            let validated_tag = validator
                .validate_tag(tag)
                .map_err(|e| format!("Tag validation failed for '{}': {}", tag, e))?;
            validated_tags.push(validated_tag);
        }

        // Create the task
        let mut task = Task::new(resolver.path.clone(), args.title, validated_priority);
        renderer.log_debug("add: task object constructed");

        // Set default status based on config (explicit default or first in issue_states)
        task.status = Self::get_default_status(&config);

        // Set validated properties
        task.task_type = validated_type;
        // Resolve @me if present so previews and persisted task show actual identity
        task.assignee = validated_assignee.and_then(|a| {
            crate::utils::identity::resolve_me_alias(&a, Some(resolver.path.as_path()))
        });
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

            // Store as custom fields using feature-aware value constructor
            task.custom_fields.insert(
                validated_key,
                crate::types::custom_value_string(validated_value),
            );
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
            let prefix =
                crate::utils::resolve_project_input(explicit_project, resolver.path.as_path());
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

        if args.dry_run {
            // Preview without saving
            match renderer.format {
                OutputFormat::Json => {
                    let mut obj = serde_json::json!({
                        "status": "preview",
                        "action": "create",
                        "project": project_for_storage,
                        "title": task.title,
                        "priority": task.priority.to_string(),
                        "status_value": task.status.to_string(),
                    });
                    if let Some(a) = &task.assignee {
                        obj["assignee"] = serde_json::Value::String(a.clone());
                    }
                    if let Some(d) = &task.due_date {
                        obj["due_date"] = serde_json::Value::String(d.clone());
                    }
                    if let Some(e) = &task.effort {
                        obj["effort"] = serde_json::Value::String(e.clone());
                    }
                    if let Some(c) = &task.category {
                        obj["category"] = serde_json::Value::String(c.clone());
                    }
                    if !task.tags.is_empty() {
                        obj["tags"] = serde_json::json!(task.tags);
                    }
                    if args.explain {
                        obj["explain"] = serde_json::Value::String("default status and priority via smart defaults; reporter/assignee per config/defaults".to_string());
                    }
                    renderer.emit_raw_stdout(&obj.to_string());
                }
                _ => {
                    renderer.emit_info(&format!(
                        "DRY RUN: Would create task in project {} with title '{}' and priority {}",
                        project_for_storage, task.title, task.priority
                    ));
                    if args.explain {
                        renderer.emit_info("Explanation: default status and priority chosen via smart defaults; reporter/assignee per config/defaults.");
                    }
                }
            }
            return Ok(format!("{}-PREVIEW", project_for_storage));
        }

        renderer.log_info(&format!(
            "add: writing task to storage project={} original={:?}",
            project_for_storage, original_project_name
        ));
        let task_id = storage.add(&task, &project_for_storage, original_project_name);
        renderer.log_info(&format!("add: created id={}", task_id));

        Ok(task_id)
    }
}

impl AddHandler {
    // Internal helper: create a warn logger that auto-silences during tests or when LOTAR_TEST_SILENT=1
    fn warn_logger() -> OutputRenderer {
        let test_silent = cfg!(test)
            || std::env::var("LOTAR_TEST_SILENT").unwrap_or_default() == "1"
            || std::env::var("RUST_TEST_THREADS").is_ok();
        let level = if test_silent {
            LogLevel::Off
        } else {
            LogLevel::Warn
        };
        OutputRenderer::new(OutputFormat::Text, level)
    }
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
                                "reporter": task.reporter,
                                "assignee": task.assignee,
                                "due_date": task.due_date,
                                "description": task.description,
                                "created": task.created,
                                "modified": task.modified
                            }
                        });
                        renderer.emit_raw_stdout(&response.to_string());
                    }
                    _ => {
                        renderer.emit_success(&format!("Created task: {}", task_id));
                        renderer.emit_raw_stdout(&format!("  Title: {}", task.title));
                        renderer.emit_raw_stdout(&format!("  Status: {}", task.status));
                        renderer.emit_raw_stdout(&format!("  Priority: {}", task.priority));
                        renderer.emit_raw_stdout(&format!("  Type: {}", task.task_type));
                        if let Some(reporter) = &task.reporter {
                            renderer.emit_raw_stdout(&format!("  Reporter: {}", reporter));
                        }
                        if let Some(assignee) = &task.assignee {
                            renderer.emit_raw_stdout(&format!("  Assignee: {}", assignee));
                        }
                        if let Some(due_date) = &task.due_date {
                            renderer.emit_raw_stdout(&format!("  Due date: {}", due_date));
                        }
                        if let Some(description) = &task.description {
                            if !description.is_empty() {
                                renderer
                                    .emit_raw_stdout(&format!("  Description: {}", description));
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
                        renderer.emit_raw_stdout(&response.to_string());
                    }
                    _ => {
                        renderer.emit_success(&format!("Created task: {}", task_id));
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
                    renderer.emit_raw_stdout(&response.to_string());
                }
                _ => {
                    renderer.emit_success(&format!("Created task: {}", task_id));
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
                // Emit warning unless silenced for tests
                Self::warn_logger().log_warn(&format!(
                    "Warning: Project default {} '{:?}' is not in configured {} list {:?}. Using smart fallback.",
                    field_name, explicit, field_name, project_values
                ));
            }
        }

        // 2. Use global default if it's valid in project values
        if project_values.contains(global_default) {
            return Ok(global_default.clone());
        } else {
            // Emit warning unless silenced for tests
            Self::warn_logger().log_warn(&format!(
                "Warning: Global default {} '{:?}' is not in project {} list {:?}. Using first configured value.",
                field_name, global_default, field_name, project_values
            ));
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
                OutputRenderer::new(OutputFormat::Text, LogLevel::Error)
                    .log_error(&format!("Error: {}", e));
                std::process::exit(1);
            }
        }
    }

    /// Get default status with smart fallback logic  
    fn get_default_status(config: &ResolvedConfig) -> TaskStatus {
        // Error if project has no status values configured (user configuration error)
        if config.issue_states.values.is_empty() {
            OutputRenderer::new(OutputFormat::Text, LogLevel::Error).log_error(
                "Project configuration error: status list is empty. Please configure at least one status value.",
            );
            std::process::exit(1);
        }

        // 1. Use project explicit default if set and valid in project values
        if let Some(explicit) = &config.default_status {
            if config.issue_states.values.contains(explicit) {
                return explicit.clone();
            } else {
                // Emit warning unless silenced for tests
                Self::warn_logger().log_warn(&format!(
                    "Warning: Project default status '{:?}' is not in configured status list {:?}. Using smart fallback.",
                    explicit, config.issue_states.values
                ));
            }
        }

        // 2. For status, there's typically no global default, so skip to step 3
        // (Global default_status is usually None)

        // 3. Use first in project values as fallback
        config.issue_states.values[0].clone()
    }
}

pub mod test_support {
    use super::*;
    use crate::config::types::ResolvedConfig;
    use crate::types::{Priority, TaskStatus};

    /// Expose smart default for String for integration tests
    pub fn smart_default_string(
        project_explicit: Option<&String>,
        global_default: &String,
        project_values: &[String],
        field_name: &str,
    ) -> Result<String, String> {
        AddHandler::get_smart_default(project_explicit, global_default, project_values, field_name)
    }

    /// Expose default priority selection for integration tests
    pub fn default_priority(config: &ResolvedConfig) -> Priority {
        AddHandler::get_default_priority(config)
    }

    /// Expose default status selection for integration tests
    pub fn default_status(config: &ResolvedConfig) -> TaskStatus {
        AddHandler::get_default_status(config)
    }
}
