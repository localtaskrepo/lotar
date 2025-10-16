use assert_cmd::prelude::*;
use serde_json::Value;
use std::process::Command;
use tempfile::TempDir;

mod common;

fn run(cmd: &mut Command, temp: &TempDir, args: &[&str]) -> assert_cmd::assert::Assert {
    cmd.current_dir(temp.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args(args)
        .assert()
}

#[test]
fn list_sort_by_effort_time_only_asc_and_desc() {
    let temp = crate::common::temp_dir();

    // Create three tasks with time efforts: 30m (0.50h), 2h (2.00h), 1d (8.00h)
    run(
        &mut Command::cargo_bin("lotar").unwrap(),
        &temp,
        &["task", "add", "A", "--effort", "30m"],
    )
    .success();
    run(
        &mut Command::cargo_bin("lotar").unwrap(),
        &temp,
        &["task", "add", "B", "--effort", "2h"],
    )
    .success();
    run(
        &mut Command::cargo_bin("lotar").unwrap(),
        &temp,
        &["task", "add", "C", "--effort", "1d"],
    )
    .success();

    // Ascending
    let out = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args(["task", "list", "--format", "json", "--sort-by=effort"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let tasks = v["tasks"].as_array().unwrap();
    let efforts: Vec<&str> = tasks
        .iter()
        .map(|t| t["effort"].as_str().unwrap_or("-"))
        .collect();
    assert_eq!(
        efforts,
        vec!["0.50h", "2.00h", "8.00h"],
        "ascending effort order"
    );

    // Descending
    let out2 = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args([
            "task",
            "list",
            "--format",
            "json",
            "--sort-by=effort",
            "--reverse",
        ])
        .output()
        .unwrap();
    assert!(out2.status.success());
    let v2: Value = serde_json::from_slice(&out2.stdout).unwrap();
    let tasks2 = v2["tasks"].as_array().unwrap();
    let efforts2: Vec<&str> = tasks2
        .iter()
        .map(|t| t["effort"].as_str().unwrap_or("-"))
        .collect();
    assert_eq!(
        efforts2,
        vec!["8.00h", "2.00h", "0.50h"],
        "descending effort order"
    );
}

#[test]
fn list_effort_min_max_time_window() {
    let temp = crate::common::temp_dir();

    // Time efforts: 0.50h, 2.00h, 8.00h
    run(
        &mut Command::cargo_bin("lotar").unwrap(),
        &temp,
        &["task", "add", "A", "--effort", "30m"],
    )
    .success();
    run(
        &mut Command::cargo_bin("lotar").unwrap(),
        &temp,
        &["task", "add", "B", "--effort", "2h"],
    )
    .success();
    run(
        &mut Command::cargo_bin("lotar").unwrap(),
        &temp,
        &["task", "add", "C", "--effort", "1d"],
    )
    .success();

    // Filter [1h, 1d] inclusive; expect 2.00h and 8.00h
    let out = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args([
            "task",
            "list",
            "--format",
            "json",
            "--effort-min",
            "1h",
            "--effort-max",
            "1d",
            "--sort-by=effort",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let tasks = v["tasks"].as_array().unwrap();
    let efforts: Vec<&str> = tasks
        .iter()
        .map(|t| t["effort"].as_str().unwrap_or("-"))
        .collect();
    assert_eq!(
        efforts,
        vec!["2.00h", "8.00h"],
        "filtered inclusive time window"
    );
}

#[test]
fn list_effort_min_points_excludes_time() {
    let temp = crate::common::temp_dir();

    // Mixed: time and points
    run(
        &mut Command::cargo_bin("lotar").unwrap(),
        &temp,
        &["task", "add", "T1", "--effort", "2h"],
    )
    .success();
    run(
        &mut Command::cargo_bin("lotar").unwrap(),
        &temp,
        &["task", "add", "P3", "--effort", "3pt"],
    )
    .success();
    run(
        &mut Command::cargo_bin("lotar").unwrap(),
        &temp,
        &["task", "add", "P5", "--effort", "5"],
    )
    .success(); // bare number => points

    // Points filter: --effort-min 4 (points). Should include only P5; time tasks excluded by kind.
    let out = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args([
            "task",
            "list",
            "--format",
            "json",
            "--effort-min",
            "4",
            "--sort-by=effort",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let tasks = v["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 1, "only points >= 4 should match");
    assert_eq!(tasks[0]["effort"], "5pt");
}
