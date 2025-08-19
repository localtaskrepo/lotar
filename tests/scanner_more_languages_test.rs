use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

mod common;
use common::TestFixtures;

#[test]
fn scan_html_block_comments() {
    let tf = TestFixtures::new();
    let root = tf.temp_dir.path();

    fs::write(
        root.join("index.html"),
        "<!DOCTYPE html>\n<!-- TODO: fix header layout -->\n<html><head></head><body></body></html>\n",
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
fn scan_css_block_comments() {
    let tf = TestFixtures::new();
    let root = tf.temp_dir.path();

    fs::write(
        root.join("styles.css"),
        "/* TODO: replace color palette */\nbody { color: #333; }\n",
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
fn scan_sql_single_line_comments() {
    let tf = TestFixtures::new();
    let root = tf.temp_dir.path();

    fs::write(
        root.join("schema.sql"),
        "-- TODO: add indexes\nCREATE TABLE t(id INT);\n",
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
fn scan_ini_semicolon_comments() {
    let tf = TestFixtures::new();
    let root = tf.temp_dir.path();

    fs::write(
        root.join("config.ini"),
        "; TODO: verify defaults\nkey=value\n",
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
fn scan_toml_hash_comments() {
    let tf = TestFixtures::new();
    let root = tf.temp_dir.path();

    fs::write(
        root.join("config.toml"),
        "# TODO: refine config\nname = \"app\"\n",
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
fn scan_hcl_hash_comments() {
    let tf = TestFixtures::new();
    let root = tf.temp_dir.path();

    fs::write(root.join("infra.hcl"), "# TODO: pin provider versions\n").unwrap();

    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(root)
        .arg("scan")
        .assert()
        .success()
        .stdout(predicate::str::contains("Found 1 TODO comment(s):"));
}

#[test]
fn scan_tf_hash_comments() {
    let tf = TestFixtures::new();
    let root = tf.temp_dir.path();

    fs::write(root.join("main.tf"), "# TODO: split modules\n").unwrap();

    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(root)
        .arg("scan")
        .assert()
        .success()
        .stdout(predicate::str::contains("Found 1 TODO comment(s):"));
}

#[test]
fn scan_lua_double_dash_comments() {
    let tf = TestFixtures::new();
    let root = tf.temp_dir.path();

    fs::write(root.join("script.lua"), "-- TODO: optimize loop\n").unwrap();

    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(root)
        .arg("scan")
        .assert()
        .success()
        .stdout(predicate::str::contains("Found 1 TODO comment(s):"));
}

#[test]
fn scan_powershell_hash_comments() {
    let tf = TestFixtures::new();
    let root = tf.temp_dir.path();

    fs::write(root.join("script.ps1"), "# TODO: handle errors\n").unwrap();

    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(root)
        .arg("scan")
        .assert()
        .success()
        .stdout(predicate::str::contains("Found 1 TODO comment(s):"));
}

#[test]
fn scan_yaml_hash_comments() {
    let tf = TestFixtures::new();
    let root = tf.temp_dir.path();

    fs::write(root.join("config.yaml"), "# TODO: adjust vars\nkey: val\n").unwrap();

    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(root)
        .arg("scan")
        .assert()
        .success()
        .stdout(predicate::str::contains("Found 1 TODO comment(s):"));
}

#[test]
fn scan_tsx_line_comments() {
    let tf = TestFixtures::new();
    let root = tf.temp_dir.path();

    fs::write(
        root.join("App.tsx"),
        "// TODO: wire props\nexport default function App(){ return null }\n",
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
fn scan_markdown_html_comments() {
    let tf = TestFixtures::new();
    let root = tf.temp_dir.path();

    fs::write(
        root.join("README.md"),
        "# Doc\n<!-- TODO: refine docs -->\n",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(root)
        .arg("scan")
        .assert()
        .success()
        .stdout(predicate::str::contains("Found 1 TODO comment(s):"));
}
