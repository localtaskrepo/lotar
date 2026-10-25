pub(crate) fn run(
    since: Option<&str>,
    until: Option<&str>,
    limit: usize,
    global: bool,
    project: Option<&str>,
    resolver: &crate::workspace::TasksDirectoryResolver,
    renderer: &crate::output::OutputRenderer,
) -> Result<(), String> {
    let (since_dt, until_dt) = crate::utils::time::parse_since_until(since, until)?;

    let scope_project = super::common::scope_project(global, project, resolver);

    let header = super::common::window_header(
        "stats.authors",
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

    let items = crate::services::audit_service::AuditService::list_authors_activity(
        repo_root,
        &tasks_rel,
        since_dt,
        until_dt,
        scope_project.as_deref(),
    )?;
    let limited: Vec<_> = items.into_iter().take(limit).collect();

    super::common::emit_items(
        renderer,
        header,
        &limited,
        |i| {
            serde_json::json!({
                "author": i.author,
                "email": i.email,
                "commits": i.commits,
                "last_date": i.last_date.to_rfc3339(),
            })
        },
        |i| {
            format!(
                "{:>4}  {}  <{}>  {}",
                i.commits,
                i.author,
                i.email,
                i.last_date.to_rfc3339()
            )
        },
        "No author activity in the selected window.",
    );
    Ok(())
}
