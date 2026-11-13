use crate::cli::handlers::CommandHandler;
use crate::cli::handlers::task::context::TaskCommandContext;
use crate::cli::handlers::task::mutation::{LoadedTask, load_task};
use crate::cli::handlers::task::render::{
    PropertyCurrent, PropertyNoop, PropertySuccess, render_property_current, render_property_noop, render_property_success,
};
use crate::cli::validation::CliValidator;
use crate::output::OutputRenderer;
use crate::storage::task::Task;
use crate::types::Priority;
use crate::workspace::TasksDirectoryResolver;

/// Handler for priority change commands
pub struct PriorityHandler;

impl CommandHandler for PriorityHandler {
    type Args = PriorityArgs;
    type Result = Result<(), String>;

    fn execute(args: Self::Args, project: Option<&str>, resolver: &TasksDirectoryResolver, renderer: &OutputRenderer) -> Self::Result {
        let PriorityArgs { task_id, new_priority, explicit_project, dry_run, explain } = args;

        let project_hint = explicit_project.as_deref().or(project);

        renderer.log_info(format_args!("priority: begin task_id={} explicit_project={:?}", task_id, project_hint));

        let mut ctx = if new_priority.is_some() && !dry_run {
            TaskCommandContext::new(resolver, project_hint, Some(task_id.as_str()))?
        } else {
            TaskCommandContext::new_read_only(resolver, project_hint, Some(task_id.as_str()))?
        };

        let LoadedTask { full_id, task, .. } = load_task(&mut ctx, &task_id, project_hint)?;

        match new_priority {
            Some(candidate) => handle_set_priority(&candidate, &full_id, &task, &mut ctx, renderer, dry_run, explain),
            None => {
                render_current_priority(renderer, &full_id, &task.priority);
                Ok(())
            }
        }
    }
}

fn handle_set_priority(
    candidate: &str,
    full_id: &str,
    task: &Task,
    ctx: &mut TaskCommandContext,
    renderer: &OutputRenderer,
    dry_run: bool,
    explain: bool,
) -> Result<(), String> {
    let validator = CliValidator::new(&ctx.config);
    renderer.log_debug(format_args!("priority: validating new_priority='{}'", candidate));

    let validated_priority = validator.validate_priority(candidate).map_err(|e| format!("Priority validation failed: {}", e))?;

    let old_priority = task.priority.clone();
    if explain && !matches!(renderer.format, crate::output::OutputFormat::Json) {
        let allowed: Vec<String> = ctx.config.issue_priorities.values.iter().map(|p| p.to_string()).collect();
        renderer.emit_info(format_args!("Explanation: '{}' matched configured priority '{}' (allowed: {})", candidate, validated_priority, allowed.join(", ")));
    }
    if old_priority == validated_priority {
        render_noop_priority(renderer, full_id, &validated_priority);
        return Ok(());
    }
    if dry_run {
        if matches!(renderer.format, crate::output::OutputFormat::Json) {
            renderer.emit_json(&serde_json::json!({
                "status": "preview",
                "action": "priority",
                "task_id": full_id,
                "old_priority": old_priority.to_string(),
                "new_priority": validated_priority.to_string(),
            }));
        } else {
            renderer.emit_info(format_args!("DRY RUN: would change priority of {} from {} to {}", full_id, old_priority, validated_priority));
        }
        return Ok(());
    }

    renderer.log_debug("priority: persisting change via TaskService");
    let patch = crate::api_types::TaskUpdate { priority: Some(validated_priority.to_string()), ..Default::default() };
    crate::services::task_service::TaskService::update(&mut ctx.storage, full_id, patch).map_err(|e| e.to_string())?;
    renderer.log_info("priority: updated successfully");

    render_priority_success(renderer, full_id, &old_priority, &validated_priority);
    Ok(())
}

fn render_current_priority(renderer: &OutputRenderer, task_id: &str, priority: &Priority) {
    let value = priority.to_string();
    let current = PropertyCurrent::new(task_id, "priority", Some(value.clone()), format!("Task {} priority: {}", task_id, value));
    render_property_current(renderer, current);
}

fn render_noop_priority(renderer: &OutputRenderer, task_id: &str, priority: &Priority) {
    let value = priority.to_string();
    let noop = PropertyNoop::new(
        task_id,
        "priority",
        Some(value.clone()),
        format!("Task {} priority unchanged", task_id),
        format!("Task {} priority is already {}", task_id, value),
    );
    render_property_noop(renderer, noop);
}

fn render_priority_success(renderer: &OutputRenderer, task_id: &str, old_priority: &Priority, new_priority: &Priority) {
    let old_value = old_priority.to_string();
    let new_value = new_priority.to_string();
    let message = format!("Task {} priority changed from {} to {}", task_id, old_value, new_value);
    let success = PropertySuccess::new(task_id, "old_priority", "new_priority", Some(old_value.clone()), Some(new_value.clone()), message.clone(), message);
    render_property_success(renderer, success);
}

/// Arguments for priority command
#[derive(Debug, Clone)]
pub struct PriorityArgs {
    pub task_id: String,
    pub new_priority: Option<String>,
    pub explicit_project: Option<String>,
    pub dry_run: bool,
    pub explain: bool,
}

impl PriorityArgs {
    pub fn new(task_id: String, new_priority: Option<String>, explicit_project: Option<String>) -> Self {
        Self { task_id, new_priority, explicit_project, dry_run: false, explain: false }
    }

    /// `-n/--dry-run` and `-e/--explain`, as the sibling commands have.
    pub fn with_preview(mut self, dry_run: bool, explain: bool) -> Self {
        self.dry_run = dry_run;
        self.explain = explain;
        self
    }
}

// inline tests moved to tests/cli_priority_unit_test.rs
