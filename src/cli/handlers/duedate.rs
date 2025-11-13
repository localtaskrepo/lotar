use crate::api_types::TaskUpdate;
use crate::cli::handlers::CommandHandler;
use crate::cli::handlers::task::context::TaskCommandContext;
use crate::cli::handlers::task::mutation::{LoadedTask, load_task};
use crate::cli::handlers::task::render::{
    ExplainPlacement, PropertyCurrent, PropertyExplain, PropertyNoop, PropertyPreview,
    PropertySuccess, render_property_current, render_property_noop, render_property_preview,
    render_property_success,
};
use crate::cli::validation::CliValidator;
use crate::output::OutputRenderer;
use crate::services::task_service::TaskService;
use crate::workspace::TasksDirectoryResolver;
use serde_json::Value;

/// Handler for due date get/set
pub struct DueDateHandler;

pub struct DueDateArgs {
    pub task_id: String,
    pub new_due_date: Option<String>, // None = get current
    pub explicit_project: Option<String>,
    pub dry_run: bool,
    pub explain: bool,
}

impl DueDateArgs {
    pub fn new(
        task_id: String,
        new_due_date: Option<String>,
        explicit_project: Option<String>,
        dry_run: bool,
        explain: bool,
    ) -> Self {
        Self {
            task_id,
            new_due_date,
            explicit_project,
            dry_run,
            explain,
        }
    }
}

const DUEDATE_DRY_RUN_EXPLANATION: &str =
    "due date validated against project settings; no write performed";

impl CommandHandler for DueDateHandler {
    type Args = DueDateArgs;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Self::Result {
        let DueDateArgs {
            task_id,
            new_due_date,
            explicit_project,
            dry_run,
            explain,
        } = args;

        let project_hint = explicit_project.as_deref().or(project);
        renderer.log_info(format_args!(
            "duedate: begin task_id={} explicit_project={:?}",
            task_id, project_hint
        ));

        let mut ctx = if new_due_date.is_some() {
            TaskCommandContext::new(resolver, project_hint, Some(task_id.as_str()))?
        } else {
            TaskCommandContext::new_read_only(resolver, project_hint, Some(task_id.as_str()))?
        };

        let LoadedTask { full_id, task, .. } = load_task(&mut ctx, &task_id, project_hint)?;

        match new_due_date {
            None => {
                render_current_due_date(renderer, &full_id, task.due_date.as_deref(), explain);
                Ok(())
            }
            Some(candidate) => handle_set_due_date(
                candidate, dry_run, explain, full_id, task, &mut ctx, renderer,
            ),
        }
    }
}

fn handle_set_due_date(
    candidate: String,
    dry_run: bool,
    explain: bool,
    full_id: String,
    mut task: crate::storage::task::Task,
    ctx: &mut TaskCommandContext,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let validator = CliValidator::new(&ctx.config);
    renderer.log_debug(format_args!(
        "duedate: validating new_due_date='{}'",
        candidate
    ));

    let normalized = validator
        .parse_due_date(&candidate)
        .map_err(|e| format!("Due date validation failed: {}", e))?;

    let previous = task.due_date.clone();

    if previous.as_deref() == Some(normalized.as_str()) {
        render_due_date_noop(renderer, &full_id, previous.as_deref());
        return Ok(());
    }

    if dry_run {
        render_due_date_dry_run(
            renderer,
            &full_id,
            previous.as_deref(),
            Some(normalized.as_str()),
            explain,
        );
        return Ok(());
    }

    task.due_date = Some(normalized.clone());

    let patch = TaskUpdate {
        title: None,
        status: None,
        priority: None,
        task_type: None,
        reporter: None,
        assignee: None,
        due_date: Some(normalized),
        effort: None,
        description: None,
        tags: None,
        relationships: None,
        custom_fields: None,
        sprints: None,
    };
    let updated =
        TaskService::update(&mut ctx.storage, &full_id, patch).map_err(|e| e.to_string())?;

    render_due_date_success(
        renderer,
        &full_id,
        previous.as_deref(),
        updated.due_date.as_deref(),
    );
    Ok(())
}

fn render_current_due_date(
    renderer: &OutputRenderer,
    task_id: &str,
    due_date: Option<&str>,
    explain: bool,
) {
    let display = due_date.unwrap_or("-");
    let mut current = PropertyCurrent::new(
        task_id,
        "due_date",
        due_date.map(|value| value.to_string()),
        format!("Task {} due date: {}", task_id, display),
    );
    if explain {
        current = current.with_explain(PropertyExplain::info(
            "Use --dry-run to preview parsing results before persisting a change.",
            ExplainPlacement::After,
        ));
    }
    render_property_current(renderer, current);
}

fn render_due_date_noop(renderer: &OutputRenderer, task_id: &str, due_date: Option<&str>) {
    let display = due_date.unwrap_or("-");
    let mut noop = PropertyNoop::new(
        task_id,
        "due_date",
        due_date.map(|value| value.to_string()),
        format!("Task {} due date unchanged", task_id),
        format!("Task {} due date is already {}", task_id, display),
    );
    let new_value = match due_date {
        Some(value) => Value::String(value.to_string()),
        None => Value::Null,
    };
    noop = noop.with_extra_json("new_due_date", new_value);
    render_property_noop(renderer, noop);
}

fn render_due_date_dry_run(
    renderer: &OutputRenderer,
    task_id: &str,
    old_due_date: Option<&str>,
    new_due_date: Option<&str>,
    explain: bool,
) {
    let old_display = old_due_date.unwrap_or("-");
    let new_display = new_due_date.unwrap_or("-");
    let mut preview = PropertyPreview::new(
        task_id,
        "due_date_change",
        "old_due_date",
        "new_due_date",
        old_due_date.map(|value| value.to_string()),
        new_due_date.map(|value| value.to_string()),
        format!(
            "DRY RUN: Would change {} due date from {} to {}",
            task_id, old_display, new_display
        ),
    );
    if explain {
        preview = preview.with_explain(PropertyExplain::info(
            DUEDATE_DRY_RUN_EXPLANATION,
            ExplainPlacement::After,
        ));
    }
    render_property_preview(renderer, preview);
}

fn render_due_date_success(
    renderer: &OutputRenderer,
    task_id: &str,
    old_due_date: Option<&str>,
    new_due_date: Option<&str>,
) {
    let old_display = old_due_date.unwrap_or("-");
    let new_display = new_due_date.unwrap_or("-");
    let success = PropertySuccess::new(
        task_id,
        "old_due_date",
        "new_due_date",
        old_due_date.map(|value| value.to_string()),
        new_due_date.map(|value| value.to_string()),
        format!(
            "Task {} due date changed from {} to {}",
            task_id, old_display, new_display
        ),
        format!(
            "Task {} due date changed: {} -> {}",
            task_id, old_display, new_display
        ),
    );
    render_property_success(renderer, success);
}
