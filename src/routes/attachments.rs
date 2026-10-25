use super::*;

pub(super) fn register(api_server: &mut ApiServer) {
    api_server.register_handler("GET", "/api/attachments/get", |req: &HttpRequest| {
        let rel = match req.query.get("path") {
            Some(v) if !v.trim().is_empty() => v.trim().to_string(),
            _ => return bad_request("Missing attachment path".into()),
        };
        let download = wants_download(req);

        let root = try_response!(attachments_root(req));

        // Typed classification (DEV-58): unsafe paths stay 400s, a missing
        // blob is a 404, and root-resolution failures are 500s — no
        // message-string matching.
        let resolved = match AttachmentService::resolve_attachment_path_typed(&root, &rel) {
            Ok(p) => p,
            Err(crate::services::attachment_service::AttachmentPathError::Invalid(msg)) => {
                return bad_request(msg.to_string());
            }
            Err(crate::services::attachment_service::AttachmentPathError::Missing) => {
                return not_found("Attachment not found".into());
            }
            Err(crate::services::attachment_service::AttachmentPathError::RootUnavailable(msg)) => {
                return internal_error(msg);
            }
        };

        let bytes = match std::fs::read(&resolved) {
            Ok(b) => b,
            Err(_) => return not_found("Attachment not found".into()),
        };

        let stored_filename = resolved
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("attachment");
        let filename = AttachmentService::download_filename(stored_filename);
        attachment_response(&resolved, bytes, &filename, download)
    });

    // GET /api/attachments/h/<hash>/<filename>
    // This is primarily for browser "Save Link As" and copy-link ergonomics.

    api_server.register_prefix_handler(
        "GET",
        "/api/attachments/h/{hash}/{filename}",
        |req: &HttpRequest| {
            let download = wants_download(req);

            let raw_path = req.path.trim_end_matches('/');
            let prefix = "/api/attachments/h";
            let rest = raw_path
                .get(prefix.len()..)
                .unwrap_or("")
                .trim_start_matches('/');

            let (hash_tag, requested_name) = match rest.split_once('/') {
                Some((h, name)) if !h.trim().is_empty() && !name.trim().is_empty() => {
                    (h.trim(), name)
                }
                _ => return bad_request("Missing attachment hash".into()),
            };

            let root = try_response!(attachments_root(req));

            let resolved = match AttachmentService::find_attachment_by_hash(&root, hash_tag) {
                Some(p) => p,
                None => return not_found("Attachment not found".into()),
            };

            let bytes = match std::fs::read(&resolved) {
                Ok(b) => b,
                Err(_) => return not_found("Attachment not found".into()),
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

            attachment_response(&resolved, bytes, &filename, download)
        },
    );

    // POST /api/tasks/attachments/upload
}

fn wants_download(req: &HttpRequest) -> bool {
    req.query
        .get("download")
        .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
}

/// Attachment store root for the request's `project` (or the global config).
fn attachments_root(req: &HttpRequest) -> Result<std::path::PathBuf, HttpResponse> {
    let resolver = resolve_tasks_dir()?;
    let base_config = load_merged_config(&resolver)?;
    let config = match req.query.get("project") {
        Some(project) => {
            resolution::get_project_config(&base_config, project, resolver.path.as_path())
                .unwrap_or(base_config)
        }
        None => base_config,
    };
    // Store-root resolution failures are server-side (DEV-58).
    AttachmentService::resolve_attachments_root(resolver.path.as_path(), &config)
        .map_err(map_lotar_error)
}

/// Serve an attachment inline or as a download with a content type derived
/// from its extension.
fn attachment_response(
    path: &std::path::Path,
    bytes: Vec<u8>,
    filename: &str,
    download: bool,
) -> HttpResponse {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let content_type = match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "txt" | "log" | "md" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    };
    let disposition = if download { "attachment" } else { "inline" };
    HttpResponse {
        status: 200,
        headers: vec![
            ("Content-Type".to_string(), content_type.to_string()),
            (
                "Content-Disposition".to_string(),
                format!("{disposition}; filename=\"{filename}\""),
            ),
        ],
        body: bytes,
    }
}
