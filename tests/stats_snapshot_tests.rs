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
fn stats_tags_and_categories_snapshot() {
    let temp = TempDir::new().unwrap();

    // Create tasks with tags and categories
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
            "--category=bug",
        ],
    ) // ID 1
    .success();
    let mut c = Command::cargo_bin("lotar").unwrap();
    run(
        &mut c,
        &temp,
        &["task", "add", "T2", "--tag=infra", "--category=feature"],
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

    // Categories: bug=1, feature=1
    let mut c = Command::cargo_bin("lotar").unwrap();
    run(&mut c, &temp, &["stats", "categories", "--global"]) // project scope not required here
        .success()
        .stdout(predicate::str::contains("bug"))
        .stdout(predicate::str::contains("feature"));

    // And JSON for categories
    let mut c = Command::cargo_bin("lotar").unwrap();
    run(
        &mut c,
        &temp,
        &["--format", "json", "stats", "categories", "--global"],
    )
    .success()
    .stdout(predicate::str::contains("\"action\":\"stats.categories\""))
    .stdout(predicate::str::contains("bug"));
}

#[test]
fn stats_distribution_snapshot() {
    let temp = TempDir::new().unwrap();

    // Add some tasks with tags and categories to populate fields
    let mut c = Command::cargo_bin("lotar").unwrap();
    run(
        &mut c,
        &temp,
        &["task", "add", "D1", "--tag=ui", "--category=feature"],
    )
    .success();
    let mut c = Command::cargo_bin("lotar").unwrap();
    run(
        &mut c,
        &temp,
        &["task", "add", "D2", "--tag=api", "--category=bug"],
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
    .stdout(predicate::str::contains("TODO"));

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
