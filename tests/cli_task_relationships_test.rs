use assert_cmd::Command;
use lotar::Storage;
use serde_json::Value as JsonValue;
use std::path::Path;

mod common;
use common::TestFixtures;

fn create_task(work_dir: &Path, title: &str) -> String {
    let output = Command::cargo_bin("lotar")
        .expect("lotar binary not found")
        .env("LOTAR_TEST_SILENT", "1")
        .current_dir(work_dir)
        .args(["task", "add", title])
        .output()
        .expect("failed to run lotar task add");

    if !output.status.success() {
        panic!(
            "failed to create task: stdout=\n{}\nstderr=\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| {
            line.strip_prefix("✅ Created task: ")
                .map(|value| value.trim().to_string())
        })
        .expect("expected created task id in output")
}

#[test]
fn relationships_command_displays_all_relationships() {
    let fixtures = TestFixtures::new();
    let work_dir = fixtures.temp_dir.path();

    let parent_id = create_task(work_dir, "Parent Task");
    let child_one = create_task(work_dir, "Child One");
    let child_two = create_task(work_dir, "Child Two");

    let prefix = parent_id
        .split('-')
        .next()
        .expect("expected project prefix")
        .to_string();

    let mut storage = Storage::new(fixtures.tasks_root.clone());
    let mut parent_task = storage
        .get(&parent_id, prefix.clone())
        .expect("expected parent task to exist");
    parent_task.relationships.depends_on = vec![child_one.clone(), "EXT-99".to_string()];
    parent_task.relationships.blocks = vec!["BLOCK-1".to_string()];
    parent_task.relationships.related = vec!["REL-7".to_string()];
    parent_task.relationships.children = vec![child_one.clone(), child_two.clone()];
    parent_task.relationships.fixes = vec!["BUG-42".to_string()];
    parent_task.relationships.duplicate_of = Some("TP-999".to_string());
    storage
        .edit(&parent_id, &parent_task)
        .expect("failed to persist parent task relationships");

    let mut child_task = storage
        .get(&child_one, prefix.clone())
        .expect("expected child task to exist");
    child_task.relationships.parent = Some(parent_id.clone());
    storage
        .edit(&child_one, &child_task)
        .expect("failed to persist child task relationships");

    let output = Command::cargo_bin("lotar")
        .expect("lotar binary not found")
        .env("LOTAR_TEST_SILENT", "1")
        .current_dir(work_dir)
        .args(["task", "relationships", &parent_id])
        .output()
        .expect("failed to run relationships command");

    if !output.status.success() {
        panic!(
            "relationships command failed: stdout=\n{}\nstderr=\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&format!("Task {parent_id} relationships")),
        "missing header in output"
    );
    assert!(stdout.contains("depends-on:"));
    assert!(stdout.contains("blocks:"));
    assert!(stdout.contains("related:"));
    assert!(stdout.contains("children:"));
    assert!(stdout.contains("fixes:"));
    assert!(stdout.contains("duplicate-of: TP-999"));
    assert!(stdout.contains(&child_one));
    assert!(stdout.contains(&child_two));
}

#[test]
fn relationships_command_supports_json_filters() {
    let fixtures = TestFixtures::new();
    let work_dir = fixtures.temp_dir.path();

    let parent_id = create_task(work_dir, "Parent");
    let child_id = create_task(work_dir, "Child");

    let prefix = parent_id
        .split('-')
        .next()
        .expect("expected project prefix")
        .to_string();

    let mut storage = Storage::new(fixtures.tasks_root.clone());
    let mut parent_task = storage
        .get(&parent_id, prefix.clone())
        .expect("expected parent task to exist");
    parent_task.relationships.children = vec![child_id.clone()];
    storage
        .edit(&parent_id, &parent_task)
        .expect("failed to persist parent task relationships");

    let mut child_task = storage
        .get(&child_id, prefix.clone())
        .expect("expected child task to exist");
    child_task.relationships.parent = Some(parent_id.clone());
    storage
        .edit(&child_id, &child_task)
        .expect("failed to persist child task relationships");

    let output = Command::cargo_bin("lotar")
        .expect("lotar binary not found")
        .env("LOTAR_TEST_SILENT", "1")
        .current_dir(work_dir)
        .args([
            "--format",
            "json",
            "task",
            "relationships",
            &child_id,
            "--kind",
            "parent",
        ])
        .output()
        .expect("failed to run relationships command");

    if !output.status.success() {
        panic!(
            "relationships command failed: stdout=\n{}\nstderr=\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let payload: JsonValue =
        serde_json::from_slice(&output.stdout).expect("expected valid json output");
    assert_eq!(payload["status"], JsonValue::String("success".to_string()));
    assert_eq!(payload["task_id"], JsonValue::String(child_id.clone()));
    assert_eq!(
        payload["relationships"]["parent"],
        JsonValue::String(parent_id)
    );
    assert!(payload["relationships"].get("children").is_none());
    let filters = payload
        .get("filters")
        .and_then(|value| value.get("kinds"))
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    assert_eq!(filters, vec![JsonValue::String("parent".to_string())]);
}
