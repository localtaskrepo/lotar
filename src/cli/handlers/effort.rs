use crate::api_types::TaskUpdate;
use crate::cli::handlers::CommandHandler;
use crate::cli::handlers::task::context::TaskCommandContext;
use crate::cli::handlers::task::errors::TaskStorageAction;
use crate::cli::handlers::task::mutation::{LoadedTask, load_task};
use crate::cli::handlers::task::render::{
    ExplainPlacement, PropertyCurrent, PropertyExplain, PropertyNoop, PropertyPreview,
    PropertySuccess, render_property_current, render_property_noop, render_property_preview,
    render_property_success,
};
use crate::output::OutputRenderer;
use crate::services::task_service::TaskService;
use crate::workspace::TasksDirectoryResolver;

/// Handler for effort get/set/clear
pub struct EffortHandler;

pub struct EffortArgs {
    pub task_id: String,
    pub new_effort: Option<String>, // None = get current
    pub clear: bool,
    pub explicit_project: Option<String>,
    pub dry_run: bool,
    pub explain: bool,
}

impl EffortArgs {
    pub fn new(
        task_id: String,
        new_effort: Option<String>,
        clear: bool,
        explicit_project: Option<String>,
        dry_run: bool,
        explain: bool,
    ) -> Self {
        Self {
            task_id,
            new_effort,
            clear,
            explicit_project,
            dry_run,
            explain,
        }
    }
}

const EFFORT_DRY_RUN_EXPLANATION: &str =
    "effort normalized into canonical units; no write performed in dry run";

impl CommandHandler for EffortHandler {
    type Args = EffortArgs;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Self::Result {
        let EffortArgs {
            task_id,
            new_effort,
            clear,
            explicit_project,
            dry_run,
            explain,
        } = args;

        let project_hint = explicit_project.as_deref().or(project);
        renderer.log_info(&format!(
            "effort: begin task_id={} explicit_project={:?}",
            task_id, project_hint
        ));

        let needs_write = new_effort.is_some() || clear;
        let mut ctx = if needs_write {
            TaskCommandContext::new(resolver, project_hint, Some(task_id.as_str()))?
        } else {
            TaskCommandContext::new_read_only(resolver, project_hint, Some(task_id.as_str()))?
        };

        let LoadedTask { full_id, task, .. } = load_task(&mut ctx, &task_id, project_hint)?;

        if new_effort.is_none() && !clear {
            render_current_effort(renderer, &full_id, task.effort.as_deref(), explain);
            return Ok(());
        }

        if clear {
            return handle_clear_effort(dry_run, explain, full_id, task, &mut ctx, renderer);
        }

        if let Some(candidate) = new_effort {
            return handle_set_effort(
                candidate, dry_run, explain, full_id, task, &mut ctx, renderer,
            );
        }

        Err("Invalid effort command state".into())
    }
}

fn handle_clear_effort(
    dry_run: bool,
    explain: bool,
    full_id: String,
    mut task: crate::storage::task::Task,
    ctx: &mut TaskCommandContext,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let previous = task.effort.take();

    if previous.is_none() {
        if dry_run {
            render_effort_preview(
                renderer,
                &full_id,
                None,
                None,
                explain,
                "Task effort already clear; nothing to change",
            );
            return Ok(());
        }
        render_effort_noop(renderer, &full_id, None);
        return Ok(());
    }

    if dry_run {
        render_effort_preview(
            renderer,
            &full_id,
            previous.as_deref(),
            None,
            explain,
            "Would clear task effort",
        );
        return Ok(());
    }

    task.effort = None;
    ctx.storage
        .edit(&full_id, &task)
        .map_err(TaskStorageAction::Update.map_err(&full_id))?;
    render_effort_clear_success(renderer, &full_id, previous.as_deref());
    Ok(())
}

