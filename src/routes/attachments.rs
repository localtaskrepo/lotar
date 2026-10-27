use super::*;
use crate::services::attachment_previews;

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
        apply_range(
            req,
            attachment_response(&resolved, bytes, &filename, download),
        )
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

            // `?w=` asks for a preview no wider than that many pixels; it
            // snaps to one of a few supported widths (DEV-216).
            let width = match req.query.get("w") {
                Some(raw) => match raw.trim().parse::<u32>() {
                    Ok(value) if value > 0 => Some(attachment_previews::snap_width(value)),
                    _ => {
                        return bad_request(format!(
                            "Invalid w: '{raw}' (expected a positive width in pixels)"
                        ));
                    }
                },
                None => None,
            };

            // Hash URLs are content-addressed: the same URL always serves the
            // same bytes, so browsers may cache them for good (DEV-216).
            let hash = hash_tag.to_ascii_lowercase();
            let etag = match width {
                Some(width) => format!("\"{hash}-w{width}\""),
                None => format!("\"{hash}\""),
            };
            let revalidated = req.headers.get("if-none-match").is_some_and(|value| {
                value
                    .split(',')
                    .any(|tag| tag.trim().trim_start_matches("W/") == etag)
            });

            let resolved = match attachment_previews::find_by_hash(&root, hash_tag) {
                Some(p) => p,
                None => return not_found("Attachment not found".into()),
            };

            if revalidated {
                return HttpResponse {
                    status: 304,
                    headers: immutable_cache_headers(&etag),
                    body: Vec::new(),
                };
            }

            // For Save Link As, browsers often use the URL path segment.
            // Still send Content-Disposition to help other download flows.
            let requested_leaf =
                percent_decode(requested_name.split('/').next_back().unwrap_or("").trim());
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

            if let Some(width) = width
                && !download
                && let Some(thumb) = attachment_previews::thumbnail(&resolved, &hash, width)
            {
                let mut headers = vec![
                    ("Content-Type".to_string(), thumb.content_type.to_string()),
                    (
                        "Content-Disposition".to_string(),
                        content_disposition("inline", &filename),
                    ),
                    ("X-Content-Type-Options".to_string(), "nosniff".to_string()),
                ];
                headers.extend(immutable_cache_headers(&etag));
                return HttpResponse {
                    status: 200,
                    headers,
                    body: thumb.bytes.clone(),
                };
            }

            let bytes = match std::fs::read(&resolved) {
                Ok(b) => b,
                Err(_) => return not_found("Attachment not found".into()),
            };
            let mut response = attachment_response(&resolved, bytes, &filename, download);
            response.headers.extend(immutable_cache_headers(&etag));
            apply_range(req, response)
        },
    );

    // POST /api/attachments/info {project?, attachments: [stored names]}
    // Size and pixel dimensions for gallery tooltips (DEV-216).
    api_server.register_handler("POST", "/api/attachments/info", |req: &HttpRequest| {
        let payload: crate::api_types::AttachmentInfoRequest =
            match serde_json::from_slice(&req.body) {
                Ok(value) => value,
                Err(err) => return bad_request(format!("Invalid JSON: {err}")),
            };
        if payload.attachments.len() > 200 {
            return bad_request("Too many attachments (at most 200 per request)".into());
        }
        let mut scoped = req.clone();
        if let Some(project) = payload.project.as_deref().filter(|p| !p.trim().is_empty()) {
            scoped
                .query
                .insert("project".into(), project.trim().to_string());
        }
        let root = try_response!(attachments_root(&scoped));
        let items = payload
            .attachments
            .iter()
            .filter_map(|stored| {
                let hash = AttachmentService::extract_hash_tag(stored)?;
                let path = attachment_previews::find_by_hash(&root, &hash)?;
                let info = attachment_previews::file_info(&path, &hash)?;
                Some(crate::api_types::AttachmentInfo {
                    attachment: stored.clone(),
                    size: info.size,
                    width: info.width,
                    height: info.height,
                })
            })
            .collect();
        ok_json(
            200,
            json!({"data": crate::api_types::AttachmentInfoResponse { items }}),
        )
    });
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

fn immutable_cache_headers(etag: &str) -> Vec<(String, String)> {
    vec![
        (
            "Cache-Control".to_string(),
            "private, max-age=31536000, immutable".to_string(),
        ),
        ("ETag".to_string(), etag.to_string()),
    ]
}

