#[expect(
    clippy::too_many_arguments,
    reason = "one parameter per CLI flag; this handler forwards them unchanged"
)]
pub(crate) fn run_changed(
    since: Option<&str>,
    until: Option<&str>,
    author: Option<&str>,
    limit: usize,
    global: bool,
    filter: &str,
    project: Option<&str>,
    resolver: &crate::workspace::TasksDirectoryResolver,
    renderer: &crate::output::OutputRenderer,
) -> Result<(), String> {
    // Resolve time window
    let (since_dt, until_dt) = crate::utils::time::parse_since_until(since, until)?;

    // Determine scope (project or global)
    let scope_project = super::common::scope_project(global, project, resolver);

    let header = super::common::window_header(
        "stats.changed",
        since_dt,
        until_dt,
        global,
        scope_project.as_deref(),
    );
    let Some(git) = super::common::git_window(resolver, renderer, header.clone())? else {
        return Ok(());
    };
    let repo_root = &git.repo_root;
    let tasks_rel = git.tasks_rel();

    // Call audit service to list changed tasks
    let items = crate::services::audit_service::AuditService::list_changed_tasks(
        repo_root,
        &tasks_rel,
        since_dt,
        until_dt,
        author,
        scope_project.as_deref(),
    )?;

    // Apply limit
    // Only tickets matching `--filter` (DEV-238).
    let keep = super::common::filter_ids(scope_project.as_deref(), resolver, filter)?;
    let limited: Vec<_> = items
        .into_iter()
        .filter(|i| keep.as_ref().is_none_or(|ids| ids.contains(&i.id)))
        .take(limit)
        .collect();

    // Render
    super::common::emit_items(
        renderer,
        header,
        &limited,
        |i| {
            serde_json::json!({
                "id": i.id,
                "project": i.project,
                "file": i.file,
                "last_commit": i.last_commit,
                "last_author": i.last_author,
                "last_date": i.last_date.to_rfc3339(),
                "commits": i.commits,
            })
        },
        |i| {
            format!(
                "{}  {}  {}  {}  {}",
                i.last_date.to_rfc3339(),
                i.id,
                i.project,
                i.last_author,
                i.last_commit
            )
        },
        "No tickets changed in the selected window.",
    );
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "one parameter per CLI flag; this handler forwards them unchanged"
)]
pub(crate) fn run_churn(
    since: Option<&str>,
    until: Option<&str>,
    author: Option<&str>,
    limit: usize,
    global: bool,
    filter: &str,
    project: Option<&str>,
    resolver: &crate::workspace::TasksDirectoryResolver,
    renderer: &crate::output::OutputRenderer,
) -> Result<(), String> {
    let (since_dt, until_dt) = crate::utils::time::parse_since_until(since, until)?;

    let mut scope_project = super::common::scope_project(global, project, resolver);

    let mut header = super::common::window_header(
        "stats.churn",
        since_dt,
        until_dt,
        global,
        scope_project.as_deref(),
    );
    let Some(git) = super::common::git_window(resolver, renderer, header.clone())? else {
        return Ok(());
    };
    let repo_root = &git.repo_root;
    let tasks_rel = git.tasks_rel_or_name();

    // If an inferred project path doesn't exist locally, widen to global to be forgiving in new repos/tests
    if let Some(ref p) = scope_project
        && !git.tasks_abs().join(p).exists()
    {
        scope_project = None;
    }
    header.insert("project".into(), serde_json::json!(scope_project));

    let mut items = crate::services::audit_service::AuditService::list_changed_tasks(
        repo_root,
        &tasks_rel,
        since_dt,
        until_dt,
        author,
        scope_project.as_deref(),
    )?;
    // Sort by commits desc (churn)
    items.sort_by(|a, b| {
        b.commits
            .cmp(&a.commits)
            .then(b.last_date.cmp(&a.last_date))
    });
    // Only tickets matching `--filter` (DEV-238).
    let keep = super::common::filter_ids(scope_project.as_deref(), resolver, filter)?;
    let limited: Vec<_> = items
        .into_iter()
        .filter(|i| keep.as_ref().is_none_or(|ids| ids.contains(&i.id)))
        .take(limit)
        .collect();

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
                "commits": i.commits,
                "last_author": i.last_author,
                "last_commit": i.last_commit,
            })
        },
        |i| {
            format!(
                "{:>4}  {}  {}  {}",
                i.commits,
                i.id,
                i.project,
                i.last_date.to_rfc3339()
            )
        },
        "No churn detected in the selected window.",
    );
    Ok(())
}
