use super::*;

pub(super) fn register(api_server: &mut ApiServer) {
    api_server.register_handler("GET", "/api/references/snippet", |req: &HttpRequest| {
        let code = match req.query.get("code") {
            Some(v) if !v.trim().is_empty() => v.trim().to_string(),
            _ => return bad_request("Missing code reference".into()),
        };
        let parse_ctx = |key: &str| -> Option<usize> {
            req.query.get(key).and_then(|s| s.parse::<usize>().ok())
        };
        let normalize = |value: usize| -> usize {
            match value {
                0 => 1,
                v if v > 20 => 20,
                v => v,
            }
        };
        let default_context = parse_ctx("context").map(normalize).unwrap_or(6);
        let before = parse_ctx("before")
            .map(normalize)
            .unwrap_or(default_context);
        let after = parse_ctx("after").map(normalize).unwrap_or(default_context);
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let repo_root = match crate::utils::git::find_repo_root(&resolver.path) {
            Some(root) => root,
            None => return bad_request("Unable to locate git repository".into()),
        };
        match ReferenceService::snippet_for_code(&repo_root, &code, before, after) {
            Ok(snippet) => ok_json(200, json!({"data": snippet})),
            Err(msg) => bad_request(msg),
        }
    });

    // GET /api/references/files?q=TEXT[&limit=N]

    api_server.register_handler("GET", "/api/references/files", |req: &HttpRequest| {
        let q = req.query.get("q").cloned().unwrap_or_default();
        let q = q.trim().to_string();
        if q.is_empty() {
            return ok_json(200, json!({"data": Vec::<String>::new()}));
        }

        let limit: usize = req
            .query
            .get("limit")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(20);
        let limit = limit.clamp(1, 200);

        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let repo_root = match crate::utils::git::find_repo_root(&resolver.path) {
            Some(root) => root,
            None => return bad_request("Unable to locate git repository".into()),
        };

        let files = ReferenceService::suggest_repo_files(&repo_root, &q, limit);
        ok_json(200, json!({"data": files}))
    });

    // GET /api/attachments/get?path=<relative>
}
