use crate::storage::task::parse_status_from_yaml;

pub(crate) fn run_status(
    id: &str,
    time_in_status: bool,
    since: Option<&str>,
    until: Option<&str>,
    project: Option<&str>,
    resolver: &crate::workspace::TasksDirectoryResolver,
    renderer: &crate::output::OutputRenderer,
) -> Result<(), String> {
    // Only --time-in-status is supported for now
    if !time_in_status {
        return Err("stats status: please pass --time-in-status".to_string());
    }

    // Resolve time window
    let (since_dt, until_dt) = crate::utils::time::parse_since_until(since, until)?;

    // Resolve project + full task id
    let mut project_resolver = crate::cli::project::ProjectResolver::new(resolver)?;
    project_resolver.validate_task_id_format(id)?;
    let final_effective_project =
        project.map(|p| crate::utils::resolve_project_input(p, resolver.path.as_path()));
    let resolved_project =
        project_resolver.resolve_project(id, final_effective_project.as_deref())?;
    let full_task_id = project_resolver.get_full_task_id(id, final_effective_project.as_deref())?;

    let mut header = super::common::window_header(
        "stats.time_in_status",
        since_dt,
        until_dt,
        false,
        Some(&resolved_project),
    );
    // A single task has no global scope.
    header.remove("global");
    let Some(git) = super::common::git_window(resolver, renderer, header)? else {
        return Ok(());
    };
    let repo_root = git.repo_root.clone();

    // Compute path to the single task file relative to repo root
    let tasks_abs = git.tasks_abs().clone();
    let project_folder =
        crate::storage::operations::StorageOperations::get_project_for_task(&full_task_id)
            .ok_or_else(|| format!("Cannot resolve project for '{}'", full_task_id))?;
    let project_path = tasks_abs.join(&project_folder);
    let rel_file = crate::storage::operations::StorageOperations::get_file_path_for_id(
        &project_path,
        &full_task_id,
    )
    .ok_or_else(|| format!("Task '{}' not found", full_task_id))?;
    let file_rel = if rel_file.starts_with(&repo_root) {
        rel_file
            .strip_prefix(&repo_root)
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| rel_file.clone())
    } else {
        rel_file
    };

    let Some(durations) = status_durations(&repo_root, &tasks_abs, &file_rel, since_dt, until_dt)?
    else {
        match renderer.format {
            crate::output::OutputFormat::Json => {
                let obj = serde_json::json!({
                    "status": "ok",
                    "action": "stats.time_in_status",
                    "since": since_dt.to_rfc3339(),
                    "until": until_dt.to_rfc3339(),
                    "project": resolved_project,
                    "count": 0,
                    "items": Vec::<serde_json::Value>::new(),
                });
                renderer.emit_json(&obj);
            }
            _ => renderer.emit_success("No status durations in the selected window."),
        }
        return Ok(());
    };
    let (total_seconds, items) = duration_items(durations);

    match renderer.format {
        crate::output::OutputFormat::Json => {
            let obj = serde_json::json!({
                "status": "ok",
                "action": "stats.time_in_status",
                "since": since_dt.to_rfc3339(),
                "until": until_dt.to_rfc3339(),
                "project": resolved_project,
                "count": if items.is_empty() { 0 } else { 1 },
                "items": if items.is_empty() { vec![] } else { vec![serde_json::json!({
                    "id": full_task_id,
                    "total_seconds": total_seconds,
                    "total_hours": (total_seconds as f64)/3600.0,
                    "items": items
                })] },
            });
            renderer.emit_json(&obj);
        }
        _ => {
            if items.is_empty() {
                renderer.emit_success("No status durations in the selected window.");
            } else {
                renderer.emit_raw_stdout(&full_task_id);
                for it in items {
                    let st = it["status"].as_str().unwrap_or("?");
                    let hrs = it["hours"].as_f64().unwrap_or(0.0);
                    let pct = it["percent"].as_f64().unwrap_or(0.0) * 100.0;
                    renderer
                        .emit_raw_stdout(format_args!("  {:>12}: {:.2}h ({:.1}%)", st, hrs, pct));
                }
                renderer.emit_raw_stdout(format_args!(
                    "  {:>12}: {:.2}h",
                    "Total",
                    (total_seconds as f64) / 3600.0
                ));
            }
        }
    }
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "one parameter per CLI flag; this handler forwards them unchanged"
)]
pub(crate) fn run_time_in_status(
    since: Option<&str>,
    until: Option<&str>,
    limit: usize,
    global: bool,
    filter: &str,
    project: Option<&str>,
    resolver: &crate::workspace::TasksDirectoryResolver,
    renderer: &crate::output::OutputRenderer,
) -> Result<(), String> {
    // Resolve time window
    let (since_dt, until_dt) = crate::utils::time::parse_since_until(since, until)?;

    let scope_project = super::common::scope_project(global, project, resolver);

    let header = super::common::window_header(
        "stats.time_in_status",
        since_dt,
        until_dt,
        global,
        scope_project.as_deref(),
    );
    let Some(git) = super::common::git_window(resolver, renderer, header.clone())? else {
        return Ok(());
    };
    let repo_root = git.repo_root.clone();
    let tasks_rel = git.tasks_rel_or_name();

    // Walk tasks to collect files in-scope
    let mut task_files: Vec<(String, std::path::PathBuf)> = Vec::new();
    let base = repo_root.join(&tasks_rel);
    let walker_root = if let Some(p) = scope_project.as_deref() {
        base.join(p)
    } else {
        base.clone()
    };
    for p in crate::utils::filesystem::list_files_with_ext_recursive(&walker_root, "yml") {
        if let Ok(in_tasks) = p.strip_prefix(&base)
            && let Some(task) = crate::storage::layout::parse_relative(in_tasks)
            && let Ok(rel) = p.strip_prefix(&repo_root)
            // A move stub's past belongs to the task it became (DEV-183),
            // which reports it; counting the stub too would count it twice.
            && crate::storage::moves::stub_target(&p).is_none()
        {
            task_files.push((task.id(), rel.to_path_buf()));
        }
    }

    // Only tickets matching `--filter` (DEV-238).
    let keep = super::common::filter_ids(scope_project.as_deref(), resolver, filter)?;
    task_files.retain(|(id, _)| keep.as_ref().is_none_or(|ids| ids.contains(id)));

    // For each task file, list commits in window and build status timeline
    // Then compute durations per status on [since, until]
    let mut results: Vec<serde_json::Value> = Vec::new();
    for (id, file_rel) in task_files.into_iter() {
        let Some(durations) = status_durations(&repo_root, &base, &file_rel, since_dt, until_dt)?
        else {
            continue;
        };
        // Skip empty
        if durations.is_empty() {
            continue;
        }
        let (total_seconds, items) = duration_items(durations);
        results.push(serde_json::json!({
            "id": id,
            "total_seconds": total_seconds,
            "total_hours": (total_seconds as f64)/3600.0,
            "items": items,
        }));
    }

    // Sort by total seconds desc and limit
    results.sort_by(|a, b| {
        let sum = |v: &serde_json::Value| -> i64 {
            v["items"]
                .as_array()
                .unwrap_or(&Vec::new())
                .iter()
                .map(|x| x["seconds"].as_i64().unwrap_or(0))
                .sum()
        };
        sum(b).cmp(&sum(a))
    });
    let limited: Vec<_> = results.into_iter().take(limit).collect();

    // Render
    match renderer.format {
        crate::output::OutputFormat::Json => {
            let obj = serde_json::json!({
                "status": "ok",
                "action": "stats.time_in_status",
                "since": since_dt.to_rfc3339(),
                "until": until_dt.to_rfc3339(),
                "global": global,
                "project": scope_project,
                "count": limited.len(),
                "items": limited,
            });
            renderer.emit_json(&obj);
        }
        _ => {
            if limited.is_empty() {
                renderer.emit_success("No status durations in the selected window.");
            } else {
                for row in &limited {
                    let id = row["id"].as_str().unwrap_or("-");
                    renderer.emit_raw_stdout(id);
                    if let Some(arr) = row["items"].as_array() {
                        for it in arr {
                            let st = it["status"].as_str().unwrap_or("?");
                            let hrs = it["hours"].as_f64().unwrap_or(0.0);
                            let pct = it["percent"].as_f64().unwrap_or(0.0) * 100.0;
                            renderer.emit_raw_stdout(format_args!(
                                "  {:>12}: {:.2}h ({:.1}%)",
                                st, hrs, pct
                            ));
                        }
                        let tot = row["total_hours"].as_f64().unwrap_or(0.0);
                        renderer.emit_raw_stdout(format_args!("  {:>12}: {:.2}h", "Total", tot));
                    }
                }
            }
        }
    }
    Ok(())
}

