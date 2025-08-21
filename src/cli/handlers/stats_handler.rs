use crate::cli::args::stats::{StatsAction, StatsArgs};
use crate::cli::handlers::CommandHandler;
use crate::output::OutputRenderer;
use crate::utils::resolve_project_input;
use crate::workspace::TasksDirectoryResolver;

pub struct StatsHandler;

impl CommandHandler for StatsHandler {
    type Args = StatsArgs;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Self::Result {
        // Helper: stringify custom field values to stable bucket keys across feature variants
        #[cfg(feature = "schema")]
        fn custom_value_key(v: &crate::types::CustomFieldValue) -> String {
            match v {
                serde_json::Value::Null => "null".to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Array(_) => "[array]".to_string(),
                serde_json::Value::Object(_) => "{object}".to_string(),
            }
        }
        #[cfg(not(feature = "schema"))]
        fn custom_value_key(v: &crate::types::CustomFieldValue) -> String {
            match v {
                serde_yaml::Value::Null => "null".to_string(),
                serde_yaml::Value::Bool(b) => b.to_string(),
                serde_yaml::Value::Number(n) => n.to_string(),
                serde_yaml::Value::String(s) => s.clone(),
                serde_yaml::Value::Sequence(_) => "[array]".to_string(),
                serde_yaml::Value::Mapping(_) => "{object}".to_string(),
                _ => "other".to_string(),
            }
        }
        match args.action {
            StatsAction::Age {
                distribution,
                limit,
                global,
            } => {
                // Determine scope (project or global)
                let scope_project = if global {
                    None
                } else {
                    project
                        .map(|p| resolve_project_input(p, resolver.path.as_path()))
                        .or_else(|| Some(crate::project::get_effective_project_name(resolver)))
                };

                // Load tasks snapshot
                let storage = crate::storage::manager::Storage::new(resolver.path.clone());
                let filter = crate::api_types::TaskListFilter {
                    status: Vec::new(),
                    priority: Vec::new(),
                    task_type: Vec::new(),
                    project: scope_project.clone(),
                    category: None,
                    tags: Vec::new(),
                    text_query: None,
                };
                let tasks = crate::services::task_service::TaskService::list(&storage, &filter);
                // ...existing code...

                // Now is reference point
                let now = chrono::Utc::now();

                // Helper to key buckets by human label and numeric order for sorting
                let mut counts: std::collections::BTreeMap<i64, usize> =
                    std::collections::BTreeMap::new();

                match distribution {
                    crate::cli::args::stats::StatsAgeDistribution::Day => {
                        for (_id, t) in tasks.into_iter() {
                            if let Ok(created) = chrono::DateTime::parse_from_rfc3339(&t.created)
                                .map(|dt| dt.with_timezone(&chrono::Utc))
                            {
                                let days = (now - created).num_days().max(0);
                                *counts.entry(days).or_insert(0) += 1;
                            }
                        }
                    }
                    crate::cli::args::stats::StatsAgeDistribution::Week => {
                        for (_id, t) in tasks.into_iter() {
                            if let Ok(created) = chrono::DateTime::parse_from_rfc3339(&t.created)
                                .map(|dt| dt.with_timezone(&chrono::Utc))
                            {
                                let weeks = ((now - created).num_days().max(0)) / 7;
                                *counts.entry(weeks).or_insert(0) += 1;
                            }
                        }
                    }
                    crate::cli::args::stats::StatsAgeDistribution::Month => {
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
                        crate::cli::args::stats::StatsAgeDistribution::Day => format!("{}d", k),
                        crate::cli::args::stats::StatsAgeDistribution::Week => format!("{}w", k),
                        crate::cli::args::stats::StatsAgeDistribution::Month => format!("{}m", k),
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
                            "distribution": match distribution { crate::cli::args::stats::StatsAgeDistribution::Day=>"day", crate::cli::args::stats::StatsAgeDistribution::Week=>"week", crate::cli::args::stats::StatsAgeDistribution::Month=>"month" },
                            "global": global,
                            "project": scope_project,
                            "count": json_items.len(),
                            "items": json_items,
                        });
                        renderer.emit_raw_stdout(&obj.to_string());
                    }
                    _ => {
                        if items.is_empty() {
                            renderer.emit_success("No tasks found for age distribution.");
                        } else {
                            for (label, count) in &items {
                                renderer.emit_raw_stdout(&format!("{:>6}  {}", count, label));
                            }
                        }
                    }
                }
                Ok(())
            }
            StatsAction::Status {
                id,
                time_in_status,
                since,
                until,
            } => {
                // Only --time-in-status is supported for now
                if !time_in_status {
                    return Err("stats status: please pass --time-in-status".to_string());
                }

                // Resolve time window
                let (since_dt, until_dt) =
                    crate::utils::time::parse_since_until(since.as_deref(), until.as_deref())?;

                // Resolve project + full task id
                let mut project_resolver = crate::cli::project::ProjectResolver::new(resolver)?;
                project_resolver.validate_task_id_format(&id)?;
                let final_effective_project =
                    project.map(|p| resolve_project_input(p, resolver.path.as_path()));
                let resolved_project =
                    project_resolver.resolve_project(&id, final_effective_project.as_deref())?;
                let full_task_id =
                    project_resolver.get_full_task_id(&id, final_effective_project.as_deref())?;

                // Repo root and task file path
                let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
                let repo_root = match crate::utils::git::find_repo_root(&cwd) {
                    Some(p) => p,
                    None => {
                        match renderer.format {
                            crate::output::OutputFormat::Json => {
                                let obj = serde_json::json!({
                                    "status": "ok",
                                    "action": "stats.time_in_status",
                                    "since": since_dt.to_rfc3339(),
                                    "until": until_dt.to_rfc3339(),
                                    "project": resolved_project,
                                    "items": Vec::<serde_json::Value>::new(),
                                    "note": "Not in a git repository; returning empty set",
                                });
                                renderer.emit_raw_stdout(&obj.to_string());
                            }
                            _ => renderer
                                .emit_warning("Not in a git repository; returning empty set"),
                        }
                        return Ok(());
                    }
                };

                // Compute path to the single task file relative to repo root
                let tasks_abs = resolver.path.clone();
                let project_folder =
                    crate::storage::operations::StorageOperations::get_project_for_task(
                        &full_task_id,
                    )
                    .ok_or_else(|| format!("Cannot resolve project for '{}'", full_task_id))?;
                let project_path = tasks_abs.join(&project_folder);
                let rel_file = crate::storage::operations::StorageOperations::get_file_path_for_id(
                    &project_path,
                    &full_task_id,
                )
                .ok_or_else(|| format!("Task '{}' not found", full_task_id))?;
                let file_rel = if rel_file.starts_with(&repo_root) {
                    rel_file.strip_prefix(&repo_root).unwrap().to_path_buf()
                } else {
                    rel_file
                };

                // Build snapshots from git history up to 'until' and compute durations
                let commits = crate::services::audit_service::AuditService::list_commits_for_file(
                    &repo_root, &file_rel,
                )?;
                let mut snaps: Vec<(
                    chrono::DateTime<chrono::Utc>,
                    String,
                    Option<crate::types::TaskStatus>,
                )> = Vec::new();
                // Helper: tolerant status parse from YAML content
                fn parse_status_from_yaml(content: &str) -> Option<crate::types::TaskStatus> {
                    fn parse_status_str_tolerant(s: &str) -> Option<crate::types::TaskStatus> {
                        let norm = s.to_ascii_lowercase().replace(['_', '-'], "");
                        match norm.as_str() {
                            "todo" => Some(crate::types::TaskStatus::Todo),
                            "inprogress" => Some(crate::types::TaskStatus::InProgress),
                            "verify" => Some(crate::types::TaskStatus::Verify),
                            "blocked" => Some(crate::types::TaskStatus::Blocked),
                            "done" => Some(crate::types::TaskStatus::Done),
                            _ => None,
                        }
                    }
                    if let Ok(task) = serde_yaml::from_str::<crate::storage::task::Task>(content) {
                        return Some(task.status);
                    }
                    if let Ok(val) = serde_yaml::from_str::<serde_yaml::Value>(content) {
                        if let Some(s) = val.get("status").and_then(|v| match v {
                            serde_yaml::Value::String(s) => Some(s.clone()),
                            _ => None,
                        }) {
                            if let Some(ts) = parse_status_str_tolerant(&s) {
                                return Some(ts);
                            }
                            return s.parse::<crate::types::TaskStatus>().ok();
                        }
                    }
                    None
                }
                for c in &commits {
                    if c.date > until_dt {
                        continue;
                    }
                    if let Ok(content) = crate::services::audit_service::AuditService::show_file_at(
                        &repo_root, &c.commit, &file_rel,
                    ) {
                        if let Some(ts) = parse_status_from_yaml(&content) {
                            snaps.push((c.date, c.commit.clone(), Some(ts)));
                        } else {
                            snaps.push((c.date, c.commit.clone(), None));
                        }
                    }
                }
                if snaps.is_empty() {
                    match renderer.format {
                        crate::output::OutputFormat::Json => {
                            let obj = serde_json::json!({
                                "status": "ok",
                                "action": "stats.time_in_status",
                                "since": since_dt.to_rfc3339(),
                                "until": until_dt.to_rfc3339(),
                                "project": resolved_project,
                                "count": 0,
                                "items": Vec::<serde_json::Value>::new(),
                            });
                            renderer.emit_raw_stdout(&obj.to_string());
                        }
                        _ => renderer.emit_success("No status durations in the selected window."),
                    }
                    return Ok(());
                }
                snaps.sort_by(|a, b| a.0.cmp(&b.0));

                let mut current_status: Option<crate::types::TaskStatus> = None;
                for (dt, _sha, st) in snaps.iter() {
                    if *dt <= since_dt {
                        if st.is_some() {
                            current_status = st.clone();
                        }
                    } else {
                        break;
                    }
                }
                if current_status.is_none() {
                    current_status = snaps.iter().find_map(|(_, _, s)| s.clone());
                }

                use std::collections::BTreeMap;
                let mut durations: BTreeMap<String, i64> = BTreeMap::new();
                let mut cursor = since_dt.max(snaps.first().map(|s| s.0).unwrap_or(since_dt));
                for (dt, _sha, st) in snaps.into_iter() {
                    if dt <= since_dt {
                        continue;
                    }
                    if dt > until_dt {
                        break;
                    }
                    let end = dt;
                    if let Some(ref s) = current_status {
                        let key = s.to_string();
                        let diff = end.signed_duration_since(cursor);
                        let secs = diff.num_seconds().max(0);
                        *durations.entry(key).or_insert(0) += secs;
                    }
                    if let Some(s) = st {
                        current_status = Some(s);
                    }
                    cursor = end;
                }
                if cursor < until_dt {
                    if let Some(ref s) = current_status {
                        let key = s.to_string();
                        let diff = until_dt.signed_duration_since(cursor);
                        let secs = diff.num_seconds().max(0);
                        *durations.entry(key).or_insert(0) += secs;
                    }
                }

                let total_seconds: i64 = durations.values().sum();
                let items: Vec<_> = durations
                    .into_iter()
                    .map(|(status, seconds)| {
                        let hours = (seconds as f64)/3600.0;
                        let percent = if total_seconds > 0 { (seconds as f64) / (total_seconds as f64) } else { 0.0 };
                        serde_json::json!({ "status": status, "seconds": seconds, "hours": hours, "percent": percent })
                    })
                    .collect();

                match renderer.format {
                    crate::output::OutputFormat::Json => {
                        let obj = serde_json::json!({
                            "status": "ok",
                            "action": "stats.time_in_status",
                            "since": since_dt.to_rfc3339(),
                            "until": until_dt.to_rfc3339(),
                            "project": resolved_project,
                            "count": if items.is_empty() { 0 } else { 1 },
                            "items": if items.is_empty() { vec![] } else { vec![serde_json::json!({
                                "id": full_task_id,
                                "total_seconds": total_seconds,
                                "total_hours": (total_seconds as f64)/3600.0,
                                "items": items
                            })] },
                        });
                        renderer.emit_raw_stdout(&obj.to_string());
                    }
                    _ => {
                        if items.is_empty() {
                            renderer.emit_success("No status durations in the selected window.");
                        } else {
                            renderer.emit_raw_stdout(&full_task_id);
                            for it in items {
                                let st = it["status"].as_str().unwrap_or("?");
                                let hrs = it["hours"].as_f64().unwrap_or(0.0);
                                let pct = it["percent"].as_f64().unwrap_or(0.0) * 100.0;
                                renderer.emit_raw_stdout(&format!(
                                    "  {:>12}: {:.2}h ({:.1}%)",
                                    st, hrs, pct
                                ));
                            }
                            renderer.emit_raw_stdout(&format!(
                                "  {:>12}: {:.2}h",
                                "Total",
                                (total_seconds as f64) / 3600.0
                            ));
                        }
                    }
                }
                Ok(())
            }
            StatsAction::Effort {
                by,
                r#where,
                unit,
                limit,
                global,
                since,
                until,
                transitions,
            } => {
                // Determine scope (project or global)
                let scope_project = if global {
                    None
                } else {
                    project
                        .map(|p| resolve_project_input(p, resolver.path.as_path()))
                        .or_else(|| Some(crate::project::get_effective_project_name(resolver)))
                };

                // Load tasks snapshot
                let storage = crate::storage::manager::Storage::new(resolver.path.clone());
                let filter = crate::api_types::TaskListFilter {
                    status: Vec::new(),
                    priority: Vec::new(),
                    task_type: Vec::new(),
                    project: scope_project.clone(),
                    category: None,
                    tags: Vec::new(),
                    text_query: None,
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
                    // Best-effort status extractor that tolerates mixed-case enum values elsewhere
                    // in the YAML (e.g., priority in UPPERCASE). Falls back to reading only the
                    // `status` field when full Task deserialization fails.
                    fn parse_status_from_yaml(content: &str) -> Option<crate::types::TaskStatus> {
                        fn parse_status_str_tolerant(s: &str) -> Option<crate::types::TaskStatus> {
                            let norm = s.to_ascii_lowercase().replace(['_', '-'], "");
                            match norm.as_str() {
                                "todo" => Some(crate::types::TaskStatus::Todo),
                                "inprogress" => Some(crate::types::TaskStatus::InProgress),
                                "verify" => Some(crate::types::TaskStatus::Verify),
                                "blocked" => Some(crate::types::TaskStatus::Blocked),
                                "done" => Some(crate::types::TaskStatus::Done),
                                _ => None,
                            }
                        }
                        // Try strict first
                        if let Ok(task) =
                            serde_yaml::from_str::<crate::storage::task::Task>(content)
                        {
                            return Some(task.status);
                        }
                        // Tolerant fallback: read just `status` as a string
                        if let Ok(val) = serde_yaml::from_str::<serde_yaml::Value>(content) {
                            if let Some(s) = val.get("status").and_then(|v| match v {
                                serde_yaml::Value::String(s) => Some(s.clone()),
                                _ => None,
                            }) {
                                if let Some(ts) = parse_status_str_tolerant(&s) {
                                    return Some(ts);
                                }
                                return s.parse::<crate::types::TaskStatus>().ok();
                            }
                        }
                        None
                    }

                    let (since_dt, until_dt) =
                        crate::utils::time::parse_since_until(since.as_deref(), until.as_deref())?;
                    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
                    let repo_root = match crate::utils::git::find_repo_root(&cwd) {
                        Some(p) => p,
                        None => {
                            return Err(
                                "Not in a git repository; --transitions requires git history"
                                    .into(),
                            );
                        }
                    };
                    {
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
                    let repo_root_real =
                        std::fs::canonicalize(&repo_root).unwrap_or(repo_root.clone());
                    let tasks_abs_real =
                        std::fs::canonicalize(&tasks_abs).unwrap_or(tasks_abs.clone());
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
                    {
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
                        if parts.len() < 2 { return false; }
                        let project = parts[0];
                        let num = parts[1];
                        let file_rel = tasks_rel.join(project).join(format!("{}.yml", num));
                        let mut commits = match crate::services::audit_service::AuditService::list_commits_for_file(&repo_root_real, &file_rel) {
                            Ok(c) => c,
                            Err(_) => return false,
                        };
                        {
                            use std::fs::OpenOptions;
                            use std::io::Write;
                            if let Ok(mut f) = OpenOptions::new().create(true).append(true).open("/tmp/lotar_transitions_debug.log") {
                                let _ = writeln!(f, "[FILE] id={}, rel={}, commits={}", id, file_rel.to_string_lossy(), commits.len());
                            }
                        }
                        if commits.is_empty() { return false; }
                        // Process in chronological order and seed baseline from before window
                        commits.sort_by(|a, b| a.date.cmp(&b.date));
                        let mut prev_status: Option<String> = None;
                        for c in commits {
                            if c.date > until_dt { break; }
                            if let Ok(content) = crate::services::audit_service::AuditService::show_file_at(&repo_root_real, &c.commit, &file_rel) {
                                if let Some(ts) = parse_status_from_yaml(&content) {
                                    let curr_status = ts.to_string();
                                    {
                                        use std::fs::OpenOptions;
                                        use std::io::Write;
                                        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open("/tmp/lotar_transitions_debug.log") {
                                            let _ = writeln!(f, "  - commit @{} status={} (prev={:?})", c.date.to_rfc3339(), curr_status, prev_status);
                                        }
                                    }
                                    if c.date >= since_dt && c.date <= until_dt {
                                        // Detect transition into target within the window
                                        if prev_status.as_deref() != Some(curr_status.as_str()) && curr_status == trans_status {
                                            {
                                                use std::fs::OpenOptions;
                                                use std::io::Write;
                                                if let Ok(mut f) = OpenOptions::new().create(true).append(true).open("/tmp/lotar_transitions_debug.log") {
                                                    let _ = writeln!(f, "  -> MATCH: id={} transitioned into {}", id, curr_status);
                                                }
                                            }
                                            return true;
                                        }
                                    }
                                    prev_status = Some(curr_status);
                                }
                            }
                        }
                        false
                    });
                    {
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
                                if let Ok(files) = fs::read_dir(&p) {
                                    for file in files.flatten() {
                                        let fpath = file.path();
                                        if fpath.extension().and_then(|e| e.to_str()) != Some("yml")
                                        {
                                            continue;
                                        }
                                        let stem = match fpath.file_stem().and_then(|s| s.to_str())
                                        {
                                            Some(s) => s,
                                            None => continue,
                                        };
                                        let num: u64 = match stem.parse() {
                                            Ok(n) => n,
                                            Err(_) => continue,
                                        };
                                        let id = format!("{}-{}", project_folder, num);
                                        let file_rel = tasks_rel
                                            .join(&project_folder)
                                            .join(format!("{}.yml", num));
                                        let mut commits = crate::services::audit_service::AuditService::list_commits_for_file(&repo_root_real, &file_rel).unwrap_or_default();
                                        if commits.is_empty() {
                                            continue;
                                        }
                                        commits.sort_by(|a, b| a.date.cmp(&b.date));
                                        let mut prev_status: Option<String> = None;
                                        let mut is_match = false;
                                        for c in commits {
                                            if c.date > until_dt {
                                                break;
                                            }
                                            if let Ok(content) = crate::services::audit_service::AuditService::show_file_at(&repo_root_real, &c.commit, &file_rel) {
                                                if let Some(ts) = parse_status_from_yaml(&content) {
                                                    let curr_status = ts.to_string();
                                                    if c.date >= since_dt && c.date <= until_dt
                                                        && prev_status.as_deref() != Some(curr_status.as_str())
                                                        && curr_status == trans_status
                                                    {
                                                        is_match = true;
                                                        break;
                                                    }
                                                    prev_status = Some(curr_status);
                                                }
                                            }
                                        }
                                        if !is_match {
                                            continue;
                                        }
                                        // Build a minimal TaskDTO by tolerantly reading current YAML
                                        let abs_file = tasks_abs_real
                                            .join(&project_folder)
                                            .join(format!("{}.yml", num));
                                        let (
                                            title,
                                            assignee,
                                            effort,
                                            priority,
                                            task_type,
                                            status,
                                            created,
                                            modified,
                                            category,
                                            reporter,
                                            tags,
                                            custom_fields,
                                            relationships,
                                            comments,
                                        ) = (|| {
                                            let content =
                                                fs::read_to_string(&abs_file).unwrap_or_default();
                                            if let Ok(task) =
                                                serde_yaml::from_str::<crate::storage::task::Task>(
                                                    &content,
                                                )
                                            {
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
                                                    category: task.category,
                                                    tags: task.tags,
                                                    relationships: task.relationships,
                                                    comments: task.comments,
                                                    custom_fields: task.custom_fields,
                                                };
                                                return (
                                                    dto.title,
                                                    dto.assignee,
                                                    dto.effort,
                                                    dto.priority,
                                                    dto.task_type,
                                                    dto.status,
                                                    dto.created,
                                                    dto.modified,
                                                    dto.category,
                                                    dto.reporter,
                                                    dto.tags,
                                                    dto.custom_fields,
                                                    dto.relationships,
                                                    dto.comments,
                                                );
                                            }
                                            // Tolerant parse via generic YAML
                                            let mut title = String::new();
                                            let mut assignee: Option<String> = None;
                                            let mut effort: Option<String> = None;
                                            let mut priority = crate::types::Priority::default();
                                            let mut task_type = crate::types::TaskType::default();
                                            let mut status = crate::types::TaskStatus::default();
                                            let mut created = String::new();
                                            let mut modified = String::new();
                                            let mut category: Option<String> = None;
                                            let mut reporter: Option<String> = None;
                                            let mut tags: Vec<String> = Vec::new();
                                            let relationships =
                                                crate::types::TaskRelationships::default();
                                            let comments: Vec<crate::types::TaskComment> =
                                                Vec::new();
                                            let custom_fields: crate::types::CustomFields =
                                                Default::default();
                                            if let Ok(val) =
                                                serde_yaml::from_str::<serde_yaml::Value>(&content)
                                            {
                                                if let Some(s) =
                                                    val.get("title").and_then(|v| v.as_str())
                                                {
                                                    title = s.to_string();
                                                }
                                                if let Some(s) =
                                                    val.get("assignee").and_then(|v| v.as_str())
                                                {
                                                    assignee = Some(s.to_string());
                                                }
                                                if let Some(s) =
                                                    val.get("effort").and_then(|v| v.as_str())
                                                {
                                                    effort = Some(s.to_string());
                                                }
                                                if let Some(s) =
                                                    val.get("priority").and_then(|v| v.as_str())
                                                {
                                                    use std::str::FromStr;
                                                    priority = crate::types::Priority::from_str(
                                                        &s.to_lowercase(),
                                                    )
                                                    .unwrap_or_default();
                                                }
                                                if let Some(s) =
                                                    val.get("task_type").and_then(|v| v.as_str())
                                                {
                                                    use std::str::FromStr;
                                                    task_type = crate::types::TaskType::from_str(s)
                                                        .unwrap_or_default();
                                                }
                                                if let Some(s) =
                                                    val.get("status").and_then(|v| v.as_str())
                                                {
                                                    use std::str::FromStr;
                                                    status = crate::types::TaskStatus::from_str(s)
                                                        .unwrap_or_default();
                                                }
                                                if let Some(s) =
                                                    val.get("created").and_then(|v| v.as_str())
                                                {
                                                    created = s.to_string();
                                                }
                                                if let Some(s) =
                                                    val.get("modified").and_then(|v| v.as_str())
                                                {
                                                    modified = s.to_string();
                                                }
                                                if let Some(s) =
                                                    val.get("category").and_then(|v| v.as_str())
                                                {
                                                    category = Some(s.to_string());
                                                }
                                                if let Some(s) =
                                                    val.get("reporter").and_then(|v| v.as_str())
                                                {
                                                    reporter = Some(s.to_string());
                                                }
                                                if let Some(arr) =
                                                    val.get("tags").and_then(|v| v.as_sequence())
                                                {
                                                    tags = arr
                                                        .iter()
                                                        .filter_map(|v| {
                                                            v.as_str().map(|s| s.to_string())
                                                        })
                                                        .collect();
                                                }
                                            }
                                            (
                                                title,
                                                assignee,
                                                effort,
                                                priority,
                                                task_type,
                                                status,
                                                created,
                                                modified,
                                                category,
                                                reporter,
                                                tags,
                                                custom_fields,
                                                relationships,
                                                comments,
                                            )
                                        })();
                                        let dto = crate::api_types::TaskDTO {
                                            id: id.clone(),
                                            title,
                                            status,
                                            priority,
                                            task_type,
                                            reporter,
                                            assignee,
                                            created,
                                            modified,
                                            due_date: None,
                                            effort,
                                            subtitle: None,
                                            description: None,
                                            category,
                                            tags,
                                            relationships,
                                            comments,
                                            custom_fields,
                                        };
                                        matched.push((id, dto));
                                    }
                                }
                            }
                        }
                        if let Ok(mut f) = std::fs::OpenOptions::new()
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
                    let raw = key.trim();
                    let k = raw.to_lowercase();
                    // Prefer built-ins first if the key collides
                    if let Some(canon) = crate::utils::fields::is_reserved_field(raw) {
                        match canon {
                            "assignee" => {
                                return Some(vec![t.assignee.clone().unwrap_or_default()]);
                            }
                            "reporter" => {
                                return Some(vec![t.reporter.clone().unwrap_or_default()]);
                            }
                            "type" => return Some(vec![t.task_type.to_string()]),
                            "status" => return Some(vec![t.status.to_string()]),
                            "priority" => return Some(vec![t.priority.to_string()]),
                            "project" => {
                                return Some(vec![id.split('-').next().unwrap_or("").to_string()]);
                            }
                            "category" => {
                                return Some(vec![t.category.clone().unwrap_or_default()]);
                            }
                            "tags" => return Some(t.tags.clone()),
                            _ => {}
                        }
                    }
                    if let Some(rest) = k.strip_prefix("field:") {
                        let name = rest.trim();
                        let v = t.custom_fields.get(name)?;
                        return Some(vec![super_key_from_custom(v)]);
                    }
                    // Treat as plain custom field if declared in config or wildcard
                    if cfg.custom_fields.has_wildcard()
                        || cfg
                            .custom_fields
                            .values
                            .iter()
                            .any(|v| v.eq_ignore_ascii_case(raw))
                    {
                        if let Some(v) = t.custom_fields.get(raw) {
                            return Some(vec![super_key_from_custom(v)]);
                        }
                        // case-insensitive fallback
                        let lname = raw.to_lowercase();
                        if let Some((_, v)) = t
                            .custom_fields
                            .iter()
                            .find(|(k, _)| k.to_lowercase() == lname)
                        {
                            return Some(vec![super_key_from_custom(v)]);
                        }
                    }
                    None
                };

                fn super_key_from_custom(v: &crate::types::CustomFieldValue) -> String {
                    #[cfg(feature = "schema")]
                    {
                        match v {
                            serde_json::Value::Null => "".to_string(),
                            serde_json::Value::Bool(b) => b.to_string(),
                            serde_json::Value::Number(n) => n.to_string(),
                            serde_json::Value::String(s) => s.clone(),
                            serde_json::Value::Array(_) => "[array]".to_string(),
                            serde_json::Value::Object(_) => "{object}".to_string(),
                        }
                    }
                    #[cfg(not(feature = "schema"))]
                    {
                        match v {
                            serde_yaml::Value::Null => "".to_string(),
                            serde_yaml::Value::Bool(b) => b.to_string(),
                            serde_yaml::Value::Number(n) => n.to_string(),
                            serde_yaml::Value::String(s) => s.clone(),
                            serde_yaml::Value::Sequence(_) => "[array]".to_string(),
                            serde_yaml::Value::Mapping(_) => "{object}".to_string(),
                            _ => "other".to_string(),
                        }
                    }
                }

                // Parse where filters as (key -> allowed set); simple equality only for now
                use std::collections::{BTreeMap, BTreeSet};
                let mut filters: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
                for (k, v) in r#where.into_iter() {
                    // Resolve '@me' to current username for assignee filter
                    let filter_value = if k.eq_ignore_ascii_case("assignee") && v == "@me" {
                        // Use crate::utils::get_current_username or similar
                        std::env::var("USER").unwrap_or(v)
                    } else {
                        v
                    };
                    filters.entry(k).or_default().insert(filter_value);
                }

                // Aggregate: keep both time hours and points totals to support unit modes
                let mut agg: BTreeMap<String, (f64, f64, usize)> = BTreeMap::new(); // (hours, points, count)
                for (id, t) in tasks {
                    // Apply filters
                    let mut passes = true;
                    // Load config once per iteration to resolve custom field keys
                    let cfg_mgr =
                        crate::config::manager::ConfigManager::new_manager_with_tasks_dir_readonly(
                            &resolver.path,
                        )
                        .map_err(|e| format!("Failed to load config: {}", e))?;
                    let cfg = cfg_mgr.get_resolved_config();
                    for (fk, allowed) in &filters {
                        if let Some(vals) = resolve_group_key(&id, &t, fk, cfg)
                            .map(|vs| vs.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>())
                        {
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
                                crate::utils::effort::EffortKind::TimeHours(h) => {
                                    (h, 0.0, "hours".to_string())
                                }
                                crate::utils::effort::EffortKind::Points(p) => {
                                    (0.0, p, "points".to_string())
                                }
                            },
                            Err(_) => (0.0, 0.0, "invalid".to_string()),
                        }
                    } else {
                        (0.0, 0.0, "none".to_string())
                    };

                    // Use literal assignee value for grouping and filtering
                    // Config for resolving custom field grouping key
                    let cfg_mgr =
                        crate::config::manager::ConfigManager::new_manager_with_tasks_dir_readonly(
                            &resolver.path,
                        )
                        .map_err(|e| format!("Failed to load config: {}", e))?;
                    let cfg = cfg_mgr.get_resolved_config();
                    let mut keys =
                        resolve_group_key(&id, &t, &by, cfg).unwrap_or_else(|| vec![String::new()]);
                    if by.trim().to_lowercase() == "assignee" {
                        if let Some(a) = &t.assignee {
                            keys = vec![a.clone()];
                        }
                    }
                    let keys = if keys.is_empty() {
                        vec![String::new()]
                    } else {
                        keys
                    };
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
                        obj["auto_value"] =
                            serde_json::json!(if hours > 0.0 { hours } else { points });
                        obj["auto_unit"] =
                            serde_json::json!(if hours > 0.0 { "hours" } else { "points" });
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
                        renderer.emit_raw_stdout(&obj.to_string());
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
                                renderer
                                    .emit_raw_stdout(&format!("{:>8}{}  {}", val_str, suffix, key));
                            }
                        }
                    }
                }
                Ok(())
            }
            StatsAction::CommentsTop { limit, global } => {
                let scope_project = if global {
                    None
                } else {
                    project
                        .map(|p| resolve_project_input(p, resolver.path.as_path()))
                        .or_else(|| Some(crate::project::get_effective_project_name(resolver)))
                };
                let storage = crate::storage::manager::Storage::new(resolver.path.clone());
                let filter = crate::api_types::TaskListFilter {
                    status: Vec::new(),
                    priority: Vec::new(),
                    task_type: Vec::new(),
                    project: scope_project.clone(),
                    category: None,
                    tags: Vec::new(),
                    text_query: None,
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
                        renderer.emit_raw_stdout(&obj.to_string());
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
            StatsAction::CommentsByAuthor { limit, global } => {
                let scope_project = if global {
                    None
                } else {
                    project
                        .map(|p| resolve_project_input(p, resolver.path.as_path()))
                        .or_else(|| Some(crate::project::get_effective_project_name(resolver)))
                };
                let storage = crate::storage::manager::Storage::new(resolver.path.clone());
                let filter = crate::api_types::TaskListFilter {
                    status: Vec::new(),
                    priority: Vec::new(),
                    task_type: Vec::new(),
                    project: scope_project.clone(),
                    category: None,
                    tags: Vec::new(),
                    text_query: None,
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
                        renderer.emit_raw_stdout(&obj.to_string());
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
            StatsAction::CustomKeys { limit, global } => {
                let scope_project = if global {
                    None
                } else {
                    project
                        .map(|p| resolve_project_input(p, resolver.path.as_path()))
                        .or_else(|| Some(crate::project::get_effective_project_name(resolver)))
                };
                let storage = crate::storage::manager::Storage::new(resolver.path.clone());
                let filter = crate::api_types::TaskListFilter {
                    status: Vec::new(),
                    priority: Vec::new(),
                    task_type: Vec::new(),
                    project: scope_project.clone(),
                    category: None,
                    tags: Vec::new(),
                    text_query: None,
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
                        renderer.emit_raw_stdout(&obj.to_string());
                    }
                    _ => {
                        if rows.is_empty() {
                            renderer.emit_success("No custom fields found.");
                        } else {
                            for r in &rows {
                                let k = r["key"].as_str().unwrap_or("");
                                let n = r["count"].as_u64().unwrap_or(0);
                                renderer.emit_raw_stdout(&format!("{:>4}  {}", n, k));
                            }
                        }
                    }
                }
                Ok(())
            }
            StatsAction::CustomField {
                name,
                limit,
                global,
            } => {
                let scope_project = if global {
                    None
                } else {
                    project
                        .map(|p| resolve_project_input(p, resolver.path.as_path()))
                        .or_else(|| Some(crate::project::get_effective_project_name(resolver)))
                };
                let storage = crate::storage::manager::Storage::new(resolver.path.clone());
                let filter = crate::api_types::TaskListFilter {
                    status: Vec::new(),
                    priority: Vec::new(),
                    task_type: Vec::new(),
                    project: scope_project.clone(),
                    category: None,
                    tags: Vec::new(),
                    text_query: None,
                };
                let tasks = crate::services::task_service::TaskService::list(&storage, &filter);
                use std::collections::BTreeMap;
                let mut counts: BTreeMap<String, u64> = BTreeMap::new();
                for (_id, t) in tasks.into_iter() {
                    if let Some(v) = t.custom_fields.get(&name) {
                        let key = custom_value_key(v);
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
                        renderer.emit_raw_stdout(&obj.to_string());
                    }
                    _ => {
                        if rows.is_empty() {
                            renderer.emit_success("No values found for field.");
                        } else {
                            for r in &rows {
                                let k = r["value"].as_str().unwrap_or("");
                                let n = r["count"].as_u64().unwrap_or(0);
                                renderer.emit_raw_stdout(&format!("{:>4}  {}", n, k));
                            }
                        }
                    }
                }
                Ok(())
            }
            StatsAction::TimeInStatus {
                since,
                until,
                limit,
                global,
            } => {
                // Resolve time window
                let (since_dt, until_dt) =
                    crate::utils::time::parse_since_until(since.as_deref(), until.as_deref())?;

                // Scope
                let scope_project = if global {
                    None
                } else {
                    project
                        .map(|p| resolve_project_input(p, resolver.path.as_path()))
                        .or_else(|| Some(crate::project::get_effective_project_name(resolver)))
                };

                // Repo root
                let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
                let repo_root = match crate::utils::git::find_repo_root(&cwd) {
                    Some(p) => p,
                    None => {
                        match renderer.format {
                            crate::output::OutputFormat::Json => {
                                let obj = serde_json::json!({
                                    "status": "ok",
                                    "action": "stats.time_in_status",
                                    "since": since_dt.to_rfc3339(),
                                    "until": until_dt.to_rfc3339(),
                                    "global": global,
                                    "project": scope_project,
                                    "items": Vec::<serde_json::Value>::new(),
                                    "note": "Not in a git repository; returning empty set",
                                });
                                renderer.emit_raw_stdout(&obj.to_string());
                            }
                            _ => renderer
                                .emit_warning("Not in a git repository; returning empty set"),
                        }
                        return Ok(());
                    }
                };
                // Canonicalize paths to avoid /var vs /private/var issues
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

                // Walk tasks to collect files in-scope
                let mut task_files: Vec<(String, std::path::PathBuf)> = Vec::new();
                let base = repo_root.join(&tasks_rel);
                let walker_root = if let Some(p) = scope_project.as_deref() {
                    base.join(p)
                } else {
                    base.clone()
                };
                if walker_root.exists() {
                    let mut stack = vec![walker_root.clone()];
                    while let Some(dir) = stack.pop() {
                        if let Ok(read) = std::fs::read_dir(&dir) {
                            for entry in read.flatten() {
                                let p = entry.path();
                                if p.is_dir() {
                                    stack.push(p);
                                } else if p.extension().and_then(|e| e.to_str()) == Some("yml") {
                                    if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                                        if let Ok(num) = stem.parse::<u64>() {
                                            let project = p
                                                .parent()
                                                .and_then(|q| q.file_name())
                                                .and_then(|s| s.to_str())
                                                .unwrap_or("");
                                            let id = format!("{}-{}", project, num);
                                            // Compute path relative to repo root for git show
                                            if let Ok(rel) = p.strip_prefix(&repo_root) {
                                                task_files.push((id, rel.to_path_buf()));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // For each task file, list commits in window and build status timeline
                // Then compute durations per status on [since, until]
                let mut results: Vec<serde_json::Value> = Vec::new();
                for (id, file_rel) in task_files.into_iter() {
                    // List all commits for the file, then filter by date window
                    let commits =
                        crate::services::audit_service::AuditService::list_commits_for_file(
                            &repo_root, &file_rel,
                        )?;
                    if commits.is_empty() {
                        continue;
                    }
                    // Keep commits whose date intersects the window when building snapshots sequence
                    // We will consider consecutive snapshots around the window edges too
                    // Build snapshots as (date, commit, status)
                    let mut snaps: Vec<(
                        chrono::DateTime<chrono::Utc>,
                        String,
                        Option<crate::types::TaskStatus>,
                    )> = Vec::new();
                    // Helper: tolerant status parse from YAML content
                    fn parse_status_from_yaml(content: &str) -> Option<crate::types::TaskStatus> {
                        fn parse_status_str_tolerant(s: &str) -> Option<crate::types::TaskStatus> {
                            let norm = s.to_ascii_lowercase().replace(['_', '-'], "");
                            match norm.as_str() {
                                "todo" => Some(crate::types::TaskStatus::Todo),
                                "inprogress" => Some(crate::types::TaskStatus::InProgress),
                                "verify" => Some(crate::types::TaskStatus::Verify),
                                "blocked" => Some(crate::types::TaskStatus::Blocked),
                                "done" => Some(crate::types::TaskStatus::Done),
                                _ => None,
                            }
                        }
                        if let Ok(task) =
                            serde_yaml::from_str::<crate::storage::task::Task>(content)
                        {
                            return Some(task.status);
                        }
                        if let Ok(val) = serde_yaml::from_str::<serde_yaml::Value>(content) {
                            if let Some(s) = val.get("status").and_then(|v| match v {
                                serde_yaml::Value::String(s) => Some(s.clone()),
                                _ => None,
                            }) {
                                if let Some(ts) = parse_status_str_tolerant(&s) {
                                    return Some(ts);
                                }
                                return s.parse::<crate::types::TaskStatus>().ok();
                            }
                        }
                        None
                    }

                    for c in &commits {
                        // Only consider up to 'until'
                        if c.date > until_dt {
                            continue;
                        }
                        if let Ok(content) =
                            crate::services::audit_service::AuditService::show_file_at(
                                &repo_root, &c.commit, &file_rel,
                            )
                        {
                            if let Some(ts) = parse_status_from_yaml(&content) {
                                snaps.push((c.date, c.commit.clone(), Some(ts)));
                            } else {
                                // tolerate parse failures
                                snaps.push((c.date, c.commit.clone(), None));
                            }
                        }
                    }
                    if snaps.is_empty() {
                        continue;
                    }
                    // Sort oldest->newest by date
                    snaps.sort_by(|a, b| a.0.cmp(&b.0));

                    // Determine starting status at 'since_dt': use latest snapshot before since_dt
                    let mut current_status: Option<crate::types::TaskStatus> = None;
                    for (dt, _sha, st) in snaps.iter() {
                        if *dt <= since_dt {
                            if st.is_some() {
                                current_status = st.clone();
                            }
                        } else {
                            break;
                        }
                    }
                    // Fallback: if none before since, use first available
                    if current_status.is_none() {
                        current_status = snaps.iter().find_map(|(_, _, s)| s.clone());
                    }

                    // Walk events within (since..=until), accumulating durations per status
                    use std::collections::BTreeMap;
                    let mut durations: BTreeMap<String, i64> = BTreeMap::new(); // seconds
                    let mut cursor = since_dt.max(snaps.first().map(|s| s.0).unwrap_or(since_dt));
                    for (dt, _sha, st) in snaps.into_iter() {
                        if dt <= since_dt {
                            continue;
                        }
                        if dt > until_dt {
                            break;
                        }
                        let end = dt;
                        if let Some(ref s) = current_status {
                            let key = s.to_string();
                            let diff = end.signed_duration_since(cursor);
                            let secs = diff.num_seconds().max(0);
                            *durations.entry(key).or_insert(0) += secs;
                        }
                        if let Some(s) = st {
                            current_status = Some(s);
                        }
                        cursor = end;
                    }
                    // Tail from last cursor to until
                    if cursor < until_dt {
                        if let Some(s) = current_status {
                            let key = s.to_string();
                            let diff = until_dt.signed_duration_since(cursor);
                            let secs = diff.num_seconds().max(0);
                            *durations.entry(key).or_insert(0) += secs;
                        }
                    }

                    // Skip empty
                    if durations.is_empty() {
                        continue;
                    }
                    let total_seconds: i64 = durations.values().sum();
                    let items: Vec<_> = durations
                        .into_iter()
                        .map(|(status, seconds)| {
                            let hours = (seconds as f64) / 3600.0;
                            let percent = if total_seconds > 0 {
                                (seconds as f64) / (total_seconds as f64)
                            } else {
                                0.0
                            };
                            serde_json::json!({
                                "status": status,
                                "seconds": seconds,
                                "hours": hours,
                                "percent": percent,
                            })
                        })
                        .collect();
                    results.push(serde_json::json!({
                        "id": id,
                        "total_seconds": total_seconds,
                        "total_hours": (total_seconds as f64)/3600.0,
                        "items": items,
                    }));
                }

                // Sort by total seconds desc and limit
                results.sort_by(|a, b| {
                    let sum = |v: &serde_json::Value| -> i64 {
                        v["items"]
                            .as_array()
                            .unwrap_or(&Vec::new())
                            .iter()
                            .map(|x| x["seconds"].as_i64().unwrap_or(0))
                            .sum()
                    };
                    sum(b).cmp(&sum(a))
                });
                let limited: Vec<_> = results.into_iter().take(limit).collect();

                // Render
                match renderer.format {
                    crate::output::OutputFormat::Json => {
                        let obj = serde_json::json!({
                            "status": "ok",
                            "action": "stats.time_in_status",
                            "since": since_dt.to_rfc3339(),
                            "until": until_dt.to_rfc3339(),
                            "global": global,
                            "project": scope_project,
                            "count": limited.len(),
                            "items": limited,
                        });
                        renderer.emit_raw_stdout(&obj.to_string());
                    }
                    _ => {
                        if limited.is_empty() {
                            renderer.emit_success("No status durations in the selected window.");
                        } else {
                            for row in &limited {
                                let id = row["id"].as_str().unwrap_or("-");
                                renderer.emit_raw_stdout(id);
                                if let Some(arr) = row["items"].as_array() {
                                    for it in arr {
                                        let st = it["status"].as_str().unwrap_or("?");
                                        let hrs = it["hours"].as_f64().unwrap_or(0.0);
                                        let pct = it["percent"].as_f64().unwrap_or(0.0) * 100.0;
                                        renderer.emit_raw_stdout(&format!(
                                            "  {:>12}: {:.2}h ({:.1}%)",
                                            st, hrs, pct
                                        ));
                                    }
                                    let tot = row["total_hours"].as_f64().unwrap_or(0.0);
                                    renderer.emit_raw_stdout(&format!(
                                        "  {:>12}: {:.2}h",
                                        "Total", tot
                                    ));
                                }
                            }
                        }
                    }
                }
                Ok(())
            }
            StatsAction::Changed {
                since,
                until,
                author,
                limit,
                global,
            } => {
                // Resolve time window
                let (since_dt, until_dt) =
                    crate::utils::time::parse_since_until(since.as_deref(), until.as_deref())?;

                // Determine scope (project or global)
                let scope_project = if global {
                    None
                } else {
                    // Use explicit CLI project if given, otherwise default/effective project name
                    project
                        .map(|p| resolve_project_input(p, resolver.path.as_path()))
                        .or_else(|| Some(crate::project::get_effective_project_name(resolver)))
                };

                // Resolve repo root and tasks dir relative path
                let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
                let repo_root = match crate::utils::git::find_repo_root(&cwd) {
                    Some(p) => p,
                    None => {
                        // Outside a git repo: emit empty result
                        match renderer.format {
                            crate::output::OutputFormat::Json => {
                                let obj = serde_json::json!({
                                    "status": "ok",
                                    "action": "stats.changed",
                                    "since": since_dt.to_rfc3339(),
                                    "until": until_dt.to_rfc3339(),
                                    "global": global,
                                    "project": scope_project,
                                    "items": Vec::<serde_json::Value>::new(),
                                    "note": "Not in a git repository; returning empty set",
                                });
                                renderer.emit_raw_stdout(&obj.to_string());
                            }
                            _ => {
                                renderer
                                    .emit_warning("Not in a git repository; returning empty set");
                            }
                        }
                        return Ok(());
                    }
                };

                // Compute tasks path relative to repo root
                let tasks_abs = resolver.path.clone();
                let tasks_rel = if tasks_abs.starts_with(&repo_root) {
                    match tasks_abs.strip_prefix(&repo_root) {
                        Ok(p) => p.to_path_buf(),
                        Err(_) => tasks_abs.clone(),
                    }
                } else {
                    tasks_abs.clone()
                };

                // Call audit service to list changed tasks
                let items = crate::services::audit_service::AuditService::list_changed_tasks(
                    &repo_root,
                    &tasks_rel,
                    since_dt,
                    until_dt,
                    author.as_deref(),
                    scope_project.as_deref(),
                )?;

                // Apply limit
                let limited: Vec<_> = items.into_iter().take(limit).collect();

                // Render
                match renderer.format {
                    crate::output::OutputFormat::Json => {
                        let json_items: Vec<_> = limited
                            .iter()
                            .map(|i| {
                                serde_json::json!({
                                    "id": i.id,
                                    "project": i.project,
                                    "file": i.file,
                                    "last_commit": i.last_commit,
                                    "last_author": i.last_author,
                                    "last_date": i.last_date.to_rfc3339(),
                                    "commits": i.commits,
                                })
                            })
                            .collect();
                        let obj = serde_json::json!({
                            "status": "ok",
                            "action": "stats.changed",
                            "since": since_dt.to_rfc3339(),
                            "until": until_dt.to_rfc3339(),
                            "global": global,
                            "project": scope_project,
                            "count": json_items.len(),
                            "items": json_items,
                        });
                        renderer.emit_raw_stdout(&obj.to_string());
                    }
                    _ => {
                        if limited.is_empty() {
                            renderer.emit_success("No tickets changed in the selected window.");
                        } else {
                            for i in &limited {
                                renderer.emit_raw_stdout(&format!(
                                    "{}  {}  {}  {}  {}",
                                    i.last_date.to_rfc3339(),
                                    i.id,
                                    i.project,
                                    i.last_author,
                                    i.last_commit,
                                ));
                            }
                        }
                    }
                }
                Ok(())
            }
            StatsAction::Churn {
                since,
                until,
                author,
                limit,
                global,
            } => {
                let (since_dt, until_dt) =
                    crate::utils::time::parse_since_until(since.as_deref(), until.as_deref())?;

                let mut scope_project = if global {
                    None
                } else {
                    project
                        .map(|p| resolve_project_input(p, resolver.path.as_path()))
                        .or_else(|| Some(crate::project::get_effective_project_name(resolver)))
                };

                let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
                let repo_root = match crate::utils::git::find_repo_root(&cwd) {
                    Some(p) => p,
                    None => {
                        match renderer.format {
                            crate::output::OutputFormat::Json => {
                                let obj = serde_json::json!({
                                    "status": "ok",
                                    "action": "stats.churn",
                                    "since": since_dt.to_rfc3339(),
                                    "until": until_dt.to_rfc3339(),
                                    "global": global,
                                    "project": scope_project,
                                    "items": Vec::<serde_json::Value>::new(),
                                    "note": "Not in a git repository; returning empty set",
                                });
                                renderer.emit_raw_stdout(&obj.to_string());
                            }
                            _ => renderer
                                .emit_warning("Not in a git repository; returning empty set"),
                        }
                        return Ok(());
                    }
                };
                let tasks_abs = resolver.path.clone();
                // Canonicalize both to avoid macOS /var vs /private/var mismatches
                let repo_root_real = std::fs::canonicalize(&repo_root).unwrap_or(repo_root.clone());
                let tasks_abs_real = std::fs::canonicalize(&tasks_abs).unwrap_or(tasks_abs.clone());
                let tasks_rel = if tasks_abs_real.starts_with(&repo_root_real) {
                    tasks_abs_real
                        .strip_prefix(&repo_root_real)
                        .unwrap()
                        .to_path_buf()
                } else {
                    tasks_abs
                        .file_name()
                        .map(std::path::PathBuf::from)
                        .unwrap_or(tasks_abs.clone())
                };

                // If an inferred project path doesn't exist locally, widen to global to be forgiving in new repos/tests
                if let Some(ref p) = scope_project {
                    if !tasks_abs.join(p).exists() {
                        scope_project = None;
                    }
                }

                let mut items = crate::services::audit_service::AuditService::list_changed_tasks(
                    &repo_root,
                    &tasks_rel,
                    since_dt,
                    until_dt,
                    author.as_deref(),
                    scope_project.as_deref(),
                )?;
                // Sort by commits desc (churn)
                items.sort_by(|a, b| {
                    b.commits
                        .cmp(&a.commits)
                        .then(b.last_date.cmp(&a.last_date))
                });
                let limited: Vec<_> = items.into_iter().take(limit).collect();

                match renderer.format {
                    crate::output::OutputFormat::Json => {
                        let json_items: Vec<_> = limited
                            .iter()
                            .map(|i| {
                                serde_json::json!({
                                    "id": i.id,
                                    "project": i.project,
                                    "file": i.file,
                                    "last_date": i.last_date.to_rfc3339(),
                                    "commits": i.commits,
                                    "last_author": i.last_author,
                                    "last_commit": i.last_commit,
                                })
                            })
                            .collect();
                        let obj = serde_json::json!({
                            "status": "ok",
                            "action": "stats.churn",
                            "since": since_dt.to_rfc3339(),
                            "until": until_dt.to_rfc3339(),
                            "global": global,
                            "project": scope_project,
                            "count": json_items.len(),
                            "items": json_items,
                        });
                        renderer.emit_raw_stdout(&obj.to_string());
                    }
                    _ => {
                        if limited.is_empty() {
                            renderer.emit_success("No churn detected in the selected window.");
                        } else {
                            for i in &limited {
                                renderer.emit_raw_stdout(&format!(
                                    "{:>4}  {}  {}  {}",
                                    i.commits,
                                    i.id,
                                    i.project,
                                    i.last_date.to_rfc3339(),
                                ));
                            }
                        }
                    }
                }
                Ok(())
            }
            StatsAction::Authors {
                since,
                until,
                limit,
                global,
            } => {
                let (since_dt, until_dt) =
                    crate::utils::time::parse_since_until(since.as_deref(), until.as_deref())?;

                let scope_project = if global {
                    None
                } else {
                    project
                        .map(|p| resolve_project_input(p, resolver.path.as_path()))
                        .or_else(|| Some(crate::project::get_effective_project_name(resolver)))
                };

                let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
                let repo_root = match crate::utils::git::find_repo_root(&cwd) {
                    Some(p) => p,
                    None => {
                        match renderer.format {
                            crate::output::OutputFormat::Json => {
                                let obj = serde_json::json!({
                                    "status": "ok",
                                    "action": "stats.authors",
                                    "since": since_dt.to_rfc3339(),
                                    "until": until_dt.to_rfc3339(),
                                    "global": global,
                                    "project": scope_project,
                                    "items": Vec::<serde_json::Value>::new(),
                                    "note": "Not in a git repository; returning empty set",
                                });
                                renderer.emit_raw_stdout(&obj.to_string());
                            }
                            _ => renderer
                                .emit_warning("Not in a git repository; returning empty set"),
                        }
                        return Ok(());
                    }
                };
                let tasks_abs = resolver.path.clone();
                let tasks_rel = if tasks_abs.starts_with(&repo_root) {
                    tasks_abs.strip_prefix(&repo_root).unwrap().to_path_buf()
                } else {
                    tasks_abs.clone()
                };

                let items = crate::services::audit_service::AuditService::list_authors_activity(
                    &repo_root,
                    &tasks_rel,
                    since_dt,
                    until_dt,
                    scope_project.as_deref(),
                )?;
                let limited: Vec<_> = items.into_iter().take(limit).collect();

                match renderer.format {
                    crate::output::OutputFormat::Json => {
                        let json_items: Vec<_> = limited
                            .iter()
                            .map(|i| {
                                serde_json::json!({
                                    "author": i.author,
                                    "email": i.email,
                                    "commits": i.commits,
                                    "last_date": i.last_date.to_rfc3339(),
                                })
                            })
                            .collect();
                        let obj = serde_json::json!({
                            "status": "ok",
                            "action": "stats.authors",
                            "since": since_dt.to_rfc3339(),
                            "until": until_dt.to_rfc3339(),
                            "global": global,
                            "project": scope_project,
                            "count": json_items.len(),
                            "items": json_items,
                        });
                        renderer.emit_raw_stdout(&obj.to_string());
                    }
                    _ => {
                        if limited.is_empty() {
                            renderer.emit_success("No author activity in the selected window.");
                        } else {
                            for i in &limited {
                                renderer.emit_raw_stdout(&format!(
                                    "{:>4}  {}  <{}>  {}",
                                    i.commits,
                                    i.author,
                                    i.email,
                                    i.last_date.to_rfc3339(),
                                ));
                            }
                        }
                    }
                }
                Ok(())
            }
            StatsAction::Activity {
                since,
                until,
                group_by,
                limit,
                global,
            } => {
                let (since_dt, until_dt) =
                    crate::utils::time::parse_since_until(since.as_deref(), until.as_deref())?;

                let scope_project = if global {
                    None
                } else {
                    project
                        .map(|p| resolve_project_input(p, resolver.path.as_path()))
                        .or_else(|| Some(crate::project::get_effective_project_name(resolver)))
                };

                let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
                let repo_root = match crate::utils::git::find_repo_root(&cwd) {
                    Some(p) => p,
                    None => {
                        match renderer.format {
                            crate::output::OutputFormat::Json => {
                                let obj = serde_json::json!({
                                    "status": "ok",
                                    "action": "stats.activity",
                                    "since": since_dt.to_rfc3339(),
                                    "until": until_dt.to_rfc3339(),
                                    "global": global,
                                    "project": scope_project,
                                    "items": Vec::<serde_json::Value>::new(),
                                    "note": "Not in a git repository; returning empty set",
                                });
                                renderer.emit_raw_stdout(&obj.to_string());
                            }
                            _ => renderer
                                .emit_warning("Not in a git repository; returning empty set"),
                        }
                        return Ok(());
                    }
                };
                let tasks_abs = resolver.path.clone();
                let tasks_rel = if tasks_abs.starts_with(&repo_root) {
                    tasks_abs.strip_prefix(&repo_root).unwrap().to_path_buf()
                } else {
                    tasks_abs.clone()
                };

                let gb = match group_by.as_str() {
                    "author" => crate::services::audit_service::GroupBy::Author,
                    "day" => crate::services::audit_service::GroupBy::Day,
                    "week" => crate::services::audit_service::GroupBy::Week,
                    "project" => crate::services::audit_service::GroupBy::Project,
                    _ => crate::services::audit_service::GroupBy::Day,
                };

                let items = crate::services::audit_service::AuditService::list_activity(
                    &repo_root,
                    &tasks_rel,
                    since_dt,
                    until_dt,
                    gb,
                    scope_project.as_deref(),
                )?;
                let limited: Vec<_> = items.into_iter().take(limit).collect();

                match renderer.format {
                    crate::output::OutputFormat::Json => {
                        let json_items: Vec<_> = limited
                            .iter()
                            .map(|i| {
                                serde_json::json!({
                                    "key": i.key,
                                    "count": i.count,
                                    "last_date": i.last_date.to_rfc3339(),
                                })
                            })
                            .collect();
                        let obj = serde_json::json!({
                            "status": "ok",
                            "action": "stats.activity",
                            "group_by": group_by,
                            "since": since_dt.to_rfc3339(),
                            "until": until_dt.to_rfc3339(),
                            "global": global,
                            "project": scope_project,
                            "count": json_items.len(),
                            "items": json_items,
                        });
                        renderer.emit_raw_stdout(&obj.to_string());
                    }
                    _ => {
                        if limited.is_empty() {
                            renderer.emit_success("No activity in the selected window.");
                        } else {
                            for i in &limited {
                                renderer.emit_raw_stdout(&format!(
                                    "{:>6}  {}  {}",
                                    i.count,
                                    i.key,
                                    i.last_date.to_rfc3339(),
                                ));
                            }
                        }
                    }
                }
                Ok(())
            }
            StatsAction::Stale {
                threshold,
                limit,
                global,
            } => {
                // Determine scope
                let scope_project = if global {
                    None
                } else {
                    project
                        .map(|p| resolve_project_input(p, resolver.path.as_path()))
                        .or_else(|| Some(crate::project::get_effective_project_name(resolver)))
                };

                // Repo root
                let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
                let repo_root = match crate::utils::git::find_repo_root(&cwd) {
                    Some(p) => p,
                    None => {
                        match renderer.format {
                            crate::output::OutputFormat::Json => {
                                let obj = serde_json::json!({
                                    "status": "ok",
                                    "action": "stats.stale",
                                    "threshold": threshold,
                                    "global": global,
                                    "project": scope_project,
                                    "items": Vec::<serde_json::Value>::new(),
                                    "note": "Not in a git repository; returning empty set",
                                });
                                renderer.emit_raw_stdout(&obj.to_string());
                            }
                            _ => renderer
                                .emit_warning("Not in a git repository; returning empty set"),
                        }
                        return Ok(());
                    }
                };

                // tasks dir relative
                let tasks_abs = resolver.path.clone();
                let tasks_rel = if tasks_abs.starts_with(&repo_root) {
                    tasks_abs.strip_prefix(&repo_root).unwrap().to_path_buf()
                } else {
                    tasks_abs.clone()
                };

                // Parse threshold (Nd or Nw)
                fn parse_threshold_to_duration(s: &str) -> Result<chrono::Duration, String> {
                    let t = s.trim().to_lowercase();
                    if let Some(num) = t.strip_suffix('d') {
                        let n: i64 = num
                            .parse()
                            .map_err(|_| format!("Invalid threshold: {}", s))?;
                        return Ok(chrono::Duration::days(n));
                    }
                    if let Some(num) = t.strip_suffix('w') {
                        let n: i64 = num
                            .parse()
                            .map_err(|_| format!("Invalid threshold: {}", s))?;
                        return Ok(chrono::Duration::weeks(n));
                    }
                    Err(format!(
                        "Invalid threshold '{}'. Use Nd or Nw, e.g., 21d, 8w",
                        s
                    ))
                }

                let thr = parse_threshold_to_duration(&threshold)?;
                let now = chrono::Utc::now();

                // Compute last change per task by scanning full history using existing aggregator
                let since_epoch = chrono::DateTime::<chrono::Utc>::from(std::time::UNIX_EPOCH);
                let until_now = now;
                let mut items = crate::services::audit_service::AuditService::list_changed_tasks(
                    &repo_root,
                    &tasks_rel,
                    since_epoch,
                    until_now,
                    None,
                    scope_project.as_deref(),
                )?;

                // Filter by age
                items.retain(|i| (now - i.last_date) >= thr);
                // Sort by age descending (older first)
                items.sort_by(|a, b| a.last_date.cmp(&b.last_date));
                let limited: Vec<_> = items.into_iter().take(limit).collect();

                match renderer.format {
                    crate::output::OutputFormat::Json => {
                        let json_items: Vec<_> = limited
                            .iter()
                            .map(|i| {
                                serde_json::json!({
                                    "id": i.id,
                                    "project": i.project,
                                    "file": i.file,
                                    "last_date": i.last_date.to_rfc3339(),
                                    "age_days": ((now - i.last_date).num_days()),
                                })
                            })
                            .collect();
                        let obj = serde_json::json!({
                            "status": "ok",
                            "action": "stats.stale",
                            "threshold": threshold,
                            "global": global,
                            "project": scope_project,
                            "count": json_items.len(),
                            "items": json_items,
                        });
                        renderer.emit_raw_stdout(&obj.to_string());
                    }
                    _ => {
                        if limited.is_empty() {
                            renderer.emit_success("No stale tickets over the threshold.");
                        } else {
                            for i in &limited {
                                renderer.emit_raw_stdout(&format!(
                                    "{:>6}d  {}  {}  {}",
                                    (now - i.last_date).num_days(),
                                    i.id,
                                    i.project,
                                    i.last_date.to_rfc3339(),
                                ));
                            }
                        }
                    }
                }
                Ok(())
            }
            StatsAction::Tags { limit, global } => {
                // Determine scope
                let scope_project = if global {
                    None
                } else {
                    project
                        .map(|p| resolve_project_input(p, resolver.path.as_path()))
                        .or_else(|| Some(crate::project::get_effective_project_name(resolver)))
                };

                // Load all tasks (current snapshot)
                let storage = crate::storage::manager::Storage::new(resolver.path.clone());
                let filter = crate::api_types::TaskListFilter {
                    status: Vec::new(),
                    priority: Vec::new(),
                    task_type: Vec::new(),
                    project: scope_project.clone(),
                    category: None,
                    tags: Vec::new(),
                    text_query: None,
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
                        renderer.emit_raw_stdout(&obj.to_string());
                    }
                    _ => {
                        if limited.is_empty() {
                            renderer.emit_success("No tags found.");
                        } else {
                            for (tag, count) in &limited {
                                renderer.emit_raw_stdout(&format!("{:>6}  {}", count, tag));
                            }
                        }
                    }
                }
                Ok(())
            }
            StatsAction::Categories { limit, global } => {
                // Determine scope
                let scope_project = if global {
                    None
                } else {
                    project
                        .map(|p| resolve_project_input(p, resolver.path.as_path()))
                        .or_else(|| Some(crate::project::get_effective_project_name(resolver)))
                };

                // Load all tasks (current snapshot)
                let storage = crate::storage::manager::Storage::new(resolver.path.clone());
                let filter = crate::api_types::TaskListFilter {
                    status: Vec::new(),
                    priority: Vec::new(),
                    task_type: Vec::new(),
                    project: scope_project.clone(),
                    category: None,
                    tags: Vec::new(),
                    text_query: None,
                };
                let tasks = crate::services::task_service::TaskService::list(&storage, &filter);

                // Aggregate category frequencies
                use std::collections::HashMap;
                let mut freq: HashMap<String, usize> = HashMap::new();
                for (_id, t) in tasks.into_iter() {
                    if let Some(cat) = t.category {
                        let key = cat.trim().to_string();
                        if key.is_empty() {
                            continue;
                        }
                        *freq.entry(key).or_insert(0) += 1;
                    }
                }
                let mut items: Vec<(String, usize)> = freq.into_iter().collect();
                items.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                let limited: Vec<_> = items.into_iter().take(limit).collect();

                match renderer.format {
                    crate::output::OutputFormat::Json => {
                        let json_items: Vec<_> = limited
                            .iter()
                            .map(
                                |(cat, count)| serde_json::json!({"category": cat, "count": count}),
                            )
                            .collect();
                        let obj = serde_json::json!({
                            "status": "ok",
                            "action": "stats.categories",
                            "global": global,
                            "project": scope_project,
                            "count": json_items.len(),
                            "items": json_items,
                        });
                        renderer.emit_raw_stdout(&obj.to_string());
                    }
                    _ => {
                        if limited.is_empty() {
                            renderer.emit_success("No categories found.");
                        } else {
                            for (cat, count) in &limited {
                                renderer.emit_raw_stdout(&format!("{:>6}  {}", count, cat));
                            }
                        }
                    }
                }
                Ok(())
            }
            StatsAction::Distribution {
                field,
                limit,
                global,
            } => {
                // Determine scope
                let scope_project = if global {
                    None
                } else {
                    project
                        .map(|p| resolve_project_input(p, resolver.path.as_path()))
                        .or_else(|| Some(crate::project::get_effective_project_name(resolver)))
                };

                // Load tasks
                let storage = crate::storage::manager::Storage::new(resolver.path.clone());
                let filter = crate::api_types::TaskListFilter {
                    status: Vec::new(),
                    priority: Vec::new(),
                    task_type: Vec::new(),
                    project: scope_project.clone(),
                    category: None,
                    tags: Vec::new(),
                    text_query: None,
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
                        crate::cli::args::stats::StatsDistributionField::Category => {
                            let key = t.category.unwrap_or_else(|| "".to_string());
                            if key.trim().is_empty() {
                                continue;
                            }
                            *freq.entry(key).or_insert(0) += 1;
                        }
                    }
                }

                let mut items: Vec<(String, usize)> =
                    freq.into_iter().filter(|(k, _)| !k.is_empty()).collect();
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
                    crate::cli::args::stats::StatsDistributionField::Category => "category",
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
                        renderer.emit_raw_stdout(&obj.to_string());
                    }
                    _ => {
                        if limited.is_empty() {
                            renderer.emit_success("No items found for the selected field.");
                        } else {
                            for (key, count) in &limited {
                                renderer.emit_raw_stdout(&format!("{:>6}  {}", count, key));
                            }
                        }
                    }
                }
                Ok(())
            }
            StatsAction::Due {
                buckets,
                overdue,
                threshold,
                global,
            } => {
                let scope_project = if global {
                    None
                } else {
                    project
                        .map(|p| resolve_project_input(p, resolver.path.as_path()))
                        .or_else(|| Some(crate::project::get_effective_project_name(resolver)))
                };
                let storage = crate::storage::manager::Storage::new(resolver.path.clone());
                let filter = crate::api_types::TaskListFilter {
                    status: Vec::new(),
                    priority: Vec::new(),
                    task_type: Vec::new(),
                    project: scope_project.clone(),
                    category: None,
                    tags: Vec::new(),
                    text_query: None,
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
                    if let Some(due) = t.due_date {
                        if let Ok(date) = chrono::NaiveDate::parse_from_str(&due, "%Y-%m-%d") {
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
                        renderer.emit_raw_stdout(&obj.to_string());
                    }
                    _ => {
                        for (k, v) in counts {
                            renderer.emit_raw_stdout(&format!("{:>6}  {}", v, k));
                        }
                    }
                }
                Ok(())
            }
        }
    }
}
