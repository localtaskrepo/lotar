// Auto-generated from stats_handler.rs.
pub(crate) fn run_status(
    id: String,
    time_in_status: bool,
    since: Option<String>,
    until: Option<String>,
    project: Option<&str>,
    resolver: &crate::workspace::TasksDirectoryResolver,
    renderer: &crate::output::OutputRenderer,
) -> Result<(), String> {
    // Only --time-in-status is supported for now
    if !time_in_status {
        return Err("stats status: please pass --time-in-status".to_string());
    }

    // Resolve time window
    let (since_dt, until_dt) =
        crate::utils::time::parse_since_until(since.as_deref(), until.as_deref())?;

    // Resolve project + full task id
    let mut project_resolver = crate::cli::project::ProjectResolver::new(resolver)?;
    project_resolver.validate_task_id_format(&id)?;
    let final_effective_project =
        project.map(|p| crate::utils::resolve_project_input(p, resolver.path.as_path()));
    let resolved_project =
        project_resolver.resolve_project(&id, final_effective_project.as_deref())?;
    let full_task_id =
        project_resolver.get_full_task_id(&id, final_effective_project.as_deref())?;

    // Repo root and task file path
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let repo_root = match crate::utils::git::find_repo_root(&cwd) {
        Some(p) => p,
        None => {
            match renderer.format {
                crate::output::OutputFormat::Json => {
                    let obj = serde_json::json!({
                        "status": "ok",
                        "action": "stats.time_in_status",
                        "since": since_dt.to_rfc3339(),
                        "until": until_dt.to_rfc3339(),
                        "project": resolved_project,
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

    // Compute path to the single task file relative to repo root
    let tasks_abs = resolver.path.clone();
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

    // Build snapshots from git history up to 'until' and compute durations
    let commits =
        crate::services::audit_service::AuditService::list_commits_for_file(&repo_root, &file_rel)?;
    let mut snaps: Vec<(
        chrono::DateTime<chrono::Utc>,
        String,
        Option<crate::types::TaskStatus>,
    )> = Vec::new();
    // Helper: tolerant status parse from YAML content
    fn parse_status_from_yaml(content: &str) -> Option<crate::types::TaskStatus> {
        fn parse_status_str_tolerant(s: &str) -> Option<crate::types::TaskStatus> {
            let norm = s.to_ascii_lowercase().replace(['_', '-'], "");
            match norm.as_str() {
                "todo" => Some(crate::types::TaskStatus::from("Todo")),
                "inprogress" => Some(crate::types::TaskStatus::from("InProgress")),
                "verify" => Some(crate::types::TaskStatus::from("Verify")),
                "blocked" => Some(crate::types::TaskStatus::from("Blocked")),
                "done" => Some(crate::types::TaskStatus::from("Done")),
                _ => None,
            }
        }
        if let Ok(task) = serde_yaml::from_str::<crate::storage::task::Task>(content) {
            return Some(task.status);
        }
        if let Ok(val) = serde_yaml::from_str::<serde_yaml::Value>(content)
            && let Some(s) = val.get("status").and_then(|v| match v {
                serde_yaml::Value::String(s) => Some(s.clone()),
                _ => None,
            })
        {
            if let Some(ts) = parse_status_str_tolerant(&s) {
                return Some(ts);
            }
            return s.parse::<crate::types::TaskStatus>().ok();
        }
        None
    }
    for c in &commits {
        if c.date > until_dt {
            continue;
        }
        if let Ok(content) = crate::services::audit_service::AuditService::show_file_at(
            &repo_root, &c.commit, &file_rel,
        ) {
            if let Some(ts) = parse_status_from_yaml(&content) {
                snaps.push((c.date, c.commit.clone(), Some(ts)));
            } else {
                snaps.push((c.date, c.commit.clone(), None));
            }
        }
    }
    if snaps.is_empty() {
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
    }
    snaps.sort_by(|a, b| a.0.cmp(&b.0));

    let mut current_status: Option<crate::types::TaskStatus> = None;
    for (dt, _sha, st) in snaps.iter() {
        if *dt <= since_dt {
            if st.is_some() {
                current_status = st.clone();
            }
        } else {
            break;
        }
    }
    if current_status.is_none() {
        current_status = snaps.iter().find_map(|(_, _, s)| s.clone());
    }

    use std::collections::BTreeMap;
    let mut durations: BTreeMap<String, i64> = BTreeMap::new();
    let mut cursor = since_dt.max(snaps.first().map(|s| s.0).unwrap_or(since_dt));
    for (dt, _sha, st) in snaps.into_iter() {
        if dt <= since_dt {
            continue;
        }
        if dt > until_dt {
            break;
        }
        let end = dt;
        if let Some(ref s) = current_status {
            let key = s.to_string();
            let diff = end.signed_duration_since(cursor);
            let secs = diff.num_seconds().max(0);
            *durations.entry(key).or_insert(0) += secs;
        }
        if let Some(s) = st {
            current_status = Some(s);
        }
        cursor = end;
    }
    if cursor < until_dt
        && let Some(ref s) = current_status
    {
        let key = s.to_string();
        let diff = until_dt.signed_duration_since(cursor);
        let secs = diff.num_seconds().max(0);
        *durations.entry(key).or_insert(0) += secs;
    }

    let total_seconds: i64 = durations.values().sum();
    let items: Vec<_> = durations
        .into_iter()
        .map(|(status, seconds)| {
            let hours = (seconds as f64)/3600.0;
            let percent = if total_seconds > 0 { (seconds as f64) / (total_seconds as f64) } else { 0.0 };
            serde_json::json!({ "status": status, "seconds": seconds, "hours": hours, "percent": percent })
        })
        .collect();

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

pub(crate) fn run_time_in_status(
    since: Option<String>,
    until: Option<String>,
    limit: usize,
    global: bool,
    project: Option<&str>,
    resolver: &crate::workspace::TasksDirectoryResolver,
    renderer: &crate::output::OutputRenderer,
) -> Result<(), String> {
    // Resolve time window
    let (since_dt, until_dt) =
        crate::utils::time::parse_since_until(since.as_deref(), until.as_deref())?;

    // Scope
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
                        "action": "stats.time_in_status",
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
    // Canonicalize paths to avoid /var vs /private/var issues
    let tasks_abs = resolver.path.clone();
    let repo_root_real = std::fs::canonicalize(&repo_root).unwrap_or(repo_root.clone());
    let tasks_abs_real = std::fs::canonicalize(&tasks_abs).unwrap_or(tasks_abs.clone());
    let tasks_rel = if tasks_abs_real.starts_with(&repo_root_real) {
        tasks_abs_real
            .strip_prefix(&repo_root_real)
            .unwrap()
            .to_path_buf()
    } else {
        // Fall back to just the tasks directory name to stay repo-relative
        tasks_abs
            .file_name()
            .map(std::path::PathBuf::from)
            .unwrap_or(tasks_abs.clone())
    };

    // Walk tasks to collect files in-scope
    let mut task_files: Vec<(String, std::path::PathBuf)> = Vec::new();
    let base = repo_root.join(&tasks_rel);
    let walker_root = if let Some(p) = scope_project.as_deref() {
        base.join(p)
    } else {
        base.clone()
    };
    if walker_root.exists() {
        let mut stack = vec![walker_root.clone()];
        while let Some(dir) = stack.pop() {
            if let Ok(read) = std::fs::read_dir(&dir) {
                for entry in read.flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        stack.push(p);
                    } else if p.extension().and_then(|e| e.to_str()) == Some("yml")
                        && let Some(stem) = p.file_stem().and_then(|s| s.to_str())
                        && let Ok(num) = stem.parse::<u64>()
                    {
                        let project = p
                            .parent()
                            .and_then(|q| q.file_name())
                            .and_then(|s| s.to_str())
                            .unwrap_or("");
                        let id = format!("{}-{}", project, num);
                        // Compute path relative to repo root for git show
                        if let Ok(rel) = p.strip_prefix(&repo_root) {
                            task_files.push((id, rel.to_path_buf()));
                        }
                    }
                }
            }
        }
    }

    // For each task file, list commits in window and build status timeline
    // Then compute durations per status on [since, until]
    let mut results: Vec<serde_json::Value> = Vec::new();
    for (id, file_rel) in task_files.into_iter() {
        // List all commits for the file, then filter by date window
        let commits = crate::services::audit_service::AuditService::list_commits_for_file(
            &repo_root, &file_rel,
        )?;
        if commits.is_empty() {
            continue;
        }
        // Keep commits whose date intersects the window when building snapshots sequence
        // We will consider consecutive snapshots around the window edges too
        // Build snapshots as (date, commit, status)
        let mut snaps: Vec<(
            chrono::DateTime<chrono::Utc>,
            String,
            Option<crate::types::TaskStatus>,
        )> = Vec::new();
        // Helper: tolerant status parse from YAML content
        fn parse_status_from_yaml(content: &str) -> Option<crate::types::TaskStatus> {
            fn parse_status_str_tolerant(s: &str) -> Option<crate::types::TaskStatus> {
                let norm = s.to_ascii_lowercase().replace(['_', '-'], "");
                match norm.as_str() {
                    "todo" => Some(crate::types::TaskStatus::from("Todo")),
                    "inprogress" => Some(crate::types::TaskStatus::from("InProgress")),
                    "verify" => Some(crate::types::TaskStatus::from("Verify")),
                    "blocked" => Some(crate::types::TaskStatus::from("Blocked")),
                    "done" => Some(crate::types::TaskStatus::from("Done")),
                    _ => None,
                }
            }
            if let Ok(task) = serde_yaml::from_str::<crate::storage::task::Task>(content) {
                return Some(task.status);
            }
            if let Ok(val) = serde_yaml::from_str::<serde_yaml::Value>(content)
                && let Some(s) = val.get("status").and_then(|v| match v {
                    serde_yaml::Value::String(s) => Some(s.clone()),
                    _ => None,
                })
            {
                if let Some(ts) = parse_status_str_tolerant(&s) {
                    return Some(ts);
                }
                return s.parse::<crate::types::TaskStatus>().ok();
            }
            None
        }

        for c in &commits {
            // Only consider up to 'until'
            if c.date > until_dt {
                continue;
            }
            if let Ok(content) = crate::services::audit_service::AuditService::show_file_at(
                &repo_root, &c.commit, &file_rel,
            ) {
                if let Some(ts) = parse_status_from_yaml(&content) {
                    snaps.push((c.date, c.commit.clone(), Some(ts)));
                } else {
                    // tolerate parse failures
                    snaps.push((c.date, c.commit.clone(), None));
                }
            }
        }
        if snaps.is_empty() {
            continue;
        }
        // Sort oldest->newest by date
        snaps.sort_by(|a, b| a.0.cmp(&b.0));

        // Determine starting status at 'since_dt': use latest snapshot before since_dt
        let mut current_status: Option<crate::types::TaskStatus> = None;
        for (dt, _sha, st) in snaps.iter() {
            if *dt <= since_dt {
                if st.is_some() {
                    current_status = st.clone();
                }
            } else {
                break;
            }
        }
        // Fallback: if none before since, use first available
        if current_status.is_none() {
            current_status = snaps.iter().find_map(|(_, _, s)| s.clone());
        }

        // Walk events within (since..=until), accumulating durations per status
        use std::collections::BTreeMap;
        let mut durations: BTreeMap<String, i64> = BTreeMap::new(); // seconds
        let mut cursor = since_dt.max(snaps.first().map(|s| s.0).unwrap_or(since_dt));
        for (dt, _sha, st) in snaps.into_iter() {
            if dt <= since_dt {
                continue;
            }
            if dt > until_dt {
                break;
            }
            let end = dt;
            if let Some(ref s) = current_status {
                let key = s.to_string();
                let diff = end.signed_duration_since(cursor);
                let secs = diff.num_seconds().max(0);
                *durations.entry(key).or_insert(0) += secs;
            }
            if let Some(s) = st {
                current_status = Some(s);
            }
            cursor = end;
        }
        // Tail from last cursor to until
        if cursor < until_dt
            && let Some(s) = current_status
        {
            let key = s.to_string();
            let diff = until_dt.signed_duration_since(cursor);
            let secs = diff.num_seconds().max(0);
            *durations.entry(key).or_insert(0) += secs;
        }

        // Skip empty
        if durations.is_empty() {
            continue;
        }
        let total_seconds: i64 = durations.values().sum();
        let items: Vec<_> = durations
            .into_iter()
            .map(|(status, seconds)| {
                let hours = (seconds as f64) / 3600.0;
                let percent = if total_seconds > 0 {
                    (seconds as f64) / (total_seconds as f64)
                } else {
                    0.0
                };
                serde_json::json!({
                    "status": status,
                    "seconds": seconds,
                    "hours": hours,
                    "percent": percent,
                })
            })
            .collect();
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
