//! DEV-800: `lotar add --finding <key> --bundle <name>` adds a finding to the
//! project's open bundle ticket of the day instead of a ticket of its own.
//! DEV-864: a bundle whose fix job runs takes a finding the job is told of.
use crate::common::env_mutex::EnvVarGuard;
use crate::common::{TestFixtures, cargo_bin_in};
use lotar::services::agent_active_jobs::{self, ActiveJob};
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

/// A fix job of `ticket` another process (this test) runs, in session `session`.
fn fix_job(id: &str, ticket: &str, session: Option<&str>, merge: bool) -> ActiveJob {
    ActiveJob {
        job_id: id.into(),
        ticket_id: ticket.into(),
        runner: "t3".into(),
        agent: Some("fix".into()),
        owner_pid: std::process::id(),
        child_pid: 0,
        started_at: "2026-11-25T10:00:00Z".into(),
        session_id: session.map(str::to_string),
        merge,
        assignee_at_start: None,
        in_setup: false,
        request: None,
    }
}

#[test]
fn a_running_fix_job_that_cant_take_the_finding_leaves_it_to_a_new_bundle() {
    let fixtures = TestFixtures::new();
    fixtures.create_config_in_dir(&fixtures.tasks_root, CONFIG);
    let queue = fixtures.temp_dir.path().join("queue");
    let _env = EnvVarGuard::set("LOTAR_AGENT_QUEUE_DIR", queue.to_str().unwrap());
    let report_with_jobs = |title: &str, key: &str| -> Value {
        let output = cargo_bin_in(&fixtures)
            .env("LOTAR_DEFAULT_REPORTER", "scanner")
            .env("LOTAR_AGENT_QUEUE_DIR", &queue)
            .args(["--tasks-dir", fixtures.tasks_root.to_str().unwrap(), "--format", "json", "add", title, "--finding", key, "--bundle", "health"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        serde_json::from_slice(&output.stdout).unwrap()
    };
    report(&fixtures, "Stale worktree", "a:x", "2 left");
    set_status(&fixtures, "REPO-1", "InProgress");
    // Its session is gone: the job can't hear of the finding, so it doesn't get it.
    agent_active_jobs::register(&fixtures.tasks_root, &fix_job("job-gone", "REPO-1", Some("dev864-no-such-session"), false));
    assert_eq!(outcome(&report_with_jobs("Red CI", "b:y")), (json!(true), json!("new"), json!("REPO-2")));
    // A merge job doesn't fix findings.
    set_status(&fixtures, "REPO-2", "InProgress");
    agent_active_jobs::register(&fixtures.tasks_root, &fix_job("job-merge", "REPO-2", Some("dev864-merge"), true));
    assert_eq!(outcome(&report_with_jobs("Old dependency", "c:z")), (json!(true), json!("new"), json!("REPO-3")));
    assert_eq!(listed_ids(&fixtures, "finding=a:x"), vec!["REPO-1".to_string()]);
}

#[test]
fn a_running_fix_job_with_a_session_keeps_its_bundle_open() {
    let fixtures = TestFixtures::new();
    fixtures.create_config_in_dir(&fixtures.tasks_root, CONFIG);
    let _env = EnvVarGuard::set("LOTAR_AGENT_QUEUE_DIR", fixtures.temp_dir.path().join("queue").to_str().unwrap());
    report(&fixtures, "Stale worktree", "a:x", "2 left");
    set_status(&fixtures, "REPO-1", "InProgress");
    let storage = lotar::storage::manager::Storage::new(&fixtures.tasks_root);
    let config = lotar::config::resolution::load_and_merge_configs(Some(&fixtures.tasks_root)).unwrap();
    let today = lotar::services::finding_service::FindingService::bundle_of_today("health");
    let open = || lotar::services::finding_service::FindingService::open_bundle(&storage, "REPO", &today, &config);
    assert_eq!(open(), None);
    let job = fix_job("job-fix", "REPO-1", Some("dev864-fix"), false);
    agent_active_jobs::register(&fixtures.tasks_root, &job);
    assert_eq!(open(), Some(("REPO-1".to_string(), Some(job))));
}

/// A stand-in owner of harnessmux session `session` (DEV-891): it answers on the
/// session's socket as harnessmux's owner does, `status` and `send` alike, and
/// hands over the first prompt it is sent. Unix only: Windows uses named pipes.
#[cfg(unix)]
fn session_owner(session: &str) -> std::sync::mpsc::Receiver<String> {
    use harnessmux::ipc::{Reply, Request};
    use std::io::{BufRead, BufReader, Write};
    let socket = std::path::PathBuf::from(harnessmux::Registry::default().socket_name(session));
    std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
    let _ = std::fs::remove_file(&socket);
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    let (sent, received) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            let mut reply = Reply::ok();
            let prompt = match serde_json::from_str(&line).unwrap() {
                Request::Send { prompt } => {
                    reply.turn = Some(2);
                    Some(prompt)
                }
                _ => None,
            };
            writeln!(stream, "{}", serde_json::to_string(&reply).unwrap()).unwrap();
            if let Some(prompt) = prompt {
                let _ = sent.send(prompt);
                break;
            }
        }
        let _ = std::fs::remove_file(&socket);
    });
    received
}

#[cfg(unix)]
#[test]
fn a_finding_joins_an_in_progress_bundle_whose_fix_job_takes_the_message() {
    let fixtures = TestFixtures::new();
    fixtures.create_config_in_dir(&fixtures.tasks_root, CONFIG);
    let queue = fixtures.temp_dir.path().join("queue");
    let home = fixtures.temp_dir.path().join("harnessmux");
    let _env = [EnvVarGuard::set("LOTAR_AGENT_QUEUE_DIR", queue.to_str().unwrap()), EnvVarGuard::set("HARNESSMUX_HOME", home.to_str().unwrap())];
    report(&fixtures, "Stale worktree", "a:x", "2 left");
    set_status(&fixtures, "REPO-1", "InProgress");
    let received = session_owner("dev891-fix");
    agent_active_jobs::register(&fixtures.tasks_root, &fix_job("job-fix", "REPO-1", Some("dev891-fix"), false));

    // `lotar add` runs in its own process: it reaches the job through the session's owner.
    let output = cargo_bin_in(&fixtures)
        .env("LOTAR_DEFAULT_REPORTER", "scanner")
        .env("LOTAR_AGENT_QUEUE_DIR", &queue)
        .env("HARNESSMUX_HOME", &home)
        .args(["--tasks-dir", fixtures.tasks_root.to_str().unwrap(), "--format", "json"])
        .args(["add", "Red CI", "--finding", "b:y", "--bundle", "health", "-D", "main is red"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let joined: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(outcome(&joined), (json!(false), json!("bundled"), json!("REPO-1")), "{joined}");

    let prompt = received.recv_timeout(std::time::Duration::from_secs(30)).expect("the fix job's session got a message");
    assert!(prompt.starts_with("A new finding joined REPO-1 while you work on it"), "{prompt}");
    assert!(prompt.contains("## Red CI\n\nmain is red\n\nFinding: `b:y`"), "{prompt}");
    assert_eq!(joined["task"]["custom_fields"]["finding"], json!(["a:x", "b:y"]));
    assert_eq!(listed_ids(&fixtures, "finding=b:y"), vec!["REPO-1".to_string()]);
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
