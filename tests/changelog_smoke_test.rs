use assert_cmd::prelude::*;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn changelog_outside_git_no_crash() {
    let temp = TempDir::new().unwrap();
    // No git repo here; command should not crash
    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .args(["changelog"])
        .assert()
        .success();
}
