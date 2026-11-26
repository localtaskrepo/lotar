//! DEV-18: `lotar show <id>` prints one task, and `update` / `edit` work as
//! top-level commands, because that is what people and agents type first.

use crate::common;
fn run(dir: &std::path::Path, args: &[&str]) -> (bool, String) {
    let mut cmd = common::cargo_bin_silent();
    cmd.env("LOTAR_IGNORE_HOME_CONFIG", "1");
    cmd.current_dir(dir);
    let out = cmd.args(args).output().unwrap();
    (out.status.success(), format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

#[test]
fn show_prints_the_whole_task_and_update_edits_it() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    assert!(run(dir, &["add", "First task", "-p", "SH"]).0);
    assert!(run(dir, &["add", "Second task", "-p", "SH"]).0);
    let (ok, out) = run(dir, &["task", "relationships", "add", "SH-1", "SH-2", "--kind", "depends-on"]);
    assert!(ok, "{out}");
    assert!(run(dir, &["comment", "SH-1", "-m", "Looked into it"]).0);

    let (ok, out) = run(dir, &["update", "SH-1", "--description", "Line one\nLine two", "--tag", "ui"]);
    assert!(ok, "{out}");
    let (ok, out) = run(dir, &["edit", "SH-1", "--priority", "High"]);
    assert!(ok, "{out}");

    let (ok, out) = run(dir, &["show", "SH-1"]);
    assert!(ok, "{out}");
    for expected in ["SH-1  First task", "Priority: High", "Tags: ui", "Depends on: SH-2", "  Line one\n  Line two", "Comments (1)", "Looked into it"] {
        assert!(out.contains(expected), "missing {expected:?} in:\n{out}");
    }

    // Short IDs, the `view` alias, `task show` and JSON all reach the same task.
    let (ok, out) = run(dir, &["view", "2", "-p", "SH"]);
    assert!(ok && out.contains("SH-2  Second task") && out.contains("Blocks: SH-1"), "{out}");
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

/// DEV-884: `--brief` cuts comments to one line and drops the history, and
/// `--comment N` prints one in full, so agents don't read every comment.
#[test]
fn brief_cuts_comments_and_comment_prints_one_in_full() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    assert!(run(dir, &["add", "Brief task", "-p", "BR", "-D", "The description"]).0);
    let long = "x".repeat(250);
    assert!(run(dir, &["comment", "BR-1", "-m", "Outcome line\n\n- detail one\n- detail two"]).0);
    assert!(run(dir, &["comment", "BR-1", "-m", &long]).0);
    assert!(run(dir, &["comment", "BR-1", "-m", "Short", "--check", "npm test=passed"]).0);

    let (ok, out) = run(dir, &["show", "BR-1", "--brief"]);
    assert!(ok, "{out}");
    assert!(out.contains("The description") && out.contains("Comments (3)"), "{out}");
    assert!(out.contains("Outcome line…\n") && !out.contains("detail one"), "{out}");
    assert!(out.contains(&format!("{}…\n", "x".repeat(200))) && !out.contains(&"x".repeat(201)), "{out}");
    assert!(out.contains(": Short\n") && !out.contains("npm test"), "{out}");
    assert!(out.trim_end().ends_with("One in full: lotar show BR-1 --comment <N>"), "{out}");

    let (ok, out) = run(dir, &["--format", "json", "show", "BR-1", "--brief"]);
    assert!(ok, "{out}");
    let json: serde_json::Value = serde_json::from_str(out.trim()).expect(&out);
    assert!(json.get("history").is_none(), "{out}");
    assert_eq!(json["comments"][0]["text"], "Outcome line…");
    assert!(json["comments"][2].get("report").is_none(), "{out}");
    let (ok, out) = run(dir, &["--format", "json", "show", "BR-1"]);
    assert!(ok && serde_json::from_str::<serde_json::Value>(out.trim()).expect(&out)["history"].is_array(), "{out}");

    let (ok, out) = run(dir, &["show", "BR-1", "--comment", "1"]);
    assert!(ok && out.starts_with("#1  ") && out.contains("    - detail two") && !out.contains("Brief task"), "{out}");
    let (ok, out) = run(dir, &["show", "BR-1", "--comment", "3"]);
    assert!(ok && out.contains("npm test"), "{out}");
    let (ok, out) = run(dir, &["--format", "json", "show", "BR-1", "--comment", "2"]);
    assert!(ok && serde_json::from_str::<serde_json::Value>(out.trim()).expect(&out)["text"] == long.as_str(), "{out}");

    assert!(run(dir, &["comment", "BR-1", "--delete", "2"]).0);
    for bad in ["0", "2", "4"] {
        let (ok, out) = run(dir, &["show", "BR-1", "--comment", bad]);
        assert!(!ok && out.contains(&format!("no comment #{bad}: comments are #1 to #3")), "{bad}: {out}");
    }
    let (ok, out) = run(dir, &["show", "BR-1", "--brief", "--comment", "1"]);
    assert!(!ok && out.contains("cannot be used with"), "{out}");
}

/// DEV-884: the show command in the job instructions' LoTaR CLI block runs
/// and prints the brief form.
#[test]
fn job_instructions_show_command_runs() {
    let docs = std::fs::read_to_string(std::path::Path::new(env!("REPO_ROOT")).join("docs/help/agent-instructions.md")).unwrap();
    let line = docs.lines().find(|line| line.starts_with("\"${{lotar.bin}}\"") && line.contains(" show ")).expect("a show command in the LoTaR CLI block");
    let command = line.split('#').next().unwrap().replace("\"${{lotar.bin}}\"", "").replace("${{ticket.id}}", "JB-1");
    let args: Vec<&str> = command.split_whitespace().collect();
    assert!(args.contains(&"--brief"), "{line}");

    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    assert!(run(dir, &["add", "Job task", "-p", "JB"]).0);
    assert!(run(dir, &["comment", "JB-1", "-m", "First line\nSecond line"]).0);
    let (ok, out) = run(dir, &args);
    assert!(ok && out.contains("First line…") && !out.contains("Second line"), "{args:?}: {out}");
}
