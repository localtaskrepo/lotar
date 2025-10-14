use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn run(cmd: &mut Command, temp_dir: &TempDir, args: &[&str]) -> assert_cmd::assert::Assert {
    cmd.current_dir(temp_dir.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args(args)
        .assert()
}

#[test]
fn stats_tags_and_custom_field_snapshot() {
    let temp = TempDir::new().unwrap();

    // Create tasks with tags and custom fields
    let mut c = Command::cargo_bin("lotar").unwrap();
    run(
        &mut c,
        &temp,
        &[
            "task",
            "add",
            "T1",
            "--tag=infra",
            "--tag=build",
            "--field=product=bug",
        ],
    ) // ID 1
    .success();
    let mut c = Command::cargo_bin("lotar").unwrap();
    run(
        &mut c,
        &temp,
        &[
            "task",
            "add",
            "T2",
            "--tag=infra",
            "--field=product=feature",
        ],
    )
    .success();

    // Tags: infra=2, build=1
    let mut c = Command::cargo_bin("lotar").unwrap();
    run(&mut c, &temp, &["stats", "tags", "--global"]) // project scope not required here
        .success()
        .stdout(predicate::str::contains("infra"))
        .stdout(predicate::str::contains("build"));

    // Also verify JSON output shape for tags
    let mut c = Command::cargo_bin("lotar").unwrap();
    run(
        &mut c,
        &temp,
        &["--format", "json", "stats", "tags", "--global"],
    )
    .success()
    .stdout(predicate::str::contains("\"action\":\"stats.tags\""))
    .stdout(predicate::str::contains("infra"));

    // Custom field values: bug=1, feature=1
    let mut c = Command::cargo_bin("lotar").unwrap();
    run(
        &mut c,
        &temp,
        &["stats", "custom-field", "--field", "product", "--global"],
    ) // project scope not required here
    .success()
    .stdout(predicate::str::contains("bug"))
    .stdout(predicate::str::contains("feature"));

    // And JSON for custom field distribution
    let mut c = Command::cargo_bin("lotar").unwrap();
    run(
        &mut c,
        &temp,
        &[
            "--format",
            "json",
            "stats",
            "custom-field",
            "--field",
            "product",
            "--global",
        ],
    )
    .success()
    .stdout(predicate::str::contains(
        "\"action\":\"stats.custom.field\"",
    ))
    .stdout(predicate::str::contains("\"field\":\"product\""))
    .stdout(predicate::str::contains("bug"));

    // Custom keys should include product
    let mut c = Command::cargo_bin("lotar").unwrap();
    run(
        &mut c,
        &temp,
        &["--format", "json", "stats", "custom-keys", "--global"],
    )
    .success()
    .stdout(predicate::str::contains("\"action\":\"stats.custom.keys\""))
    .stdout(predicate::str::contains("product"));
}

#[test]
fn stats_distribution_snapshot() {
    let temp = TempDir::new().unwrap();

    // Add some tasks with tags and custom fields to populate fields
    let mut c = Command::cargo_bin("lotar").unwrap();
    run(
        &mut c,
        &temp,
        &["task", "add", "D1", "--tag=ui", "--field=product=feature"],
    )
    .success();
    let mut c = Command::cargo_bin("lotar").unwrap();
    run(
        &mut c,
        &temp,
        &["task", "add", "D2", "--tag=api", "--field=product=bug"],
    )
    .success();

    // By status (all default to TODO)
    let mut c = Command::cargo_bin("lotar").unwrap();
    run(
        &mut c,
        &temp,
        &[
            "--format",
            "json",
            "stats",
            "distribution",
            "--field",
            "status",
        ],
    )
    .success()
    .stdout(predicate::str::contains(
        "\"action\":\"stats.distribution\"",
    ))
    .stdout(predicate::str::contains("\"field\":\"status\""))
    .stdout(predicate::str::contains("\"Todo\""));

    // By tag should include our tags
    let mut c = Command::cargo_bin("lotar").unwrap();
    run(
        &mut c,
        &temp,
        &["stats", "distribution", "--field", "tag", "--global"],
    )
    .success()
    .stdout(predicate::str::contains("ui"))
    .stdout(predicate::str::contains("api"));
}

#[test]
fn stats_due_buckets_snapshot() {
    use chrono::{Duration, Utc};

    let temp = TempDir::new().unwrap();

    let today = Utc::now().date_naive();
    let fmt = |d: chrono::NaiveDate| d.format("%Y-%m-%d").to_string();

    let overdue = fmt(today - Duration::days(1));
    let today_s = fmt(today);
    let week = fmt(today + Duration::days(3));
    let month = fmt(today + Duration::days(20));
    let later = fmt(today + Duration::days(40));

    // Create tasks with different due dates
    let mut c = Command::cargo_bin("lotar").unwrap();
    run(
        &mut c,
        &temp,
        &["task", "add", "Overdue", "--due", &overdue],
    )
    .success();
    let mut c = Command::cargo_bin("lotar").unwrap();
    run(&mut c, &temp, &["task", "add", "Today", "--due", &today_s]).success();
    let mut c = Command::cargo_bin("lotar").unwrap();
    run(&mut c, &temp, &["task", "add", "Week", "--due", &week]).success();
    let mut c = Command::cargo_bin("lotar").unwrap();
    run(&mut c, &temp, &["task", "add", "Month", "--due", &month]).success();
    let mut c = Command::cargo_bin("lotar").unwrap();
    run(&mut c, &temp, &["task", "add", "Later", "--due", &later]).success();

    let mut c = Command::cargo_bin("lotar").unwrap();
    run(&mut c, &temp, &["stats", "due", "--global"]) // default buckets
        .success()
        .stdout(predicate::str::contains("overdue"))
        .stdout(predicate::str::contains("today"))
        .stdout(predicate::str::contains("week"))
        .stdout(predicate::str::contains("month"))
        .stdout(predicate::str::contains("later"));
}

#[test]
fn stats_due_overdue_threshold_snapshot() {
    use chrono::{Duration, Utc};

    let temp = TempDir::new().unwrap();

    let today = Utc::now().date_naive();
    let fmt = |d: chrono::NaiveDate| d.format("%Y-%m-%d").to_string();

    let overdue_1 = fmt(today - Duration::days(1));
    let overdue_10 = fmt(today - Duration::days(10));
    let future_1 = fmt(today + Duration::days(1));

    // Create tasks with due dates: 1d overdue, 10d overdue, 1d future
    let mut c = Command::cargo_bin("lotar").unwrap();
    run(&mut c, &temp, &["task", "add", "A", "--due", &overdue_1]).success();
    let mut c = Command::cargo_bin("lotar").unwrap();
    run(&mut c, &temp, &["task", "add", "B", "--due", &overdue_10]).success();
    let mut c = Command::cargo_bin("lotar").unwrap();
    run(&mut c, &temp, &["task", "add", "C", "--due", &future_1]).success();

    // Overdue only with 0d threshold should count both overdue tasks
    let mut c = Command::cargo_bin("lotar").unwrap();
    run(
        &mut c,
        &temp,
        &[
            "--format",
            "json",
            "stats",
            "due",
            "--overdue",
            "--threshold",
            "0d",
            "--global",
        ],
    )
    .success()
    .stdout(predicate::str::contains("\"overdue_only\":true"))
    .stdout(predicate::str::contains("\"buckets\":\"overdue\""))
    .stdout(predicate::str::contains("overdue"));

    // Overdue only with 7d threshold should count only the 10d overdue
    let mut c = Command::cargo_bin("lotar").unwrap();
    run(
        &mut c,
        &temp,
        &[
            "--format",
            "json",
            "stats",
            "due",
            "--overdue",
            "--threshold",
            "7d",
            "--global",
        ],
    )
    .success()
    .stdout(predicate::str::contains("\"overdue_only\":true"));
}

// =============================================================================
// Effort-related snapshots (consolidated)
// =============================================================================

mod effort_unit {
    use assert_cmd::Command;
    use serde_json::Value;
    use tempfile::TempDir;

    #[test]
    fn stats_effort_respects_unit_flag() {
        let temp = TempDir::new().unwrap();

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
                "--format", "json", "stats", "effort", "--by", "assignee", "--unit", "days",
                "--global",
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
}

mod effort_points_auto_filters {
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
}

mod effort_comments_custom {
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
    fn stats_effort_and_comments_and_custom_snapshot() {
        let temp = TempDir::new().unwrap();

        // Create a few tasks with effort and custom fields
        run(
            &mut Command::cargo_bin("lotar").unwrap(),
            &temp,
            &["task", "add", "A", "--effort", "2d", "--field", "team=eng"],
        )
        .success();
        run(
            &mut Command::cargo_bin("lotar").unwrap(),
            &temp,
            &[
                "task", "add", "B", "--effort", "5h", "--assign", "@bob", "--field", "team=eng",
            ],
        )
        .success();
        run(
            &mut Command::cargo_bin("lotar").unwrap(),
            &temp,
            &[
                "task",
                "add",
                "C",
                "--effort",
                "1w",
                "--assign",
                "@alice",
                "--field",
                "priority_hint=low",
            ],
        )
        .success();

        // Effort by assignee
        let out = Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(temp.path())
            .env("LOTAR_TEST_SILENT", "1")
            .args([
                "--format", "json", "stats", "effort", "--by", "assignee", "--global",
            ])
            .output()
            .unwrap();
        assert!(out.status.success());
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["status"], "ok");
        assert_eq!(v["action"], "stats.effort");

        // Comments top (should be zero counts)
        let out = Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(temp.path())
            .env("LOTAR_TEST_SILENT", "1")
            .args(["--format", "json", "stats", "comments-top", "--global"])
            .output()
            .unwrap();
        assert!(out.status.success());
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["status"], "ok");
        assert_eq!(v["action"], "stats.comments.top");

        // Custom keys
        let out = Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(temp.path())
            .env("LOTAR_TEST_SILENT", "1")
            .args(["--format", "json", "stats", "custom-keys", "--global"])
            .output()
            .unwrap();
        assert!(out.status.success());
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["status"], "ok");
        assert_eq!(v["action"], "stats.custom.keys");

        // Custom field distribution
        let out = Command::cargo_bin("lotar")
            .unwrap()
            .current_dir(temp.path())
            .env("LOTAR_TEST_SILENT", "1")
            .args([
                "--format",
                "json",
                "stats",
                "custom-field",
                "--field",
                "team",
                "--global",
            ])
            .output()
            .unwrap();
        assert!(out.status.success());
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["status"], "ok");
        assert_eq!(v["action"], "stats.custom.field");
    }
}
