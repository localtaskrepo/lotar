use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

mod common;
use common::env_mutex::lock_var;

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
    let _env_tasks = lock_var("LOTAR_TASKS_DIR");

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

    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().to_string());
    }

    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp.path())
        .args(["whoami"]) // text mode
        .assert()
        .success()
        .stdout(predicate::str::contains("manifest-user"));

    unsafe {
        std::env::remove_var("LOTAR_TASKS_DIR");
    }
}
