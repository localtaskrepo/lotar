use assert_cmd::Command;
use tempfile::TempDir;

mod common;
use common::env_mutex::lock_var;

fn write_global(tasks_dir: &std::path::Path, extra: &str) {
    let base = r#"default.project: TEST
issue.states: [Todo, InProgress, Done]
issue.types: [Feature, Bug, Chore]
issue.priorities: [Low, Medium, High]
issue.tags: [api, web]
"#;
    let mut content = String::from(base);
    if !extra.is_empty() {
        content.push_str(extra);
        if !extra.ends_with('\n') {
            content.push('\n');
        }
    }
    std::fs::write(tasks_dir.join("config.yml"), content).unwrap();
}

#[test]
fn disables_path_tag_when_flag_off() {
    let _env_tasks = lock_var("LOTAR_TASKS_DIR");
    let temp = TempDir::new().unwrap();
    let repo_root = temp.path();
    let tasks_dir = repo_root.join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();

    // Create monorepo-like path
    let pkg_api = repo_root.join("packages").join("api");
    std::fs::create_dir_all(&pkg_api).unwrap();

    write_global(&tasks_dir, "auto.tags_from_path: false\n");

    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().to_string());
        std::env::set_var("LOTAR_TEST_SILENT", "1");
    }

    // Dry-run JSON to inspect derived tags
    let assert = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(&pkg_api)
        .args([
            "add",
            "No path tag",
            "--project=TEST",
            "--dry-run",
            "--format=json",
        ])
        .assert()
        .success();

    let output = String::from_utf8_lossy(&assert.get_output().stdout);
    // Should have no tags when feature disabled (field may be omitted in JSON preview)
    assert!(
        !output.contains("\"tags\":"),
        "Expected no derived tags field, got: {output}"
    );

    unsafe {
        std::env::remove_var("LOTAR_TASKS_DIR");
        std::env::remove_var("LOTAR_TEST_SILENT");
    }
}

#[test]
fn enables_path_tag_when_flag_on() {
    let _env_tasks = lock_var("LOTAR_TASKS_DIR");
    let temp = TempDir::new().unwrap();
    let repo_root = temp.path();
    let tasks_dir = repo_root.join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let pkg_api = repo_root.join("packages").join("api");
    std::fs::create_dir_all(&pkg_api).unwrap();

    write_global(&tasks_dir, "auto.tags_from_path: true\n");

    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().to_string());
        std::env::set_var("LOTAR_TEST_SILENT", "1");
    }

    let assert = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(&pkg_api)
        .args([
            "add",
            "With path tag",
            "--project=TEST",
            "--dry-run",
            "--format=json",
        ])
        .assert()
        .success();

    let output = String::from_utf8_lossy(&assert.get_output().stdout);
    // When flag on and tag is allowed (api), it should appear
    assert!(
        output.contains("\"tags\":[\"api\"]"),
        "Expected derived tag 'api', got: {output}"
    );

    unsafe {
        std::env::remove_var("LOTAR_TASKS_DIR");
        std::env::remove_var("LOTAR_TEST_SILENT");
    }
}
