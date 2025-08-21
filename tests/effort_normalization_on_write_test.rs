use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn run(cmd: &mut Command, temp_dir: &TempDir, args: &[&str]) -> assert_cmd::assert::Assert {
    cmd.current_dir(temp_dir.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args(args)
        .assert()
}

#[test]
fn effort_is_normalized_on_add_and_edit() {
    let temp = TempDir::new().unwrap();

    // Create a task with a variety of effort spellings that should normalize to hours
    run(
        &mut Command::cargo_bin("lotar").unwrap(),
        &temp,
        &["task", "add", "A", "--effort", "1 hr 30 min"],
    )
    .success();

    // List as JSON and verify the stored effort is canonical (1.50h)
    let out = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args(["task", "list", "--format", "json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let tasks = v["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 1);
    let effort = tasks[0]["effort"].as_str().unwrap();
    assert_eq!(effort, "1.50h");

    // Now edit with a different shape and confirm it re-normalizes
    let id = tasks[0]["id"].as_str().unwrap();
    run(
        &mut Command::cargo_bin("lotar").unwrap(),
        &temp,
        &["task", "edit", id, "--effort", "2 days"],
    )
    .success();

    let out2 = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args(["task", "list", "--format", "json"])
        .output()
        .unwrap();
    assert!(out2.status.success());
    let v2: Value = serde_json::from_slice(&out2.stdout).unwrap();
    let tasks2 = v2["tasks"].as_array().unwrap();
    assert_eq!(tasks2.len(), 1);
    let effort2 = tasks2[0]["effort"].as_str().unwrap();
    assert_eq!(effort2, "16.00h");
}
