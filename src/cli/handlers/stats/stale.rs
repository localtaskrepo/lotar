pub(crate) fn run(
    threshold: &str,
    limit: usize,
    global: bool,
    project: Option<&str>,
    resolver: &crate::workspace::TasksDirectoryResolver,
    renderer: &crate::output::OutputRenderer,
) -> Result<(), String> {
    // Determine scope
    let scope_project = super::common::scope_project(global, project, resolver);

    let mut header = super::common::header("stats.stale", global, scope_project.as_deref());
    header.insert("threshold".into(), serde_json::json!(threshold));
    let Some(git) = super::common::git_window(resolver, renderer, header.clone())? else {
        return Ok(());
    };
    let repo_root = &git.repo_root;
    let tasks_rel = git.tasks_rel();

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

    let thr = parse_threshold_to_duration(threshold)?;
    let now = chrono::Utc::now();

    // Compute last change per task by scanning full history using existing aggregator
    let since_epoch = chrono::DateTime::<chrono::Utc>::from(std::time::UNIX_EPOCH);
    let until_now = now;
    let mut items = crate::services::audit_service::AuditService::list_changed_tasks(
        repo_root,
        &tasks_rel,
        since_epoch,
        until_now,
        None,
        scope_project.as_deref(),
    )?;

    // Filter by age
    items.retain(|i| (now - i.last_date) >= thr);
    // Sort by age descending (older first)
    items.sort_by_key(|a| a.last_date);
    let limited: Vec<_> = items.into_iter().take(limit).collect();

    super::common::emit_items(
        renderer,
        header,
        &limited,
        |i| {
            serde_json::json!({
                "id": i.id,
                "project": i.project,
                "file": i.file,
                "last_date": i.last_date.to_rfc3339(),
                "age_days": ((now - i.last_date).num_days()),
            })
        },
        |i| {
            format!(
                "{:>6}d  {}  {}  {}",
                (now - i.last_date).num_days(),
                i.id,
                i.project,
                i.last_date.to_rfc3339()
            )
        },
        "No stale tickets over the threshold.",
    );
    Ok(())
}
