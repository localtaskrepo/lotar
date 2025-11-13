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
issue.tags: [team, backend, ui, one, two]
custom.fields: [product]
default.tags: [team, backend]
"#;
    std::fs::create_dir_all(tasks_dir).unwrap();
    std::fs::write(tasks_dir.join("config.yml"), yaml).unwrap();
}

#[test]
fn add_applies_default_tags_and_custom_field_flag() {
    let temp = TempDir::new().unwrap();
    let tasks_dir = temp.path().join(".tasks");
    write_global_with_defaults(&tasks_dir);

    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().as_ref());
    let _guard_silent = EnvVarGuard::set("LOTAR_TEST_SILENT", "1");

    let assert = crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp.path())
        .args([
            "add",
            "Task with defaults",
            "--project=TEST",
            "--field",
            "product=Bugfix",
            "--dry-run",
            "--format=json",
        ])
        .assert()
        .success();

    let value: serde_json::Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("valid json output");
    let custom = value
        .get("custom_fields")
        .and_then(|v| v.as_object())
        .expect("custom_fields present");
    assert_eq!(custom.get("product").unwrap().as_str().unwrap(), "Bugfix");

    let tags = value
        .get("tags")
        .and_then(|v| v.as_array())
        .expect("tags present");
    let tag_strings: Vec<_> = tags
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(tag_strings, vec!["team".to_string(), "backend".to_string()]);
}

#[test]
fn add_explicit_tags_override_defaults() {
    let temp = TempDir::new().unwrap();
    let tasks_dir = temp.path().join(".tasks");
    write_global_with_defaults(&tasks_dir);

    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().as_ref());
    let _guard_silent = EnvVarGuard::set("LOTAR_TEST_SILENT", "1");

    let assert = crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp.path())
        .args([
            "add",
            "Task with explicit values",
            "--project=TEST",
            "--field",
            "product=Feat",
            "--tag",
            "team",
            "--tag",
            "ui",
            "--dry-run",
            "--format=json",
        ])
        .assert()
        .success();

    let value: serde_json::Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("valid json output");
    let custom = value
        .get("custom_fields")
        .and_then(|v| v.as_object())
        .expect("custom_fields present");
    assert_eq!(custom.get("product").unwrap().as_str().unwrap(), "Feat");

    let tags = value
        .get("tags")
        .and_then(|v| v.as_array())
        .expect("tags present");
    let tag_strings: Vec<_> = tags
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(tag_strings, vec!["team".to_string(), "ui".to_string()]);
}

#[test]
fn normalize_canonical_includes_custom_fields_global() {
    let temp = TempDir::new().unwrap();
    let tasks = temp.path().join(".tasks");
    std::fs::create_dir_all(&tasks).unwrap();

    std::fs::write(
        tasks.join("config.yml"),
    "server.port: 8080\ndefault.project: TEST\nissue.states: [Todo]\nissue.types: [Feature]\nissue.priorities: [Low, Medium]\ncustom.fields: [product]\ndefault.tags: [team, backend]\n",
    )
    .unwrap();

    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp.path())
        .args(["config", "normalize"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("custom:")
                .and(predicate::str::contains("fields:"))
                .and(predicate::str::contains("- product"))
                .and(predicate::str::contains("default:"))
                .and(predicate::str::contains("tags:")),
        );
}

#[test]
fn normalize_project_canonical_includes_custom_fields() {
    let temp = TempDir::new().unwrap();
    let tasks = temp.path().join(".tasks");
    std::fs::create_dir_all(&tasks).unwrap();

    std::fs::write(
        tasks.join("config.yml"),
        "issue_states: [Todo]\nissue_types: [Feature]\nissue_priorities: [Low, Medium]\n",
    )
    .unwrap();

    let proj = tasks.join("TEST");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(
        proj.join("config.yml"),
    "project.name: Test Project\ncustom.fields: [product, component]\nissue.tags: [team, backend]\ndefault.tags: [team]\n",
    )
    .unwrap();

    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", tasks.to_string_lossy().as_ref());
    let _guard_silent = EnvVarGuard::set("LOTAR_TEST_SILENT", "1");

    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp.path())
        .args(["config", "normalize", "--project", "TEST", "--write"])
        .assert()
        .success();

    let contents = std::fs::read_to_string(proj.join("config.yml")).unwrap();
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
    assert!(field_names.contains(&"component".to_string()));

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

    let issue_tags = doc
        .get("issue")
        .and_then(|v| v.get("tags"))
        .and_then(|v| v.as_sequence())
        .expect("issue.tags present");
    let issue_names: Vec<String> = issue_tags
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();
    assert!(issue_names.contains(&"backend".to_string()));
}
