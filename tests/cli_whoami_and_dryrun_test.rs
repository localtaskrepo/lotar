use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

mod common;
use crate::common::env_mutex::EnvVarGuard;

use lotar::api_types::TaskCreate;
use lotar::services::task_service::TaskService;
use lotar::storage::manager::Storage;
use lotar::utils::paths;

fn write_minimal_config(tasks_dir: &std::path::Path, extra: &str) {
    // Provide minimal, valid lists so CLI validation passes where needed
    let base = r#"default.project: TEST
issue.states: [Todo, InProgress, Done]
issue.types: [Feature, Bug, Chore]
issue.priorities: [Low, Medium, High]
"#;
    let mut content = String::from(base);
    if !extra.is_empty() {
        content.push_str(extra);
        if !extra.ends_with('\n') {
            content.push('\n');
        }
    }
    std::fs::write(paths::global_config_path(tasks_dir), content).unwrap();
}

#[test]
fn whoami_uses_config_default_reporter() {
    // EnvVarGuard serializes the env var safely

    let temp = TempDir::new().unwrap();
    let tasks_dir = temp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();

    // Configure a deterministic reporter identity
    write_minimal_config(&tasks_dir, "default.reporter: alice@example.com\n");

    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp.path())
        .args(["whoami"]) // text mode
        .assert()
        .success()
        .stdout(predicate::str::contains("alice@example.com"));

    // restored by _guard drop
}

#[test]
fn whoami_explain_text_includes_source_and_confidence() {
    // EnvVarGuard serializes the env var safely

    let temp = TempDir::new().unwrap();
    let tasks_dir = temp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();

    // Deterministic reporter identity
    write_minimal_config(&tasks_dir, "default.reporter: carol\n");

    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp.path())
        .args(["whoami", "--explain"]) // text mode
        .assert()
        .success()
        .stdout(predicate::str::contains("carol"))
        .stdout(predicate::str::contains("source:"))
        .stdout(predicate::str::contains("confidence:"));

    // restored by _guard drop
}

#[test]
fn whoami_json_compact_and_explain_fields() {
    // EnvVarGuard serializes the env var safely

    let temp = TempDir::new().unwrap();
    let tasks_dir = temp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();

    write_minimal_config(&tasks_dir, "default.reporter: dave\n");

    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

    // Compact JSON (no extra fields)
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp.path())
        .args(["whoami", "--format=json"]) // json mode
        .assert()
        .success()
        .stdout(predicate::str::contains("{\"user\":\"dave\"}"));

    // Explain JSON includes source/confidence/details
    let mut cmd2 = Command::cargo_bin("lotar").unwrap();
    cmd2.current_dir(temp.path())
        .args(["whoami", "--format=json", "--explain"]) // json + explain
        .assert()
        .success()
        .stdout(predicate::str::contains("\"user\":"))
        .stdout(predicate::str::contains("\"source\":"))
        .stdout(predicate::str::contains("\"confidence\":"));

    // restored by _guard drop
}

#[test]
fn status_dry_run_explain_previews_and_does_not_write() {
    // EnvVarGuard serializes the env var safely

    let temp = TempDir::new().unwrap();
    let tasks_dir = temp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();

    // Default reporter used for auto-assign preview
    write_minimal_config(&tasks_dir, "default.reporter: bob\n");

    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

    // Create a task via service to get a known ID
    let mut storage = Storage::new(tasks_dir.clone());
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Preview status change".to_string(),
            project: Some("TEST".to_string()),
            priority: None,
            task_type: None,
            reporter: None,
            assignee: None,
            due_date: None,
            effort: None,
            description: None,
            tags: vec![],
            relationships: None,
            custom_fields: None,
        },
    )
    .expect("create task");

    // Dry-run with explain should preview change and show auto-assign effect
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp.path())
        .args(["status", &created.id, "Done", "--dry-run", "--explain"])
        .assert()
        .success()
        .stdout(predicate::str::contains("DRY RUN: Would change"))
        .stdout(predicate::str::contains("assignee"))
        .stdout(predicate::str::contains("bob"))
        .stdout(predicate::str::contains("Explanation:"));

    // Verify status wasn't changed (get path prints current status)
    let mut cmd2 = Command::cargo_bin("lotar").unwrap();
    cmd2.current_dir(temp.path())
        .args(["status", &created.id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Task "))
        .stdout(predicate::str::contains(" status: "))
        .stdout(predicate::str::contains("Todo"));

    // restored by _guard drop
}

#[test]
fn add_dry_run_explain_creates_no_task_files() {
    // EnvVarGuard serializes the env var safely

    let temp = TempDir::new().unwrap();
    let tasks_dir = temp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();

    write_minimal_config(&tasks_dir, "");

    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

    // Snapshot YAML files before
    let before_count = count_task_yaml_files(&tasks_dir);

    // Run add in dry-run mode
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp.path())
        .args([
            "add",
            "Dry run task",
            "--project=TEST",
            "--dry-run",
            "--explain",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("DRY RUN: Would create task"))
        .stdout(predicate::str::contains("Explanation:"));

    // No new YAML files should be written
    let after_count = count_task_yaml_files(&tasks_dir);
    assert_eq!(
        before_count, after_count,
        "dry-run should not write task files"
    );

    // restored by _guard drop
}

fn count_task_yaml_files(tasks_dir: &std::path::Path) -> usize {
    // Count all project task YAML files (exclude top-level config.yml)
    let mut count = 0usize;
    if let Ok(read) = std::fs::read_dir(tasks_dir) {
        for entry in read.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // inside project dir
                if let Ok(inner) = std::fs::read_dir(&path) {
                    for f in inner.flatten() {
                        let fp = f.path();
                        if fp.is_file() {
                            let name = fp.file_name().and_then(|s| s.to_str()).unwrap_or("");
                            let ext = fp.extension().and_then(|s| s.to_str()).unwrap_or("");
                            if ext.eq_ignore_ascii_case("yml") && name != "config.yml" {
                                count += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    count
}
