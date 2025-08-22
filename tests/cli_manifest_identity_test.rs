use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

mod common;
use crate::common::env_mutex::EnvVarGuard;
use lotar::utils::paths;

fn write_minimal_config_without_reporter(tasks_dir: &std::path::Path) {
    let content = r#"default.project: TEST
issue.states: [Todo, InProgress, Done]
issue.types: [Feature, Bug, Chore]
issue.priorities: [Low, Medium, High]
"#;
    std::fs::write(paths::global_config_path(tasks_dir), content).unwrap();
}

#[test]
fn whoami_uses_project_manifest_author_when_no_default_reporter() {
    // Use EnvVarGuard; no separate lock_var to avoid double-locking

    let temp = TempDir::new().unwrap();
    let tasks_dir = temp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();

    // No default_reporter in config
    write_minimal_config_without_reporter(&tasks_dir);

    // Create a package.json with author
    let pkg = temp.path().join("package.json");
    let pkg_contents = r#"{
    "name": "demo",
    "version": "0.0.1",
    "author": {
        "name": "manifest-user",
        "email": "m@example.com"
    }
}
"#;
    std::fs::write(&pkg, pkg_contents).unwrap();

    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .args(["whoami"]) // text mode
        .assert()
        .success()
        .stdout(predicate::str::contains("manifest-user"));
}