fn handle_set_effort(
    candidate: String,
    dry_run: bool,
    explain: bool,
    full_id: String,
    task: crate::storage::task::Task,
    ctx: &mut TaskCommandContext,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let parsed = crate::utils::effort::parse_effort(&candidate)
        .map_err(|e| format!("Effort validation failed: {}", e))?;
    let normalized = parsed.canonical;

    let previous = task.effort.clone();
    if previous.as_deref() == Some(normalized.as_str()) {
        render_effort_noop(renderer, &full_id, previous.as_deref());
        return Ok(());
    }

    if dry_run {
        render_effort_preview(
            renderer,
            &full_id,
            previous.as_deref(),
            Some(normalized.as_str()),
            explain,
            "Would update task effort",
        );
        return Ok(());
    }

    let patch = TaskUpdate {
        title: None,
        status: None,
        priority: None,
        task_type: None,
        reporter: None,
        assignee: None,
        due_date: None,
        effort: Some(normalized.clone()),
        description: None,
        tags: None,
        relationships: None,
        custom_fields: None,
        sprints: None,
    };
    let updated =
        TaskService::update(&mut ctx.storage, &full_id, patch).map_err(|e| e.to_string())?;

    render_effort_set_success(
        renderer,
        &full_id,
        previous.as_deref(),
        updated.effort.as_deref(),
    );
    Ok(())
}

fn render_current_effort(
    renderer: &OutputRenderer,
    task_id: &str,
    effort: Option<&str>,
    explain: bool,
) {
    let display = effort.unwrap_or("-");
    let mut current = PropertyCurrent::new(
        task_id,
        "effort",
        effort.map(|value| value.to_string()),
        format!("Task {} effort: {}", task_id, display),
    );
    if explain {
        current = current.with_explain(PropertyExplain::info(
            "Effort values are normalized on write using time units (m/h/d/w) or points.",
            ExplainPlacement::Before,
        ));
    }
    render_property_current(renderer, current);
}

fn render_effort_noop(renderer: &OutputRenderer, task_id: &str, effort: Option<&str>) {
    let display = effort.unwrap_or("-");
    let noop = PropertyNoop::new(
        task_id,
        "effort",
        effort.map(|value| value.to_string()),
        format!("Task {} effort unchanged", task_id),
        format!("Task {} effort is already {}", task_id, display),
    );
    render_property_noop(renderer, noop);
}

fn render_effort_preview(
    renderer: &OutputRenderer,
    task_id: &str,
    old_effort: Option<&str>,
    new_effort: Option<&str>,
    explain: bool,
    message: &str,
) {
    let old_display = old_effort.unwrap_or("-");
    let text = if let Some(new) = new_effort {
        format!(
            "DRY RUN: Would change {} effort: {} -> {}",
            task_id, old_display, new
        )
    } else if old_effort.is_some() {
        format!(
            "DRY RUN: Would clear effort for task {} (old: {})",
            task_id, old_display
        )
    } else {
        format!("DRY RUN: {}", message)
    };

    let mut preview = PropertyPreview::new(
        task_id,
        "effort_change",
        "old_effort",
        "new_effort",
        old_effort.map(|value| value.to_string()),
        new_effort.map(|value| value.to_string()),
        text,
    )
    .with_json_message(message.to_string());

    if explain {
        preview = preview.with_explain(PropertyExplain::info(
            EFFORT_DRY_RUN_EXPLANATION,
            ExplainPlacement::After,
        ));
    }

    render_property_preview(renderer, preview);
}

fn render_effort_set_success(
    renderer: &OutputRenderer,
    task_id: &str,
    old_effort: Option<&str>,
    new_effort: Option<&str>,
) {
    let old_display = old_effort.unwrap_or("-");
    let new_display = new_effort.unwrap_or("-");
    let success = PropertySuccess::new(
        task_id,
        "old_effort",
        "new_effort",
        old_effort.map(|value| value.to_string()),
        new_effort.map(|value| value.to_string()),
        format!("Task {} effort updated", task_id),
        format!(
            "Task {} effort changed: {} -> {}",
            task_id, old_display, new_display
        ),
    );
    render_property_success(renderer, success);
}

fn render_effort_clear_success(renderer: &OutputRenderer, task_id: &str, old_effort: Option<&str>) {
    let old_display = old_effort.unwrap_or("-");
    let success = PropertySuccess::new(
        task_id,
        "old_effort",
        "new_effort",
        old_effort.map(|value| value.to_string()),
        None,
        format!("Task {} effort cleared", task_id),
        format!("Task {} effort cleared (was: {})", task_id, old_display),
    );
    render_property_success(renderer, success);
}
