//! DEV-283: CLI outputs use canonical IDs and honest no-ops, history takes
//! numeric IDs, and fields can be cleared from the CLI.

use crate::common;
use common::TestFixtures;

fn run(fixtures: &TestFixtures, args: &[&str]) -> std::process::Output {
    common::lotar_cmd()
        .unwrap()
        .current_dir(fixtures.get_temp_path())
        .args(args)
        .output()
        .unwrap()
}

fn json(out: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stdout)))
}

#[test]
fn lifecycle_output_is_canonical_and_honest() {
    let fixtures = TestFixtures::new();
    assert!(
        run(
            &fixtures,
            &["add", "one", "-p", "ALPH", "--tag", "x", "--tag", "y"]
        )
        .status
        .success()
    );

    let restored = json(&run(
        &fixtures,
        &["-f", "json", "task", "restore", "1", "-p", "ALPH"],
    ));
    assert_eq!(restored["task_id"], "ALPH-1");
    assert_eq!(restored["changed"], false);

    let edited = json(&run(
        &fixtures,
        &["-f", "json", "edit", "1", "-p", "ALPH", "-T", "X"],
    ));
    assert_eq!(edited["task_id"], "ALPH-1");
    assert_eq!(edited["task"]["title"], "X");

    let noop = json(&run(&fixtures, &["-f", "json", "status", "ALPH-1", "Todo"]));
    assert_eq!(noop["status"], "success");
    assert_eq!(noop["task_status"], "Todo");

    let first = json(&run(
        &fixtures,
        &["-f", "json", "task", "delete", "1", "-p", "ALPH", "-y"],
    ));
    assert_eq!(first["task_id"], "ALPH-1");
    let again = json(&run(
        &fixtures,
        &["-f", "json", "task", "delete", "1", "-p", "ALPH", "-y"],
    ));
    assert!(
        again["message"]
            .as_str()
            .unwrap()
            .contains("already deleted"),
        "{again}"
    );

    let added = json(&run(
        &fixtures,
        &["-f", "json", "add", "two", "-p", "ALPH", "--tag", "z"],
    ));
    assert_eq!(added["task"]["tags"], serde_json::json!(["z"]));
}

#[test]
fn fields_can_be_cleared_and_tags_removed() {
    let fixtures = TestFixtures::new();
    assert!(
        run(
            &fixtures,
            &[
                "add",
                "one",
                "-p",
                "ALPH",
                "--due",
                "tomorrow",
                "--assignee",
                "bob",
                "--tag",
                "x",
                "--tag",
                "y"
            ]
        )
        .status
        .success()
    );
    assert!(
        run(&fixtures, &["priority", "ALPH-1", "High", "-n"])
            .status
            .success()
    );
    assert!(
        run(&fixtures, &["due-date", "ALPH-1", "--clear"])
            .status
            .success()
    );
    assert!(
        run(&fixtures, &["assignee", "ALPH-1", "--clear"])
            .status
            .success()
    );
    assert!(
        run(&fixtures, &["edit", "ALPH-1", "--untag", "X"])
            .status
            .success()
    );
    let task = json(&run(&fixtures, &["-f", "json", "show", "ALPH-1"]));
    assert!(task.get("due_date").is_none_or(|v| v.is_null()), "{task}");
    assert!(task.get("assignee").is_none_or(|v| v.is_null()), "{task}");
    assert_eq!(task["tags"], serde_json::json!(["y"]));
    assert_eq!(task["priority"], "Medium", "dry run must not write");
}

#[test]
fn changelog_reports_bad_refs() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path();
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .current_dir(root)
            .args(args)
            .output()
            .unwrap()
    };
    git(&["init", "-q"]);
    git(&[
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@e",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "-q",
        "--allow-empty",
        "-m",
        "init",
    ]);
    let out = run(&fixtures, &["changelog", "no-such-ref"]);
    assert!(
        !out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
}
