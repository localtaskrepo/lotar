use super::*;

pub(super) fn register(api_server: &mut ApiServer) {
    api_server.register_handler("GET", "/api/activity/feed", |req: &HttpRequest| {
        let (repo_root, tasks_rel) = try_response!(resolve_repo_tasks_paths());
        let (since, until) = match activity_window(req) {
            Ok(window) => window,
            Err(response) => return response,
        };
        let project = req.query.get("project").map(|s| s.as_str());
        let limit = req
            .query
            .get("limit")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(200);
        match crate::services::audit_service::AuditService::list_activity_feed(
            &repo_root, &tasks_rel, since, until, project, limit,
        ) {
            Ok(items) => ok_json(200, json!({"data": items})),
            Err(e)
                if e.starts_with("Unknown project") || e.starts_with("Invalid project filter") =>
            {
                bad_request(e)
            }
            Err(e) => internal_error(e),
        }
    });

    // GET /api/activity/series?group=author|day|week|project[&since=ISO][&until=ISO][&project=PREFIX]

    api_server.register_handler("GET", "/api/activity/series", |req: &HttpRequest| {
        let (repo_root, tasks_rel) = try_response!(resolve_repo_tasks_paths());
        // Parse group
        let gb = match req.query.get("group").map(|s| s.as_str()) {
            Some("author") => crate::services::audit_service::GroupBy::Author,
            Some("day") => crate::services::audit_service::GroupBy::Day,
            Some("week") => crate::services::audit_service::GroupBy::Week,
            Some("project") => crate::services::audit_service::GroupBy::Project,
            None => crate::services::audit_service::GroupBy::Day,
            Some(other) => {
                return bad_request(format!(
                    "Invalid group '{other}': use author, day, week or project"
                ));
            }
        };
        // Parse since/until
        let (since, until) = match activity_window(req) {
            Ok(window) => window,
            Err(response) => return response,
        };
        let project = req.query.get("project").map(|s| s.as_str());
        match crate::services::audit_service::AuditService::list_activity(
            &repo_root, &tasks_rel, since, until, gb, project,
        ) {
            Ok(items) => ok_json(200, json!({"data": items})),
            Err(e)
                if e.starts_with("Unknown project") || e.starts_with("Invalid project filter") =>
            {
                bad_request(e)
            }
            Err(e) => internal_error(e),
        }
    });

    // GET /api/activity/authors[?since=ISO][&until=ISO][&project=PREFIX]

    api_server.register_handler("GET", "/api/activity/authors", |req: &HttpRequest| {
        let (repo_root, tasks_rel) = try_response!(resolve_repo_tasks_paths());
        let (since, until) = match activity_window(req) {
            Ok(window) => window,
            Err(response) => return response,
        };
        let project = req.query.get("project").map(|s| s.as_str());
        match crate::services::audit_service::AuditService::list_authors_activity(
            &repo_root, &tasks_rel, since, until, project,
        ) {
            Ok(items) => ok_json(200, json!({"data": items})),
            Err(e)
                if e.starts_with("Unknown project") || e.starts_with("Invalid project filter") =>
            {
                bad_request(e)
            }
            Err(e) => internal_error(e),
        }
    });

    // GET /api/activity/changed_tasks[?since=ISO][&until=ISO][&author=str][&project=PREFIX]

    api_server.register_handler("GET", "/api/activity/changed_tasks", |req: &HttpRequest| {
        let (repo_root, tasks_rel) = try_response!(resolve_repo_tasks_paths());
        let (since, until) = match activity_window(req) {
            Ok(window) => window,
            Err(response) => return response,
        };
        let author = req.query.get("author").map(|s| s.as_str());
        let project = req.query.get("project").map(|s| s.as_str());
        match crate::services::audit_service::AuditService::list_changed_tasks(
            &repo_root, &tasks_rel, since, until, author, project,
        ) {
            Ok(items) => ok_json(200, json!({"data": items})),
            Err(e)
                if e.starts_with("Unknown project") || e.starts_with("Invalid project filter") =>
            {
                bad_request(e)
            }
            Err(e) => internal_error(e),
        }
    });

    // GET /api/tasks/commit_diff?id=ID&commit=SHA
}

/// `since`/`until` query window (RFC 3339); unparseable or missing values
/// fall back to the last 30 days.
/// `since`/`until` window (RFC 3339), defaulting to the last 30 days. A
/// value that does not parse is a 400, not a silent default (DEV-276).
fn activity_window(
    req: &HttpRequest,
) -> Result<(chrono::DateTime<Utc>, chrono::DateTime<Utc>), HttpResponse> {
    let parse = |key: &str| -> Result<Option<chrono::DateTime<Utc>>, HttpResponse> {
        match req.query.get(key).map(|s| s.trim()).filter(|s| !s.is_empty()) {
            None => Ok(None),
            Some(raw) => chrono::DateTime::parse_from_rfc3339(raw)
                .map(|dt| Some(dt.with_timezone(&Utc)))
                .map_err(|_| {
                    bad_request(format!(
                        "Invalid {key} '{raw}': use an RFC 3339 timestamp such as 2026-01-31T00:00:00Z"
                    ))
                }),
        }
    };
    let now = Utc::now();
    let since = parse("since")?.unwrap_or(now - chrono::Duration::days(30));
    let until = parse("until")?.unwrap_or(now);
    Ok((since, until))
}
