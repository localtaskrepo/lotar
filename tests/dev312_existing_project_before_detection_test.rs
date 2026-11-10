//! DEV-312: `add` and REST/MCP creates use an existing project before any
//! name detection. A project counts when its folder has a `config.yml` or
//! task files, so a checkout whose project holds only tickets never gets a
//! second project named after the repository or `package.json`.

use crate::common;
use common::env_mutex::EnvVarGuard;
use lotar::api_types::TaskCreate;
use lotar::services::task_service::TaskService;
use std::path::Path;

/// A repo like Foundera's: a pnpm workspace with a root `package.json`
/// named `foundera`, `infra/package.json` named `@foundera/infra`, and
/// project FND with tickets. `config` is `.tasks/config.yml`, if any.
fn foundera(config: Option<&str>, project_config: bool) -> tempfile::TempDir {
    let tmp = tempfile::Builder::new()
        .prefix("foundera")
        .tempdir()
        .unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    std::fs::create_dir_all(root.join(".tasks/FND")).unwrap();
    std::fs::create_dir_all(root.join(".tasks/@sprints")).unwrap();
    std::fs::create_dir_all(root.join("infra")).unwrap();
    std::fs::write(root.join("package.json"), r#"{"name":"foundera"}"#).unwrap();
    std::fs::write(
        root.join("infra/package.json"),
        r#"{"name":"@foundera/infra"}"#,
    )
    .unwrap();
    std::fs::write(root.join("pnpm-workspace.yaml"), "packages:\n  - infra\n").unwrap();
    if let Some(config) = config {
        std::fs::write(root.join(".tasks/config.yml"), config).unwrap();
    }
    if project_config {
        std::fs::write(
            root.join(".tasks/FND/config.yml"),
            "project:\n  name: FND\n",
        )
        .unwrap();
    }
    for n in 1..=3 {
        std::fs::write(
            root.join(format!(".tasks/FND/{n}.yml")),
            format!("title: Ticket {n}\nstatus: Todo\npriority: Medium\ntype: Task\n"),
        )
        .unwrap();
    }
    tmp
}

fn lotar(dir: &Path, args: &[&str]) -> std::process::Output {
    common::lotar_cmd()
        .unwrap()
        .current_dir(dir)
        .env("LOTAR_DEFAULT_REPORTER", "tester")
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
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

/// From the root and from `infra/`, `add` previews and writes FND, quietly.
/// The first preview names `source` (a real add may save FND as the default).
fn assert_adds_go_to_fnd(root: &Path, source: &str) {
    let out = lotar(root, &["add", "x", "--dry-run", "--explain"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(&format!("project FND from {source}")),
        "{stdout}"
    );
    for dir in [root.to_path_buf(), root.join("infra")] {
        let out = lotar(&dir, &["add", "Plain title", "--dry-run"]);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{stderr}");
        assert!(stdout.contains("in project FND "), "{stdout}");
        assert!(stderr.is_empty(), "{stderr}");

        let out = lotar(&dir, &["add", "Plain title"]);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{stderr}");
        assert!(stderr.is_empty(), "{stderr}");
    }
    assert_eq!(projects(&root.join(".tasks")), ["FND"]);
    assert!(root.join(".tasks/FND/5.yml").exists());
}

#[test]
fn foundera_default_project_beats_package_json() {
    let tmp = foundera(Some("default:\n  project: FND\n"), true);
    assert_adds_go_to_fnd(tmp.path(), "default.project");
}

#[test]
fn a_project_folder_with_only_tickets_is_used_without_a_default() {
    for config in [None, Some(""), Some("default:\n  priority: Medium\n")] {
        let tmp = foundera(config, false);
        assert_adds_go_to_fnd(tmp.path(), "the existing project");
    }
}

#[test]
fn service_create_without_a_default_uses_the_existing_project() {
    let _home = EnvVarGuard::set("LOTAR_IGNORE_HOME_CONFIG", "1");
    let _env = EnvVarGuard::clear("LOTAR_PROJECT");
    let _env2 = EnvVarGuard::clear("LOTAR_DEFAULT_PROJECT");
    // No config (storage writes one) and a config without a default.
    for config in [None, Some("default:\n  priority: Medium\n")] {
        let tmp = foundera(config, false);
        let tasks = tmp.path().join(".tasks");
        let mut storage = lotar::storage::manager::Storage::new(&tasks);
        let created = TaskService::create(
            &mut storage,
            TaskCreate {
                title: "From MCP".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(created.id, "FND-4");
        assert_eq!(projects(&tasks), ["FND"]);
    }
}
