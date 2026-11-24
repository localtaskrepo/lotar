//! DEV-800: `lotar add --finding <key> --bundle <name>` adds a finding to the
//! project's open bundle ticket of the day instead of a ticket of its own.
use crate::common::{TestFixtures, cargo_bin_in};
use serde_json::{Value, json};

const CONFIG: &str = "default:\n  project: REPO\nissue:\n  states: [Todo, InProgress, Reviewing, Merging, Done, WontDo]\n  done_states: [Done, WontDo]\ncustom:\n  fields: [product]\n";

fn lotar(fixtures: &TestFixtures, args: &[&str]) -> std::process::Output {
    cargo_bin_in(fixtures).env("LOTAR_DEFAULT_REPORTER", "scanner").args(["--tasks-dir", fixtures.tasks_root.to_str().unwrap()]).args(args).output().unwrap()
}

fn report(fixtures: &TestFixtures, title: &str, key: &str, description: &str) -> Value {
    let output = lotar(fixtures, &["--format", "json", "add", title, "--finding", key, "--bundle", "health", "-D", description]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap()
}

fn outcome(value: &Value) -> (Value, Value, Value) {
    (value["created"].clone(), value["state"].clone(), value["id"].clone())
}

fn listed_ids(fixtures: &TestFixtures, query: &str) -> Vec<String> {
    let output = lotar(fixtures, &["--format", "json", "list", query]);
    let listed: Value = serde_json::from_slice(&output.stdout).unwrap();
    listed["tasks"].as_array().unwrap().iter().map(|task| task["id"].as_str().unwrap().to_string()).collect()
}

fn set_status(fixtures: &TestFixtures, id: &str, status: &str) {
    let output = lotar(fixtures, &["status", id, status]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn findings_of_one_day_share_one_bundle_ticket() {
    let fixtures = TestFixtures::new();
    fixtures.create_config_in_dir(&fixtures.tasks_root, CONFIG);
    let today = jiff::ZonedDateTime::now().strftime("%Y-%m-%d").to_string();

    let first = report(&fixtures, "Stale worktree", "a:x", "2 worktrees left");
    assert_eq!(outcome(&first), (json!(true), json!("new"), json!("REPO-1")));
    assert_eq!(first["task"]["title"], json!(format!("Health {today}: Stale worktree")));
    assert_eq!(first["task"]["custom_fields"]["finding_bundle"], json!(format!("health:{today}")));

    let second = report(&fixtures, "Red CI", "b:y", "main is red");
    assert_eq!(outcome(&second), (json!(false), json!("bundled"), json!("REPO-1")));
    assert!(second["message"].as_str().unwrap().contains("Added finding b:y to REPO-1"), "{second}");
    let bundle = &second["task"];
    assert_eq!(bundle["custom_fields"]["finding"], json!(["a:x", "b:y"]));
    assert_eq!(bundle["title"], json!(format!("Health {today}: Stale worktree and 1 more")));
    let description = bundle["description"].as_str().unwrap();
    for part in ["## Stale worktree\n\n2 worktrees left\n\nFinding: `a:x`", "## Red CI\n\nmain is red\n\nFinding: `b:y`"] {
        assert!(description.contains(part), "{description}");
    }

    // A key of the bundle is found by its list filter, and a repeat report
    // of it is a repeat on the bundle, not a second section.
    assert_eq!(listed_ids(&fixtures, "finding=a:x"), vec!["REPO-1".to_string()]);
    assert_eq!(listed_ids(&fixtures, "finding=b:y"), vec!["REPO-1".to_string()]);
    assert_eq!(listed_ids(&fixtures, &format!("finding_bundle=health:{today}")), vec!["REPO-1".to_string()]);
    let again = report(&fixtures, "Red CI", "b:y", "main is still red");
    assert_eq!(outcome(&again), (json!(false), json!("open"), json!("REPO-1")));
    assert_eq!(again["task"]["description"].as_str().unwrap().matches("## Red CI").count(), 1);

    // The next day starts a new bundle.
    let file = fixtures.tasks_root.join("REPO/1.yml");
    let stored = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, stored.replace(&format!("health:{today}"), "health:2000-01-01")).unwrap();
    let next = report(&fixtures, "Old dependency", "c:z", "jest 29");
    assert_eq!(outcome(&next), (json!(true), json!("new"), json!("REPO-2")));
    assert_eq!(next["task"]["custom_fields"]["finding"], json!("c:z"));
}

#[test]
fn a_wont_do_bundle_suppresses_its_keys() {
    let fixtures = TestFixtures::new();
    fixtures.create_config_in_dir(&fixtures.tasks_root, CONFIG);
    report(&fixtures, "Stale worktree", "a:x", "2 left");
    report(&fixtures, "Red CI", "b:y", "red");
    set_status(&fixtures, "REPO-1", "WontDo");
    for key in ["a:x", "b:y"] {
        let suppressed = report(&fixtures, "Again", key, "still there");
        assert_eq!(outcome(&suppressed), (json!(false), json!("suppressed"), json!("REPO-1")));
    }
    // A new key doesn't join a won't-do bundle.
    let new = report(&fixtures, "Old dependency", "c:z", "jest 29");
    assert_eq!(outcome(&new), (json!(true), json!("new"), json!("REPO-2")));
}

#[test]
fn a_bundle_in_a_job_takes_no_new_findings() {
    let fixtures = TestFixtures::new();
    fixtures.create_config_in_dir(&fixtures.tasks_root, CONFIG);
    report(&fixtures, "Stale worktree", "a:x", "2 left");
    for (number, status) in [(1, "InProgress"), (2, "Reviewing"), (3, "Merging")] {
        set_status(&fixtures, &format!("REPO-{number}"), status);
        let key = format!("k:{number}");
        let next = report(&fixtures, "Another problem", &key, "found while the job ran");
        assert_eq!(outcome(&next), (json!(true), json!("new"), json!(format!("REPO-{}", number + 1))), "{status}");
    }
    // The bundles in a job kept their findings; the newest open one takes the next.
    let joined = report(&fixtures, "One more", "k:9", "late");
    assert_eq!(outcome(&joined), (json!(false), json!("bundled"), json!("REPO-4")));
    assert_eq!(listed_ids(&fixtures, "finding=a:x"), vec!["REPO-1".to_string()]);
}

#[test]
fn bundle_needs_a_finding_and_a_one_word_name() {
    let fixtures = TestFixtures::new();
    fixtures.create_config_in_dir(&fixtures.tasks_root, CONFIG);
    assert!(!lotar(&fixtures, &["add", "No key", "--bundle", "health"]).status.success());
    let output = lotar(&fixtures, &["add", "Bad name", "--finding", "a:x", "--bundle", "health:2026"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Invalid bundle name"), "{}", String::from_utf8_lossy(&output.stderr));
}
