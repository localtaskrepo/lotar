//! `lotar release` (DEV-393): release records and the tickets in them.

use std::io::{self, IsTerminal, Write};
use std::path::Path;

use crate::api_types::{ReleaseAssignmentResponse, ReleaseDeleteResponse, ReleaseGetResponse, ReleaseListItem, ReleaseListResponse, ReleaseMutationResponse};
use crate::cli::args::release::{ReleaseAction, ReleaseArgs, ReleaseAssignArgs, ReleaseBacklogArgs, ReleaseCreateArgs, ReleaseUpdateArgs};
use crate::cli::handlers::{CommandHandler, emit_subcommand_overview};
use crate::output::{OutputFormat, OutputRenderer};
use crate::services::release_service::{ReleaseAssignmentAction, ReleasePatch, ReleaseRecord, ReleaseService};
use crate::storage::manager::Storage;
use crate::storage::release::Release;
use crate::workspace::{TasksDirectoryResolver, TasksDirectorySource};

pub struct ReleaseHandler;

impl CommandHandler for ReleaseHandler {
    type Args = ReleaseArgs;
    type Result = Result<(), String>;

    fn execute(args: Self::Args, _project: Option<&str>, resolver: &TasksDirectoryResolver, renderer: &OutputRenderer) -> Self::Result {
        let root = resolver.path.as_path();
        let Some(action) = args.action else {
            emit_subcommand_overview(renderer, &["release"]);
            return Ok(());
        };
        match action {
            ReleaseAction::Create(args) => create(&args, root, renderer),
            ReleaseAction::List => list(root, renderer),
            ReleaseAction::Show(args) => show(&args.release, root, renderer),
            ReleaseAction::Update(args) => update(&args, root, renderer),
            ReleaseAction::Delete(args) => delete(&args.release, args.force, root, renderer),
            ReleaseAction::Add(args) => assign(&args, ReleaseAssignmentAction::Add, root, renderer),
            ReleaseAction::Move(args) => assign(&args, ReleaseAssignmentAction::Move, root, renderer),
            ReleaseAction::Remove(args) => assign(&args, ReleaseAssignmentAction::Remove, root, renderer),
            ReleaseAction::Backlog(args) => backlog(&args, root, renderer),
            ReleaseAction::Release(args) => {
                let record = ReleaseService::resolve(root, &args.release).map_err(|e| e.to_string())?;
                let (record, changed) = ReleaseService::mark_released(root, record.id, args.at).map_err(|e| e.to_string())?;
                emit_mutation(root, &record, changed, "Released", renderer);
                Ok(())
            }
            ReleaseAction::Reopen(args) => {
                let record = ReleaseService::resolve(root, &args.release).map_err(|e| e.to_string())?;
                let (record, changed) = ReleaseService::reopen(root, record.id).map_err(|e| e.to_string())?;
                emit_mutation(root, &record, changed, "Reopened", renderer);
                Ok(())
            }
        }
    }
}

fn create(args: &ReleaseCreateArgs, root: &Path, renderer: &OutputRenderer) -> Result<(), String> {
    let release = Release {
        version: args.version.clone(),
        label: args.label.clone(),
        notes: args.notes.clone(),
        planned_date: args.planned_date.clone(),
        released_at: args.released_at.clone(),
        ..Release::default()
    };
    let record = ReleaseService::create(root, release).map_err(|e| e.to_string())?;
    emit_mutation(root, &record, true, "Created", renderer);
    Ok(())
}

fn update(args: &ReleaseUpdateArgs, root: &Path, renderer: &OutputRenderer) -> Result<(), String> {
    let patch = ReleasePatch::from_text(args.version.clone(), args.label.clone(), args.notes.clone(), args.planned_date.clone(), args.released_at.clone());
    if patch.is_empty() {
        return Err("Nothing to update: give --version, --label, --notes, --planned or --released-at".to_string());
    }
    let record = ReleaseService::resolve(root, &args.release).map_err(|e| e.to_string())?;
    let (record, changed) = ReleaseService::update(root, record.id, &patch).map_err(|e| e.to_string())?;
    emit_mutation(root, &record, changed, "Updated", renderer);
    Ok(())
}

