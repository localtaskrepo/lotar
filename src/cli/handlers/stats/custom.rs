// Auto-generated from stats_handler.rs.
pub(crate) fn run_keys(
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
    use std::collections::BTreeMap;
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    for (_id, t) in tasks.into_iter() {
        for k in t.custom_fields.keys() {
            *counts.entry(k.clone()).or_insert(0) += 1;
        }
    }
    let mut rows: Vec<_> = counts
        .into_iter()
        .map(|(k, n)| serde_json::json!({"key":k,"count":n}))
        .collect();
    rows.sort_by(|a, b| b["count"].as_u64().cmp(&a["count"].as_u64()));
    let rows: Vec<_> = rows.into_iter().take(limit).collect();
    match renderer.format {
        crate::output::OutputFormat::Json => {
            let obj = serde_json::json!({"status":"ok","action":"stats.custom.keys","global":global,"project":scope_project,"count":rows.len(),"items":rows});
            renderer.emit_json(&obj);
        }
        _ => {
            if rows.is_empty() {
                renderer.emit_success("No custom fields found.");
            } else {
                for r in &rows {
                    let k = r["key"].as_str().unwrap_or("");
                    let n = r["count"].as_u64().unwrap_or(0);
                    renderer.emit_raw_stdout(format_args!("{:>4}  {}", n, k));
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn run_field(
    name: String,
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
    use std::collections::BTreeMap;
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    for (_id, t) in tasks.into_iter() {
        if let Some(v) = t.custom_fields.get(&name) {
            let key = crate::cli::handlers::stats::common::custom_value_key(v);
            *counts.entry(key).or_insert(0) += 1;
        }
    }
    let mut rows: Vec<_> = counts
        .into_iter()
        .map(|(k, n)| serde_json::json!({"value":k,"count":n}))
        .collect();
    rows.sort_by(|a, b| b["count"].as_u64().cmp(&a["count"].as_u64()));
    let rows: Vec<_> = rows.into_iter().take(limit).collect();
    match renderer.format {
        crate::output::OutputFormat::Json => {
            let obj = serde_json::json!({"status":"ok","action":"stats.custom.field","field":name,"global":global,"project":scope_project,"count":rows.len(),"items":rows});
            renderer.emit_json(&obj);
        }
        _ => {
            if rows.is_empty() {
                renderer.emit_success("No values found for field.");
            } else {
                for r in &rows {
                    let k = r["value"].as_str().unwrap_or("");
                    let n = r["count"].as_u64().unwrap_or(0);
                    renderer.emit_raw_stdout(format_args!("{:>4}  {}", n, k));
                }
            }
        }
    }
    Ok(())
}
