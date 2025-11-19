// Auto-generated from stats_handler.rs.
pub(crate) fn run(
    field: crate::cli::args::stats::StatsDistributionField,
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

    // Load tasks
    let storage = crate::storage::manager::Storage::new(resolver.path.clone());
    let filter = crate::api_types::TaskListFilter {
        project: scope_project.clone(),
        ..Default::default()
    };
    let tasks = crate::services::task_service::TaskService::list(&storage, &filter);

    use std::collections::HashMap;
    let mut freq: HashMap<String, usize> = HashMap::new();
    for (_id, t) in tasks.into_iter() {
        match field {
            crate::cli::args::stats::StatsDistributionField::Status => {
                *freq.entry(t.status.to_string()).or_insert(0) += 1;
            }
            crate::cli::args::stats::StatsDistributionField::Priority => {
                *freq.entry(t.priority.to_string()).or_insert(0) += 1;
            }
            crate::cli::args::stats::StatsDistributionField::Type => {
                *freq.entry(t.task_type.to_string()).or_insert(0) += 1;
            }
            crate::cli::args::stats::StatsDistributionField::Assignee => {
                let key = t.assignee.unwrap_or_else(|| "".to_string());
                *freq.entry(key).or_insert(0) += 1;
            }
            crate::cli::args::stats::StatsDistributionField::Reporter => {
                let key = t.reporter.unwrap_or_else(|| "".to_string());
                *freq.entry(key).or_insert(0) += 1;
            }
            crate::cli::args::stats::StatsDistributionField::Project => {
                // Group by project prefix embedded in ID
                let proj = t.id.split('-').next().unwrap_or("").to_string();
                *freq.entry(proj).or_insert(0) += 1;
            }
            crate::cli::args::stats::StatsDistributionField::Tag => {
                for tag in t.tags.into_iter() {
                    if tag.trim().is_empty() {
                        continue;
                    }
                    *freq.entry(tag).or_insert(0) += 1;
                }
            }
        }
    }

    let mut items: Vec<(String, usize)> = freq.into_iter().filter(|(k, _)| !k.is_empty()).collect();
    items.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let limited: Vec<_> = items.into_iter().take(limit).collect();

    let field_name = match field {
        crate::cli::args::stats::StatsDistributionField::Status => "status",
        crate::cli::args::stats::StatsDistributionField::Priority => "priority",
        crate::cli::args::stats::StatsDistributionField::Type => "type",
        crate::cli::args::stats::StatsDistributionField::Assignee => "assignee",
        crate::cli::args::stats::StatsDistributionField::Reporter => "reporter",
        crate::cli::args::stats::StatsDistributionField::Project => "project",
        crate::cli::args::stats::StatsDistributionField::Tag => "tag",
    };

    match renderer.format {
        crate::output::OutputFormat::Json => {
            let json_items: Vec<_> = limited
                .iter()
                .map(|(key, count)| serde_json::json!({"key": key, "count": count}))
                .collect();
            let obj = serde_json::json!({
                "status": "ok",
                "action": "stats.distribution",
                "field": field_name,
                "global": global,
                "project": scope_project,
                "count": json_items.len(),
                "items": json_items,
            });
            renderer.emit_json(&obj);
        }
        _ => {
            if limited.is_empty() {
                renderer.emit_success("No items found for the selected field.");
            } else {
                for (key, count) in &limited {
                    renderer.emit_raw_stdout(format_args!("{:>6}  {}", count, key));
                }
            }
        }
    }
    Ok(())
}
