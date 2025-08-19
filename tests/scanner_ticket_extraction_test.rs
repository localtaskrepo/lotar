use assert_cmd::Command;
use std::fs;

mod common;
use common::TestFixtures;

#[test]
fn extracts_ticket_from_bracket_attr() {
    let tf = TestFixtures::new();
    let root = tf.temp_dir.path();

    fs::write(
        root.join("a.js"),
        "// TODO [ticket=DEMO-123] implement thing",
    )
    .unwrap();

    // JSON format to assert uuid presence without relying on text rendering
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    let out = cmd
        .current_dir(root)
        .arg("--format")
        .arg("json")
        .arg("scan")
        .output()
        .unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("DEMO-123"), "expected DEMO-123 in JSON: {s}");
}

#[test]
fn extracts_ticket_from_bare_key() {
    let tf = TestFixtures::new();
    let root = tf.temp_dir.path();

    fs::write(root.join("b.rs"), "// TODO DEMO-999: implement more").unwrap();

    let mut cmd = Command::cargo_bin("lotar").unwrap();
    let out = cmd
        .current_dir(root)
        .arg("--format")
        .arg("json")
        .arg("scan")
        .output()
        .unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("DEMO-999"), "expected DEMO-999 in JSON: {s}");
}
