use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

#[test]
fn stats_effort_points_and_auto_and_filters() {
    let temp = TempDir::new().unwrap();

    // Create three tasks with mixed effort and attributes
    // T1: hours, assignee @me, tag x
    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args([
            "task",
            "add",
            "T1",
            "--assignee",
            "@me",
            "--tag",
            "x",
            "--effort",
            "1d 2h",
        ]) // 10h
        .assert()
        .success();

    // T2: points, assignee @me, tag y
    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args([
            "task",
            "add",
            "T2",
            "--assignee",
            "@me",
            "--tag",
            "y",
            "--effort",
            "5pt",
        ]) // 5 points
        .assert()
        .success();

    // T3: hours, assignee @me, tag x
    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args([
            "task",
            "add",
            "T3",
            "--assignee",
            "@me",
            "--tag",
            "x",
            "--effort",
            "8h",
        ]) // 8h
        .assert()
        .success();

    // Points mode grouped by assignee
    let out_points = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args([
            "--format", "json", "stats", "effort", "--by", "assignee", "--unit", "points",
            "--global",
        ])
        .output()
        .unwrap();
    assert!(out_points.status.success());
    let v_points: Value = serde_json::from_slice(&out_points.stdout).unwrap();
    assert_eq!(v_points["unit"], "points");
    let items_p = v_points["items"].as_array().unwrap();
    // Expect a single entry for the resolved username with 5 points
    let mut seen_user = false;
    for r in items_p {
        let key = r["key"].as_str().unwrap_or("");
        if key == "mallox" {
            seen_user = true;
            assert_eq!(r["points"].as_f64().unwrap_or(0.0), 5.0);
        }
    }
    assert!(seen_user);

    // Auto mode grouped by tag with filter where assignee=@me, to only include T2 and T3
    let out_auto = Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(temp.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args([
            "--format",
            "json",
            "stats",
            "effort",
            "--by",
            "tag",
            "--where",
            "assignee=@me",
            "--unit",
            "auto",
            "--global",
        ])
        .output()
        .unwrap();
    assert!(out_auto.status.success());
    let v_auto: Value = serde_json::from_slice(&out_auto.stdout).unwrap();
    assert_eq!(v_auto["unit"], "auto");
    let items_a = v_auto["items"].as_array().unwrap();
    // There should be two tag groups: x (from T3 hours) and y (from T2 points)
    let mut seen_x = false;
    let mut seen_y = false;
    for r in items_a {
        let key = r["key"].as_str().unwrap_or("");
        if key == "x" {
            seen_x = true;
            assert_eq!(r["hours"].as_f64().unwrap_or(0.0), 18.0); // sum of T1 (10h) and T3 (8h)
            assert_eq!(r["auto_unit"].as_str().unwrap(), "hours");
            assert!((r["auto_value"].as_f64().unwrap_or(0.0) - 18.0).abs() < 1e-9);
        }
        if key == "y" {
            seen_y = true;
            assert_eq!(r["points"].as_f64().unwrap_or(0.0), 5.0);
            assert_eq!(r["auto_unit"].as_str().unwrap(), "points");
            assert!((r["auto_value"].as_f64().unwrap_or(0.0) - 5.0).abs() < 1e-9);
        }
    }
    assert!(seen_x && seen_y);
}
