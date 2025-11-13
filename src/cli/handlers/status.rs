use crate::cli::handlers::CommandHandler;
use crate::cli::handlers::task::context::TaskCommandContext;
use crate::cli::handlers::task::errors::TaskStorageAction;
use crate::cli::handlers::task::mutation::{LoadedTask, load_task};
use crate::cli::handlers::task::render::{
    ExplainPlacement, PropertyCurrent, PropertyExplain, PropertyNoop, PropertyPreview,
    PropertySuccess, render_property_current, render_property_noop, render_property_preview,
    render_property_success,
};
use crate::cli::validation::CliValidator;
use crate::config::types::ResolvedConfig;
use crate::output::OutputRenderer;
use crate::types::TaskStatus;
use crate::workspace::TasksDirectoryResolver;
use serde_json::Value;

const DRY_RUN_EXPLANATION: &str = "status validated against project config; auto-assign uses CODEOWNERS default when enabled, otherwise default_reporter→git user.name/email→system username.";

pub struct StatusHandler;

pub struct StatusArgs {
    pub task_id: String,
    pub new_status: Option<String>,
    pub explicit_project: Option<String>,
    pub dry_run: bool,
    pub explain: bool,
}

impl StatusArgs {
    pub fn new(
        task_id: String,
        new_status: Option<String>,
        explicit_project: Option<String>,
    ) -> Self {
        Self {
            task_id,
            new_status,
            explicit_project,
            dry_run: false,
            explain: false,
        }
    }
}

impl CommandHandler for StatusHandler {
    type Args = StatusArgs;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Self::Result {
        let StatusArgs {
            task_id,
            new_status,
            explicit_project,
            dry_run,
            explain,
        } = args;

        let project_hint = explicit_project.as_deref().or(project);

        renderer.log_info(format_args!(
            "status: begin task_id={} explicit_project={:?}",
            task_id, project_hint
        ));

        let mut ctx = if new_status.is_some() {
            TaskCommandContext::new(resolver, project_hint, Some(task_id.as_str()))?
        } else {
            TaskCommandContext::new_read_only(resolver, project_hint, Some(task_id.as_str()))?
        };

        let LoadedTask { full_id, task, .. } = load_task(&mut ctx, &task_id, project_hint)?;

        if let Some(candidate) = new_status {
            return handle_set_status(
                candidate, dry_run, explain, full_id, task, &mut ctx, renderer,
            );
        }

        render_current_status(renderer, &full_id, &task.status);
        Ok(())
    }
}

fn handle_set_status(
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
        "status: validating new_status='{}'",
        candidate
    ));
    let validated_status = validator
        .validate_status(&candidate)
        .map_err(|e| format!("Status validation failed: {}", e))?;

    let old_status = task.status.clone();
    if old_status == validated_status {
        renderer.log_info("status: no-op (old == new)");
        render_status_noop(renderer, &full_id, &old_status);
        return Ok(());
    }

    let was_unassigned = task.assignee.is_none();
    let should_assign =
        should_auto_assign(&ctx.config, &old_status, &validated_status, was_unassigned);
    let mut resolved_assignee = if should_assign {
        resolve_auto_assign_candidate(ctx, &ctx.config)
    } else {
        None
    };

    if dry_run {
        render_status_preview(
            renderer,
            &full_id,
            &old_status,
            &validated_status,
            resolved_assignee.as_deref(),
            explain,
        );
        return Ok(());
    }

    task.status = validated_status.clone();
    if let Some(assignee) = resolved_assignee.take() {
        task.assignee = Some(assignee);
    }

    renderer.log_debug("status: persisting change to storage");
    ctx.storage
        .edit(&full_id, &task)
        .map_err(TaskStorageAction::Update.map_err(&full_id))?;
    renderer.log_info("status: updated successfully");

    render_status_success(
        renderer,
        &full_id,
        &old_status,
        &validated_status,
        task.assignee.as_deref(),
    );
    Ok(())
}

fn render_current_status(renderer: &OutputRenderer, task_id: &str, status: &TaskStatus) {
    let message = format!("Task {} status: {}", task_id, status);
    let payload = PropertyCurrent::new(
        task_id,
        "status_value",
        Some(status.to_string()),
        message.clone(),
    );
    render_property_current(renderer, payload);
}

fn render_status_noop(renderer: &OutputRenderer, task_id: &str, status: &TaskStatus) {
    let json_message = format!("Task {} status unchanged", task_id);
    let text_message = format!("Task {} already has status '{}'", task_id, status);
    let payload = PropertyNoop::new(
        task_id,
        "status",
        Some(status.to_string()),
        json_message,
        text_message,
    )
    .with_notice(format!("Task {} status unchanged", task_id));
    render_property_noop(renderer, payload);
}

fn render_status_preview(
    renderer: &OutputRenderer,
    task_id: &str,
    old_status: &TaskStatus,
    new_status: &TaskStatus,
    assignee: Option<&str>,
    explain: bool,
) {
    let mut text_message = format!(
        "DRY RUN: Would change {} status from {} to {}",
        task_id, old_status, new_status
    );
    if let Some(candidate) = assignee {
        text_message.push_str("; would set assignee = ");
        text_message.push_str(candidate);
    }
    let mut payload = PropertyPreview::new(
        task_id,
        "status_change",
        "old_status",
        "new_status",
        Some(old_status.to_string()),
        Some(new_status.to_string()),
        text_message.clone(),
    )
    .with_json_message(text_message.clone());

    if let Some(candidate) = assignee {
        payload =
            payload.with_extra_json("would_set_assignee", Value::String(candidate.to_string()));
    }

    if explain {
        let explain_message = format!("Explanation: {}", DRY_RUN_EXPLANATION);
        payload = payload.with_explain(PropertyExplain::info(
            explain_message,
            ExplainPlacement::After,
        ));
    }

    render_property_preview(renderer, payload);
}

fn render_status_success(
    renderer: &OutputRenderer,
    task_id: &str,
    old_status: &TaskStatus,
    new_status: &TaskStatus,
    assignee: Option<&str>,
) {
    let message = format!(
        "Task {} status changed from {} to {}",
        task_id, old_status, new_status
    );
    let mut payload = PropertySuccess::new(
        task_id,
        "old_status",
        "new_status",
        Some(old_status.to_string()),
        Some(new_status.to_string()),
        message.clone(),
        message,
    );

    if let Some(value) = assignee {
        payload = payload.with_extra_json("assignee", Value::String(value.to_string()));
    }

    render_property_success(renderer, payload);
}

fn should_auto_assign(
    config: &ResolvedConfig,
    old_status: &TaskStatus,
    new_status: &TaskStatus,
    was_unassigned: bool,
) -> bool {
    if !config.auto_assign_on_status || !was_unassigned {
        return false;
    }
    if old_status == new_status {
        return false;
    }
    config
        .effective_default_status()
        .as_ref()
        .map(|default| default == old_status)
        .unwrap_or(false)
}

fn resolve_auto_assign_candidate(
    ctx: &TaskCommandContext,
    config: &ResolvedConfig,
) -> Option<String> {
    if config.auto_codeowners_assign
        && let Some(repo_root) =
            crate::utils::codeowners::repo_root_from_tasks_root(&ctx.tasks_dir.path)
        && let Some(codeowners) = crate::utils::codeowners::CodeOwners::load_from_repo(&repo_root)
        && let Some(owner) = codeowners.default_owner()
    {
        return Some(owner);
    }

    crate::utils::identity::resolve_current_user(Some(ctx.tasks_dir.path.as_path()))
}
