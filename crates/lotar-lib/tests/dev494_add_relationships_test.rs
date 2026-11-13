//! DEV-494: `lotar add` stores relationships in the write that creates the
//! task, with their reciprocals, so the `created` rules see them.

use serde_json::Value;
use std::path::Path;

use crate::common;

fn run(dir: &Path, args: &[&str]) -> (bool, String) {
    let mut cmd = common::cargo_bin_silent();
    cmd.env("LOTAR_IGNORE_HOME_CONFIG", "1");
    cmd.current_dir(dir);
    let out = cmd.args(args).output().unwrap();
    (out.status.success(), format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

fn show(dir: &Path, id: &str) -> Value {
    let (ok, out) = run(dir, &["--format", "json", "show", id]);
    assert!(ok, "{out}");
    serde_json::from_str(out.trim()).unwrap()
}

fn ids(value: &Value) -> Vec<&str> {
    value.as_array().map(|items| items.iter().filter_map(Value::as_str).collect()).unwrap_or_default()
}

/// RL-1 and RL-2, with a Blocked status and a `created` rule whose condition
/// asks for a `depends_on` link and that comments what the ticket waits for.
fn workspace() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let tasks = tmp.path().join(".tasks");
    std::fs::create_dir_all(tasks.join("RL")).unwrap();
    std::fs::write(tasks.join("config.yml"), "default:\n  project: RL\nissue:\n  states: [Todo, Blocked, InProgress, Done]\n").unwrap();
    for n in 1..=2 {
        std::fs::write(
            tasks.join(format!("RL/{n}.yml")),
            format!("title: Task {n}\nstatus: Todo\npriority: Medium\ntype: Feature\ncreated: 2026-11-01T00:00:00Z\n"),
        )
        .unwrap();
    }
    std::fs::write(
        tasks.join("automation.yml"),
        "automation:\n  rules:\n    - name: Note the wait\n      when:\n        status: Blocked\n        depends_on: { exists: true }\n      on:\n        created:\n          comment: \"Waits for ${{ticket.relationships.depends_on[*]}}\"\n",
    )
    .unwrap();
    tmp
}

#[test]
fn add_stores_links_and_reciprocals_in_the_creating_write() {
    let tmp = workspace();
    let dir = tmp.path();
    let (ok, out) = run(dir, &["add", "Waits", "--status", "Blocked", "--depends-on", "RL-1", "--related", "2", "--depends-on", "RL-1"]);
    assert!(ok, "{out}");

    let task = show(dir, "RL-3");
    assert_eq!(ids(&task["relationships"]["depends_on"]), ["RL-1"]);
    assert_eq!(ids(&task["relationships"]["related"]), ["RL-2"]);
    // Created with the links, not changed afterwards.
    let file = std::fs::read_to_string(dir.join(".tasks/RL/3.yml")).unwrap();
    assert!(!file.contains("field: relationships"), "{file}");
    assert_eq!(ids(&show(dir, "RL-1")["relationships"]["blocks"]), ["RL-3"]);
    assert_eq!(ids(&show(dir, "RL-2")["relationships"]["related"]), ["RL-3"]);

    // The `created` rule's condition saw the link: it ran on RL-3 only.
    assert_eq!(comments(&task), ["Waits for RL-1"]);
    let (ok, out) = run(dir, &["add", "Waits for nothing", "--status", "Blocked", "--related", "RL-1"]);
    assert!(ok, "{out}");
    assert!(comments(&show(dir, "RL-4")).is_empty());
}

fn comments(task: &Value) -> Vec<&str> {
    task["comments"].as_array().map(|items| items.iter().filter_map(|c| c["text"].as_str()).collect()).unwrap_or_default()
}

#[test]
fn add_takes_every_kind_and_previews_it() {
    let tmp = workspace();
    let dir = tmp.path();
    let (ok, out) =
        run(dir, &["--format", "json", "add", "All kinds", "--blocks", "RL-1", "--parent", "RL-2", "--fixes", "RL-1", "--duplicate-of", "RL-2", "--dry-run"]);
    assert!(ok, "{out}");
    let preview: Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(preview["relationships"], serde_json::json!({"blocks": ["RL-1"], "parent": "RL-2", "fixes": ["RL-1"], "duplicate_of": "RL-2"}));
    assert!(!dir.join(".tasks/RL/3.yml").exists());

    let (ok, out) = run(dir, &["add", "All kinds", "--blocks", "RL-1", "--parent", "RL-2", "--fixes", "RL-1", "--duplicate-of", "RL-2"]);
    assert!(ok, "{out}");
    let rels = &show(dir, "RL-3")["relationships"];
    assert_eq!((ids(&rels["blocks"]), rels["parent"].as_str(), ids(&rels["fixes"])), (vec!["RL-1"], Some("RL-2"), vec!["RL-1"]));
    assert_eq!(rels["duplicate_of"], "RL-2");
    assert_eq!(ids(&show(dir, "RL-1")["relationships"]["depends_on"]), ["RL-3"]);
    assert_eq!(ids(&show(dir, "RL-2")["relationships"]["children"]), ["RL-3"]);
}

#[test]
fn missing_or_malformed_ids_fail_without_creating_the_task() {
    let tmp = workspace();
    let dir = tmp.path();
    for dry_run in [true, false] {
        let mut args = vec!["add", "Waits", "--depends-on", "RL-9"];
        if dry_run {
            args.push("--dry-run");
        }
        let (ok, out) = run(dir, &args);
        assert!(!ok && out.contains("Relationship task 'RL-9' is missing"), "{out}");
    }
    let (ok, out) = run(dir, &["add", "Waits", "--blocks", "not an id"]);
    assert!(!ok && out.contains("Invalid task ID format"), "{out}");
    assert!(!dir.join(".tasks/RL/3.yml").exists());
    assert!(show(dir, "RL-1")["relationships"].get("blocks").is_none());
}
