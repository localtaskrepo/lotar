use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

mod common;
use common::env_mutex::lock_var;

#[test]
fn config_set_global_categories_and_tags_and_defaults() {
    let _env_tasks = lock_var("LOTAR_TASKS_DIR");
    let temp = TempDir::new().unwrap();
    let tasks_dir = temp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();

    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().to_string());
        std::env::set_var("LOTAR_TEST_SILENT", "1");
    }

    // Set categories, tags, default_category, default_tags in global
    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .args(["config", "set", "categories", "Feat,Bug,Chore", "--global"])
        .assert()
        .success();

    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .args(["config", "set", "tags", "team,backend,ui", "--global"])
        .assert()
        .success();

    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .args(["config", "set", "default_category", "Bug", "--global"])
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
            predicate::str::contains("issue:")
                .and(predicate::str::contains("categories:"))
                .and(predicate::str::contains("- Feat"))
                .and(predicate::str::contains("- Bug"))
                .and(predicate::str::contains("- Chore"))
                .and(predicate::str::contains("tags:"))
                .and(predicate::str::contains("- team"))
                .and(predicate::str::contains("- backend"))
                .and(predicate::str::contains("- ui"))
                .and(predicate::str::contains("default:"))
                .and(predicate::str::contains("category: Bug"))
                .and(
                    predicate::str::contains("tags:")
                        .and(predicate::str::contains("- team"))
                        .and(predicate::str::contains("- ui")),
                ),
        );

    unsafe {
        std::env::remove_var("LOTAR_TASKS_DIR");
        std::env::remove_var("LOTAR_TEST_SILENT");
    }
}

#[test]
fn config_set_project_categories_and_tags_and_defaults() {
    let _env_tasks = lock_var("LOTAR_TASKS_DIR");
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

    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().to_string());
        std::env::set_var("LOTAR_TEST_SILENT", "1");
    }

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
        .success();

    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .args(["config", "set", "tags", "team,backend"])
        .assert()
        .success();

    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .args(["config", "set", "default_category", "Bugfix"])
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
    assert!(contents.contains("issue:"));
    assert!(contents.contains("categories:"));
    assert!(contents.contains("- Feat"));
    assert!(contents.contains("- Bugfix"));
    assert!(contents.contains("tags:"));
    assert!(contents.contains("- team"));
    assert!(contents.contains("- backend"));
    assert!(contents.contains("default:"));
    assert!(contents.contains("category: Bugfix"));
    assert!(contents.contains("tags:"));

    unsafe {
        std::env::remove_var("LOTAR_TASKS_DIR");
        std::env::remove_var("LOTAR_TEST_SILENT");
    }
}
