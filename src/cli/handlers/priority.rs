use crate::cli::handlers::CommandHandler;
use crate::cli::handlers::task::context::TaskCommandContext;
use crate::cli::handlers::task::errors::TaskStorageAction;
use crate::cli::handlers::task::mutation::{LoadedTask, load_task};
use crate::cli::handlers::task::render::{
    PropertyCurrent, PropertyNoop, PropertySuccess, render_property_current, render_property_noop,
    render_property_success,
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

    fn execute(
        args: Self::Args,
        project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Self::Result {
        let PriorityArgs {
            task_id,
            new_priority,
            explicit_project,
        } = args;

        let project_hint = explicit_project.as_deref().or(project);

        renderer.log_info(&format!(
            "priority: begin task_id={} explicit_project={:?}",
            task_id, project_hint
        ));

        let mut ctx = if new_priority.is_some() {
            TaskCommandContext::new(resolver, project_hint, Some(task_id.as_str()))?
        } else {
            TaskCommandContext::new_read_only(resolver, project_hint, Some(task_id.as_str()))?
        };

        let LoadedTask { full_id, task, .. } = load_task(&mut ctx, &task_id, project_hint)?;

        match new_priority {
            Some(candidate) => handle_set_priority(candidate, full_id, task, &mut ctx, renderer),
            None => {
                render_current_priority(renderer, &full_id, &task.priority);
                Ok(())
            }
        }
    }
}

fn handle_set_priority(
    candidate: String,
    full_id: String,
    mut task: Task,
    ctx: &mut TaskCommandContext,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let validator = CliValidator::new(&ctx.config);
    renderer.log_debug(&format!(
        "priority: validating new_priority='{}'",
        candidate
    ));

    let validated_priority = validator
        .validate_priority(&candidate)
        .map_err(|e| format!("Priority validation failed: {}", e))?;

    let old_priority = task.priority.clone();
    if old_priority == validated_priority {
        render_noop_priority(renderer, &full_id, &validated_priority);
        return Ok(());
    }

    task.priority = validated_priority.clone();

    renderer.log_debug("priority: persisting change to storage");
    ctx.storage
        .edit(&full_id, &task)
        .map_err(TaskStorageAction::Update.map_err(&full_id))?;
    renderer.log_info("priority: updated successfully");

    render_priority_success(renderer, &full_id, &old_priority, &validated_priority);
    Ok(())
}

fn render_current_priority(renderer: &OutputRenderer, task_id: &str, priority: &Priority) {
    let value = priority.to_string();
    let current = PropertyCurrent::new(
        task_id,
        "priority",
        Some(value.clone()),
        format!("Task {} priority: {}", task_id, value),
    );
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

fn render_priority_success(
    renderer: &OutputRenderer,
    task_id: &str,
    old_priority: &Priority,
    new_priority: &Priority,
) {
    let old_value = old_priority.to_string();
    let new_value = new_priority.to_string();
    let message = format!(
        "Task {} priority changed from {} to {}",
        task_id, old_value, new_value
    );
    let success = PropertySuccess::new(
        task_id,
        "old_priority",
        "new_priority",
        Some(old_value.clone()),
        Some(new_value.clone()),
        message.clone(),
        message,
    );
    render_property_success(renderer, success);
}

/// Arguments for priority command
#[derive(Debug, Clone)]
pub struct PriorityArgs {
    pub task_id: String,
    pub new_priority: Option<String>,
    pub explicit_project: Option<String>,
}

impl PriorityArgs {
    pub fn new(
        task_id: String,
        new_priority: Option<String>,
        explicit_project: Option<String>,
    ) -> Self {
        Self {
            task_id,
            new_priority,
            explicit_project,
        }
    }
}

// inline tests moved to tests/cli_priority_unit_test.rs
