use super::*;

pub(super) fn register(api_server: &mut ApiServer) {
    api_server.register_handler("GET", "/api/attachments/get", |req: &HttpRequest| {
        let rel = match req.query.get("path") {
            Some(v) if !v.trim().is_empty() => v.trim().to_string(),
            _ => return bad_request("Missing attachment path".into()),
        };
        let download = req
            .query
            .get("download")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let base_config = match resolution::load_and_merge_configs(Some(resolver.path.as_path())) {
            Ok(c) => c,
            Err(e) => return bad_request(format!("Failed to load config: {}", e)),
        };
        let config = if let Some(project) = req.query.get("project").map(|s| s.as_str()) {
            resolution::get_project_config(&base_config, project, resolver.path.as_path())
                .unwrap_or(base_config)
        } else {
            base_config
        };
        let root =
            match AttachmentService::resolve_attachments_root(resolver.path.as_path(), &config) {
                Ok(p) => p,
                Err(e) => return bad_request(e.to_string()),
            };

        let resolved = match AttachmentService::resolve_attachment_path(&root, &rel) {
            Ok(p) => p,
            Err(msg) if msg.contains("not found") => return not_found(msg),
            Err(msg) => return bad_request(msg),
        };

        let bytes = match std::fs::read(&resolved) {
            Ok(b) => b,
            Err(_) => return not_found("Attachment not found".into()),
        };

        let content_type = match resolved
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "svg" => "image/svg+xml",
            "pdf" => "application/pdf",
            "txt" | "log" | "md" => "text/plain; charset=utf-8",
            _ => "application/octet-stream",
        };

        let stored_filename = resolved
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("attachment");
        let filename = AttachmentService::download_filename(stored_filename);
        let disposition = if download {
            format!("attachment; filename=\"{}\"", filename)
        } else {
            format!("inline; filename=\"{}\"", filename)
        };

        HttpResponse {
            status: 200,
            headers: vec![
                ("Content-Type".to_string(), content_type.to_string()),
                ("Content-Disposition".to_string(), disposition),
            ],
            body: bytes,
        }
    });

    // GET /api/attachments/h/<hash>/<filename>
    // This is primarily for browser "Save Link As" and copy-link ergonomics.

    api_server.register_prefix_handler("GET", "/api/attachments/h", |req: &HttpRequest| {
        let download = req
            .query
            .get("download")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let raw_path = req.path.trim_end_matches('/');
        let prefix = "/api/attachments/h";
        let rest = raw_path
            .get(prefix.len()..)
            .unwrap_or("")
            .trim_start_matches('/');

        let (hash_tag, requested_name) = match rest.split_once('/') {
            Some((h, name)) if !h.trim().is_empty() && !name.trim().is_empty() => (h.trim(), name),
            _ => return bad_request("Missing attachment hash".into()),
        };

        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };
        let base_config = match resolution::load_and_merge_configs(Some(resolver.path.as_path())) {
            Ok(c) => c,
            Err(e) => return bad_request(format!("Failed to load config: {}", e)),
        };
        let config = if let Some(project) = req.query.get("project").map(|s| s.as_str()) {
            resolution::get_project_config(&base_config, project, resolver.path.as_path())
                .unwrap_or(base_config)
        } else {
            base_config
        };
        let root =
            match AttachmentService::resolve_attachments_root(resolver.path.as_path(), &config) {
                Ok(p) => p,
                Err(e) => return bad_request(e.to_string()),
            };

        let resolved = match AttachmentService::find_attachment_by_hash(&root, hash_tag) {
            Some(p) => p,
            None => return not_found("Attachment not found".into()),
        };

        let bytes = match std::fs::read(&resolved) {
            Ok(b) => b,
            Err(_) => return not_found("Attachment not found".into()),
        };

        let content_type = match resolved
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "svg" => "image/svg+xml",
            "pdf" => "application/pdf",
            "txt" | "log" | "md" => "text/plain; charset=utf-8",
            _ => "application/octet-stream",
        };

        // For Save Link As, browsers often use the URL path segment.
        // Still send Content-Disposition to help other download flows.
        let requested_leaf = requested_name.split('/').next_back().unwrap_or("").trim();
        let stored_filename = resolved
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("attachment");
        let computed = AttachmentService::download_filename(stored_filename);
        let filename = if requested_leaf.contains('.') {
            requested_leaf.to_string()
        } else {
            computed
        };

        let disposition = if download {
            format!("attachment; filename=\"{}\"", filename)
        } else {
            format!("inline; filename=\"{}\"", filename)
        };

        HttpResponse {
            status: 200,
            headers: vec![
                ("Content-Type".to_string(), content_type.to_string()),
                ("Content-Disposition".to_string(), disposition),
            ],
            body: bytes,
        }
    });

    // POST /api/tasks/attachments/upload
}
