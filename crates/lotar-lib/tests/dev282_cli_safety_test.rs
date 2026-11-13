//! DEV-282: CLI commands refuse unsafe or invalid input instead of acting
//! on everything or reporting success.

use crate::common;
use common::TestFixtures;

fn run(fixtures: &TestFixtures, args: &[&str], stdin: &str) -> std::process::Output {
    common::lotar_cmd().unwrap().current_dir(fixtures.get_temp_path()).write_stdin(stdin).args(args).output().unwrap()
}

fn workspace() -> TestFixtures {
    let fixtures = TestFixtures::new();
    std::fs::write(fixtures.tasks_root.join("config.yml"), "default:\n  project: SAFE\nissue:\n  tags: [ui, backend]\n").unwrap();
    for title in ["one", "two"] {
        assert!(run(&fixtures, &["add", title], "").status.success());
    }
    fixtures
}

#[test]
fn blank_select_is_refused() {
    let fixtures = workspace();
    for args in
        [&["task", "archive", "--select", ""][..], &["task", "archive", "--select", "   ", "-n"], &["task", "move", "--select", "", "--to", "OTHER", "-n"]]
    {
        let out = run(&fixtures, args, "");
        assert!(!out.status.success(), "{args:?}");
    }
    let listed = String::from_utf8_lossy(&run(&fixtures, &["list"], "").stdout).to_string();
    assert!(listed.contains("SAFE-1") && listed.contains("SAFE-2"), "{listed}");
}

#[test]
fn edit_validates_effort_and_tags() {
    let fixtures = workspace();
    assert!(!run(&fixtures, &["edit", "SAFE-1", "--effort", "banana"], "").status.success());
    assert!(!run(&fixtures, &["edit", "SAFE-1", "--tag", "notallowed"], "").status.success());
    assert!(run(&fixtures, &["edit", "SAFE-1", "--effort", "2h", "--tag", "ui"], "").status.success());
    let yaml = std::fs::read_to_string(fixtures.tasks_root.join("SAFE").join("1.yml")).unwrap();
    assert!(!yaml.contains("banana") && !yaml.contains("notallowed"), "{yaml}");
}

#[test]
fn comments_fail_loudly_and_never_hang() {
    let fixtures = workspace();
    for args in [&["comment", "SAFE-1", "-F", "/nonexistent/comment.txt"][..], &["task", "comment", "SAFE-1", "-F", "/nonexistent/comment.txt"]] {
        let out = run(&fixtures, args, "");
        assert!(!out.status.success(), "{args:?}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("/nonexistent/comment.txt"));
    }
    let out = run(&fixtures, &["comment", "SAFE-1", "-F", "-"], "from stdin\n");
    assert!(out.status.success());
    let shown = String::from_utf8_lossy(&run(&fixtures, &["show", "SAFE-1"], "").stdout).to_string();
    assert!(shown.contains("from stdin"), "{shown}");
}

#[test]
fn delete_without_force_fails_when_it_cannot_ask() {
    let fixtures = workspace();
    let out = run(&fixtures, &["--format", "json", "task", "delete", "SAFE-2"], "");
    assert!(!out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains("Are you sure"), "no prompt on stdout: {stdout}");
    if !stdout.trim().is_empty() {
        serde_json::from_str::<serde_json::Value>(stdout.trim()).unwrap_or_else(|e| panic!("stdout must stay JSON ({e}): {stdout}"));
    }
    let yaml = std::fs::read_to_string(fixtures.tasks_root.join("SAFE").join("2.yml")).unwrap();
    assert!(!yaml.contains("deleted_at"));
}

#[test]
fn sort_by_typos_are_errors() {
    let fixtures = workspace();
    assert!(!run(&fixtures, &["list", "--sort-by", "prority"], "").status.success());
    assert!(run(&fixtures, &["list", "--sort-by", "field:points"], "").status.success());
    assert!(run(&fixtures, &["list", "--sort-by", "priority"], "").status.success());
}