fn list(root: &Path, renderer: &OutputRenderer) -> Result<(), String> {
    let storage = Storage { root_path: root.to_path_buf() };
    let releases = ReleaseService::list_items(&storage).map_err(|e| e.to_string())?;
    if matches!(renderer.format, OutputFormat::Json) {
        renderer.emit_json(&ReleaseListResponse { status: "ok".to_string(), total: releases.len(), releases });
        return Ok(());
    }
    if releases.is_empty() {
        renderer.emit_success("No releases yet. Create one with `lotar release create <version>`.");
        return Ok(());
    }
    for item in &releases {
        renderer.emit_raw_stdout(summary_line(item));
    }
    Ok(())
}

fn show(reference: &str, root: &Path, renderer: &OutputRenderer) -> Result<(), String> {
    let storage = Storage { root_path: root.to_path_buf() };
    let record = ReleaseService::resolve(root, reference).map_err(|e| e.to_string())?;
    let (item, tasks) = ReleaseService::detail(&storage, &record);
    if matches!(renderer.format, OutputFormat::Json) {
        renderer.emit_json(&ReleaseGetResponse { status: "ok".to_string(), release: item, tasks, history: record.release.history });
        return Ok(());
    }
    renderer.emit_raw_stdout(summary_line(&item));
    if let Some(notes) = item.notes.as_deref() {
        renderer.emit_raw_stdout(format_args!("  Notes: {notes}"));
    }
    if tasks.is_empty() {
        renderer.emit_raw_stdout("  No tickets yet: `lotar release add <release> <task-id>`.");
    }
    for task in &tasks {
        renderer.emit_raw_stdout(format_args!("  {:<10} {:<12} {}", task.id, task.status, task.title));
    }
    Ok(())
}

fn delete(reference: &str, force: bool, root: &Path, renderer: &OutputRenderer) -> Result<(), String> {
    let mut storage = Storage::new(root);
    let record = ReleaseService::resolve(root, reference).map_err(|e| e.to_string())?;
    if !force {
        if matches!(renderer.format, OutputFormat::Json) || !io::stdin().is_terminal() {
            return Err("Release deletion requires --force with JSON output or non-interactive input.".to_string());
        }
        print!("Delete release #{} ({}) and take it off its tickets? [y/N]: ", record.id, record.release.display_name());
        let _ = io::stdout().flush();
        let mut input = String::new();
        io::stdin().read_line(&mut input).map_err(|err| format!("Failed to read confirmation. Aborting: {err}"))?;
        if !matches!(input.trim().to_lowercase().as_str(), "y" | "yes") {
            renderer.emit_warning("Aborted: the release was not deleted.");
            return Ok(());
        }
    }
    let removed_from = ReleaseService::delete(&mut storage, record.id).map_err(|e| e.to_string())?;
    if matches!(renderer.format, OutputFormat::Json) {
        renderer.emit_json(&ReleaseDeleteResponse { status: "ok".to_string(), deleted: true, release_id: record.id, removed_from });
    } else {
        renderer.emit_success(format_args!("Deleted release #{} ({}).", record.id, record.release.display_name()));
        if !removed_from.is_empty() {
            renderer.emit_info(format_args!("Taken off: {}", removed_from.join(", ")));
        }
    }
    Ok(())
}

