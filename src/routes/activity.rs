use super::*;

pub(super) fn register(api_server: &mut ApiServer) {
    api_server.register_handler("GET", "/api/activity/feed", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let cwd = std::env::current_dir().unwrap_or_else(|_| resolver.path.clone());
        let repo_root = match crate::utils::git::find_repo_root(&cwd) {
            Some(r) => r,
            None => return bad_request("Not inside a git repository".into()),
        };
        let tasks_abs = resolver.path.clone();
        let tasks_rel = match tasks_abs.strip_prefix(&repo_root) {
            Ok(p) => p.to_path_buf(),
            Err(_) => return bad_request("Tasks directory not inside repository".into()),
        };
        let now = chrono::Utc::now();
        let default_since = now - chrono::Duration::days(30);
        let since = req
            .query
            .get("since")
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or(default_since);
        let until = req
            .query
            .get("until")
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or(now);
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
            Err(e) => internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        }
    });

    // GET /api/activity/series?group=author|day|week|project[&since=ISO][&until=ISO][&project=PREFIX]

    api_server.register_handler("GET", "/api/activity/series", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        // Repo root and tasks relative
        let cwd = std::env::current_dir().unwrap_or_else(|_| resolver.path.clone());
        let repo_root = match crate::utils::git::find_repo_root(&cwd) {
            Some(r) => r,
            None => return bad_request("Not inside a git repository".into()),
        };
        let tasks_abs = resolver.path.clone();
        let tasks_rel = match tasks_abs.strip_prefix(&repo_root) {
            Ok(p) => p.to_path_buf(),
            Err(_) => return bad_request("Tasks directory not inside repository".into()),
        };
        // Parse group
        let gb = match req.query.get("group").map(|s| s.as_str()) {
            Some("author") => crate::services::audit_service::GroupBy::Author,
            Some("day") => crate::services::audit_service::GroupBy::Day,
            Some("week") => crate::services::audit_service::GroupBy::Week,
            Some("project") => crate::services::audit_service::GroupBy::Project,
            _ => crate::services::audit_service::GroupBy::Day,
        };
        // Parse since/until
        let now = chrono::Utc::now();
        let default_since = now - chrono::Duration::days(30);
        let since = req
            .query
            .get("since")
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or(default_since);
        let until = req
            .query
            .get("until")
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or(now);
        let project = req.query.get("project").map(|s| s.as_str());
        match crate::services::audit_service::AuditService::list_activity(
            &repo_root, &tasks_rel, since, until, gb, project,
        ) {
            Ok(items) => ok_json(200, json!({"data": items})),
            Err(e) => internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        }
    });

    // GET /api/activity/authors[?since=ISO][&until=ISO][&project=PREFIX]

    api_server.register_handler("GET", "/api/activity/authors", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let cwd = std::env::current_dir().unwrap_or_else(|_| resolver.path.clone());
        let repo_root = match crate::utils::git::find_repo_root(&cwd) {
            Some(r) => r,
            None => return bad_request("Not inside a git repository".into()),
        };
        let tasks_abs = resolver.path.clone();
        let tasks_rel = match tasks_abs.strip_prefix(&repo_root) {
            Ok(p) => p.to_path_buf(),
            Err(_) => return bad_request("Tasks directory not inside repository".into()),
        };
        let now = chrono::Utc::now();
        let default_since = now - chrono::Duration::days(30);
        let since = req
            .query
            .get("since")
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or(default_since);
        let until = req
            .query
            .get("until")
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or(now);
        let project = req.query.get("project").map(|s| s.as_str());
        match crate::services::audit_service::AuditService::list_authors_activity(
            &repo_root, &tasks_rel, since, until, project,
        ) {
            Ok(items) => ok_json(200, json!({"data": items})),
            Err(e) => internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        }
    });

    // GET /api/activity/changed_tasks[?since=ISO][&until=ISO][&author=str][&project=PREFIX]

    api_server.register_handler("GET", "/api/activity/changed_tasks", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let cwd = std::env::current_dir().unwrap_or_else(|_| resolver.path.clone());
        let repo_root = match crate::utils::git::find_repo_root(&cwd) {
            Some(r) => r,
            None => return bad_request("Not inside a git repository".into()),
        };
        let tasks_abs = resolver.path.clone();
        let tasks_rel = match tasks_abs.strip_prefix(&repo_root) {
            Ok(p) => p.to_path_buf(),
            Err(_) => return bad_request("Tasks directory not inside repository".into()),
        };
        let now = chrono::Utc::now();
        let default_since = now - chrono::Duration::days(30);
        let since = req
            .query
            .get("since")
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or(default_since);
        let until = req
            .query
            .get("until")
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or(now);
        let author = req.query.get("author").map(|s| s.as_str());
        let project = req.query.get("project").map(|s| s.as_str());
        match crate::services::audit_service::AuditService::list_changed_tasks(
            &repo_root, &tasks_rel, since, until, author, project,
        ) {
            Ok(items) => ok_json(200, json!({"data": items})),
            Err(e) => internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        }
    });

    // GET /api/tasks/commit_diff?id=ID&commit=SHA
}
