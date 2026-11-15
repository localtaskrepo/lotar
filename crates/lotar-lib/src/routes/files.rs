use super::attachments::{attachment_content_type, content_disposition};
use super::*;
use crate::api_types::{FileResolveItem, FileResolveRequest, FileResolveResponse};
use crate::services::repo_file_service::{MAX_FILE_BYTES, MAX_RESOLVE_REFERENCES, RepoFileError, RepoFileService};

// Repository files named in ticket text, for the web UI's file viewer (DEV-549).
pub(super) fn register(api_server: &mut ApiServer) {
    // POST /api/files/resolve {base?, references: [names]}
    api_server.register_handler("POST", "/api/files/resolve", |req: &HttpRequest| {
        let payload: FileResolveRequest = try_response!(parse_body(req));
        if payload.references.len() > MAX_RESOLVE_REFERENCES {
            return bad_request(format!("Too many references (at most {MAX_RESOLVE_REFERENCES} per request)"));
        }
        // Outside a repository nothing links, which is not an error.
        let Some(repo_root) = try_response!(repo_root()) else {
            return ok_json(200, json!({"data": FileResolveResponse { items: Vec::new() }}));
        };
        let files = RepoFileService::list(&repo_root);
        let items = payload
            .references
            .iter()
            .filter_map(|reference| {
                let path = RepoFileService::resolve(&repo_root, &files, payload.base.as_deref(), reference)?;
                Some(FileResolveItem { reference: reference.clone(), path })
            })
            .collect();
        ok_json(200, json!({"data": FileResolveResponse { items }}))
    });

    // GET /api/files/content?path=<repo path>[&download=1]
    api_server.register_handler("GET", "/api/files/content", |req: &HttpRequest| {
        let path = match req.query.get("path") {
            Some(v) if !v.trim().is_empty() => v.trim().to_string(),
            _ => return bad_request("Missing file path".into()),
        };
        let not_found_response = || not_found(format!("File not found: {path}"));
        let Some(repo_root) = try_response!(repo_root()) else {
            return not_found_response();
        };
        let files = RepoFileService::list(&repo_root);
        let bytes = match RepoFileService::read(&repo_root, &files, &path) {
            Ok(bytes) => bytes,
            Err(RepoFileError::NotFound) => return not_found_response(),
            Err(RepoFileError::TooLarge { size }) => {
                return ok_json(
                    413,
                    json!({"error": {"code": "FILE_TOO_LARGE", "message": format!("{path} is {size} bytes; the viewer reads files up to {MAX_FILE_BYTES} bytes")}}),
                );
            }
            Err(RepoFileError::Io(message)) => return internal_error(format!("Failed to read {path}: {message}")),
        };
        let download = req.query.get("download").is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
        file_response(&path, bytes, download)
    });
}

/// Repository that holds the tasks directory, `None` outside one.
fn repo_root() -> Result<Option<std::path::PathBuf>, HttpResponse> {
    let resolver = resolve_tasks_dir()?;
    Ok(crate::utils::git::find_repo_root(&resolver.path))
}

/// Images keep their type so the viewer can show them; any other text is
/// plain text (never HTML or script on the app's origin), the rest binary.
fn file_response(path: &str, bytes: Vec<u8>, download: bool) -> HttpResponse {
    let extension = path.rsplit_once('.').map(|(_, ext)| ext.to_ascii_lowercase()).unwrap_or_default();
    let known = attachment_content_type(&extension);
    let content_type = if known.starts_with("image/") || known == "application/pdf" {
        known
    } else if !bytes.contains(&0) && std::str::from_utf8(&bytes).is_ok() {
        "text/plain; charset=utf-8"
    } else {
        "application/octet-stream"
    };
    let filename = path.rsplit('/').next().unwrap_or(path);
    let mut headers = vec![
        ("Content-Type".to_string(), content_type.to_string()),
        ("Content-Disposition".to_string(), content_disposition(if download { "attachment" } else { "inline" }, filename)),
        ("X-Content-Type-Options".to_string(), "nosniff".to_string()),
        // Files change under the server; always ask again.
        ("Cache-Control".to_string(), "no-cache".to_string()),
    ];
    if extension == "svg" {
        // Opened in a tab of its own, an SVG could run script on the app's origin.
        headers.push(("Content-Security-Policy".to_string(), "default-src 'none'; img-src data:; style-src 'unsafe-inline'; sandbox".to_string()));
    }
    HttpResponse { status: 200, headers, body: bytes }
}
