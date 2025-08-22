use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

#[test]
fn derive_label_from_monorepo_paths() {
    let tmp = tempfile::TempDir::new().unwrap();
    let base = tmp.path().join("repo");
    fs::create_dir_all(base.join("packages").join("api")).unwrap();
    let api = base.join("packages").join("api");
    let label = lotar::utils::workspace_labels::derive_path_label_from(&api);
    assert_eq!(label.as_deref(), Some("api"));

    fs::create_dir_all(base.join("apps").join("web")).unwrap();
    let web = base.join("apps").join("web");
    let label2 = lotar::utils::workspace_labels::derive_path_label_from(&web);
    assert_eq!(label2.as_deref(), Some("web"));

    // Hidden names should be ignored
    fs::create_dir_all(base.join("packages").join(".hidden")).unwrap();
    let hidden = base.join("packages").join(".hidden");
    let none = lotar::utils::workspace_labels::derive_path_label_from(&hidden);
    assert!(none.is_none());
}

static CWD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn with_cwd(dir: &PathBuf, f: impl FnOnce()) {
    let lock = CWD_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock.lock().unwrap();
    let old = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir).unwrap();
    f();
    std::env::set_current_dir(old).unwrap();
}

#[test]
fn detect_project_name_prefers_nearest_package_json() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().join("repo");
    fs::create_dir_all(root.join(".git")).unwrap(); // mark as repo root
    let pkg_root = root.join("package.json");
    fs::write(&pkg_root, r#"{ "name": "monorepo" }"#).unwrap();

    // packages/api has its own package.json
    let api_dir = root.join("packages").join("api");
    fs::create_dir_all(&api_dir).unwrap();
    fs::write(api_dir.join("package.json"), r#"{ "name": "api" }"#).unwrap();

    with_cwd(&api_dir, || {
        let name = lotar::project::detect_project_name();
        assert_eq!(name.as_deref(), Some("api"));
    });
}

#[test]
fn detect_project_name_from_cargo_package() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("crate");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("Cargo.toml"),
        r#"[package]
name = "awesome_crate"
version = "0.1.0"
"#,
    )
    .unwrap();

    with_cwd(&dir, || {
        let name = lotar::project::detect_project_name();
        assert_eq!(name.as_deref(), Some("awesome_crate"));
    });
}

#[test]
fn detect_project_name_from_go_mod() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("goproj");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("go.mod"),
        r#"module github.com/acme/tooling/service

go 1.22
"#,
    )
    .unwrap();

    with_cwd(&dir, || {
        let name = lotar::project::detect_project_name();
        assert_eq!(name.as_deref(), Some("service"));
    });
}
