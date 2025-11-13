// Auto-generated from stats_handler.rs.
pub(crate) fn run_top(
    limit: usize,
    global: bool,
    project: Option<&str>,
    resolver: &crate::workspace::TasksDirectoryResolver,
    renderer: &crate::output::OutputRenderer,
) -> Result<(), String> {
    let scope_project = if global {
        None
    } else {
        project
            .map(|p| crate::utils::resolve_project_input(p, resolver.path.as_path()))
            .or_else(|| Some(crate::project::get_effective_project_name(resolver)))
    };
    let storage = crate::storage::manager::Storage::new(resolver.path.clone());
    let filter = crate::api_types::TaskListFilter {
        status: Vec::new(),
        priority: Vec::new(),
        task_type: Vec::new(),
        project: scope_project.clone(),
        tags: Vec::new(),
        text_query: None,
        sprints: Vec::new(),
    };
    let tasks = crate::services::task_service::TaskService::list(&storage, &filter);
    let mut rows: Vec<_> = tasks
        .into_iter()
        .map(|(id, t)| {
            let c = t.comments.len() as u64;
            serde_json::json!({"id": id, "title": t.title, "comments": c})
        })
        .collect();
    rows.sort_by(|a, b| b["comments"].as_u64().cmp(&a["comments"].as_u64()));
    let rows: Vec<_> = rows.into_iter().take(limit).collect();
    match renderer.format {
        crate::output::OutputFormat::Json => {
            let obj = serde_json::json!({
                "status":"ok","action":"stats.comments.top","global":global,"project":scope_project,"count":rows.len(),"items":rows
            });
            renderer.emit_json(&obj);
        }
        _ => {
            if rows.is_empty() {
                renderer.emit_success("No comments found.");
            } else {
                for r in &rows {
                    let id = r["id"].as_str().unwrap_or("");
                    let n = r["comments"].as_u64().unwrap_or(0);
                    renderer.emit_raw_stdout(&format!("{:>4}  {}", n, id));
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn run_by_author(
    limit: usize,
    global: bool,
    project: Option<&str>,
    resolver: &crate::workspace::TasksDirectoryResolver,
    renderer: &crate::output::OutputRenderer,
) -> Result<(), String> {
    let scope_project = if global {
        None
    } else {
        project
            .map(|p| crate::utils::resolve_project_input(p, resolver.path.as_path()))
            .or_else(|| Some(crate::project::get_effective_project_name(resolver)))
    };
    let storage = crate::storage::manager::Storage::new(resolver.path.clone());
    let filter = crate::api_types::TaskListFilter {
        status: Vec::new(),
        priority: Vec::new(),
        task_type: Vec::new(),
        project: scope_project.clone(),
        tags: Vec::new(),
        text_query: None,
        sprints: Vec::new(),
    };
    let tasks = crate::services::task_service::TaskService::list(&storage, &filter);
    // Author attribute removed from TaskComment; we can't group by author without blame here.
    // Fallback: show tasks with most comments.
    let mut rows: Vec<_> = tasks
        .into_iter()
        .map(|(id, t)| serde_json::json!({"task": id, "comments": t.comments.len()}))
        .collect();
    rows.sort_by(|a, b| b["comments"].as_u64().cmp(&a["comments"].as_u64()));
    let rows: Vec<_> = rows.into_iter().take(limit).collect();
    match renderer.format {
        crate::output::OutputFormat::Json => {
            let obj = serde_json::json!({"status":"ok","action":"stats.comments.by_task","global":global,"project":scope_project,"count":rows.len(),"items":rows});
            renderer.emit_json(&obj);
        }
        _ => {
            if rows.is_empty() {
                renderer.emit_success("No comments found.");
            } else {
                for r in &rows {
                    let id = r["task"].as_str().unwrap_or("");
                    let n = r["comments"].as_u64().unwrap_or(0);
                    renderer.emit_raw_stdout(&format!("{:>4}  {}", n, id));
                }
            }
        }
    }
    Ok(())
}
