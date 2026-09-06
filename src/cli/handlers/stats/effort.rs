// Auto-generated from stats_handler.rs.
use crate::cli::args::stats::StatsEffortUnit;

#[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
pub(crate) fn run(
    by: &str,
    r#where: Vec<(String, String)>,
    unit: StatsEffortUnit,
    limit: usize,
    global: bool,
    since: Option<&str>,
    until: Option<&str>,
    transitions: Option<&str>,
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
    let storage = crate::storage::manager::Storage::new(&resolver.path.clone());
    let filter = crate::api_types::TaskListFilter {
        project: scope_project.clone(),
        ..Default::default()
    };
    let mut tasks = crate::services::task_service::TaskService::list(&storage, &filter);
    // Performance guardrail: cap tasks processed for aggregation
    let cap: usize = std::env::var("LOTAR_STATS_EFFORT_CAP")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(20000);
    if tasks.len() > cap {
        tasks.truncate(cap);
    }
    // If transitions is set, filter tasks to those that transitioned into the given status within the window
    if let Some(trans_status) = transitions {
        use crate::storage::task::parse_status_from_yaml;

        let (since_dt, until_dt) = crate::utils::time::parse_since_until(since, until)?;
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        let repo_root = match crate::utils::git::find_repo_root(&cwd) {
            Some(p) => p,
            None => {
                return Err("Not in a git repository; --transitions requires git history".into());
            }
        };
        if std::env::var("LOTAR_DEBUG").is_ok() {
            use std::fs::OpenOptions;
            use std::io::Write;
            if let Ok(mut f) = OpenOptions::new()
                .create(true)
                .append(true)
                .open("/tmp/lotar_transitions_debug.log")
            {
                let _ = writeln!(
                    f,
                    "[START] tasks_pre={} cwd={}",
                    tasks.len(),
                    cwd.to_string_lossy()
                );
            }
        }
        // Canonicalize both to avoid /var vs /private/var mismatches on macOS and similar
        let tasks_abs = resolver.path.clone();
        let repo_root_real = std::fs::canonicalize(&repo_root).unwrap_or(repo_root.clone());
        let tasks_abs_real = std::fs::canonicalize(&tasks_abs).unwrap_or(tasks_abs.clone());
        let tasks_rel = if tasks_abs_real.starts_with(&repo_root_real) {
            tasks_abs_real
                .strip_prefix(&repo_root_real)
                .unwrap()
                .to_path_buf()
        } else {
            // Fall back to just the tasks directory name to stay repo-relative
            tasks_abs
                .file_name()
                .map(std::path::PathBuf::from)
                .unwrap_or(tasks_abs.clone())
        };
        if std::env::var("LOTAR_DEBUG").is_ok() {
            use std::fs::OpenOptions;
            use std::io::Write;
            if let Ok(mut f) = OpenOptions::new()
                .create(true)
                .append(true)
                .open("/tmp/lotar_transitions_debug.log")
            {
                let _ = writeln!(
                    f,
                    "[PATHS] repo_root_real={} tasks_abs_real={} tasks_rel={}",
                    repo_root_real.to_string_lossy(),
                    tasks_abs_real.to_string_lossy(),
                    tasks_rel.to_string_lossy()
                );
            }
        }
        tasks.retain(|(id, _t)| {
            // Find the corresponding file for this task
            let parts: Vec<&str> = id.split('-').collect();
            if parts.len() < 2 {
                return false;
            }
            let project = parts[0];
            let num = parts[1];
            let file_rel = tasks_rel.join(project).join(format!("{}.yml", num));
            let mut commits =
                match crate::services::audit_service::AuditService::list_commits_for_file(
                    &repo_root_real,
                    &file_rel,
                ) {
                    Ok(c) => c,
                    Err(_) => return false,
                };
            if std::env::var("LOTAR_DEBUG").is_ok() {
                use std::fs::OpenOptions;
                use std::io::Write;
                if let Ok(mut f) = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("/tmp/lotar_transitions_debug.log")
                {
                    let _ = writeln!(
                        f,
                        "[FILE] id={}, rel={}, commits={}",
                        id,
                        file_rel.to_string_lossy(),
                        commits.len()
                    );
                }
            }
            if commits.is_empty() {
                return false;
            }
            // Process in chronological order and seed baseline from before window
            commits.sort_by_key(|a| a.date);
            let mut prev_status: Option<String> = None;
            for c in commits {
                if c.date > until_dt {
                    break;
                }
                if let Ok(content) = crate::services::audit_service::AuditService::show_file_at(
                    &repo_root_real,
                    &c.commit,
                    &file_rel,
                ) && let Some(ts) = parse_status_from_yaml(&content)
                {
                    let curr_status = ts.to_string();
                    if std::env::var("LOTAR_DEBUG").is_ok() {
                        use std::fs::OpenOptions;
                        use std::io::Write;
                        if let Ok(mut f) = OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open("/tmp/lotar_transitions_debug.log")
                        {
                            let _ = writeln!(
                                f,
                                "  - commit @{} status={} (prev={:?})",
                                c.date.to_rfc3339(),
                                curr_status,
                                prev_status
                            );
                        }
                    }
                    if c.date >= since_dt && c.date <= until_dt {
                        // Detect transition into target within the window
                        if prev_status.as_deref() != Some(curr_status.as_str())
                            && curr_status == trans_status
                        {
                            if std::env::var("LOTAR_DEBUG").is_ok() {
                                use std::fs::OpenOptions;
                                use std::io::Write;
                                if let Ok(mut f) = OpenOptions::new()
                                    .create(true)
                                    .append(true)
                                    .open("/tmp/lotar_transitions_debug.log")
                                {
                                    let _ = writeln!(
                                        f,
                                        "  -> MATCH: id={} transitioned into {}",
                                        id, curr_status
                                    );
                                }
                            }
                            return true;
                        }
                    }
                    prev_status = Some(curr_status);
                }
            }
            false
        });
        if std::env::var("LOTAR_DEBUG").is_ok() {
            use std::fs::OpenOptions;
            use std::io::Write;
            if let Ok(mut f) = OpenOptions::new()
                .create(true)
                .append(true)
                .open("/tmp/lotar_transitions_debug.log")
            {
                let _ = writeln!(f, "[END] tasks_post={}", tasks.len());
            }
        }
        // Fallback: if strict listing yielded no tasks (e.g., due to YAML casing),
        // scan the tasks directory and build candidates tolerantly.
        if tasks.is_empty() {
            use std::fs;
            let mut matched: Vec<(String, crate::api_types::TaskDTO)> = Vec::new();
            // Enumerate project folders directly under tasks_abs_real
            if let Ok(project_dirs) = fs::read_dir(&tasks_abs_real) {
                let fallback_storage =
                    crate::storage::manager::Storage::new(&tasks_abs_real.clone());
                let sprint_lookup = crate::services::task_service::TaskService::load_sprint_lookup(
                    &fallback_storage,
                );
                for entry in project_dirs.flatten() {
                    let p = entry.path();
                    if !p.is_dir() {
                        continue;
                    }
                    let project_folder = match p.file_name().and_then(|s| s.to_str()) {
                        Some(s) => s.to_string(),
                        None => continue,
                    };
                    // List .yml files in this project folder
                    for fpath in fs::read_dir(&p)
                        .map(|rd| {
                            rd.flatten()
                                .map(|e| e.path())
                                .collect::<Vec<std::path::PathBuf>>()
                        })
                        .unwrap_or_default()
                    {
                        let num = match crate::utils::filesystem::file_numeric_stem(&fpath) {
                            Some(n) => n,
                            None => continue,
                        };
                        let id = format!("{}-{}", project_folder, num);
                        let file_rel = tasks_rel.join(&project_folder).join(format!("{}.yml", num));
                        let mut commits =
                            crate::services::audit_service::AuditService::list_commits_for_file(
                                &repo_root_real,
                                &file_rel,
                            )
                            .unwrap_or_default();
                        if commits.is_empty() {
                            continue;
                        }
                        commits.sort_by_key(|a| a.date);
                        let mut prev_status: Option<String> = None;
                        let mut is_match = false;
                        for c in commits {
                            if c.date > until_dt {
                                break;
                            }
                            if let Ok(content) =
                                crate::services::audit_service::AuditService::show_file_at(
                                    &repo_root_real,
                                    &c.commit,
                                    &file_rel,
                                )
                                && let Some(ts) = parse_status_from_yaml(&content)
                            {
                                let curr_status = ts.to_string();
                                if c.date >= since_dt
                                    && c.date <= until_dt
                                    && prev_status.as_deref() != Some(curr_status.as_str())
                                    && curr_status == trans_status
                                {
                                    is_match = true;
                                    break;
                                }
                                prev_status = Some(curr_status);
                            }
                        }
                        if !is_match {
                            continue;
                        }
                        // Build a TaskDTO by tolerantly reading current YAML
                        let abs_file = tasks_abs_real
                            .join(&project_folder)
                            .join(format!("{}.yml", num));
                        let task = crate::storage::task::parse_task_yaml_tolerant(
                            &fs::read_to_string(&abs_file).unwrap_or_default(),
                        )
                        .unwrap_or_default();
                        let sprints: Vec<u32> = sprint_lookup
                            .get(&id)
                            .map(|orders| orders.keys().copied().collect::<Vec<u32>>())
                            .unwrap_or_default();
                        let dto = crate::api_types::TaskDTO {
                            id: id.clone(),
                            title: task.title,
                            status: task.status,
                            priority: task.priority,
                            task_type: task.task_type,
                            reporter: task.reporter,
                            assignee: task.assignee,
                            created: task.created,
                            modified: task.modified,
                            due_date: task.due_date,
                            effort: task.effort,
                            subtitle: task.subtitle,
                            description: task.description,
                            tags: task.tags,
                            relationships: task.relationships,
                            comments: task.comments,
                            references: task.references,
                            sprints,
                            sprint_order: std::collections::BTreeMap::new(),
                            history: task.history,
                            custom_fields: task.custom_fields,
                        };
                        matched.push((id, dto));
                    }
                }
            }
            if std::env::var("LOTAR_DEBUG").is_ok()
                && let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("/tmp/lotar_transitions_debug.log")
            {
                use std::io::Write;
                let _ = writeln!(f, "[FALLBACK] tasks_added={}", matched.len());
            }
            if !matched.is_empty() {
                tasks = matched;
            }
        }
    }

    // Unified field resolver: returns optional grouping key for a task given a key pattern
    let resolve_group_key = |id: &str,
                             t: &crate::api_types::TaskDTO,
                             key: &str,
                             cfg: &crate::config::types::ResolvedConfig|
     -> Option<Vec<String>> {
        crate::utils::custom_fields::resolve_task_filter_values(id, t, key, cfg)
    };

    // Parse where filters as (key -> allowed set); simple equality only for now
    use std::collections::{BTreeMap, BTreeSet};
    let mut filters: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (k, v) in r#where.into_iter() {
        // Resolve '@me' to current username for assignee filter
        let filter_value = if k.eq_ignore_ascii_case("assignee") && v == "@me" {
            crate::utils::identity::resolve_me_alias(&v, Some(resolver.path.as_path())).unwrap_or(v)
        } else {
            v
        };
        filters.entry(k).or_default().insert(filter_value);
    }

    // Load config once to resolve custom field keys
    let cfg = crate::config::resolution::load_and_merge_configs(Some(resolver.path.as_path()))
        .map_err(|e| format!("Failed to load config: {}", e))?;

    // Aggregate: keep both time hours and points totals to support unit modes
    let mut agg: BTreeMap<String, (f64, f64, usize)> = BTreeMap::new(); // (hours, points, count)
    for (id, t) in tasks {
        // Apply filters
        let mut passes = true;
        for (fk, allowed) in &filters {
            if let Some(vals) = resolve_group_key(&id, &t, fk, &cfg)
                .map(|vs| vs.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>())
            {
                if std::env::var("LOTAR_DEBUG").is_ok() {
                    // Debug output for filter matching
                    use std::fs::OpenOptions;
                    use std::io::Write;
                    if let Ok(mut f) = OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open("/tmp/lotar_stats_debug.log")
                    {
                        let _ = writeln!(
                            f,
                            "[FILTER] key={}, allowed={:?}, vals={:?}",
                            fk, allowed, vals
                        );
                    }
                }
                // Use centralized fuzzy property matching utility
                use crate::utils::fuzzy_match::fuzzy_set_match;
                let allowed_vec: Vec<String> = allowed.iter().cloned().collect();
                let vals_vec: Vec<String> = vals.to_vec();
                if vals_vec.is_empty() || !fuzzy_set_match(&vals_vec, &allowed_vec) {
                    passes = false;
                    break;
                }
            } else {
                passes = false;
                break;
            }
        }
        if !passes {
            continue;
        }

        // Parse effort into hours or points
        let (hours, points, effort_kind) = if let Some(e) = t.effort.as_deref() {
            match crate::utils::effort::parse_effort(e) {
                Ok(parsed) => match parsed.kind {
                    crate::utils::effort::EffortKind::TimeHours(h) => (h, 0.0, "hours".to_string()),
                    crate::utils::effort::EffortKind::Points(p) => (0.0, p, "points".to_string()),
                },
                Err(_) => (0.0, 0.0, "invalid".to_string()),
            }
        } else {
            (0.0, 0.0, "none".to_string())
        };

        // Use literal assignee value for grouping and filtering
        // Config for resolving custom field grouping key
        let mut keys = resolve_group_key(&id, &t, by, &cfg).unwrap_or_else(|| vec![String::new()]);
        if by.trim().to_lowercase() == "assignee"
            && let Some(a) = &t.assignee
        {
            keys = vec![a.clone()];
        }
        let keys = if keys.is_empty() {
            vec![String::new()]
        } else {
            keys
        };
        if std::env::var("LOTAR_DEBUG").is_ok() {
            for key in keys.iter() {
                use std::fs::OpenOptions;
                use std::io::Write;
                if let Ok(mut f) = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("/tmp/lotar_stats_debug.log")
                {
                    let _ = writeln!(
                        f,
                        "[DEBUG] Task: id={}, assignee={:?}, effort={:?}, kind={}, group_key={:?}",
                        id, t.assignee, t.effort, effort_kind, key
                    );
                }
            }
        }
        for key in keys.into_iter() {
            let entry = agg.entry(key).or_insert((0.0, 0.0, 0));
            entry.0 += hours;
            entry.1 += points;
            entry.2 += 1;
        }
    }

    // Convert to selected unit and prepare rows
    // Prepare rows based on unit selection
    let (unit_key, mode_points, auto_mode) = match unit {
        crate::cli::args::stats::StatsEffortUnit::Hours => ("hours", false, false),
        crate::cli::args::stats::StatsEffortUnit::Days => ("days", false, false),
        crate::cli::args::stats::StatsEffortUnit::Weeks => ("weeks", false, false),
        crate::cli::args::stats::StatsEffortUnit::Points => ("points", true, false),
        crate::cli::args::stats::StatsEffortUnit::Auto => ("auto", false, true),
    };
    let mut rows: Vec<_> = Vec::new();
    for (k, (hours, points, count)) in agg.into_iter() {
        let mut obj = serde_json::json!({
            "key": k,
            "hours": hours,
            "days": hours/8.0,
            "weeks": hours/40.0,
            "points": points,
            "tasks": count
        });
        if mode_points {
            obj["points_value"] = serde_json::json!(points);
        } else if auto_mode {
            // auto: choose hours-based if total hours>0 for any row; else points
            obj["auto_value"] = serde_json::json!(if hours > 0.0 { hours } else { points });
            obj["auto_unit"] = serde_json::json!(if hours > 0.0 { "hours" } else { "points" });
        } else {
            let v = match unit_key {
                "hours" => hours,
                "days" => hours / 8.0,
                "weeks" => hours / 40.0,
                _ => hours,
            };
            obj[unit_key] = serde_json::json!(v);
        }
        rows.push(obj);
    }
    // Sort by hours desc
    rows.sort_by(|a, b| {
        b["hours"]
            .as_f64()
            .partial_cmp(&a["hours"].as_f64())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let rows: Vec<_> = rows.into_iter().take(limit).collect();

    match renderer.format {
        crate::output::OutputFormat::Json => {
            let obj = serde_json::json!({
                "status": "ok",
                "action": "stats.effort",
                "by": by,
                "global": global,
                "project": scope_project,
                "count": rows.len(),
                "items": rows,
                "unit": match unit { crate::cli::args::stats::StatsEffortUnit::Hours=>"hours", crate::cli::args::stats::StatsEffortUnit::Days=>"days", crate::cli::args::stats::StatsEffortUnit::Weeks=>"weeks", crate::cli::args::stats::StatsEffortUnit::Points=>"points", crate::cli::args::stats::StatsEffortUnit::Auto=>"auto" },
            });
            renderer.emit_json(&obj);
        }
        _ => {
            if rows.is_empty() {
                renderer.emit_success("No tasks with effort found.");
            } else {
                for r in &rows {
                    let key = r["key"].as_str().unwrap_or("");
                    let (val_str, suffix) = match unit {
                        crate::cli::args::stats::StatsEffortUnit::Hours => {
                            (format!("{:.2}", r["hours"].as_f64().unwrap_or(0.0)), "h")
                        }
                        crate::cli::args::stats::StatsEffortUnit::Days => {
                            (format!("{:.2}", r["days"].as_f64().unwrap_or(0.0)), "d")
                        }
                        crate::cli::args::stats::StatsEffortUnit::Weeks => {
                            (format!("{:.2}", r["weeks"].as_f64().unwrap_or(0.0)), "w")
                        }
                        crate::cli::args::stats::StatsEffortUnit::Points => {
                            (format!("{}", r["points"].as_f64().unwrap_or(0.0)), "pt")
                        }
                        crate::cli::args::stats::StatsEffortUnit::Auto => {
                            let unit = r["auto_unit"].as_str().unwrap_or("hours");
                            let v = r["auto_value"].as_f64().unwrap_or(0.0);
                            let suf = if unit == "points" { "pt" } else { "h" };
                            (format!("{:.2}", v), suf)
                        }
                    };
                    renderer.emit_raw_stdout(format_args!("{:>8}{}  {}", val_str, suffix, key));
                }
            }
        }
    }
    Ok(())
}
