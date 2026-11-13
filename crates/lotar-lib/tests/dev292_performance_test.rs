//! DEV-292: the activity feed reads blobs through one batched git process,
//! `@least_busy` spreads work across a manual run, and exclusions filter the
//! main query's results in memory.
#![cfg(unix)]

use crate::common;
use common::TestFixtures;
use lotar::services::audit_service::AuditService;

fn git(root: &std::path::Path, args: &[&str]) {
    let out = std::process::Command::new("git").current_dir(root).env_remove("GIT_DIR").args(args).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
}

fn lotar(root: &std::path::Path, args: &[&str]) -> String {
    let out = common::lotar_cmd().unwrap().current_dir(root).env("LOTAR_DEFAULT_REPORTER", "tester").args(args).output().unwrap();
    assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn ids(json: &str) -> Vec<String> {
    let value: serde_json::Value = serde_json::from_str(json).unwrap();
    let mut ids: Vec<String> = value["tasks"].as_array().unwrap().iter().map(|task| task["id"].as_str().unwrap().to_string()).collect();
    ids.sort();
    ids
}

#[test]
fn feed_reports_every_task_of_a_bulk_commit() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path();
    git(root, &["init", "-q"]);
    git(root, &["config", "user.name", "Dev"]);
    git(root, &["config", "user.email", "dev@example.com"]);
    for i in 0..12 {
        lotar(root, &["add", &format!("task {i}"), "-p", "DEMO"]);
    }
    lotar(root, &["priority", "DEMO-1", "high"]);
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "create"]);
    for i in 1..=12 {
        lotar(root, &["status", &format!("DEMO-{i}"), "InProgress"]);
    }
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "bulk status"]);

    let since = chrono::Utc::now() - chrono::Duration::days(1);
    let until = chrono::Utc::now() + chrono::Duration::days(1);
    let feed = AuditService::list_activity_feed(root, std::path::Path::new(".tasks"), since, until, None, 200).unwrap();

    let bulk: Vec<_> = feed.iter().filter(|i| i.message == "bulk status").collect();
    assert_eq!(bulk.len(), 12, "{feed:#?}");
    for item in &bulk {
        assert!(item.task_title.as_deref().unwrap().starts_with("task "));
        assert!(item.history.iter().any(|entry| entry.changes.iter().any(|c| c.field == "status" && c.new.as_deref() == Some("InProgress"))), "{item:#?}");
    }
    // The root commit has no parent; its history entries still show (new
    // tasks start without history, so only the priority change appears).
    let created: Vec<_> = feed.iter().filter(|i| i.message == "create").collect();
    assert_eq!(created.len(), 1, "{feed:#?}");
    assert_eq!(created[0].task_id, "DEMO-1");
}

#[test]
fn least_busy_spreads_a_manual_run() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path();
    lotar(root, &["add", "busy", "-p", "LB", "--assignee", "alice"]);
    for i in 0..6 {
        lotar(root, &["add", &format!("open {i}"), "-p", "LB"]);
    }
    std::fs::write(root.join(".tasks/config.yml"), "default:\n  project: LB\n  members: [alice, bob, carol]\n").unwrap();
    std::fs::write(
        root.join(".tasks/automation.yml"),
        r#"automation:
  rules:
    - name: Balance
      when:
        assignee: { exists: false }
      on:
        manual:
          set:
            assignee: "@least_busy"
"#,
    )
    .unwrap();
    lotar(root, &["automation", "run", "Balance"]);

    let json = lotar(root, &["list", "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let mut counts = std::collections::BTreeMap::new();
    for task in value["tasks"].as_array().unwrap() {
        *counts.entry(task["assignee"].as_str().unwrap_or("-").to_string()).or_insert(0) += 1;
    }
    // Every task is assigned and the load stays within one task per member
    // (the project's members are alice plus the auto-added reporter).
    assert_eq!(counts.get("-"), None, "{counts:?}");
    assert!(counts.len() >= 2, "{counts:?}");
    let max = counts.values().max().unwrap();
    let min = counts.values().min().unwrap();
    assert!(max - min <= 1, "{counts:?}");
}

#[test]
fn exclusions_filter_the_main_results() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path();
    lotar(root, &["add", "a", "-p", "DEMO", "--tag", "ui", "-P", "low"]);
    lotar(root, &["add", "b", "-p", "DEMO", "--tag", "api", "-P", "high"]);
    lotar(root, &["add", "c", "-p", "DEMO", "--tag", "ui", "-P", "high"]);
    lotar(root, &["add", "d", "-p", "DEMO", "-P", "low", "--type", "bug"]);
    lotar(root, &["add", "e", "-p", "DEMO", "-P", "high", "--assignee", "bob"]);
    lotar(root, &["status", "DEMO-5", "Done"]);

    let list = |args: &[&str]| {
        let mut all = vec!["list", "--format", "json"];
        all.extend_from_slice(args);
        ids(&lotar(root, &all))
    };
    assert_eq!(list(&["-tag:ui"]), ["DEMO-2", "DEMO-4", "DEMO-5"]);
    assert_eq!(list(&["-tag:ui", "-priority:low"]), ["DEMO-2", "DEMO-5"]);
    assert_eq!(list(&["--", "-type:bug", "-tag:api"]), ["DEMO-1", "DEMO-3", "DEMO-5"]);
    assert_eq!(list(&["-assignee:bob"]), ["DEMO-1", "DEMO-2", "DEMO-3", "DEMO-4"]);
    assert_eq!(list(&["priority:high", "-status:Done"]), ["DEMO-2", "DEMO-3"]);
}
