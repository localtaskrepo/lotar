//! DEV-218: repository branding for the web UI — `ui.*` settings, the
//! /api/branding endpoints, and uploads.
use base64::Engine;
use lotar::api_server::{ApiServer, HttpRequest, HttpResponse};
use lotar::routes;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;
mod common;
use crate::common::env_mutex::EnvVarGuard;

struct Fixture {
    _tmp: tempfile::TempDir,
    repo: PathBuf,
    tasks_dir: PathBuf,
    _guard_tasks: EnvVarGuard,
}

const SVG: &[u8] = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"/>";

fn workspace(config: &str) -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    let tasks_dir = repo.join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    std::fs::create_dir_all(repo.join("assets")).unwrap();
    std::fs::write(tasks_dir.join("config.yml"), config).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
    Fixture {
        _tmp: tmp,
        repo,
        tasks_dir,
        _guard_tasks,
    }
}

fn server() -> ApiServer {
    let mut api = ApiServer::new();
    routes::initialize(&mut api);
    api
}

fn get(api: &ApiServer, path: &str) -> HttpResponse {
    api.handle_request(&HttpRequest {
        method: "GET".into(),
        path: path.into(),
        query: HashMap::new(),
        headers: HashMap::new(),
        body: Vec::new(),
    })
}

fn post(api: &ApiServer, path: &str, body: Value) -> HttpResponse {
    api.handle_request(&HttpRequest {
        method: "POST".into(),
        path: path.into(),
        query: HashMap::new(),
        headers: HashMap::new(),
        body: serde_json::to_vec(&body).unwrap(),
    })
}

fn data(resp: &HttpResponse) -> Value {
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    serde_json::from_slice::<Value>(&resp.body).unwrap()["data"].clone()
}

fn header<'a>(resp: &'a HttpResponse, name: &str) -> Option<&'a str> {
    resp.headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

