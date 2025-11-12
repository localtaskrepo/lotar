// Auto-generated from stats_handler.rs.
pub(crate) fn run(
    since: Option<String>,
    until: Option<String>,
    limit: usize,
    global: bool,
    project: Option<&str>,
    resolver: &crate::workspace::TasksDirectoryResolver,
    renderer: &crate::output::OutputRenderer,
) -> Result<(), String> {
    let (since_dt, until_dt) =
        crate::utils::time::parse_since_until(since.as_deref(), until.as_deref())?;

    let scope_project = if global {
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
                        "action": "stats.authors",
                        "since": since_dt.to_rfc3339(),
                        "until": until_dt.to_rfc3339(),
                        "global": global,
                        "project": scope_project,
                        "items": Vec::<serde_json::Value>::new(),
                        "note": "Not in a git repository; returning empty set",
                    });
                    renderer.emit_raw_stdout(&obj.to_string());
                }
                _ => renderer.emit_warning("Not in a git repository; returning empty set"),
            }
            return Ok(());
        }
    };
    let tasks_abs = resolver.path.clone();
    let tasks_rel = if tasks_abs.starts_with(&repo_root) {
        tasks_abs.strip_prefix(&repo_root).unwrap().to_path_buf()
    } else {
        tasks_abs.clone()
    };

    let items = crate::services::audit_service::AuditService::list_authors_activity(
        &repo_root,
        &tasks_rel,
        since_dt,
        until_dt,
        scope_project.as_deref(),
    )?;
    let limited: Vec<_> = items.into_iter().take(limit).collect();

    match renderer.format {
        crate::output::OutputFormat::Json => {
            let json_items: Vec<_> = limited
                .iter()
                .map(|i| {
                    serde_json::json!({
                        "author": i.author,
                        "email": i.email,
                        "commits": i.commits,
                        "last_date": i.last_date.to_rfc3339(),
                    })
                })
                .collect();
            let obj = serde_json::json!({
                "status": "ok",
                "action": "stats.authors",
                "since": since_dt.to_rfc3339(),
                "until": until_dt.to_rfc3339(),
                "global": global,
                "project": scope_project,
                "count": json_items.len(),
                "items": json_items,
            });
            renderer.emit_raw_stdout(&obj.to_string());
        }
        _ => {
            if limited.is_empty() {
                renderer.emit_success("No author activity in the selected window.");
            } else {
                for i in &limited {
                    renderer.emit_raw_stdout(&format!(
                        "{:>4}  {}  <{}>  {}",
                        i.commits,
                        i.author,
                        i.email,
                        i.last_date.to_rfc3339(),
                    ));
                }
            }
        }
    }
    Ok(())
}
