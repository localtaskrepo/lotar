// Auto-generated from stats_handler.rs.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_changed(
    since: Option<String>,
    until: Option<String>,
    author: Option<String>,
    limit: usize,
    global: bool,
    project: Option<&str>,
    resolver: &crate::workspace::TasksDirectoryResolver,
    renderer: &crate::output::OutputRenderer,
) -> Result<(), String> {
    // Resolve time window
    let (since_dt, until_dt) =
        crate::utils::time::parse_since_until(since.as_deref(), until.as_deref())?;

    // Determine scope (project or global)
    let scope_project = if global {
        None
    } else {
        // Use explicit CLI project if given, otherwise default/effective project name
        project
            .map(|p| crate::utils::resolve_project_input(p, resolver.path.as_path()))
            .or_else(|| Some(crate::project::get_effective_project_name(resolver)))
    };

    // Resolve repo root and tasks dir relative path
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let repo_root = match crate::utils::git::find_repo_root(&cwd) {
        Some(p) => p,
        None => {
            // Outside a git repo: emit empty result
            match renderer.format {
                crate::output::OutputFormat::Json => {
                    let obj = serde_json::json!({
                        "status": "ok",
                        "action": "stats.changed",
                        "since": since_dt.to_rfc3339(),
                        "until": until_dt.to_rfc3339(),
                        "global": global,
                        "project": scope_project,
                        "items": Vec::<serde_json::Value>::new(),
                        "note": "Not in a git repository; returning empty set",
                    });
                    renderer.emit_json(&obj);
                }
                _ => {
                    renderer.emit_warning("Not in a git repository; returning empty set");
                }
            }
            return Ok(());
        }
    };

    // Compute tasks path relative to repo root
    let tasks_abs = resolver.path.clone();
    let tasks_rel = if tasks_abs.starts_with(&repo_root) {
        match tasks_abs.strip_prefix(&repo_root) {
            Ok(p) => p.to_path_buf(),
            Err(_) => tasks_abs.clone(),
        }
    } else {
        tasks_abs.clone()
    };

    // Call audit service to list changed tasks
    let items = crate::services::audit_service::AuditService::list_changed_tasks(
        &repo_root,
        &tasks_rel,
        since_dt,
        until_dt,
        author.as_deref(),
        scope_project.as_deref(),
    )?;

    // Apply limit
    let limited: Vec<_> = items.into_iter().take(limit).collect();

    // Render
    match renderer.format {
        crate::output::OutputFormat::Json => {
            let json_items: Vec<_> = limited
                .iter()
                .map(|i| {
                    serde_json::json!({
                        "id": i.id,
                        "project": i.project,
                        "file": i.file,
                        "last_commit": i.last_commit,
                        "last_author": i.last_author,
                        "last_date": i.last_date.to_rfc3339(),
                        "commits": i.commits,
                    })
                })
                .collect();
            let obj = serde_json::json!({
                "status": "ok",
                "action": "stats.changed",
                "since": since_dt.to_rfc3339(),
                "until": until_dt.to_rfc3339(),
                "global": global,
                "project": scope_project,
                "count": json_items.len(),
                "items": json_items,
            });
            renderer.emit_json(&obj);
        }
        _ => {
            if limited.is_empty() {
                renderer.emit_success("No tickets changed in the selected window.");
            } else {
                for i in &limited {
                    renderer.emit_raw_stdout(&format!(
                        "{}  {}  {}  {}  {}",
                        i.last_date.to_rfc3339(),
                        i.id,
                        i.project,
                        i.last_author,
                        i.last_commit,
                    ));
                }
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_churn(
    since: Option<String>,
    until: Option<String>,
    author: Option<String>,
    limit: usize,
    global: bool,
    project: Option<&str>,
    resolver: &crate::workspace::TasksDirectoryResolver,
    renderer: &crate::output::OutputRenderer,
) -> Result<(), String> {
    let (since_dt, until_dt) =
        crate::utils::time::parse_since_until(since.as_deref(), until.as_deref())?;

    let mut scope_project = if global {
        None
    } else {
        project
            .map(|p| crate::utils::resolve_project_input(p, resolver.path.as_path()))
            .or_else(|| Some(crate::project::get_effective_project_name(resolver)))
    };

    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let repo_root = match crate::utils::git::find_repo_root(&cwd) {
        Some(p) => p,
        None => {
            match renderer.format {
                crate::output::OutputFormat::Json => {
                    let obj = serde_json::json!({
                        "status": "ok",
                        "action": "stats.churn",
                        "since": since_dt.to_rfc3339(),
                        "until": until_dt.to_rfc3339(),
                        "global": global,
                        "project": scope_project,
                        "items": Vec::<serde_json::Value>::new(),
                        "note": "Not in a git repository; returning empty set",
                    });
                    renderer.emit_json(&obj);
                }
                _ => renderer.emit_warning("Not in a git repository; returning empty set"),
            }
            return Ok(());
        }
    };
    let tasks_abs = resolver.path.clone();
    // Canonicalize both to avoid macOS /var vs /private/var mismatches
    let repo_root_real = std::fs::canonicalize(&repo_root).unwrap_or(repo_root.clone());
    let tasks_abs_real = std::fs::canonicalize(&tasks_abs).unwrap_or(tasks_abs.clone());
    let tasks_rel = if tasks_abs_real.starts_with(&repo_root_real) {
        tasks_abs_real
            .strip_prefix(&repo_root_real)
            .unwrap()
            .to_path_buf()
    } else {
        tasks_abs
            .file_name()
            .map(std::path::PathBuf::from)
            .unwrap_or(tasks_abs.clone())
    };

    // If an inferred project path doesn't exist locally, widen to global to be forgiving in new repos/tests
    if let Some(ref p) = scope_project {
        if !tasks_abs.join(p).exists() {
            scope_project = None;
        }
    }

    let mut items = crate::services::audit_service::AuditService::list_changed_tasks(
        &repo_root,
        &tasks_rel,
        since_dt,
        until_dt,
        author.as_deref(),
        scope_project.as_deref(),
    )?;
    // Sort by commits desc (churn)
    items.sort_by(|a, b| {
        b.commits
            .cmp(&a.commits)
            .then(b.last_date.cmp(&a.last_date))
    });
    let limited: Vec<_> = items.into_iter().take(limit).collect();

    match renderer.format {
        crate::output::OutputFormat::Json => {
            let json_items: Vec<_> = limited
                .iter()
                .map(|i| {
                    serde_json::json!({
                        "id": i.id,
                        "project": i.project,
                        "file": i.file,
                        "last_date": i.last_date.to_rfc3339(),
                        "commits": i.commits,
                        "last_author": i.last_author,
                        "last_commit": i.last_commit,
                    })
                })
                .collect();
            let obj = serde_json::json!({
                "status": "ok",
                "action": "stats.churn",
                "since": since_dt.to_rfc3339(),
                "until": until_dt.to_rfc3339(),
                "global": global,
                "project": scope_project,
                "count": json_items.len(),
                "items": json_items,
            });
            renderer.emit_json(&obj);
        }
        _ => {
            if limited.is_empty() {
                renderer.emit_success("No churn detected in the selected window.");
            } else {
                for i in &limited {
                    renderer.emit_raw_stdout(&format!(
                        "{:>4}  {}  {}  {}",
                        i.commits,
                        i.id,
                        i.project,
                        i.last_date.to_rfc3339(),
                    ));
                }
            }
        }
    }
    Ok(())
}
