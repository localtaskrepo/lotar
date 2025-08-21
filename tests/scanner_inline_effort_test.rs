use assert_cmd::Command;
use serde_yaml::Value as Y;
use std::fs;

mod common;
use common::TestFixtures;

fn read_task_yaml_effort(root: &std::path::Path) -> String {
    // Determine project folder (default or derived)
    let tasks_dir = root.join(".tasks");
    let mut projects = std::fs::read_dir(&tasks_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    projects.sort();
    assert!(
        !projects.is_empty(),
        "expected a project folder under .tasks"
    );
    let project = &projects[0];

    let task_file = tasks_dir.join(project).join("1.yml");
    assert!(
        task_file.exists(),
        "expected {} to exist",
        task_file.display()
    );
    let yaml = fs::read_to_string(&task_file).unwrap();
    let parsed: Y = serde_yaml::from_str(&yaml).expect("valid yaml");
    parsed
        .get("effort")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

#[test]
fn scan_inline_effort_minutes_normalized_to_hours() {
    let tf = TestFixtures::new();
    let root = tf.temp_dir.path();

    // Source with inline effort in minutes
    let src = r#"// TODO: implement foo [effort=90m]"#;
    fs::write(root.join("main.rs"), src).unwrap();

    // Run scan (apply-by-default)
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.env("LOTAR_TEST_SILENT", "1")
        .current_dir(root)
        .arg("scan")
        .assert()
        .success();

    let effort = read_task_yaml_effort(root);
    assert_eq!(effort, "1.50h"); // 90m -> 1.5h
}

#[test]
fn scan_inline_effort_combined_tokens() {
    let tf = TestFixtures::new();
    let root = tf.temp_dir.path();

    // Source with combined time tokens
    let src = r#"// TODO: handle bar [effort=1h 30m]"#;
    fs::write(root.join("lib.rs"), src).unwrap();

    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.env("LOTAR_TEST_SILENT", "1")
        .current_dir(root)
        .arg("scan")
        .assert()
        .success();

    let effort = read_task_yaml_effort(root);
    assert_eq!(effort, "1.50h");
}

#[test]
fn scan_inline_effort_points_preserved() {
    let tf = TestFixtures::new();
    let root = tf.temp_dir.path();

    // Source with points effort
    let src = r#"// TODO: estimate baz [effort=3pt]"#;
    fs::write(root.join("mod.rs"), src).unwrap();

    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.env("LOTAR_TEST_SILENT", "1")
        .current_dir(root)
        .arg("scan")
        .assert()
        .success();

    let effort = read_task_yaml_effort(root);
    assert_eq!(effort, "3pt");
}
