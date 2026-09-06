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
    // Manifest authors are a last-resort identity source: they must only be
    // used when neither git identity nor an OS username is available.
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

    // With an OS username present, whoami must prefer it over the manifest.
    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp.path())
        .args(["whoami"]) // text mode
        .assert()
        .success()
        .stdout(predicate::str::contains(
            std::env::var("USER").unwrap_or_default(),
        ));

    // Without git or OS usernames, the manifest author is the only source left.
    crate::common::lotar_cmd()
        .unwrap()
        .env_remove("USER")
        .env_remove("USERNAME")
        .current_dir(temp.path())
        .args(["whoami"]) // text mode
        .assert()
        .success()
        .stdout(predicate::str::contains("manifest-user"));
}