fn assign(args: &ReleaseAssignArgs, action: ReleaseAssignmentAction, root: &Path, renderer: &OutputRenderer) -> Result<(), String> {
    let Some((reference, ids)) = args.items.split_first() else {
        return Err("Name a release: its ID, version or label.".to_string());
    };
    let mut storage = Storage::new(root);
    let resolver = TasksDirectoryResolver { path: root.to_path_buf(), source: TasksDirectorySource::CommandLineFlag };
    let config = crate::config::resolution::load_and_merge_configs(Some(root)).map_err(|e| format!("Failed to load config: {e}"))?;
    let mut tasks = ids.to_vec();
    tasks.extend(crate::cli::handlers::sprint::select_task_ids(&args.select, &storage, &resolver, &config)?);
    tasks.sort();
    tasks.dedup();
    let outcome = ReleaseService::assign(&mut storage, reference, &tasks, action)?;
    if matches!(renderer.format, OutputFormat::Json) {
        renderer.emit_json(&ReleaseAssignmentResponse {
            status: "ok".to_string(),
            action: action.as_str().to_string(),
            release_id: outcome.record.id,
            release_version: outcome.record.release.version.clone(),
            modified: outcome.modified,
            unchanged: outcome.unchanged,
        });
        return Ok(());
    }
    let name = format!("#{} ({})", outcome.record.id, outcome.record.release.display_name());
    match action {
        ReleaseAssignmentAction::Add => renderer.emit_success(format_args!("Added {} task(s) to release {name}.", outcome.modified.len())),
        ReleaseAssignmentAction::Move => renderer.emit_success(format_args!("Moved {} task(s) to release {name}.", outcome.modified.len())),
        ReleaseAssignmentAction::Remove => renderer.emit_success(format_args!("Removed {} task(s) from release {name}.", outcome.modified.len())),
    }
    if !outcome.unchanged.is_empty() {
        renderer.emit_info(format_args!("Unchanged: {}", outcome.unchanged.join(", ")));
    }
    Ok(())
}

fn backlog(args: &ReleaseBacklogArgs, root: &Path, renderer: &OutputRenderer) -> Result<(), String> {
    if args.limit == 0 {
        return Err("--limit must be greater than zero".to_string());
    }
    let storage = Storage { root_path: root.to_path_buf() };
    let mut input = crate::services::task_list::TaskQueryInput {
        project: args.project.as_deref().map(|project| crate::utils::resolve_project_input(project, root)),
        ..Default::default()
    };
    input.add_search(&crate::utils::search_syntax::parse(&[args.query.join(" "), args.query_terms.join(" ")].join(" "))).map_err(|e| e.cli_message())?;
    let config = crate::config::resolution::load_and_merge_configs(Some(root)).map_err(|e| format!("Failed to load config: {e}"))?;
    let tasks = ReleaseService::backlog(&storage, input, &config).map_err(|e| e.cli_message())?;
    let total = tasks.len();
    let shown: Vec<_> = tasks.into_iter().take(args.limit).map(|(_, task)| task).collect();
    if matches!(renderer.format, OutputFormat::Json) {
        renderer.emit_json(&crate::api_types::ReleaseBacklogResponse {
            status: "ok".to_string(),
            total,
            count: shown.len(),
            limit: args.limit,
            offset: 0,
            tasks: shown,
        });
        return Ok(());
    }
    if shown.is_empty() {
        renderer.emit_success("Every open ticket is in a release.");
        return Ok(());
    }
    for task in &shown {
        renderer.emit_raw_stdout(format_args!("{:<10} {:<12} {}", task.id, task.status, task.title));
    }
    if total > shown.len() {
        renderer.emit_info(format_args!("Showing {} of {total}; raise --limit to see more.", shown.len()));
    }
    Ok(())
}

fn emit_mutation(root: &Path, record: &ReleaseRecord, changed: bool, verb: &str, renderer: &OutputRenderer) {
    let storage = Storage { root_path: root.to_path_buf() };
    let (item, _) = ReleaseService::detail(&storage, record);
    if matches!(renderer.format, OutputFormat::Json) {
        renderer.emit_json(&ReleaseMutationResponse { status: "ok".to_string(), release: item, changed });
    } else if changed {
        renderer.emit_success(format_args!("{verb} release #{} ({}).", item.id, item.display_name));
    } else {
        renderer.emit_info(format_args!("Release #{} ({}) is unchanged.", item.id, item.display_name));
    }
}

fn summary_line(item: &ReleaseListItem) -> String {
    let when = match (item.released_at.as_deref(), item.planned_date.as_deref()) {
        (Some(at), _) => format!("released {}", at.get(..10).unwrap_or(at)),
        (None, Some(date)) if item.overdue => format!("planned {date}, overdue"),
        (None, Some(date)) => format!("planned {date}"),
        (None, None) => "planned".to_string(),
    };
    format!("#{:<3} {:<24} {:<26} {}/{} done", item.id, item.display_name, when, item.done_count, item.task_count)
}
