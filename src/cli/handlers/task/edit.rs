use crate::cli::TaskEditArgs;
use crate::cli::handlers::CommandHandler;
use crate::cli::handlers::task::context::TaskCommandContext;
use crate::cli::handlers::task::mutation::{LoadedTask, ensure_membership, load_task, plan_auto_populate_members, render_edit_preview};
use crate::cli::validation::CliValidator;
use crate::types::custom_value_string;
use crate::utils::identity::resolve_me_alias_for_project;
use crate::workspace::TasksDirectoryResolver;

/// Handler for editing tasks
pub struct EditHandler;

impl CommandHandler for EditHandler {
    type Args = TaskEditArgs;
    type Result = Result<(), String>;

    fn execute(args: Self::Args, project: Option<&str>, resolver: &TasksDirectoryResolver, renderer: &crate::output::OutputRenderer) -> Self::Result {
        renderer.log_info("edit: begin");
        let Some(select) = args.select.as_deref() else {
            let id = args.id.clone().ok_or("Name the task to edit: a task ID, or --select '<query>'")?;
            let Some((full_id, updated)) = edit_one(&args, &id, project, resolver, renderer)? else {
                return Ok(());
            };
            if matches!(renderer.format, crate::output::OutputFormat::Json) {
                renderer.emit_json(&serde_json::json!({
                    "status": "success",
                    "message": format!("Task '{}' updated successfully", full_id),
                    "task_id": full_id,
                    "task": updated,
                }));
            } else {
                renderer.emit_success(format_args!("Task '{}' updated successfully", full_id));
            }
            return Ok(());
        };

        // `--select` (DEV-394) edits every matching task the same way, as
        // `lotar sprint add --select` and `task archive --select` do.
        // A blank query would select every task (DEV-282).
        if select.trim().is_empty() {
            return Err("--select is empty: give a query, or name the task by ID".to_string());
        }
        let mut ctx = TaskCommandContext::new(resolver, project, args.id.as_deref())?;
        let mut ids: Vec<String> = Vec::new();
        if let Some(id) = args.id.as_deref() {
            ids.push(ctx.resolve_full_task_id(id, project)?);
        }
        let mut search = String::new();
        if let Some(prefix) = project {
            search.push_str(&format!("project={} ", ctx.project_prefix_for(Some(prefix))));
        }
        search.push_str(select);
        ids.extend(crate::services::task_list::select_ids(&ctx.storage, &ctx.config, ctx.storage_root(), &search).map_err(|e| e.cli_message())?);
        drop(ctx);
        let mut seen = std::collections::HashSet::new();
        ids.retain(|id| seen.insert(id.clone()));

        let mut updated = Vec::new();
        let mut failed: Vec<(String, String)> = Vec::new();
        for id in &ids {
            match edit_one(&args, id, project, resolver, renderer) {
                Ok(Some((full_id, _))) => updated.push(full_id),
                Ok(None) => {}
                Err(error) => failed.push((id.clone(), error)),
            }
        }
        match renderer.format {
            crate::output::OutputFormat::Json => renderer.emit_json(&serde_json::json!({
                "status": if failed.is_empty() { "ok" } else { "error" },
                "action": "task.edit",
                "dry_run": args.dry_run,
                "updated": updated,
                "failed": failed.iter().map(|(id, error)| serde_json::json!({"id": id, "error": error})).collect::<Vec<_>>(),
            })),
            _ => {
                if ids.is_empty() {
                    renderer.emit_info("No tasks match the query");
                } else if !args.dry_run {
                    renderer.emit_success(format_args!(
                        "Updated {} task(s){}",
                        updated.len(),
                        if updated.is_empty() { String::new() } else { format!(": {}", updated.join(", ")) }
                    ));
                }
                for (id, error) in &failed {
                    renderer.emit_error(format_args!("{id}: {error}"));
                }
            }
        }
        if failed.is_empty() { Ok(()) } else { Err(format!("{} task(s) could not be edited", failed.len())) }
    }
}

