// Auto-generated from stats_handler.rs.
pub(crate) fn run(
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

    // Load all tasks (current snapshot)
    let storage = crate::storage::manager::Storage::new(resolver.path.clone());
    let filter = crate::api_types::TaskListFilter {
        project: scope_project.clone(),
        ..Default::default()
    };
    let tasks = crate::services::task_service::TaskService::list(&storage, &filter);

    // Aggregate tag frequencies
    use std::collections::HashMap;
    let mut freq: HashMap<String, usize> = HashMap::new();
    for (_id, t) in tasks.into_iter() {
        for tag in t.tags.into_iter() {
            if tag.trim().is_empty() {
                continue;
            }
            *freq.entry(tag).or_insert(0) += 1;
        }
    }
    let mut items: Vec<(String, usize)> = freq.into_iter().collect();
    items.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let limited: Vec<_> = items.into_iter().take(limit).collect();

    match renderer.format {
        crate::output::OutputFormat::Json => {
            let json_items: Vec<_> = limited
                .iter()
                .map(|(tag, count)| serde_json::json!({"tag": tag, "count": count}))
                .collect();
            let obj = serde_json::json!({
                "status": "ok",
                "action": "stats.tags",
                "global": global,
                "project": scope_project,
                "count": json_items.len(),
                "items": json_items,
            });
            renderer.emit_json(&obj);
        }
        _ => {
            if limited.is_empty() {
                renderer.emit_success("No tags found.");
            } else {
                for (tag, count) in &limited {
                    renderer.emit_raw_stdout(format_args!("{:>6}  {}", count, tag));
                }
            }
        }
    }
    Ok(())
}
