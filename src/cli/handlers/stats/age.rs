// Auto-generated from stats_handler.rs.
use crate::cli::args::stats::StatsAgeDistribution;

pub(crate) fn run(
    distribution: StatsAgeDistribution,
    limit: usize,
    global: bool,
    project: Option<&str>,
    resolver: &crate::workspace::TasksDirectoryResolver,
    renderer: &crate::output::OutputRenderer,
) -> Result<(), String> {
    // Determine scope (project or global)
    let scope_project = if global {
        None
    } else {
        project
            .map(|p| crate::utils::resolve_project_input(p, resolver.path.as_path()))
            .or_else(|| Some(crate::project::get_effective_project_name(resolver)))
    };

    // Load tasks snapshot
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
    // ...existing code...

    // Now is reference point
    let now = chrono::Utc::now();

    // Helper to key buckets by human label and numeric order for sorting
    let mut counts: std::collections::BTreeMap<i64, usize> = std::collections::BTreeMap::new();

    match distribution {
        StatsAgeDistribution::Day => {
            for (_id, t) in tasks.into_iter() {
                if let Ok(created) = chrono::DateTime::parse_from_rfc3339(&t.created)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                {
                    let days = (now - created).num_days().max(0);
                    *counts.entry(days).or_insert(0) += 1;
                }
            }
        }
        StatsAgeDistribution::Week => {
            for (_id, t) in tasks.into_iter() {
                if let Ok(created) = chrono::DateTime::parse_from_rfc3339(&t.created)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                {
                    let weeks = ((now - created).num_days().max(0)) / 7;
                    *counts.entry(weeks).or_insert(0) += 1;
                }
            }
        }
        StatsAgeDistribution::Month => {
            for (_id, t) in tasks.into_iter() {
                if let Ok(created) = chrono::DateTime::parse_from_rfc3339(&t.created)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                {
                    let months = ((now - created).num_days().max(0)) / 30; // approx
                    *counts.entry(months).or_insert(0) += 1;
                }
            }
        }
    }

    // Transform into sorted items with labels
    let mut items: Vec<(String, usize)> = Vec::new();
    for (k, v) in counts.into_iter() {
        let label = match distribution {
            StatsAgeDistribution::Day => format!("{}d", k),
            StatsAgeDistribution::Week => format!("{}w", k),
            StatsAgeDistribution::Month => format!("{}m", k),
        };
        items.push((label, v));
    }
    // Newest first (largest counts by recency? Prefer most recent age buckets at top): sort by numeric age asc, then by count desc not necessary
    // We'll reverse to show newest first (0d/0w/0m at top)
    items.sort_by(|a, b| {
        // parse numeric prefix
        let pa =
            a.0.trim_end_matches(|c: char| c.is_alphabetic())
                .parse::<i64>()
                .unwrap_or(i64::MAX);
        let pb =
            b.0.trim_end_matches(|c: char| c.is_alphabetic())
                .parse::<i64>()
                .unwrap_or(i64::MAX);
        pa.cmp(&pb)
    });
    let items: Vec<_> = items.into_iter().take(limit).collect();

    match renderer.format {
        crate::output::OutputFormat::Json => {
            let json_items: Vec<_> = items
                .iter()
                .map(|(label, count)| serde_json::json!({"age": label, "count": count}))
                .collect();
            let obj = serde_json::json!({
                "status": "ok",
                "action": "stats.age",
                "distribution": match distribution { StatsAgeDistribution::Day=>"day", StatsAgeDistribution::Week=>"week", StatsAgeDistribution::Month=>"month" },
                "global": global,
                "project": scope_project,
                "count": json_items.len(),
                "items": json_items,
            });
            renderer.emit_json(&obj);
        }
        _ => {
            if items.is_empty() {
                renderer.emit_success("No tasks found for age distribution.");
            } else {
                for (label, count) in &items {
                    renderer.emit_raw_stdout(format_args!("{:>6}  {}", count, label));
                }
            }
        }
    }
    Ok(())
}
