// Auto-generated from stats_handler.rs.
pub(crate) fn run(
    threshold: String,
    limit: usize,
    global: bool,
    project: Option<&str>,
    resolver: &crate::workspace::TasksDirectoryResolver,
    renderer: &crate::output::OutputRenderer,
) -> Result<(), String> {
    // Determine scope
    let scope_project = if global {
        None
    } else {
        project
            .map(|p| crate::utils::resolve_project_input(p, resolver.path.as_path()))
            .or_else(|| Some(crate::project::get_effective_project_name(resolver)))
    };

    // Repo root
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let repo_root = match crate::utils::git::find_repo_root(&cwd) {
        Some(p) => p,
        None => {
            match renderer.format {
                crate::output::OutputFormat::Json => {
                    let obj = serde_json::json!({
                        "status": "ok",
                        "action": "stats.stale",
                        "threshold": threshold,
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

    // tasks dir relative
    let tasks_abs = resolver.path.clone();
    let tasks_rel = if tasks_abs.starts_with(&repo_root) {
        tasks_abs
            .strip_prefix(&repo_root)
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| tasks_abs.clone())
    } else {
        tasks_abs.clone()
    };

    // Parse threshold (Nd or Nw)
    fn parse_threshold_to_duration(s: &str) -> Result<chrono::Duration, String> {
        let t = s.trim().to_lowercase();
        if let Some(num) = t.strip_suffix('d') {
            let n: i64 = num
                .parse()
                .map_err(|_| format!("Invalid threshold: {}", s))?;
            return Ok(chrono::Duration::days(n));
        }
        if let Some(num) = t.strip_suffix('w') {
            let n: i64 = num
                .parse()
                .map_err(|_| format!("Invalid threshold: {}", s))?;
            return Ok(chrono::Duration::weeks(n));
        }
        Err(format!(
            "Invalid threshold '{}'. Use Nd or Nw, e.g., 21d, 8w",
            s
        ))
    }

    let thr = parse_threshold_to_duration(&threshold)?;
    let now = chrono::Utc::now();

    // Compute last change per task by scanning full history using existing aggregator
    let since_epoch = chrono::DateTime::<chrono::Utc>::from(std::time::UNIX_EPOCH);
    let until_now = now;
    let mut items = crate::services::audit_service::AuditService::list_changed_tasks(
        &repo_root,
        &tasks_rel,
        since_epoch,
        until_now,
        None,
        scope_project.as_deref(),
    )?;

    // Filter by age
    items.retain(|i| (now - i.last_date) >= thr);
    // Sort by age descending (older first)
    items.sort_by(|a, b| a.last_date.cmp(&b.last_date));
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
                        "age_days": ((now - i.last_date).num_days()),
                    })
                })
                .collect();
            let obj = serde_json::json!({
                "status": "ok",
                "action": "stats.stale",
                "threshold": threshold,
                "global": global,
                "project": scope_project,
                "count": json_items.len(),
                "items": json_items,
            });
            renderer.emit_json(&obj);
        }
        _ => {
            if limited.is_empty() {
                renderer.emit_success("No stale tickets over the threshold.");
            } else {
                for i in &limited {
                    renderer.emit_raw_stdout(format_args!(
                        "{:>6}d  {}  {}  {}",
                        (now - i.last_date).num_days(),
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
