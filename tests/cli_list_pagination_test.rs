//! CLI pagination / "more results" footer tests for `lotar list`.

use tempfile::TempDir;

mod common;

fn seed(dir: &std::path::Path, count: usize) {
    for i in 1..=count {
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(dir)
            .args(["add", &format!("Task {i}"), "--project=TEST"])
            .assert()
            .success();
    }
}

#[test]
fn list_default_shows_more_hint_when_over_page_size() {
    let temp = TempDir::new().unwrap();
    seed(temp.path(), 25);

    let mut cmd = crate::common::lotar_cmd().unwrap();
    let out = cmd
        .current_dir(temp.path())
        .args(["list", "--sort-by=id"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();

    assert!(
        stdout.contains("page 1 of 2"),
        "expected page indicator, got: {stdout}"
    );
    assert!(
        stdout.contains("5 more task(s) not shown"),
        "expected remaining-count hint, got: {stdout}"
    );
    assert!(
        stdout.contains("--page 2"),
        "expected next-page hint, got: {stdout}"
    );
    assert!(
        stdout.contains("--offset 20"),
        "expected offset hint, got: {stdout}"
    );
}

#[test]
fn list_last_page_shows_end_of_results_hint() {
    let temp = TempDir::new().unwrap();
    seed(temp.path(), 25);

    let mut cmd = crate::common::lotar_cmd().unwrap();
    let out = cmd
        .current_dir(temp.path())
        .args(["list", "--page=2", "--sort-by=id"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();

    assert!(
        stdout.contains("page 2 of 2"),
        "expected page 2 indicator, got: {stdout}"
    );
    assert!(
        stdout.contains("end of results"),
        "expected end-of-results hint, got: {stdout}"
    );
    assert!(
        !stdout.contains("more task(s) not shown"),
        "should not advertise more results on last page, got: {stdout}"
    );
}

#[test]
fn list_single_page_has_no_pagination_hints() {
    let temp = TempDir::new().unwrap();
    seed(temp.path(), 3);

    let mut cmd = crate::common::lotar_cmd().unwrap();
    let out = cmd
        .current_dir(temp.path())
        .args(["list", "--sort-by=id"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();

    assert!(
        stdout.contains("page 1 of 1"),
        "single page should say 'page 1 of 1', got: {stdout}"
    );
    assert!(
        !stdout.contains("more task(s) not shown"),
        "single page must not advertise more, got: {stdout}"
    );
    assert!(
        !stdout.contains("end of results"),
        "single page must not show end-of-results hint, got: {stdout}"
    );
}

#[test]
fn list_custom_page_size_changes_pagination() {
    let temp = TempDir::new().unwrap();
    seed(temp.path(), 25);

    let mut cmd = crate::common::lotar_cmd().unwrap();
    let out = cmd
        .current_dir(temp.path())
        .args(["list", "--page-size=5", "--sort-by=id"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();

    assert!(
        stdout.contains("page 1 of 5"),
        "expected 5 total pages for 25 tasks at page-size=5, got: {stdout}"
    );
    assert!(
        stdout.contains("20 more task(s) not shown"),
        "expected 20 remaining, got: {stdout}"
    );
    assert!(
        stdout.contains("--page 2"),
        "expected next-page hint, got: {stdout}"
    );
}

#[test]
fn list_json_reports_pagination_metadata() {
    let temp = TempDir::new().unwrap();
    seed(temp.path(), 25);

    let mut cmd = crate::common::lotar_cmd().unwrap();
    let out = cmd
        .current_dir(temp.path())
        .args([
            "list",
            "--page-size=10",
            "--page=2",
            "--sort-by=id",
            "--format=json",
        ])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("list --format=json must return JSON");

    assert_eq!(json["total"].as_u64(), Some(25));
    assert_eq!(json["limit"].as_u64(), Some(10));
    assert_eq!(json["offset"].as_u64(), Some(10));
    assert_eq!(json["page"].as_u64(), Some(2));
    assert_eq!(json["total_pages"].as_u64(), Some(3));
    assert_eq!(json["has_more"].as_bool(), Some(true));
    assert_eq!(json["has_previous"].as_bool(), Some(true));
    assert_eq!(json["next_offset"].as_u64(), Some(20));
    assert_eq!(json["next_page"].as_u64(), Some(3));
}

#[test]
fn list_json_last_page_clears_next_cursors() {
    let temp = TempDir::new().unwrap();
    seed(temp.path(), 25);

    let mut cmd = crate::common::lotar_cmd().unwrap();
    let out = cmd
        .current_dir(temp.path())
        .args([
            "list",
            "--page-size=10",
            "--page=3",
            "--sort-by=id",
            "--format=json",
        ])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("list --format=json must return JSON");

    assert_eq!(json["page"].as_u64(), Some(3));
    assert_eq!(json["total_pages"].as_u64(), Some(3));
    assert_eq!(json["has_more"].as_bool(), Some(false));
    assert!(
        json["next_offset"].is_null(),
        "next_offset should be null on last page: {stdout}"
    );
    assert!(
        json["next_page"].is_null(),
        "next_page should be null on last page: {stdout}"
    );
}
