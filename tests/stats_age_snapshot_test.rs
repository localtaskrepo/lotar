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
fn stats_age_day_distribution_snapshot() {
    let temp = TempDir::new().unwrap();

    // Create a few tasks; their created timestamps will be "now"
    let mut c = Command::cargo_bin("lotar").unwrap();
    run(&mut c, &temp, &["task", "add", "A"]).success();
    let mut c = Command::cargo_bin("lotar").unwrap();
    run(&mut c, &temp, &["task", "add", "B"]).success();
    let mut c = Command::cargo_bin("lotar").unwrap();
    run(&mut c, &temp, &["task", "add", "C"]).success();

    // Query age distribution by day in JSON to assert exact shape
    let out = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args([
            "--format",
            "json",
            "stats",
            "age",
            "--distribution",
            "day",
            "--global",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["status"], "ok");
    assert_eq!(v["action"], "stats.age");
    assert_eq!(v["distribution"], "day");
    let items = v["items"].as_array().unwrap();
    // Expect at least a 0d bucket with count >= 3 (all created now)
    let zero = items.iter().find(|it| it["age"] == "0d").cloned();
    assert!(zero.is_some(), "expected 0d bucket present");
    let count = zero.unwrap()["count"].as_u64().unwrap();
    assert!(count >= 3, "expected at least 3 tasks in 0d, got {count}");
}
