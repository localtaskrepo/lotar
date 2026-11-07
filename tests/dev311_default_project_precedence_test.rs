//! DEV-311: a configured `default.project` wins over the nearest manifest
//! name, for `add` and for REST/MCP creates without a project. A project
//! picked by detection is announced with its prefix, and `add --explain`
//! names where the project came from.

use crate::common;
use common::env_mutex::EnvVarGuard;
use lotar::api_types::TaskCreate;
use lotar::services::task_service::TaskService;
use std::path::Path;

/// A repo like Gairola's: `default.project: G`, a root `package.json`
/// named `gairola` and `site/package.json` named `gairola-site`.
fn workspace() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    std::fs::create_dir_all(root.join(".tasks")).unwrap();
    std::fs::create_dir_all(root.join("site")).unwrap();
    std::fs::write(root.join(".tasks/config.yml"), "default:\n  project: G\n").unwrap();
    std::fs::write(root.join("package.json"), r#"{"name":"gairola"}"#).unwrap();
    std::fs::write(root.join("site/package.json"), r#"{"name":"gairola-site"}"#).unwrap();
    tmp
}

fn lotar(dir: &Path, args: &[&str]) -> std::process::Output {
    common::lotar_cmd()
        .unwrap()
        .current_dir(dir)
        .env("LOTAR_DEFAULT_REPORTER", "tester")
        .env_remove("LOTAR_PROJECT")
        .env_remove("LOTAR_DEFAULT_PROJECT")
        .args(args)
        .output()
        .unwrap()
}

fn projects(tasks: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(tasks)
        .unwrap()
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| !name.starts_with('@'))
        .collect();
    names.sort();
    names
}

#[test]
fn configured_default_beats_the_nearest_package_json() {
    let tmp = workspace();
    let root = tmp.path();

    for dir in [root.to_path_buf(), root.join("site")] {
        let out = lotar(&dir, &["add", "Plain title", "--dry-run", "--explain"]);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "{stdout}");
        assert!(stdout.contains("in project G "), "{stdout}");
        assert!(
            stdout.contains("project G from default.project"),
            "{stdout}"
        );

        let out = lotar(&dir, &["add", "Plain title"]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            out.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    assert_eq!(projects(&root.join(".tasks")), ["G"]);
    assert!(root.join(".tasks/G/2.yml").exists());

    // JSON explain carries the source as its own field.
    let out = lotar(
        root,
        &["add", "x", "--dry-run", "--explain", "--format", "json"],
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["project"], "G");
    assert_eq!(
        value["project_source"],
        "default.project in the configuration"
    );

    // The environment and the flag still win over the configured default.
    let out = lotar(root, &["add", "x", "--dry-run", "--explain", "-p", "OPS"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("project OPS from --project"), "{stdout}");
    let out = common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .env("LOTAR_PROJECT", "ENVP")
        .args(["add", "x", "--dry-run", "--explain"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("project ENVP from the LOTAR_PROJECT environment variable"),
        "{stdout}"
    );
}

#[test]
fn a_project_from_detection_is_announced() {
    let tmp = tempfile::tempdir().unwrap();
    let web = tmp.path().join("web");
    std::fs::create_dir_all(tmp.path().join(".git")).unwrap();
    std::fs::create_dir_all(&web).unwrap();
    std::fs::write(web.join("package.json"), r#"{"name":"@acme/web"}"#).unwrap();

    // A dry run warns too, and writes nothing (it used to save the
    // detected prefix as default.project).
    let out = lotar(&web, &["add", "first", "--dry-run"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(
        stderr.contains("would create a new project WEB")
            && stderr.contains("package.json")
            && stderr.contains("--project"),
        "{stderr}"
    );
    assert!(!web.join(".tasks").exists());

    let out = lotar(&web, &["add", "first"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("creates a new project WEB"), "{stderr}");
    assert_eq!(projects(&web.join(".tasks")), ["WEB"]);

    // Once the project exists, adding to it says nothing.
    let out = lotar(&web, &["add", "second"]);
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(projects(&web.join(".tasks")), ["WEB"]);
}

#[test]
fn service_create_without_a_project_uses_the_default() {
    let tmp = workspace();
    let tasks = tmp.path().join(".tasks");
    let _home = EnvVarGuard::set("LOTAR_IGNORE_HOME_CONFIG", "1");
    let _env = EnvVarGuard::clear("LOTAR_PROJECT");
    let _env2 = EnvVarGuard::clear("LOTAR_DEFAULT_PROJECT");

    let mut storage = lotar::storage::manager::Storage::new(&tasks);
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "From MCP".into(),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(created.id, "G-1");
    assert_eq!(projects(&tasks), ["G"]);
}