/// Content type for an attachment's extension. Active document types (HTML,
/// XML, JS) stay `application/octet-stream` so they are never rendered on the
/// app's origin.
fn attachment_content_type(extension: &str) -> &'static str {
    match extension {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "svg" => "image/svg+xml",
        "tif" | "tiff" => "image/tiff",
        // Safari shows HEIC itself; other browsers download it.
        "heic" | "heif" => "image/heic",
        "pdf" => "application/pdf",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "m4a" => "audio/mp4",
        "csv" => "text/csv; charset=utf-8",
        "txt" | "log" | "md" | "json" | "yaml" | "yml" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// `Content-Disposition` with an ASCII fallback name plus the exact UTF-8
/// name (RFC 6266 / RFC 5987), so "Save as" keeps umlauts and emoji.
fn content_disposition(disposition: &str, filename: &str) -> String {
    let fallback: String = filename
        .chars()
        .map(|c| {
            if c.is_ascii() && !c.is_ascii_control() && c != '"' && c != '\\' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if fallback == filename {
        return format!("{disposition}; filename=\"{filename}\"");
    }
    let mut encoded = String::new();
    for byte in filename.bytes() {
        if byte.is_ascii_alphanumeric() || b"!#$&+-.^_`|~".contains(&byte) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    format!("{disposition}; filename=\"{fallback}\"; filename*=UTF-8''{encoded}")
}

/// Answer a single `Range: bytes=…` request with 206 Partial Content (or 416
/// when unsatisfiable). Video and audio need this: Safari will not play
/// media without it, and other browsers use it to seek and to read just the
/// metadata for a first-frame preview.
fn apply_range(req: &HttpRequest, mut response: HttpResponse) -> HttpResponse {
    response
        .headers
        .push(("Accept-Ranges".to_string(), "bytes".to_string()));
    let Some(raw) = req.headers.get("range") else {
        return response;
    };
    if response.status != 200 {
        return response;
    }
    let total = response.body.len() as u64;
    let unsatisfiable = |mut response: HttpResponse| {
        response.status = 416;
        response.body = Vec::new();
        response
            .headers
            .push(("Content-Range".to_string(), format!("bytes */{total}")));
        response
    };
    // Only single ranges; anything else gets the whole file, as RFC 9110 allows.
    let Some(spec) = raw.trim().strip_prefix("bytes=") else {
        return response;
    };
    if spec.contains(',') {
        return response;
    }
    let Some((start, end)) = spec.split_once('-') else {
        return response;
    };
    let (start, end) = match (start.trim(), end.trim()) {
        ("", "") => return response,
        // Suffix range: the last N bytes.
        ("", suffix) => match suffix.parse::<u64>() {
            Ok(0) | Err(_) => return unsatisfiable(response),
            Ok(n) => (total.saturating_sub(n), total.saturating_sub(1)),
        },
        (start, end) => {
            let Ok(start) = start.parse::<u64>() else {
                return response;
            };
            let end = if end.is_empty() {
                total.saturating_sub(1)
            } else {
                match end.parse::<u64>() {
                    Ok(end) => end.min(total.saturating_sub(1)),
                    Err(_) => return response,
                }
            };
            (start, end)
        }
    };
    if total == 0 || start >= total || start > end {
        return unsatisfiable(response);
    }
    response.status = 206;
    response.body = response.body[start as usize..=end as usize].to_vec();
    response.headers.push((
        "Content-Range".to_string(),
        format!("bytes {start}-{end}/{total}"),
    ));
    response
}

/// Decode `%XX` escapes in a URL path segment; invalid UTF-8 is replaced.
fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(hex) = raw.get(i + 1..i + 3)
            && let Ok(value) = u8::from_str_radix(hex, 16)
        {
            out.push(value);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
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
    let content_type = attachment_content_type(&extension);
    let disposition = if download { "attachment" } else { "inline" };
    let mut response = HttpResponse {
        status: 200,
        headers: vec![
            ("Content-Type".to_string(), content_type.to_string()),
            (
                "Content-Disposition".to_string(),
                content_disposition(disposition, filename),
            ),
            // Never let the browser guess a more active type than we declare.
            ("X-Content-Type-Options".to_string(), "nosniff".to_string()),
        ],
        body: bytes,
    };
    if extension == "svg" {
        // An SVG opened in its own tab is a document that can run script with
        // the app's API on the same origin. Sandbox it; <img> previews are
        // unaffected because images never run script.
        response.headers.push((
            "Content-Security-Policy".to_string(),
            "default-src 'none'; img-src data:; style-src 'unsafe-inline'; sandbox".to_string(),
        ));
    }
    response
}
