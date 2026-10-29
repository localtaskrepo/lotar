/// Tasks not changed for at least `threshold` (DEV-3): the shared task query
/// with `-recent:<threshold>`, measured from each task's `modified` time
/// (not git history), oldest change first.
pub(crate) fn run(
    threshold: &str,
    limit: usize,
    global: bool,
    filter: &str,
    project: Option<&str>,
    resolver: &crate::workspace::TasksDirectoryResolver,
    renderer: &crate::output::OutputRenderer,
) -> Result<(), String> {
    let scope_project = super::common::scope_project(global, project, resolver);
    let mut header = super::common::header("stats.stale", global, scope_project.as_deref());
    header.insert("threshold".into(), serde_json::json!(threshold));

    // Same grammar as `recent:` (Nd or Nw), so `stats stale --threshold 30d`
    // lists exactly what `lotar list -recent:30d` does.
    // A zero threshold lists every task.
    let exclude_recent = if matches!(threshold.trim(), "0" | "0d" | "0w") {
        String::new()
    } else {
        let recent = crate::services::task_query::parse_recent(threshold)
            .map_err(|_| format!("Invalid threshold '{threshold}'. Use Nd or Nw, e.g., 21d, 8w"))?;
        format!("-recent:{}d", recent.days)
    };
    let query = format!("{filter} {exclude_recent} sort_by=modified order=asc");
    let tasks = super::common::list_tasks(scope_project.as_deref(), resolver, &query)?;

    let now = chrono::Utc::now();
    let modified = |task: &crate::api_types::TaskDTO| {
        chrono::DateTime::parse_from_rfc3339(&task.modified)
            .map(|at| at.with_timezone(&chrono::Utc))
            .ok()
    };
    let age_days = |task: &crate::api_types::TaskDTO| {
        modified(task)
            .map(|at| (now - at).num_days())
            .unwrap_or_default()
    };
    let limited: Vec<_> = tasks.into_iter().take(limit).collect();

    super::common::emit_items(
        renderer,
        header,
        &limited,
        |(id, task)| {
            serde_json::json!({
                "id": id,
                "project": crate::storage::TaskId::parse(id).map(|parsed| parsed.project).ok(),
                "title": task.title,
                "status": task.status.to_string(),
                "last_date": task.modified,
                "age_days": age_days(task),
            })
        },
        |(id, task)| {
            format!(
                "{:>6}d  {}  {}  {}",
                age_days(task),
                id,
                task.modified,
                task.title
            )
        },
        "No stale tickets over the threshold.",
    );
    Ok(())
}
