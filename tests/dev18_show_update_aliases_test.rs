//! DEV-18: `lotar show <id>` prints one task, and `update` / `edit` work as
//! top-level commands, because that is what people and agents type first.

mod common;

fn run(dir: &std::path::Path, args: &[&str]) -> (bool, String) {
    let mut cmd = common::cargo_bin_silent();
    cmd.env("LOTAR_IGNORE_HOME_CONFIG", "1");
    cmd.current_dir(dir);
    let out = cmd.args(args).output().unwrap();
    (
        out.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

#[test]
fn show_prints_the_whole_task_and_update_edits_it() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    assert!(run(dir, &["add", "First task", "-p", "SH"]).0);
    assert!(run(dir, &["add", "Second task", "-p", "SH"]).0);
    let (ok, out) = run(
        dir,
        &[
            "task",
            "relationships",
            "add",
            "SH-1",
            "SH-2",
            "--kind",
            "depends-on",
        ],
    );
    assert!(ok, "{out}");
    assert!(run(dir, &["comment", "SH-1", "-m", "Looked into it"]).0);

    let (ok, out) = run(
        dir,
        &[
            "update",
            "SH-1",
            "--description",
            "Line one\nLine two",
            "--tag",
            "ui",
        ],
    );
    assert!(ok, "{out}");
    let (ok, out) = run(dir, &["edit", "SH-1", "--priority", "High"]);
    assert!(ok, "{out}");

    let (ok, out) = run(dir, &["show", "SH-1"]);
    assert!(ok, "{out}");
    for expected in [
        "SH-1  First task",
        "Priority: High",
        "Tags: ui",
        "Depends on: SH-2",
        "  Line one\n  Line two",
        "Comments (1)",
        "Looked into it",
    ] {
        assert!(out.contains(expected), "missing {expected:?} in:\n{out}");
    }

    // Short IDs, the `view` alias, `task show` and JSON all reach the same task.
    let (ok, out) = run(dir, &["view", "2", "-p", "SH"]);
    assert!(
        ok && out.contains("SH-2  Second task") && out.contains("Blocks: SH-1"),
        "{out}"
    );
    let (ok, out) = run(dir, &["task", "show", "SH-2"]);
    assert!(ok && out.contains("SH-2  Second task"), "{out}");
    let (ok, out) = run(dir, &["--format", "json", "show", "SH-1"]);
    assert!(ok, "{out}");
    let json: serde_json::Value = serde_json::from_str(out.trim()).expect(&out);
    assert_eq!(json["id"], "SH-1");
    assert_eq!(json["comments"][0]["text"], "Looked into it");

    let (ok, out) = run(dir, &["show", "SH-99"]);
    assert!(!ok && out.contains("SH-99"), "{out}");
}

#[test]
fn help_is_available_for_the_new_spellings() {
    let tmp = tempfile::tempdir().unwrap();
    for command in ["show", "update", "edit"] {
        let (ok, out) = run(tmp.path(), &["help", command]);
        assert!(ok, "{command}: {out}");
        assert!(out.contains("lotar show"), "{command}: {out}");
    }
}