#[test]
fn branding_reports_settings_and_serves_repository_images() {
    let fx = workspace(
        "default:\n  project: APP\nui:\n  theme: forest\n  name: Acme Tracker\n  logo: assets/logo.svg\n  logo_dark: assets/missing.svg\n  icon: assets/fake.png\n",
    );
    std::fs::write(fx.repo.join("assets/logo.svg"), SVG).unwrap();
    std::fs::write(fx.repo.join("assets/fake.png"), b"not a png").unwrap();
    let api = server();

    let branding = data(&get(&api, "/api/branding"));
    assert_eq!(branding["theme"], "forest");
    // DEV-219: say where the default comes from (home config is ignored in tests).
    assert_eq!(branding["theme_source"], "repository");
    assert!(branding.get("repository_theme").is_none());
    assert_eq!(branding["name"], "Acme Tracker");
    assert!(
        branding["logo"]
            .as_str()
            .unwrap()
            .starts_with("/api/branding/logo?v=")
    );
    assert!(branding.get("logo_dark").is_none());
    assert!(branding.get("icon").is_none());
    let problems: Vec<&str> = branding["problems"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(problems.len(), 2, "{problems:?}");
    assert!(
        problems
            .iter()
            .any(|p| p.contains("assets/missing.svg was not found"))
    );
    assert!(
        problems
            .iter()
            .any(|p| p.contains("assets/fake.png is not an image"))
    );

    let logo = get(&api, "/api/branding/logo");
    assert_eq!(logo.status, 200);
    assert_eq!(logo.body, SVG);
    assert_eq!(header(&logo, "content-type"), Some("image/svg+xml"));
    assert!(
        header(&logo, "content-security-policy")
            .unwrap()
            .contains("sandbox")
    );
    assert_eq!(header(&logo, "cache-control"), Some("no-cache"));

    let etag = header(&logo, "etag").unwrap().to_string();
    let mut conditional = HttpRequest {
        method: "GET".into(),
        path: "/api/branding/logo".into(),
        query: HashMap::new(),
        headers: HashMap::new(),
        body: Vec::new(),
    };
    conditional.headers.insert("if-none-match".into(), etag);
    assert_eq!(api.handle_request(&conditional).status, 304);

    assert_eq!(get(&api, "/api/branding/icon").status, 404);
    assert_eq!(get(&api, "/api/branding/logo-dark").status, 404);
    assert_eq!(get(&api, "/api/branding/wallpaper").status, 404);
}

#[test]
fn settings_are_validated_and_stay_inside_the_repository() {
    let fx = workspace("default:\n  project: APP\n");
    let api = server();
    for (field, value) in [
        ("ui.theme", "neon"),
        ("ui.logo", "../outside.png"),
        ("ui.logo", "/etc/logo.png"),
        ("ui.icon", "assets/readme.md"),
    ] {
        let resp = post(
            &api,
            "/api/config/set",
            json!({"global": true, "values": {field: value}}),
        );
        assert_eq!(
            resp.status,
            400,
            "{field}={value}: {}",
            String::from_utf8_lossy(&resp.body)
        );
    }

    let resp = post(
        &api,
        "/api/config/set",
        json!({"global": true, "values": {"ui.theme": "rose", "ui.name": "Shop"}}),
    );
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    let saved = std::fs::read_to_string(fx.tasks_dir.join("config.yml")).unwrap();
    assert!(
        saved.contains("ui:") && saved.contains("theme: rose") && saved.contains("name: Shop"),
        "{saved}"
    );
    assert_eq!(data(&get(&api, "/api/branding"))["theme"], "rose");
}

#[cfg(unix)]
#[test]
fn symlinks_out_of_the_repository_are_not_served() {
    let fx = workspace("default:\n  project: APP\nui:\n  logo: assets/link.png\n");
    let outside = fx.repo.parent().unwrap().join("private.png");
    std::fs::write(&outside, b"\x89PNG\r\n\x1a\nprivate").unwrap();
    std::os::unix::fs::symlink(&outside, fx.repo.join("assets/link.png")).unwrap();
    let api = server();
    assert_eq!(get(&api, "/api/branding/logo").status, 404);
    assert!(data(&get(&api, "/api/branding")).get("logo").is_none());
}

#[test]
fn uploads_store_the_image_in_the_repository_and_point_the_setting_at_it() {
    let fx = workspace("default:\n  project: APP\n");
    let api = server();
    let upload = |image: &str, filename: &str, bytes: &[u8]| {
        post(
            &api,
            "/api/branding/upload",
            json!({
                "image": image,
                "filename": filename,
                "content_base64": base64::engine::general_purpose::STANDARD.encode(bytes),
            }),
        )
    };

    let stored = data(&upload("logo", "Brand Mark.svg", SVG));
    assert_eq!(stored["path"], ".tasks/@branding/logo.svg");
    assert_eq!(
        std::fs::read(fx.tasks_dir.join("@branding/logo.svg")).unwrap(),
        SVG
    );
    // Storing a file does not change the settings; the Config page saves them.
    let saved = std::fs::read_to_string(fx.tasks_dir.join("config.yml")).unwrap();
    assert!(!saved.contains("logo"), "{saved}");
    let resp = post(
        &api,
        "/api/config/set",
        json!({"global": true, "values": {"ui.logo": ".tasks/@branding/logo.svg"}}),
    );
    assert_eq!(resp.status, 200);
    assert_eq!(get(&api, "/api/branding/logo").body, SVG);

    // A new upload in another format replaces the old file.
    let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x02\0\0\0\x90wS\xde\0\0\0\x0cIDAT\x08\xd7c\xf8\xff\xff?\0\x05\xfe\x02\xfe\xdc\xccY\xe7\0\0\0\0IEND\xaeB`\x82";
    data(&upload("logo", "logo.png", png));
    assert!(!fx.tasks_dir.join("@branding/logo.svg").exists());
    assert!(fx.tasks_dir.join("@branding/logo.png").exists());

    assert_eq!(upload("logo", "notes.txt", b"hello").status, 400);
    assert_eq!(upload("logo", "fake.png", b"hello").status, 400);
    assert_eq!(upload("banner", "logo.svg", SVG).status, 400);
}
