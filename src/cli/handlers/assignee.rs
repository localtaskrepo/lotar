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

/// Handler for assignee get/set
pub struct AssigneeHandler;

pub struct AssigneeArgs {
    pub task_id: String,
    pub new_assignee: Option<String>, // None = get current
    pub explicit_project: Option<String>,
    pub dry_run: bool,
    pub explain: bool,
}

impl AssigneeArgs {
    pub fn new(
        task_id: String,
        new_assignee: Option<String>,
        explicit_project: Option<String>,
        dry_run: bool,
        explain: bool,
    ) -> Self {
        Self {
            task_id,
            new_assignee,
            explicit_project,
            dry_run,
            explain,
        }
    }
}

const ASSIGNEE_DRY_RUN_EXPLANATION: &str =
    "assignee validated against project members; @me resolved during persistence";

impl CommandHandler for AssigneeHandler {
    type Args = AssigneeArgs;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Self::Result {
        let AssigneeArgs {
            task_id,
            new_assignee,
            explicit_project,
            dry_run,
            explain,
        } = args;

        let project_hint = explicit_project.as_deref().or(project);
        renderer.log_info(format_args!(
            "assignee: begin task_id={} explicit_project={:?}",
            task_id, project_hint
        ));

        let mut ctx = if new_assignee.is_some() {
            TaskCommandContext::new(resolver, project_hint, Some(task_id.as_str()))?
        } else {
            TaskCommandContext::new_read_only(resolver, project_hint, Some(task_id.as_str()))?
        };

        let LoadedTask { full_id, task, .. } = load_task(&mut ctx, &task_id, project_hint)?;

        match new_assignee {
            None => {
                render_current_assignee(renderer, &full_id, task.assignee.as_deref(), explain);
                Ok(())
            }
            Some(candidate) => handle_set_assignee(
                candidate, dry_run, explain, full_id, task, &mut ctx, renderer,
            ),
        }
    }
}

fn handle_set_assignee(
    candidate: String,
    dry_run: bool,
    explain: bool,
    full_id: String,
    mut task: crate::storage::task::Task,
    ctx: &mut TaskCommandContext,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let validator = CliValidator::new(&ctx.config);
    let autop_members_enabled = ctx.config.auto_populate_members;
    renderer.log_debug(format_args!(
        "assignee: validating new_assignee='{}'",
        candidate
    ));

    let trimmed = candidate.trim();
    if trimmed.is_empty() {
        return Err("Assignee validation failed: value cannot be empty".to_string());
    }

    let validation = if autop_members_enabled {
        validator.validate_assignee_allow_unknown(trimmed)
    } else {
        validator.validate_assignee(trimmed)
    };
    let validated = validation.map_err(|e| format!("Assignee validation failed: {}", e))?;

    let previous = task.assignee.clone();

    if previous.as_deref() == Some(validated.as_str()) {
        render_assignee_noop(renderer, &full_id, previous.as_deref());
        return Ok(());
    }

    if dry_run {
        render_assignee_dry_run(
            renderer,
            &full_id,
            previous.as_deref(),
            Some(validated.as_str()),
            explain,
        );
        return Ok(());
    }

    task.assignee = Some(validated.clone());

    let patch = TaskUpdate {
        title: None,
        status: None,
        priority: None,
        task_type: None,
        reporter: None,
        assignee: Some(validated),
        due_date: None,
        effort: None,
        description: None,
        tags: None,
        relationships: None,
        custom_fields: None,
        sprints: None,
    };
    let updated =
        TaskService::update(&mut ctx.storage, &full_id, patch).map_err(|e| e.to_string())?;

    render_assignee_success(
        renderer,
        &full_id,
        previous.as_deref(),
        updated.assignee.as_deref(),
    );
    Ok(())
}

fn render_current_assignee(
    renderer: &OutputRenderer,
    task_id: &str,
    assignee: Option<&str>,
    explain: bool,
) {
    let display = assignee.unwrap_or("-");
    let mut current = PropertyCurrent::new(
        task_id,
        "assignee",
        assignee.map(|value| value.to_string()),
        format!("Task {} assignee: {}", task_id, display),
    );
    if explain {
        current = current.with_explain(PropertyExplain::info(
            "When updating, @me resolves using git user.name/email or system username.",
            ExplainPlacement::After,
        ));
    }
    render_property_current(renderer, current);
}

fn render_assignee_noop(renderer: &OutputRenderer, task_id: &str, assignee: Option<&str>) {
    let display = assignee.unwrap_or("-");
    let noop = PropertyNoop::new(
        task_id,
        "assignee",
        assignee.map(|value| value.to_string()),
        format!("Task {} assignee unchanged", task_id),
        format!("Task {} assignee is already {}", task_id, display),
    );
    render_property_noop(renderer, noop);
}

fn render_assignee_dry_run(
    renderer: &OutputRenderer,
    task_id: &str,
    old_assignee: Option<&str>,
    new_assignee: Option<&str>,
    explain: bool,
) {
    let old_display = old_assignee.unwrap_or("-");
    let new_display = new_assignee.unwrap_or("-");
    let mut preview = PropertyPreview::new(
        task_id,
        "assignee_change",
        "old_assignee",
        "new_assignee",
        old_assignee.map(|value| value.to_string()),
        new_assignee.map(|value| value.to_string()),
        format!(
            "DRY RUN: Would change {} assignee from {} to {}",
            task_id, old_display, new_display
        ),
    );
    if explain {
        preview = preview.with_explain(PropertyExplain::info(
            ASSIGNEE_DRY_RUN_EXPLANATION,
            ExplainPlacement::After,
        ));
    }
    render_property_preview(renderer, preview);
}

fn render_assignee_success(
    renderer: &OutputRenderer,
    task_id: &str,
    old_assignee: Option<&str>,
    new_assignee: Option<&str>,
) {
    let old_display = old_assignee.unwrap_or("-");
    let new_display = new_assignee.unwrap_or("-");
    let success = PropertySuccess::new(
        task_id,
        "old_assignee",
        "new_assignee",
        old_assignee.map(|value| value.to_string()),
        new_assignee.map(|value| value.to_string()),
        format!(
            "Task {} assignee changed from {} to {}",
            task_id, old_display, new_display
        ),
        format!(
            "Task {} assignee changed: {} -> {}",
            task_id, old_display, new_display
        ),
    );
    render_property_success(renderer, success);
}