/// Apply the edit to one task. Returns the saved task, or `None` after a
/// dry run, which renders its preview.
fn edit_one(
    args: &TaskEditArgs,
    id: &str,
    project: Option<&str>,
    resolver: &TasksDirectoryResolver,
    renderer: &crate::output::OutputRenderer,
) -> Result<Option<(String, crate::api_types::TaskDTO)>, String> {
    let TaskEditArgs { title, task_type, priority, reporter, assignee, effort, due, description, tags, untag, fields, dry_run, .. } = args;
    let dry_run = *dry_run;
    let mut ctx = TaskCommandContext::new(resolver, project, Some(id))?;
    let LoadedTask { full_id, project_prefix, mut task } = load_task(&mut ctx, id, project)?;

    let autop_members_enabled = ctx.config.auto_populate_members;
    let validator = CliValidator::new(&ctx.config);

    // Build a TaskUpdate from validated args
    let mut patch = crate::api_types::TaskUpdate::default();

    if let Some(title) = title {
        task.title = title.clone();
        patch.title = Some(title.clone());
    }

    if let Some(task_type) = task_type {
        let validated = validator.validate_task_type(task_type).map_err(|e| format!("Task type validation failed: {}", e))?;
        task.task_type = validated.clone();
        patch.task_type = Some(validated.to_string());
    }

    if let Some(priority) = priority {
        let validated = validator.validate_priority(priority).map_err(|e| format!("Priority validation failed: {}", e))?;
        task.priority = validated.clone();
        patch.priority = Some(validated.to_string());
    }

    if let Some(reporter) = reporter {
        let trimmed = reporter.trim();
        if trimmed.is_empty() {
            task.reporter = None;
            patch.reporter = Some(String::new());
        } else {
            let validation = if autop_members_enabled { validator.validate_reporter_allow_unknown(trimmed) } else { validator.validate_reporter(trimmed) };
            let validated = validation.map_err(|e| format!("Reporter validation failed: {}", e))?;
            let resolved = resolve_me_alias_for_project(&validated, Some(ctx.tasks_dir.path.as_path()), Some(&project_prefix));
            task.reporter = resolved.clone();
            patch.reporter = resolved;
        }
    }

    if let Some(assignee) = assignee {
        let trimmed = assignee.trim();
        if trimmed.is_empty() {
            task.assignee = None;
            patch.assignee = Some(String::new());
        } else {
            let validation = if autop_members_enabled { validator.validate_assignee_allow_unknown(trimmed) } else { validator.validate_assignee(trimmed) };
            let validated = validation.map_err(|e| format!("Assignee validation failed: {}", e))?;
            let resolved = resolve_me_alias_for_project(&validated, Some(ctx.tasks_dir.path.as_path()), Some(&project_prefix));
            task.assignee = resolved.clone();
            patch.assignee = resolved;
        }
    }

    // Same validation as `add` and `effort` (DEV-282).
    if let Some(effort) = effort {
        let normalized = crate::utils::effort::parse_effort(effort).map(|parsed| parsed.canonical).map_err(|e| format!("Effort validation failed: {}", e))?;
        task.effort = Some(normalized.clone());
        patch.effort = Some(normalized);
    }

    if let Some(due) = due {
        let value = validator.parse_due_date(due).map_err(|e| format!("Due date validation failed: {}", e))?;
        task.due_date = Some(value.clone());
        patch.due_date = Some(value);
    }

    if let Some(description) = description {
        task.description = Some(description.clone());
        patch.description = Some(description.clone());
    }

    if !tags.is_empty() {
        for tag in tags {
            let tag = validator.validate_tag(tag).map_err(|e| format!("Tag validation failed: {}", e))?;
            if !task.tags.contains(&tag) {
                task.tags.push(tag);
            }
        }
        patch.tags = Some(task.tags.clone());
    }

    // `--untag` removes tags (DEV-283); matching ignores case.
    if !untag.is_empty() {
        task.tags.retain(|tag| !untag.iter().any(|gone| gone.trim().eq_ignore_ascii_case(tag)));
        patch.tags = Some(task.tags.clone());
    }

    if !fields.is_empty() {
        let mut merged = task.custom_fields.clone();
        for (key, value) in fields {
            let validated_key = validator.validate_custom_field_name(key).map_err(|e| format!("Custom field validation failed for '{}': {}", key, e))?;
            // `key=` drops the field, as in `task move` (DEV-394).
            merged.retain(|existing, _| !existing.eq_ignore_ascii_case(&validated_key));
            if !value.trim().is_empty() {
                merged.insert(validated_key, custom_value_string(value.clone()));
            }
        }
        task.custom_fields = merged.clone();
        patch.custom_fields = Some(merged);
    }

    #[expect(clippy::drop_non_drop, reason = "marks where the validator's borrow ends before the configuration is used again")]
    drop(validator);

    plan_auto_populate_members(&mut ctx, &task);
    ensure_membership(&ctx, &task, &project_prefix)?;

    if dry_run {
        render_edit_preview(renderer, id, &task);
        return Ok(None);
    }

    renderer.log_debug("edit: persisting edits via TaskService");
    let updated = crate::services::task_service::TaskService::update(&mut ctx.storage, &full_id, patch).map_err(|e| e.to_string())?;
    Ok(Some((full_id, updated)))
}

#[cfg(test)]
#[path = "../../../../tests/common/dev99_cli_edit_cases.rs"]
mod dev99_cli_edit_cases;
