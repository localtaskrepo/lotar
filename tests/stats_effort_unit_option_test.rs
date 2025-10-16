use assert_cmd::Command;
use serde_json::Value;
mod common;

#[test]
fn stats_effort_respects_unit_flag() {
    let temp = crate::common::temp_dir();

    // Two tasks: 8h (1 day) and 2d (16h, 2 days)
    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args(["task", "add", "T1", "--effort", "8h"]) // 1 day
        .assert()
        .success();
    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args(["task", "add", "T2", "--effort", "2d"]) // 2 days
        .assert()
        .success();

    // Group by assignee (empty) and request days
    let out = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args([
            "--format", "json", "stats", "effort", "--by", "assignee", "--unit", "days", "--global",
        ]) // project-agnostic
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["status"], "ok");
    assert_eq!(v["action"], "stats.effort");
    assert_eq!(v["unit"], "days");
    let items = v["items"].as_array().unwrap();
    assert!(!items.is_empty());
    // Single group (empty assignee), total should be 3.0 days
    let row = &items[0];
    assert_eq!(row["days"].as_f64().unwrap(), 3.0);
    // Hours remain available for sorting/reference
    assert_eq!(row["hours"].as_f64().unwrap(), 24.0);

    // Now request weeks
    let out2 = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args([
            "--format", "json", "stats", "effort", "--by", "assignee", "--unit", "weeks",
            "--global",
        ]) // project-agnostic
        .output()
        .unwrap();
    assert!(out2.status.success());
    let v2: Value = serde_json::from_slice(&out2.stdout).unwrap();
    assert_eq!(v2["unit"], "weeks");
    let items2 = v2["items"].as_array().unwrap();
    let row2 = &items2[0];
    assert!((row2["weeks"].as_f64().unwrap() - 0.6).abs() < 1e-9); // 24h / 40h = 0.6
}
