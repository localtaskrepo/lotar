//! DEV-16: links are written to both tasks unless
//! `auto.reciprocal_relationships` is off. DEV-7: ticket IDs mentioned in a
//! description or comment add `related` links unless `auto.link_mentions` is
//! off. Both work the same from the CLI and the REST API.

use lotar::api_server::{ApiServer, HttpRequest};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::Path;

mod common;
use common::env_mutex::EnvVarGuard;

fn run(dir: &Path, args: &[&str]) -> (bool, String) {
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

fn rels(dir: &Path, id: &str) -> Value {
    let (ok, out) = run(dir, &["--format", "json", "show", id]);
    assert!(ok, "{out}");
    serde_json::from_str::<Value>(out.trim()).unwrap()["relationships"].clone()
}

fn ids(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| item.as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn workspace(tasks: usize) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    for n in 1..=tasks {
        let (ok, out) = run(tmp.path(), &["add", &format!("Task {n}"), "-p", "RL"]);
        assert!(ok, "{out}");
    }
    tmp
}

#[test]
fn reciprocal_links_can_be_switched_off() {
    let tmp = workspace(3);
    let dir = tmp.path();
    let (ok, out) = run(
        dir,
        &[
            "task",
            "relationships",
            "add",
            "RL-1",
            "RL-2",
            "--kind",
            "depends-on",
        ],
    );
    assert!(ok, "{out}");
    assert_eq!(ids(&rels(dir, "RL-2")["blocks"]), ["RL-1"]);

    assert!(
        run(
            dir,
            &[
                "config",
                "set",
                "auto.reciprocal_relationships",
                "false",
                "--global"
            ]
        )
        .0
    );
    let (ok, out) = run(
        dir,
        &[
            "task",
            "relationships",
            "add",
            "RL-1",
            "RL-3",
            "--kind",
            "depends-on",
        ],
    );
    assert!(ok, "{out}");
    assert_eq!(ids(&rels(dir, "RL-1")["depends_on"]), ["RL-2", "RL-3"]);
    assert!(
        rels(dir, "RL-3").get("blocks").is_none(),
        "only the edited task changes"
    );

    // One-sided links are expected now, so check stays clean.
    let (ok, out) = run(dir, &["task", "relationships", "check"]);
    assert!(ok && out.contains("issues: 0"), "{out}");

    // Removing still clears both sides of a link that has both.
    let (ok, out) = run(
        dir,
        &[
            "task",
            "relationships",
            "remove",
            "RL-1",
            "RL-2",
            "--kind",
            "depends-on",
        ],
    );
    assert!(ok, "{out}");
    assert!(rels(dir, "RL-2").get("blocks").is_none());
    assert_eq!(ids(&rels(dir, "RL-1")["depends_on"]), ["RL-3"]);

    // Turned back on, check reports the missing side again.
    assert!(
        run(
            dir,
            &[
                "config",
                "set",
                "auto.reciprocal_relationships",
                "true",
                "--global"
            ]
        )
        .0
    );
    let (_, out) = run(dir, &["task", "relationships", "check"]);
    assert!(out.contains("missing_mirror"), "{out}");
}

#[test]
fn mentions_in_descriptions_and_comments_add_related_links() {
    let tmp = workspace(4);
    let dir = tmp.path();
    let (ok, out) = run(
        dir,
        &[
            "task",
            "relationships",
            "add",
            "RL-1",
            "RL-2",
            "--kind",
            "depends-on",
        ],
    );
    assert!(ok, "{out}");

    let (ok, out) = run(
        dir,
        &[
            "add",
            "Mentions",
            "-p",
            "RL",
            "--description",
            "See RL-3 and RL-2, not `RL-4` or RL-99, and UTF-8 is not a task.",
        ],
    );
    assert!(ok, "{out}");
    assert_eq!(ids(&rels(dir, "RL-5")["related"]), ["RL-2", "RL-3"]);
    assert_eq!(ids(&rels(dir, "RL-3")["related"]), ["RL-5"], "two-way");

    // Already linked another way: no extra related link.
    assert!(
        run(
            dir,
            &["comment", "RL-1", "-m", "Blocked on RL-2, maybe RL-4 too"]
        )
        .0
    );
    let rl1 = rels(dir, "RL-1");
    assert_eq!(ids(&rl1["depends_on"]), ["RL-2"]);
    assert_eq!(ids(&rl1["related"]), ["RL-4"]);

    // A link removed by hand stays removed on later edits of the same text.
    assert!(
        run(
            dir,
            &[
                "task",
                "relationships",
                "remove",
                "RL-5",
                "RL-3",
                "--kind",
                "related"
            ]
        )
        .0
    );
    assert!(
        run(
            dir,
            &[
                "edit",
                "RL-5",
                "--description",
                "See RL-3 and RL-2, now longer."
            ]
        )
        .0
    );
    assert_eq!(ids(&rels(dir, "RL-5")["related"]), ["RL-2"]);

    // Switched off per project.
    assert!(
        run(
            dir,
            &["config", "set", "auto_link_mentions", "false", "-p", "RL"]
        )
        .0
    );
    assert!(run(dir, &["edit", "RL-3", "--description", "About RL-4"]).0);
    assert!(rels(dir, "RL-3").get("related").is_none());
}

#[test]
fn rest_updates_and_comments_link_mentions() {
    let tmp = workspace(3);
    let tasks = tmp.path().join(".tasks");
    let _env = [
        EnvVarGuard::set("LOTAR_TASKS_DIR", tasks.to_str().unwrap()),
        EnvVarGuard::set("LOTAR_IGNORE_HOME_CONFIG", "1"),
        EnvVarGuard::clear("LOTAR_PROJECT"),
    ];
    let mut api = ApiServer::new();
    lotar::routes::initialize(&mut api);
    let post = |path: &str, body: Value| {
        let response = api.handle_request(&HttpRequest {
            method: "POST".into(),
            path: path.into(),
            query: HashMap::new(),
            headers: HashMap::new(),
            body: serde_json::to_vec(&body).unwrap(),
        });
        let body: Value = serde_json::from_slice(&response.body).unwrap_or(Value::Null);
        assert_eq!(response.status, 200, "{body}");
        body
    };
    let body = post(
        "/api/tasks/update",
        json!({"id": "RL-1", "description": "Follows up RL-2"}),
    );
    assert_eq!(
        ids(&body["data"]["relationships"]["related"]),
        ["RL-2"],
        "{body}"
    );
    let body = post(
        "/api/tasks/comment",
        json!({"id": "RL-1", "text": "and RL-3"}),
    );
    assert_eq!(
        ids(&body["data"]["relationships"]["related"]),
        ["RL-2", "RL-3"],
        "{body}"
    );
}
