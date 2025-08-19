use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

mod common;
use common::TestFixtures;

#[test]
fn scan_js_block_comments() {
    let tf = TestFixtures::new();
    let root = tf.temp_dir.path();

    fs::write(
        root.join("file.js"),
        "/*\n * TODO: inside block js\n */\n const x = 1;",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(root)
        .arg("scan")
        .assert()
        .success()
        .stdout(predicate::str::contains("Found 1 TODO comment(s):"));
}

#[test]
fn scan_rust_block_comments() {
    let tf = TestFixtures::new();
    let root = tf.temp_dir.path();

    fs::write(
        root.join("file.rs"),
        "/* TODO: block in rust */\nfn main() {}",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(root)
        .arg("scan")
        .assert()
        .success()
        .stdout(predicate::str::contains("Found 1 TODO comment(s):"));
}
