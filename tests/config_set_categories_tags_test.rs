use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

mod common;
use common::env_mutex::EnvVarGuard;

#[test]
fn config_set_global_custom_fields_and_tags() {
    // EnvVarGuard serializes and restores env safely
    let temp = TempDir::new().unwrap();
    let tasks_dir = temp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let _tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
    let _silent = EnvVarGuard::set("LOTAR_TEST_SILENT", "1");

    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .args(["config", "set", "categories", "Feat,Bug,Chore", "--global"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Invalid global config field")
                .and(predicate::str::contains("'categories'")),
        );

    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .args(["config", "set", "tags", "team,backend,ui", "--global"])
        .assert()
        .success();

    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "config",
            "set",
            "custom_fields",
            "product,component",
            "--global",
        ])
        .assert()
        .success();

    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .args(["config", "set", "default_tags", "team,ui", "--global"])
        .assert()
        .success();

    // Normalize and check canonical output contains the values
    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .args(["config", "normalize", "--global"]) // no write: print
        .assert()
        .success()
        .stdout(
            predicate::str::contains("custom:")
                .and(predicate::str::contains("fields:"))
                .and(predicate::str::contains("- product"))
                .and(predicate::str::contains("- component"))
                .and(predicate::str::contains("issue:"))
                .and(predicate::str::contains("tags:"))
                .and(predicate::str::contains("- team"))
                .and(predicate::str::contains("- backend"))
                .and(predicate::str::contains("- ui"))
                .and(predicate::str::contains("default:"))
                .and(
                    predicate::str::contains("tags:")
                        .and(predicate::str::contains("- team"))
                        .and(predicate::str::contains("- ui")),
                )
                .and(predicate::str::contains("product")),
        );

    // restored by guards
}

#[test]
fn config_set_project_custom_fields_and_tags() {
    // EnvVarGuard serializes and restores env safely
    let temp = TempDir::new().unwrap();
    let tasks_dir = temp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();

    // Minimal global so project parsing works
    std::fs::write(
        tasks_dir.join("config.yml"),
        "issue.states: [Todo]\nissue.types: [Feature]\nissue.priorities: [Low]\n",
    )
    .unwrap();

    // Create project directory
    let proj_dir = tasks_dir.join("TEST");
    std::fs::create_dir_all(&proj_dir).unwrap();
    std::fs::write(proj_dir.join("config.yml"), "project.id: Test\n").unwrap();

    let _tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
    let _silent = EnvVarGuard::set("LOTAR_TEST_SILENT", "1");

    // Use global default_project to enable project set without explicit --project
    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .args(["config", "set", "default_project", "TEST", "--global"])
        .assert()
        .success();

    // Now set project-scoped fields (no --global)
    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .args(["config", "set", "categories", "Feat,Bugfix"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Invalid project config field")
                .and(predicate::str::contains("'categories'")),
        );

    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .args(["config", "set", "tags", "team,backend"])
        .assert()
        .success();

    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .args(["config", "set", "custom_fields", "product,feature"])
        .assert()
        .success();

    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .args(["config", "set", "default_tags", "team"])
        .assert()
        .success();

    // Normalize project and verify
    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .args(["config", "normalize", "--project", "TEST", "--write"]) // write so we can read file back
        .assert()
        .success();

    let contents = std::fs::read_to_string(proj_dir.join("config.yml")).unwrap();
    let doc: serde_yaml::Value = serde_yaml::from_str(&contents).expect("valid yaml");

    let custom_fields = doc
        .get("custom")
        .and_then(|v| v.get("fields"))
        .and_then(|v| v.as_sequence())
        .expect("custom.fields present");
    let field_names: Vec<String> = custom_fields
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();
    assert!(field_names.contains(&"product".to_string()));
    assert!(field_names.contains(&"feature".to_string()));

    let issue_tags = doc
        .get("issue")
        .and_then(|v| v.get("tags"))
        .and_then(|v| v.as_sequence())
        .expect("issue.tags present");
    let tag_names: Vec<String> = issue_tags
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();
    assert!(tag_names.contains(&"team".to_string()));
    assert!(tag_names.contains(&"backend".to_string()));

    let default_tags = doc
        .get("default")
        .and_then(|v| v.get("tags"))
        .and_then(|v| v.as_sequence())
        .expect("default.tags present");
    let default_names: Vec<String> = default_tags
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();
    assert!(default_names.contains(&"team".to_string()));
    assert!(doc.get("issue").and_then(|v| v.get("categories")).is_none());

    // restored by guards
}
