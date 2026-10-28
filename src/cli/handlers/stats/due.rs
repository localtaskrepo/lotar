#[expect(
    clippy::too_many_arguments,
    reason = "one parameter per CLI flag; this handler forwards them unchanged"
)]
pub(crate) fn run(
    buckets: Option<String>,
    overdue: bool,
    threshold: String,
    global: bool,
    filter: &str,
    project: Option<&str>,
    resolver: &crate::workspace::TasksDirectoryResolver,
    renderer: &crate::output::OutputRenderer,
) -> Result<(), String> {
    let scope_project = super::common::scope_project(global, project, resolver);
    let tasks = super::common::list_tasks(scope_project.as_deref(), resolver, filter)?;

    // The shared due buckets (DEV-238), the same as `due:<bucket>` in
    // `lotar list` and the Insights due outlook: soon is the 7 days after
    // today, later anything after that.
    const BUCKETS: [&str; 4] = ["overdue", "today", "soon", "later"];
    let buckets_csv = if overdue {
        // In overdue-only mode, force buckets to just 'overdue'
        "overdue".to_string()
    } else {
        buckets.unwrap_or_else(|| BUCKETS.join(","))
    };
    let mut enabled: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for bucket in buckets_csv
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
    {
        if !BUCKETS.contains(&bucket.as_str()) {
            return Err(format!(
                "Invalid bucket '{bucket}' (expected overdue, today, soon, later)"
            ));
        }
        enabled.insert(bucket);
    }
    if enabled.is_empty() {
        enabled.extend(BUCKETS.iter().map(|s| s.to_string()));
    }

    // DEV-21: shared due parser + local calendar day (injectable-clock
    // semantics), replacing the date-only/UTC island: RFC3339 due values
    // bucket on the local date they fall on, matching the REST/CLI
    // executor.
    let now = chrono::Utc::now();
    let today = crate::services::task_query::today_local(now);
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

    use crate::services::task_query::{DueFilter, due_raw_matches_bucket};
    for (_id, t) in tasks.into_iter() {
        let Some(due_raw) = t.due_date.as_deref() else {
            continue;
        };
        let bucket = [
            (DueFilter::Overdue, "overdue"),
            (DueFilter::Today, "today"),
            (DueFilter::Soon, "soon"),
            (DueFilter::Later, "later"),
        ]
        .into_iter()
        .find(|(filter, _)| due_raw_matches_bucket(Some(due_raw), *filter, today))
        .map(|(_, name)| name);
        let Some(bucket) = bucket.filter(|name| enabled.contains(*name)) else {
            continue;
        };
        if bucket == "overdue" {
            // Terminal tasks are never overdue (DEV-21); the classification
            // comes from the task's own project policy via task_state.
            if t.task_state.as_ref().is_some_and(|state| state.is_done) {
                continue;
            }
            // In overdue-only mode, count only tasks overdue by the threshold.
            if overdue {
                let days_overdue = crate::services::task_query::parse_stored_due(due_raw)
                    .map(|due| (today - due.local_date()).num_days())
                    .unwrap_or(0);
                if days_overdue < overdue_cutoff_days {
                    continue;
                }
            }
        }
        *counts.entry(bucket.to_string()).or_insert(0) += 1;
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
