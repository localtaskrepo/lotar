// Auto-generated from stats_handler.rs.
use crate::cli::args::stats::StatsEffortUnit;

#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    by: String,
    r#where: Vec<(String, String)>,
    unit: StatsEffortUnit,
    limit: usize,
    global: bool,
    since: Option<String>,
    until: Option<String>,
    transitions: Option<String>,
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
                    "todo" => Some(crate::types::TaskStatus::from("Todo")),
                    "inprogress" => Some(crate::types::TaskStatus::from("InProgress")),
                    "verify" => Some(crate::types::TaskStatus::from("Verify")),
                    "blocked" => Some(crate::types::TaskStatus::from("Blocked")),
                    "done" => Some(crate::types::TaskStatus::from("Done")),
                    _ => None,
                }
            }
            // Try strict first
            if let Ok(task) = serde_yaml::from_str::<crate::storage::task::Task>(content) {
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
            commits.sort_by(|a, b| a.date.cmp(&b.date));
            let mut prev_status: Option<String> = None;
            for c in commits {
                if c.date > until_dt {
                    break;
                }
                if let Ok(content) = crate::services::audit_service::AuditService::show_file_at(
                    &repo_root_real,
                    &c.commit,
                    &file_rel,
                ) {
                    if let Some(ts) = parse_status_from_yaml(&content) {
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
                    crate::storage::manager::Storage::new(tasks_abs_real.clone());
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
                    if let Ok(files) = fs::read_dir(&p) {
                        for file in files.flatten() {
                            let fpath = file.path();
                            if fpath.extension().and_then(|e| e.to_str()) != Some("yml") {
                                continue;
                            }
                            let stem = match fpath.file_stem().and_then(|s| s.to_str()) {
                                Some(s) => s,
                                None => continue,
                            };
                            let num: u64 = match stem.parse() {
                                Ok(n) => n,
                                Err(_) => continue,
                            };
                            let id = format!("{}-{}", project_folder, num);
                            let file_rel =
                                tasks_rel.join(&project_folder).join(format!("{}.yml", num));
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
                                if let Ok(content) =
                                    crate::services::audit_service::AuditService::show_file_at(
                                        &repo_root_real,
                                        &c.commit,
                                        &file_rel,
                                    )
                                {
                                    if let Some(ts) = parse_status_from_yaml(&content) {
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
                                reporter,
                                tags,
                                custom_fields,
                                relationships,
                                comments,
                                references,
                                sprints,
                                history,
                            ) = (|| {
                                let content = fs::read_to_string(&abs_file).unwrap_or_default();
                                if let Ok(task) =
                                    serde_yaml::from_str::<crate::storage::task::Task>(&content)
                                {
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
                                    return (
                                        dto.title,
                                        dto.assignee,
                                        dto.effort,
                                        dto.priority,
                                        dto.task_type,
                                        dto.status,
                                        dto.created,
                                        dto.modified,
                                        dto.reporter,
                                        dto.tags,
                                        dto.custom_fields,
                                        dto.relationships,
                                        dto.comments,
                                        dto.references,
                                        dto.sprints,
                                        dto.history,
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
                                let mut reporter: Option<String> = None;
                                let mut tags: Vec<String> = Vec::new();
                                let relationships = crate::types::TaskRelationships::default();
                                let comments: Vec<crate::types::TaskComment> = Vec::new();
                                let references: Vec<crate::types::ReferenceEntry> = Vec::new();
                                let sprints: Vec<u32> = sprint_lookup
                                    .get(&id)
                                    .map(|orders| orders.keys().copied().collect::<Vec<u32>>())
                                    .unwrap_or_default();
                                let history: Vec<crate::types::TaskChangeLogEntry> = Vec::new();
                                let custom_fields: crate::types::CustomFields = Default::default();
                                if let Ok(val) = serde_yaml::from_str::<serde_yaml::Value>(&content)
                                {
                                    if let Some(s) = val.get("title").and_then(|v| v.as_str()) {
                                        title = s.to_string();
                                    }
                                    if let Some(s) = val.get("assignee").and_then(|v| v.as_str()) {
                                        assignee = Some(s.to_string());
                                    }
                                    if let Some(s) = val.get("effort").and_then(|v| v.as_str()) {
                                        effort = Some(s.to_string());
                                    }
                                    if let Some(s) = val.get("priority").and_then(|v| v.as_str()) {
                                        use std::str::FromStr;
                                        priority =
                                            crate::types::Priority::from_str(&s.to_lowercase())
                                                .unwrap_or_default();
                                    }
                                    if let Some(s) = val.get("task_type").and_then(|v| v.as_str()) {
                                        use std::str::FromStr;
                                        task_type =
                                            crate::types::TaskType::from_str(s).unwrap_or_default();
                                    }
                                    if let Some(s) = val.get("status").and_then(|v| v.as_str()) {
                                        use std::str::FromStr;
                                        status = crate::types::TaskStatus::from_str(s)
                                            .unwrap_or_default();
                                    }
                                    if let Some(s) = val.get("created").and_then(|v| v.as_str()) {
                                        created = s.to_string();
                                    }
                                    if let Some(s) = val.get("modified").and_then(|v| v.as_str()) {
                                        modified = s.to_string();
                                    }
                                    if let Some(s) = val.get("reporter").and_then(|v| v.as_str()) {
                                        reporter = Some(s.to_string());
                                    }
                                    if let Some(arr) = val.get("tags").and_then(|v| v.as_sequence())
                                    {
                                        tags = arr
                                            .iter()
                                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
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
                                    reporter,
                                    tags,
                                    custom_fields,
                                    relationships,
                                    comments,
                                    references,
                                    sprints,
                                    history,
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
                                tags,
                                relationships,
                                comments,
                                references,
                                sprints,
                                sprint_order: std::collections::BTreeMap::new(),
                                history,
                                custom_fields,
                            };
                            matched.push((id, dto));
                        }
                    }
                }
            }
            if std::env::var("LOTAR_DEBUG").is_ok() {
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("/tmp/lotar_transitions_debug.log")
                {
                    use std::io::Write;
                    let _ = writeln!(f, "[FALLBACK] tasks_added={}", matched.len());
                }
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
        let cfg_mgr = crate::config::manager::ConfigManager::new_manager_with_tasks_dir_readonly(
            &resolver.path,
        )
        .map_err(|e| format!("Failed to load config: {}", e))?;
        let cfg = cfg_mgr.get_resolved_config();
        for (fk, allowed) in &filters {
            if let Some(vals) = resolve_group_key(&id, &t, fk, cfg)
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
        let cfg_mgr = crate::config::manager::ConfigManager::new_manager_with_tasks_dir_readonly(
            &resolver.path,
        )
        .map_err(|e| format!("Failed to load config: {}", e))?;
        let cfg = cfg_mgr.get_resolved_config();
        let mut keys = resolve_group_key(&id, &t, &by, cfg).unwrap_or_else(|| vec![String::new()]);
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
                    renderer.emit_raw_stdout(&format!("{:>8}{}  {}", val_str, suffix, key));
                }
            }
        }
    }
    Ok(())
}
