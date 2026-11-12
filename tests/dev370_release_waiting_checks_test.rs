//! DEV-370, DEV-385, DEV-396: closing a "Release vX.Y.Z" ticket, or any other
//! prerequisite, tells every Blocked ticket that waited for it to @fix, with
//! lotar's own `.tasks/DEV/automation.yml`, also one a merge agent handed back.
#![cfg(unix)]

use crate::common;
use common::TestFixtures;
use std::time::{Duration, Instant};

/// Lets the fake agent finish and waits until its job has ended, also when an
/// assertion fails: the worker may start the job late, and a job that runs
/// after the workspace is deleted writes the ticket back into it. The agent
/// also stops once the workspace is gone.
struct Finish<'a> {
    go: std::path::PathBuf,
    cmd: &'a dyn Fn() -> std::process::Command,
}

impl Drop for Finish<'_> {
    fn drop(&mut self) {
        let _ = std::fs::write(&self.go, b"");
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            let out = (self.cmd)().args(["--format", "json", "agent", "list-jobs"]).output();
            let json: serde_json::Value = out.ok().and_then(|out| serde_json::from_slice(&out.stdout).ok()).unwrap_or_default();
            let ended = json["jobs"].as_array().is_some_and(|jobs| {
                ["DEV-2", "DEV-3"]
                    .iter()
                    .all(|id| jobs.iter().any(|job| job["ticket_id"] == *id && !matches!(job["status"].as_str(), Some("queued" | "running"))))
            });
            if ended {
                return;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

fn field<'a>(task: &'a str, name: &str) -> &'a str {
    task.lines().find_map(|line| line.strip_prefix(&format!("{name}: "))).unwrap_or("")
}

/// Records that a merge agent handed the ticket to mallox, as for DEV-361
/// and DEV-364, without starting a merge job.
fn handed_back_by_merge(path: &std::path::Path) {
    let mut task: serde_yaml_ng::Value = serde_yaml_ng::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let entry = serde_yaml_ng::from_str("at: 2026-11-12T00:00:00Z\nactor: merge\nchanges:\n- field: assignee\n  old: '@merge'\n  new: mallox\n").unwrap();
    let history = task.as_mapping_mut().unwrap().entry("history".into()).or_insert_with(|| serde_yaml_ng::Value::Sequence(Vec::new()));
    history.as_sequence_mut().unwrap().push(entry);
    std::fs::write(path, serde_yaml_ng::to_string(&task).unwrap()).unwrap();
}

#[test]
fn closing_a_release_ticket_starts_the_checks_that_wait_for_it() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    let queue = root.join("queue");
    let go = root.join("GO");
    std::fs::write(
        fixtures.tasks_root.join("config.yml"),
        format!(
            "default:\n  project: DEV\nagents:\n  fix: &wait\n    runner: command\n    command: sh\n    args: ['-c', 'while [ ! -f \"{}\" ] && [ -d \"{}\" ]; do sleep 0.1; done']\n  merge: *wait\n",
            go.display(),
            root.display()
        ),
    )
    .unwrap();
    // As in lotar's own .tasks: only the project has a Blocked status.
    std::fs::create_dir_all(fixtures.tasks_root.join("DEV")).unwrap();
    std::fs::write(fixtures.tasks_root.join("DEV/config.yml"), "issue:\n  states: [Todo, InProgress, HelpNeeded, Blocked, Done, WontDo]\n").unwrap();
    std::fs::copy(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".tasks/DEV/automation.yml"), fixtures.tasks_root.join("DEV/automation.yml")).unwrap();
    // The rule runs the lotar on PATH: here the one under test.
    let lotar = assert_cmd::cargo::cargo_bin!("lotar");
    let path = format!("{}:{}", lotar.parent().unwrap().display(), std::env::var("PATH").unwrap_or_default());
    let cmd = || {
        let mut cmd = std::process::Command::new(lotar);
        cmd.current_dir(&root)
            .env_remove("RUST_TEST_THREADS")
            .env_remove("LOTAR_AGENT_JOB_ID")
            .env_remove("LOTAR_TICKET_ID")
            .env("PATH", &path)
            .env("LOTAR_IGNORE_HOME_CONFIG", "1")
            .env("LOTAR_TRUST_ALL", "1")
            .env("LOTAR_AGENT_QUEUE_DIR", &queue)
            .env("LOTAR_AUTOMATION_STATE_DIR", root.join("state"))
            .env("LOTAR_TEST_SILENT", "1");
        cmd
    };
    let run = |args: &[&str]| {
        let out = cmd().args(args).output().unwrap();
        assert!(out.status.success(), "lotar {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    };
    let _finish = Finish { go, cmd: &cmd };

    run(&["add", "Release v9.9.9", "--assignee", "mallox"]);
    run(&["add", "Waits for the release", "--assignee", "mallox"]);
    run(&["add", "Waits for other work", "--assignee", "mallox"]);
    run(&["add", "Other work", "--assignee", "mallox"]);
    run(&["task", "relationships", "add", "DEV-2", "DEV-1", "--kind", "depends-on"]);
    run(&["task", "relationships", "add", "DEV-3", "DEV-4", "--kind", "depends-on"]);
    handed_back_by_merge(&fixtures.tasks_root.join("DEV/2.yml"));
    run(&["status", "DEV-2", "Blocked"]);
    run(&["status", "DEV-3", "Blocked"]);

    // Each prerequisite that goes Done starts only the ticket that waited for it.
    run(&["status", "DEV-4", "Done"]);
    let other = std::fs::read_to_string(fixtures.tasks_root.join("DEV/3.yml")).unwrap();
    assert_eq!(field(&other, "assignee"), "'@fix'", "{other}");
    let waiting = std::fs::read_to_string(fixtures.tasks_root.join("DEV/2.yml")).unwrap();
    assert_eq!(field(&waiting, "assignee"), "mallox", "{waiting}");

    let out = cmd().args(["status", "DEV-1", "Done"]).output().unwrap();
    let printed = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "{printed}");
    let waiting = std::fs::read_to_string(fixtures.tasks_root.join("DEV/2.yml")).unwrap();
    assert_eq!(field(&waiting, "assignee"), "'@fix'", "@fix, not the merge agent that handed it back: {waiting}");
}
