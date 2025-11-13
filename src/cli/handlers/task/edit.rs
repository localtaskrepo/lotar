use crate::cli::TaskEditArgs;
use crate::cli::handlers::CommandHandler;
use crate::cli::handlers::task::context::TaskCommandContext;
use crate::cli::handlers::task::errors::TaskStorageAction;
use crate::cli::handlers::task::mutation::{
    LoadedTask, apply_auto_populate_members, ensure_membership, load_task, render_edit_preview,
};
use crate::cli::validation::CliValidator;
use crate::types::custom_value_string;
use crate::utils::identity::resolve_me_alias;
use crate::workspace::TasksDirectoryResolver;

/// Handler for editing tasks
pub struct EditHandler;

impl CommandHandler for EditHandler {
    type Args = TaskEditArgs;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &crate::output::OutputRenderer,
    ) -> Self::Result {
        renderer.log_info("edit: begin");
        let TaskEditArgs {
            id,
            title,
            task_type,
            priority,
            reporter,
            assignee,
            effort,
            due,
            description,
            tags,
            fields,
            dry_run,
        } = args;

        let mut ctx = TaskCommandContext::new(resolver, project, Some(id.as_str()))?;
        let LoadedTask {
            full_id,
            project_prefix,
            mut task,
        } = load_task(&mut ctx, &id, project)?;

        let autop_members_enabled = ctx.config.auto_populate_members;
        let validator = CliValidator::new(&ctx.config);
        if let Some(title) = title {
            task.title = title;
        }

        if let Some(task_type) = task_type {
            task.task_type = validator
                .validate_task_type(&task_type)
                .map_err(|e| format!("Task type validation failed: {}", e))?;
        }

        if let Some(priority) = priority {
            task.priority = validator
                .validate_priority(&priority)
                .map_err(|e| format!("Priority validation failed: {}", e))?;
        }

        if let Some(reporter) = reporter {
            let trimmed = reporter.trim();
            if trimmed.is_empty() {
                task.reporter = None;
            } else {
                let validation = if autop_members_enabled {
                    validator.validate_reporter_allow_unknown(trimmed)
                } else {
                    validator.validate_reporter(trimmed)
                };
                let validated =
                    validation.map_err(|e| format!("Reporter validation failed: {}", e))?;
                task.reporter = resolve_me_alias(&validated, Some(ctx.tasks_dir.path.as_path()));
            }
        }

        if let Some(assignee) = assignee {
            let trimmed = assignee.trim();
            if trimmed.is_empty() {
                task.assignee = None;
            } else {
                let validation = if autop_members_enabled {
                    validator.validate_assignee_allow_unknown(trimmed)
                } else {
                    validator.validate_assignee(trimmed)
                };
                let validated =
                    validation.map_err(|e| format!("Assignee validation failed: {}", e))?;
                task.assignee = resolve_me_alias(&validated, Some(ctx.tasks_dir.path.as_path()));
            }
        }

        if let Some(effort) = effort {
            task.effort = match crate::utils::effort::parse_effort(&effort) {
                Ok(parsed) => Some(parsed.canonical),
                Err(_) => Some(effort),
            };
        }

        if let Some(due) = due {
            let value = validator
                .parse_due_date(&due)
                .map_err(|e| format!("Due date validation failed: {}", e))?;
            task.due_date = Some(value);
        }

        if let Some(description) = description {
            task.description = Some(description);
        }

        for tag in tags {
            if !task.tags.contains(&tag) {
                task.tags.push(tag);
            }
        }

        for (key, value) in fields {
            task.custom_fields.insert(key, custom_value_string(value));
        }

        #[allow(clippy::drop_non_drop)]
        drop(validator);

        apply_auto_populate_members(&mut ctx, &project_prefix, &task, dry_run)?;
        ensure_membership(&ctx, &task, &project_prefix)?;

        if dry_run {
            render_edit_preview(renderer, &id, &task);
            return Ok(());
        }

        renderer.log_debug("edit: persisting edits");
        ctx.storage
            .edit(&full_id, &task)
            .map_err(TaskStorageAction::Update.map_err(&full_id))?;
        renderer.emit_success(format_args!("Task '{}' updated successfully", id));
        Ok(())
    }
}