type StatusSnapshot = (
    chrono::DateTime<chrono::Utc>,
    Option<crate::types::TaskStatus>,
);

/// Seconds a task file spent in each status within `since..=until`, replayed
/// from its git history. `None` when the file has no commits up to `until`.
fn status_durations(
    repo_root: &std::path::Path,
    tasks_root: &std::path::Path,
    file_rel: &std::path::Path,
    since_dt: chrono::DateTime<chrono::Utc>,
    until_dt: chrono::DateTime<chrono::Utc>,
) -> Result<Option<std::collections::BTreeMap<String, i64>>, String> {
    use crate::services::audit_service::AuditService;

    // A moved task's history includes its time under earlier IDs (DEV-183).
    let commits = AuditService::list_commits_for_task(repo_root, tasks_root, file_rel)?;
    // Unparseable snapshots are kept as `None` so they still advance time.
    let mut snaps: Vec<StatusSnapshot> = commits
        .iter()
        .filter(|c| c.date <= until_dt)
        .filter_map(|c| {
            AuditService::show_file_at(repo_root, &c.commit, &c.path)
                .ok()
                .map(|content| (c.date, parse_status_from_yaml(&content)))
        })
        .collect();
    if snaps.is_empty() {
        return Ok(None);
    }
    snaps.sort_by_key(|snap| snap.0);

    // Status at `since`: the latest known one before it, else the first known.
    let mut current_status = snaps
        .iter()
        .take_while(|(dt, _)| *dt <= since_dt)
        .filter_map(|(_, st)| st.clone())
        .last()
        .or_else(|| snaps.iter().find_map(|(_, st)| st.clone()));

    let mut durations = std::collections::BTreeMap::new();
    let mut add =
        |status: &Option<crate::types::TaskStatus>, from, to: chrono::DateTime<chrono::Utc>| {
            if let Some(status) = status {
                let secs = to.signed_duration_since(from).num_seconds().max(0);
                *durations.entry(status.to_string()).or_insert(0) += secs;
            }
        };
    let mut cursor = since_dt.max(snaps[0].0);
    for (dt, st) in snaps {
        if dt <= since_dt {
            continue;
        }
        if dt > until_dt {
            break;
        }
        add(&current_status, cursor, dt);
        if st.is_some() {
            current_status = st;
        }
        cursor = dt;
    }
    if cursor < until_dt {
        add(&current_status, cursor, until_dt);
    }
    Ok(Some(durations))
}

/// Total seconds and per-status JSON items (`seconds`, `hours`, `percent`).
fn duration_items(
    durations: std::collections::BTreeMap<String, i64>,
) -> (i64, Vec<serde_json::Value>) {
    let total_seconds: i64 = durations.values().sum();
    let items = durations
        .into_iter()
        .map(|(status, seconds)| {
            let percent = if total_seconds > 0 {
                seconds as f64 / total_seconds as f64
            } else {
                0.0
            };
            serde_json::json!({
                "status": status,
                "seconds": seconds,
                "hours": seconds as f64 / 3600.0,
                "percent": percent,
            })
        })
        .collect();
    (total_seconds, items)
}
