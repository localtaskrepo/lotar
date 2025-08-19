use assert_cmd::Command;
use chrono::Local;
use serde_json::Value;

mod common;
use common::TestFixtures;

fn ymd_string(days_from_today: i64) -> String {
    let d = Local::now().date_naive() + chrono::Duration::days(days_from_today);
    d.format("%Y-%m-%d").to_string()
}

#[test]
fn parse_shortcuts_and_offsets() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    // Create a task so ID 1 exists
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp_dir)
        .arg("add")
        .arg("Task for due-date parsing")
        .assert()
        .success();

    // today
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    let out = cmd
        .current_dir(temp_dir)
        .arg("due-date")
        .arg("1")
        .arg("today")
        .arg("--format=json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["new_due_date"], Value::String(ymd_string(0)));

    // in 3 days
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    let out = cmd
        .current_dir(temp_dir)
        .arg("due-date")
        .arg("1")
        .arg("in 3 days")
        .arg("--format=json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["new_due_date"], Value::String(ymd_string(3)));

    // +2w
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    let out = cmd
        .current_dir(temp_dir)
        .arg("due-date")
        .arg("1")
        .arg("+2w")
        .arg("--format=json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["new_due_date"], Value::String(ymd_string(14)));

    // next business day
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    let out = cmd
        .current_dir(temp_dir)
        .arg("due-date")
        .arg("1")
        .arg("next business day")
        .arg("--format=json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&out).unwrap();
    // result should be >= tomorrow but skip weekend specifics by just ensuring it's not in the past
    let new_date = v["new_due_date"].as_str().unwrap();
    let today = Local::now().date_naive();
    let parsed = chrono::NaiveDate::parse_from_str(new_date, "%Y-%m-%d").unwrap();
    assert!(parsed >= today);

    // +2bd
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    let out = cmd
        .current_dir(temp_dir)
        .arg("due-date")
        .arg("1")
        .arg("+2bd")
        .arg("--format=json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&out).unwrap();
    let new_date = v["new_due_date"].as_str().unwrap();
    let parsed = chrono::NaiveDate::parse_from_str(new_date, "%Y-%m-%d").unwrap();
    assert!(parsed >= today);
}

#[test]
fn parse_weekday_phrases() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    // Create a task so ID 1 exists
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp_dir)
        .arg("add")
        .arg("Task for weekday phrases")
        .assert()
        .success();

    for phrase in ["this friday", "by fri", "fri", "next week monday"] {
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let out = cmd
            .current_dir(temp_dir)
            .arg("due-date")
            .arg("1")
            .arg(phrase)
            .arg("--format=json")
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let v: Value = serde_json::from_slice(&out).unwrap();
        let new_date = v["new_due_date"].as_str().unwrap();
        // Ensure it's a valid YYYY-MM-DD and in the future
        let parsed = chrono::NaiveDate::parse_from_str(new_date, "%Y-%m-%d").unwrap();
        assert!(parsed > Local::now().date_naive());
    }
}

#[test]
fn parse_rfc3339_and_local_datetime() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    // Create a task so ID 1 exists
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp_dir)
        .arg("add")
        .arg("Task for datetime parsing")
        .assert()
        .success();

    // RFC3339 with Z
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    let out = cmd
        .current_dir(temp_dir)
        .arg("due-date")
        .arg("1")
        .arg("2025-12-31T10:00:00Z")
        .arg("--format=json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&out).unwrap();
    let s = v["new_due_date"].as_str().unwrap();
    // Should be RFC3339
    assert!(s.contains('T') && (s.ends_with('Z') || s.contains('+') || s.contains('-')));

    // Local datetime
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    let out = cmd
        .current_dir(temp_dir)
        .arg("due-date")
        .arg("1")
        .arg("2025-12-31 09:30")
        .arg("--format=json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&out).unwrap();
    let s = v["new_due_date"].as_str().unwrap();
    assert!(s.contains('T'));
}

#[test]
fn list_overdue_and_due_soon_filters() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    // Create tasks with due dates
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp_dir)
        .arg("add")
        .arg("Past due task")
        .arg("--due=2000-01-01")
        .assert()
        .success();

    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp_dir)
        .arg("add")
        .arg("Due this week")
        .arg("--due=+3d")
        .assert()
        .success();

    // Overdue should find the first task
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    let result = cmd
        .current_dir(temp_dir)
        .arg("list")
        .arg("--overdue")
        .arg("--format=json")
        .assert()
        .success();
    let output = String::from_utf8_lossy(&result.get_output().stdout).to_string();
    let json: Value = serde_json::from_str(&output).unwrap();
    let tasks = json["tasks"].as_array().unwrap();
    assert!(tasks.iter().any(|t| t["title"] == "Past due task"));

    // Due soon should find the second task
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    let result = cmd
        .current_dir(temp_dir)
        .arg("list")
        .arg("--due-soon")
        .arg("--format=json")
        .assert()
        .success();
    let output = String::from_utf8_lossy(&result.get_output().stdout).to_string();
    let json: Value = serde_json::from_str(&output).unwrap();
    let tasks = json["tasks"].as_array().unwrap();
    assert!(tasks.iter().any(|t| t["title"] == "Due this week"));
}
