//! Repository branding for the web UI (DEV-218): theme, name, logo and icon.
use super::*;
use crate::services::branding_service::{self, BrandingFileError, BrandingImage};

pub(super) fn register(api_server: &mut ApiServer) {
    // GET /api/branding -> what the UI should show, with URLs for the images
    // that can actually be served and the reason for those that cannot.
    api_server.register_handler("GET", "/api/branding", |_req: &HttpRequest| {
        let resolver = try_response!(resolve_tasks_dir());
        let config = try_response!(load_merged_config(&resolver));
        ok_json(200, json!({"data": branding_response(&resolver, &config)}))
    });

    // GET /api/branding/{image} -> the configured logo, dark logo or icon.
    api_server.register_prefix_handler("GET", "/api/branding/{image}", |req: &HttpRequest| {
        let slug = req
            .path
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or_default();
        if slug == "theme.css" {
            return serve_skin(req);
        }
        let Some(image) = BrandingImage::parse(slug) else {
            return not_found(format!("Unknown branding image '{slug}'"));
        };
        let resolver = try_response!(resolve_tasks_dir());
        let config = try_response!(load_merged_config(&resolver));
        let Some(relative) = image.configured(&config) else {
            return not_found(format!("No {} is configured", image.slug()));
        };
        let root = branding_service::workspace_root(&resolver.path);
        let (_path, bytes) = match branding_service::read_image(&root, relative) {
            Ok(found) => found,
            Err(err) => return not_found(problem_text(image, relative, &err)),
        };
        // The file can change at any time; browsers revalidate by content.
        let etag = format!("\"{}\"", &blake3::hash(&bytes).to_hex()[..32]);
        let mut headers = vec![
            ("Cache-Control".to_string(), "no-cache".to_string()),
            ("ETag".to_string(), etag.clone()),
            ("X-Content-Type-Options".to_string(), "nosniff".to_string()),
        ];
        let matches = req.headers.get("if-none-match").is_some_and(|value| {
            value
                .split(',')
                .any(|tag| tag.trim().trim_start_matches("W/") == etag)
        });
        if matches {
            return HttpResponse {
                status: 304,
                headers,
                body: Vec::new(),
            };
        }
        let format = crate::services::attachment_metadata::sniff_image_format(&bytes).unwrap_or("");
        let content_type = match format {
            "svg" => "image/svg+xml",
            "png" => "image/png",
            "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "ico" => "image/x-icon",
            "avif" => "image/avif",
            _ => "application/octet-stream",
        };
        headers.push(("Content-Type".to_string(), content_type.to_string()));
        if format == "svg" {
            // Opened on its own, an SVG is a document; never let it run script.
            headers.push((
                "Content-Security-Policy".to_string(),
                "default-src 'none'; img-src data:; style-src 'unsafe-inline'; sandbox".to_string(),
            ));
        }
        HttpResponse {
            status: 200,
            headers,
            body: bytes,
        }
    });

    // POST /api/branding/upload {image, filename, content_base64} -> {path}
    // Stores the image under <tasks>/@branding and returns its repository
    // path; the Config page puts it in the field and saves it with the rest.
    api_server.register_handler("POST", "/api/branding/upload", |req: &HttpRequest| {
        use base64::Engine;
        let payload: crate::api_types::BrandingUploadRequest = try_response!(parse_body_or_empty(req));
        if payload.image.trim() == "theme" {
            return upload_skin(&payload);
        }
        let Some(image) = BrandingImage::parse(payload.image.trim()) else {
            return bad_request("image must be one of: logo, logo-dark, icon, theme".into());
        };
        let file_name = match branding_service::upload_file_name(image, payload.filename.trim()) {
            Ok(name) => name,
            Err(message) => return bad_request(message),
        };
        let bytes = match base64::engine::general_purpose::STANDARD.decode(payload.content_base64.trim()) {
            Ok(bytes) => bytes,
            Err(_) => return bad_request("Invalid base64 content".into()),
        };
        if bytes.len() as u64 > branding_service::MAX_BRANDING_BYTES {
            return bad_request(format!(
                "Image too large: {} bytes (at most {} KiB)",
                bytes.len(),
                branding_service::MAX_BRANDING_BYTES / 1024
            ));
        }
        if crate::services::attachment_metadata::sniff_image_format(&bytes).is_none() {
            return bad_request(format!("{} is not an image", payload.filename.trim()));
        }
        // Logos are shared through git: keep camera and location data out.
        let bytes = crate::services::attachment_metadata::strip_metadata(&bytes).unwrap_or(bytes);

        let resolver = try_response!(resolve_tasks_dir());
        let dir = resolver.path.join(branding_service::UPLOAD_DIR);
        if let Err(err) = std::fs::create_dir_all(&dir) {
            return internal_error(format!("Could not create {}: {err}", dir.display()));
        }
        // One file per slot: drop an earlier upload with another extension.
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.file_stem().and_then(|stem| stem.to_str()) == Some(image.slug())
                    && path.file_name().and_then(|name| name.to_str()) != Some(file_name.as_str())
                {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
        let target = dir.join(&file_name);
        if let Err(err) = std::fs::write(&target, &bytes) {
            return internal_error(format!("Could not write {}: {err}", target.display()));
        }

        let root = branding_service::workspace_root(&resolver.path);
        let relative = match target.strip_prefix(&root) {
            Ok(relative) => relative.to_string_lossy().replace('\\', "/"),
            Err(_) => {
                return bad_request(
                    "The tasks folder is outside the repository; set the path in the config instead".into(),
                );
            }
        };
        ok_json(
            200,
            json!({"data": crate::api_types::BrandingUploadResponse { path: relative }}),
        )
    });
}

/// The repository skin (`ui.theme_file`), reduced to theme variables. Always
/// a stylesheet (empty without a skin), so index.html can link it before
/// first paint.
fn serve_skin(req: &HttpRequest) -> HttpResponse {
    let css = (|| {
        let resolver = resolve_tasks_dir().ok()?;
        let config = load_merged_config(&resolver).ok()?;
        let relative = config.ui.theme_file.as_deref()?;
        let root = branding_service::workspace_root(&resolver.path);
        crate::services::theme_file::read_skin(&root, relative)
            .ok()
            .map(|(_, skin)| skin.css)
    })()
    .unwrap_or_default();
    let etag = format!("\"{}\"", &blake3::hash(css.as_bytes()).to_hex()[..32]);
    let headers = vec![
        (
            "Content-Type".to_string(),
            "text/css; charset=utf-8".to_string(),
        ),
        ("Cache-Control".to_string(), "no-cache".to_string()),
        ("ETag".to_string(), etag.clone()),
        ("X-Content-Type-Options".to_string(), "nosniff".to_string()),
    ];
    let matches = req.headers.get("if-none-match").is_some_and(|value| {
        value
            .split(',')
            .any(|tag| tag.trim().trim_start_matches("W/") == etag)
    });
    HttpResponse {
        status: if matches { 304 } else { 200 },
        headers,
        body: if matches {
            Vec::new()
        } else {
            css.into_bytes()
        },
    }
}

/// Store an uploaded skin as <tasks>/@branding/theme.css.
fn upload_skin(payload: &crate::api_types::BrandingUploadRequest) -> HttpResponse {
    use base64::Engine;
    if !payload
        .filename
        .trim()
        .to_ascii_lowercase()
        .ends_with(".css")
    {
        return bad_request(format!("{} is not a CSS file", payload.filename.trim()));
    }
    let bytes =
        match base64::engine::general_purpose::STANDARD.decode(payload.content_base64.trim()) {
            Ok(bytes) => bytes,
            Err(_) => return bad_request("Invalid base64 content".into()),
        };
    if bytes.len() as u64 > crate::services::theme_file::MAX_SKIN_BYTES {
        return bad_request(format!(
            "Skin too large: {} bytes (at most {} KiB)",
            bytes.len(),
            crate::services::theme_file::MAX_SKIN_BYTES / 1024
        ));
    }
    let Ok(text) = String::from_utf8(bytes) else {
        return bad_request("The skin must be a UTF-8 text file".into());
    };
    if crate::services::theme_file::sanitize(&text, "upload").kept == 0 {
        return bad_request(
            "No theme variables found; a skin sets variables like --color-accent: #c0392b;".into(),
        );
    }
    let resolver = try_response!(resolve_tasks_dir());
    let dir = resolver.path.join(branding_service::UPLOAD_DIR);
    if let Err(err) = std::fs::create_dir_all(&dir) {
        return internal_error(format!("Could not create {}: {err}", dir.display()));
    }
    let target = dir.join("theme.css");
    if let Err(err) = std::fs::write(&target, text) {
        return internal_error(format!("Could not write {}: {err}", target.display()));
    }
    let root = branding_service::workspace_root(&resolver.path);
    match target.strip_prefix(&root) {
        Ok(relative) => ok_json(
            200,
            json!({"data": crate::api_types::BrandingUploadResponse { path: relative.to_string_lossy().replace('\\', "/") }}),
        ),
        Err(_) => bad_request(
            "The tasks folder is outside the repository; set the path in the config instead".into(),
        ),
    }
}

fn problem_text(image: BrandingImage, relative: &str, err: &BrandingFileError) -> String {
    let what = match image {
        BrandingImage::Logo => "Logo",
        BrandingImage::LogoDark => "Dark logo",
        BrandingImage::Icon => "Icon",
    };
    match err {
        BrandingFileError::Invalid(message) => format!("{what}: {message}"),
        BrandingFileError::Missing => format!("{what}: {relative} was not found in the repository"),
        BrandingFileError::TooLarge => format!(
            "{what}: {relative} is larger than {} KiB",
            branding_service::MAX_BRANDING_BYTES / 1024
        ),
        BrandingFileError::NotAnImage => format!("{what}: {relative} is not an image"),
    }
}

fn branding_response(
    resolver: &crate::workspace::TasksDirectoryResolver,
    config: &crate::config::types::ResolvedConfig,
) -> crate::api_types::BrandingResponse {
    let root = branding_service::workspace_root(&resolver.path);
    let mut problems = Vec::new();
    // The repository skin: usable when it has at least one theme variable.
    let mut theme_file = None;
    if let Some(relative) = config.ui.theme_file.as_deref() {
        match crate::services::theme_file::read_skin(&root, relative) {
            Ok((_, skin)) if skin.kept > 0 => {
                if skin.dropped > 0 {
                    problems.push(format!(
                        "Skin: {} declarations in {relative} were ignored; only theme variables (--name: value) without url() are used",
                        skin.dropped
                    ));
                }
                theme_file = Some(relative.to_string());
            }
            Ok(_) => problems.push(format!("Skin: {relative} sets no theme variables")),
            Err(crate::services::theme_file::SkinError::Invalid(message)) => {
                problems.push(format!("Skin: {message}"));
            }
            Err(crate::services::theme_file::SkinError::Missing) => {
                problems.push(format!("Skin: {relative} was not found in the repository"));
            }
            Err(crate::services::theme_file::SkinError::TooLarge) => problems.push(format!(
                "Skin: {relative} is larger than {} KiB",
                crate::services::theme_file::MAX_SKIN_BYTES / 1024
            )),
            Err(crate::services::theme_file::SkinError::NotText) => {
                problems.push(format!("Skin: {relative} is not a UTF-8 text file"));
            }
        }
    }
    let mut url_for = |image: BrandingImage| -> Option<String> {
        let relative = image.configured(config)?;
        match branding_service::read_image(&root, relative) {
            // The version makes a changed file a new URL for the <img> tags.
            Ok((_, bytes)) => Some(format!(
                "/api/branding/{}?v={}",
                image.slug(),
                &blake3::hash(&bytes).to_hex()[..12]
            )),
            Err(err) => {
                problems.push(problem_text(image, relative, &err));
                None
            }
        }
    };
    let logo = url_for(BrandingImage::Logo);
    let logo_dark = url_for(BrandingImage::LogoDark);
    let icon = url_for(BrandingImage::Icon);
    let theme = config
        .ui
        .theme
        .clone()
        .filter(|theme| branding_service::is_valid_ui_theme(theme));
    // ~/.lotar overrides the repository's config.yml (see resolution.rs).
    let valid =
        |theme: Option<String>| theme.filter(|theme| branding_service::is_valid_ui_theme(theme));
    let home_theme = valid(
        crate::config::persistence::load_home_config()
            .ok()
            .and_then(|home| home.ui_theme),
    );
    let repository_theme = valid(
        crate::config::persistence::load_global_config(Some(resolver.path.as_path()))
            .ok()
            .and_then(|global| global.ui_theme),
    );
    let theme_source = match (&theme, &home_theme, &repository_theme) {
        (Some(theme), Some(home), _) if theme == home => Some("home".to_string()),
        (Some(theme), _, Some(repo)) if theme == repo => Some("repository".to_string()),
        _ => None,
    };
    let repository_theme = if theme_source.as_deref() == Some("home") {
        repository_theme
    } else {
        None
    };
    crate::api_types::BrandingResponse {
        theme,
        theme_source,
        repository_theme,
        theme_file,
        name: config
            .ui
            .name
            .clone()
            .filter(|name| !name.trim().is_empty()),
        logo,
        logo_dark,
        icon,
        problems,
    }
}
