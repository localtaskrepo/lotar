//! DEV-549: the web UI's file viewer reads repository files named in ticket
//! text through `/api/files/resolve` and `/api/files/content`.
use crate::common::env_mutex::EnvVarGuard;
use lotar::api_server::{ApiServer, HttpRequest, HttpResponse};
use lotar::routes;
use serde_json::{Value, json};
use std::collections::HashMap;

struct Fixture {
    _tmp: tempfile::TempDir,
    _guard_tasks: EnvVarGuard,
    _guard_fast: EnvVarGuard,
}

fn repository() -> Fixture {
    let _guard_fast = EnvVarGuard::set("LOTAR_TEST_FAST_IO", "1");
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    for (path, text) in [
        (".gitignore", ".env\n"),
        (".env", "SECRET=1\n"),
        ("docs/guide.md", "# Guide\n\nSee [the code](../src/main.rs).\n"),
        ("src/main.rs", "fn main() {}\n"),
        ("page.html", "<script>alert(1)</script>\n"),
    ] {
        let full = root.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, text).unwrap();
    }
    std::fs::write(root.join("logo.png"), [0x89, b'P', b'N', b'G', 0, 0]).unwrap();
    std::fs::write(root.join("blob.bin"), [0u8, 1, 2, 3]).unwrap();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    let tasks_dir = root.join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
    Fixture { _tmp: tmp, _guard_tasks, _guard_fast }
}

fn server() -> ApiServer {
    let mut api = ApiServer::new();
    routes::initialize(&mut api);
    api
}

fn request(method: &str, path: &str, query: &[(&str, &str)], body: Value) -> HttpRequest {
    HttpRequest {
        method: method.to_string(),
        path: path.to_string(),
        query: query.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect::<HashMap<_, _>>(),
        headers: HashMap::new(),
        body: serde_json::to_vec(&body).unwrap(),
    }
}

fn header<'a>(resp: &'a HttpResponse, name: &str) -> Option<&'a str> {
    resp.headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
}

fn content(api: &ApiServer, path: &str) -> HttpResponse {
    api.handle_request(&request("GET", "/api/files/content", &[("path", path)], Value::Null))
}

#[test]
fn resolve_links_only_visible_files() {
    let _fixture = repository();
    let api = server();
    let resp = api.handle_request(&request(
        "POST",
        "/api/files/resolve",
        &[],
        json!({"references": ["docs/guide.md", "main.rs", ".env", "missing.md", "../etc/passwd"]}),
    ));
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    let body: Value = serde_json::from_slice(&resp.body).unwrap();
    assert_eq!(body["data"]["items"], json!([{"reference": "docs/guide.md", "path": "docs/guide.md"}, {"reference": "main.rs", "path": "src/main.rs"}]));

    let relative = api.handle_request(&request("POST", "/api/files/resolve", &[], json!({"base": "docs", "references": ["../src/main.rs"]})));
    let body: Value = serde_json::from_slice(&relative.body).unwrap();
    assert_eq!(body["data"]["items"][0]["path"], "src/main.rs");
}

#[test]
fn content_serves_text_images_and_binaries_safely() {
    let _fixture = repository();
    let api = server();

    let markdown = content(&api, "docs/guide.md");
    assert_eq!(markdown.status, 200);
    assert_eq!(header(&markdown, "Content-Type"), Some("text/plain; charset=utf-8"));
    assert!(markdown.body.starts_with(b"# Guide"));

    // HTML is never served as HTML on the app's origin.
    assert_eq!(header(&content(&api, "page.html"), "Content-Type"), Some("text/plain; charset=utf-8"));
    assert_eq!(header(&content(&api, "logo.png"), "Content-Type"), Some("image/png"));
    assert_eq!(header(&content(&api, "blob.bin"), "Content-Type"), Some("application/octet-stream"));

    let download = api.handle_request(&request("GET", "/api/files/content", &[("path", "src/main.rs"), ("download", "1")], Value::Null));
    assert_eq!(header(&download, "Content-Disposition"), Some("attachment; filename=\"main.rs\""));
}

#[test]
fn content_refuses_ignored_and_outside_paths() {
    let _fixture = repository();
    let api = server();
    for path in [".env", ".git", "../outside.txt", "/etc/hosts", "docs"] {
        assert_eq!(content(&api, path).status, 404, "{path}");
    }
    assert_eq!(api.handle_request(&request("GET", "/api/files/content", &[], Value::Null)).status, 400);
}
