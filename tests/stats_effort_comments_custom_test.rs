mod common;

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
fn stats_effort_and_comments_and_custom_snapshot() {
    let temp = TempDir::new().unwrap();

    // Create a few tasks with effort and custom fields
    run(
        &mut crate::common::lotar_cmd().unwrap(),
        &temp,
        &["task", "add", "A", "--effort", "2d", "--field", "team=eng"],
    )
    .success();
    run(
        &mut crate::common::lotar_cmd().unwrap(),
        &temp,
        &[
            "task", "add", "B", "--effort", "5h", "--assign", "@bob", "--field", "team=eng",
        ],
    )
    .success();
    run(
        &mut crate::common::lotar_cmd().unwrap(),
        &temp,
        &[
            "task",
            "add",
            "C",
            "--effort",
            "1w",
            "--assign",
            "@alice",
            "--field",
            "priority_hint=low",
        ],
    )
    .success();

    // Effort by assignee
    let out = crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args([
            "--format", "json", "stats", "effort", "--by", "assignee", "--global",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["status"], "ok");
    assert_eq!(v["action"], "stats.effort");

    // Comments top (should be zero counts)
    let out = crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args(["--format", "json", "stats", "comments-top", "--global"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["status"], "ok");
    assert_eq!(v["action"], "stats.comments.top");

    // Custom keys
    let out = crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args(["--format", "json", "stats", "custom-keys", "--global"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["status"], "ok");
    assert_eq!(v["action"], "stats.custom.keys");

    // Custom field distribution
    let out = crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args([
            "--format",
            "json",
            "stats",
            "custom-field",
            "--field",
            "team",
            "--global",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["status"], "ok");
    assert_eq!(v["action"], "stats.custom.field");
}
