// Auto-generated from stats_handler.rs.
pub(crate) fn run(
    buckets: Option<String>,
    overdue: bool,
    threshold: String,
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
        project: scope_project.clone(),
        ..Default::default()
    };
    let tasks = crate::services::task_service::TaskService::list(&storage, &filter);

    let buckets_csv = if overdue {
        // In overdue-only mode, force buckets to just 'overdue'
        "overdue".to_string()
    } else {
        buckets.unwrap_or_else(|| "overdue,today,week,month,later".to_string())
    };
    let mut enabled: std::collections::BTreeSet<String> = buckets_csv
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    if enabled.is_empty() {
        enabled.extend(
            ["overdue", "today", "week", "month", "later"]
                .iter()
                .map(|s| s.to_string()),
        );
    }

    let now = chrono::Utc::now().date_naive();
    // Parse threshold for overdue mode (Nd or Nw)
    let overdue_cutoff_days: i64 = if overdue {
        let t = threshold.trim().to_lowercase();
        if let Some(num) = t.strip_suffix('d') {
            num.parse::<i64>()
                .map_err(|_| format!("Invalid threshold: {}", threshold))?
        } else if let Some(num) = t.strip_suffix('w') {
            let n = num
                .parse::<i64>()
                .map_err(|_| format!("Invalid threshold: {}", threshold))?;
            n * 7
        } else if t.is_empty() || t == "0" || t == "0d" {
            0
        } else {
            return Err(format!(
                "Invalid threshold '{}'. Use Nd or Nw, e.g., 0d, 7d, 2w",
                threshold
            ));
        }
    } else {
        0
    };
    let mut counts = std::collections::BTreeMap::new();
    for k in &enabled {
        counts.insert(k.clone(), 0usize);
    }

    for (_id, t) in tasks.into_iter() {
        if let Some(due) = t.due_date
            && let Ok(date) = chrono::NaiveDate::parse_from_str(&due, "%Y-%m-%d")
        {
            let diff = (date - now).num_days();
            // Classify
            if diff < 0 && enabled.contains("overdue") {
                // In overdue-only mode, filter by threshold age
                if overdue {
                    let age_days = -diff; // how many days overdue
                    if age_days >= overdue_cutoff_days {
                        *counts.get_mut("overdue").unwrap() += 1;
                    }
                } else {
                    *counts.get_mut("overdue").unwrap() += 1;
                }
            } else if diff == 0 && enabled.contains("today") {
                *counts.get_mut("today").unwrap() += 1;
            } else if diff <= 7 && enabled.contains("week") {
                *counts.get_mut("week").unwrap() += 1;
            } else if diff <= 31 && enabled.contains("month") {
                *counts.get_mut("month").unwrap() += 1;
            } else if enabled.contains("later") {
                *counts.get_mut("later").unwrap() += 1;
            }
        }
    }

    match renderer.format {
        crate::output::OutputFormat::Json => {
            let items: Vec<_> = counts
                .into_iter()
                .map(|(k, v)| serde_json::json!({"bucket": k, "count": v}))
                .collect();
            let obj = serde_json::json!({
                "status":"ok",
                "action":"stats.due",
                "buckets": buckets_csv,
                "overdue_only": overdue,
                "threshold": if overdue { Some(threshold) } else { None },
                "global": global,
                "project": scope_project,
                "items": items,
            });
            renderer.emit_json(&obj);
        }
        _ => {
            for (k, v) in counts {
                renderer.emit_raw_stdout(format_args!("{:>6}  {}", v, k));
            }
        }
    }
    Ok(())
}
