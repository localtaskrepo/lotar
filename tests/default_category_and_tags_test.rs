use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

mod common;
use common::env_mutex::EnvVarGuard;

fn write_global_with_defaults(tasks_dir: &std::path::Path) {
    let yaml = r#"
default.project: TEST
issue.states: [Todo, InProgress, Done]
issue.types: [Feature, Bug, Epic]
issue.priorities: [Low, Medium, High, Critical]
issue.categories: [Feat, Bugfix]
issue.tags: [team, backend, one, two]
default.category: Bugfix
default.tags: [team, backend]
"#;
    std::fs::create_dir_all(tasks_dir).unwrap();
    std::fs::write(tasks_dir.join("config.yml"), yaml).unwrap();
}

#[test]
fn add_uses_global_default_category_and_tags_when_missing() {
    // EnvVarGuard will serialize and restore env vars per-var
    let temp = TempDir::new().unwrap();
    let tasks_dir = temp.path().join(".tasks");

    write_global_with_defaults(&tasks_dir);

    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().as_ref());
    let _guard_silent = EnvVarGuard::set("LOTAR_TEST_SILENT", "1");

    // Dry-run JSON to avoid creating files; verify category/tags injected
    let assert = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "add",
            "Task with defaults",
            "--project=TEST",
            "--dry-run",
            "--format=json",
        ])
        .assert()
        .success();

    let output = String::from_utf8_lossy(&assert.get_output().stdout);
    // Expect category and tags present from defaults
    assert!(output.contains("\"category\":\"Bugfix\""), "JSON: {output}");
    assert!(
        output.contains("\"tags\":[\"team\",\"backend\"]"),
        "JSON: {output}"
    );

    // guards drop here
}

#[test]
fn add_user_values_override_defaults_for_category_and_tags() {
    // EnvVarGuard per-var lock
    let temp = TempDir::new().unwrap();
    let tasks_dir = temp.path().join(".tasks");

    write_global_with_defaults(&tasks_dir);

    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().as_ref());
    let _guard_silent = EnvVarGuard::set("LOTAR_TEST_SILENT", "1");

    // Provide explicit category and a single allowed tag; defaults must not be applied
    let assert = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "add",
            "Task with explicit values",
            "--project=TEST",
            "--category=Feat",
            "--tag",
            "team",
            "--dry-run",
            "--format=json",
        ])
        .assert()
        .success();

    let output = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(output.contains("\"category\":\"Feat\""), "JSON: {output}");
    // Only the provided tag should appear (no merging with defaults when any tag is supplied)
    assert!(output.contains("\"tags\":[\"team\"]"), "JSON: {output}");

    // guards drop here
}

#[test]
fn normalize_canonical_includes_default_category_and_tags_global() {
    let temp = TempDir::new().unwrap();
    let tasks = temp.path().join(".tasks");
    std::fs::create_dir_all(&tasks).unwrap();

    // Write dotted-form config to test canonicalization
    std::fs::write(
        tasks.join("config.yml"),
        "server.port: 8080\ndefault.project: TEST\nissue.states: [Todo]\nissue.types: [Feature]\nissue.priorities: [Low]\ndefault.category: Bugfix\ndefault.tags: [team, backend]\n",
    )
    .unwrap();

    // Dry run normalize should print nested default block with category/tags
    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .args(["config", "normalize"]) // no --write
        .assert()
        .success()
        .stdout(
            predicate::str::contains("default:")
                .and(predicate::str::contains("category: Bugfix"))
                .and(predicate::str::contains("tags:")),
        );
}

#[test]
fn normalize_global_flag_includes_default_category_and_tags() {
    // EnvVarGuard per-var lock
    let temp = tempfile::TempDir::new().unwrap();
    let tasks = temp.path().join(".tasks");
    std::fs::create_dir_all(&tasks).unwrap();

    // Write a minimal global config including defaults in dotted form
    std::fs::write(
        tasks.join("config.yml"),
        "server.port: 8080\ndefault.project: TEST\nissue.states: [Todo]\nissue.types: [Feature]\nissue.priorities: [Low]\ndefault.category: Bugfix\ndefault.tags: [team]\n",
    )
    .unwrap();

    // Pin to this tasks dir to avoid resolver choosing a different root in read-only mode
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", tasks.to_string_lossy().as_ref());
    let _guard_silent = EnvVarGuard::set("LOTAR_TEST_SILENT", "1");

    // Run normalize explicitly with --global and verify defaults appear
    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .args(["config", "normalize", "--global"]) // explicit scope
        .assert()
        .success()
        .stdout(
            predicate::str::contains("default:")
                .and(predicate::str::contains("category: Bugfix"))
                .and(predicate::str::contains("tags:")),
        );

    // guards drop here
}

#[test]
fn normalize_project_canonical_includes_default_category_and_tags() {
    // EnvVarGuard per-var lock
    let temp = TempDir::new().unwrap();
    let tasks = temp.path().join(".tasks");
    std::fs::create_dir_all(&tasks).unwrap();

    // Minimal global to appease loader
    std::fs::write(
        tasks.join("config.yml"),
        "issue_states: [Todo]\nissue_types: [Feature]\nissue_priorities: [Low]\n",
    )
    .unwrap();

    // Create project config with defaults
    let proj = tasks.join("TEST");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(
        proj.join("config.yml"),
        "project.id: Test Project\nissue.categories: [Feat, Bugfix]\nissue.tags: [team, backend]\ndefault.category: Bugfix\ndefault.tags: [team]\n",
    )
    .unwrap();

    // Normalize project with --write and ensure defaults remain in canonical output
    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", tasks.to_string_lossy().as_ref());
    let _guard_silent = EnvVarGuard::set("LOTAR_TEST_SILENT", "1");

    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .args(["config", "normalize", "--project", "TEST", "--write"])
        .assert()
        .success();

    let contents = std::fs::read_to_string(proj.join("config.yml")).unwrap();
    assert!(contents.contains("default:"));
    assert!(contents.contains("category: Bugfix"));
    assert!(contents.contains("tags:"));

    // guards drop here
}
