use assert_cmd::prelude::*;
use serde_json::Value;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn list_alias_ls_works() {
    let temp = TempDir::new().unwrap();

    // Create a task to ensure list returns something
    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .args(["task", "add", "Hello world"])
        .assert()
        .success();

    // Use alias `ls`
    let out = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .args(["--format", "json", "ls"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["status"], "success");
    assert!(v["tasks"].as_array().is_some());
}
