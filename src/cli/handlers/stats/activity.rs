#[expect(
    clippy::too_many_arguments,
    reason = "one parameter per CLI flag; this handler forwards them unchanged"
)]
pub(crate) fn run(
    since: Option<&str>,
    until: Option<&str>,
    group_by: &str,
    limit: usize,
    global: bool,
    project: Option<&str>,
    resolver: &crate::workspace::TasksDirectoryResolver,
    renderer: &crate::output::OutputRenderer,
) -> Result<(), String> {
    let (since_dt, until_dt) = crate::utils::time::parse_since_until(since, until)?;

    let scope_project = super::common::scope_project(global, project, resolver);

    let mut header = super::common::window_header(
        "stats.activity",
        since_dt,
        until_dt,
        global,
        scope_project.as_deref(),
    );
    header.insert("group_by".into(), serde_json::json!(group_by));
    let Some(git) = super::common::git_window(resolver, renderer, header.clone())? else {
        return Ok(());
    };
    let repo_root = &git.repo_root;
    let tasks_rel = git.tasks_rel();

    let gb = match group_by {
        "author" => crate::services::audit_service::GroupBy::Author,
        "day" => crate::services::audit_service::GroupBy::Day,
        "week" => crate::services::audit_service::GroupBy::Week,
        "project" => crate::services::audit_service::GroupBy::Project,
        _ => crate::services::audit_service::GroupBy::Day,
    };

    let items = crate::services::audit_service::AuditService::list_activity(
        repo_root,
        &tasks_rel,
        since_dt,
        until_dt,
        gb,
        scope_project.as_deref(),
    )?;
    let limited: Vec<_> = items.into_iter().take(limit).collect();

    super::common::emit_items(
        renderer,
        header,
        &limited,
        |i| {
            serde_json::json!({
                "key": i.key,
                "count": i.count,
                "last_date": i.last_date.to_rfc3339(),
            })
        },
        |i| format!("{:>6}  {}  {}", i.count, i.key, i.last_date.to_rfc3339()),
        "No activity in the selected window.",
    );
    Ok(())
}
