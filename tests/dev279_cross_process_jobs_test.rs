//! DEV-279: a job run by one process is visible to, guarded against and
//! cancellable from another, without the lotar-agent-wrapper binary.
#![cfg(unix)]

use crate::common;
use common::TestFixtures;
use std::time::{Duration, Instant};

#[test]
fn jobs_in_another_process_are_seen_guarded_and_cancelled() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    let queue = root.join("queue");
    let done = root.join("AGENT_DONE");
    std::fs::write(
        fixtures.tasks_root.join("config.yml"),
        format!(
            "default:\n  project: TEST\nagents:\n  slow:\n    runner: command\n    command: sh\n    args: ['-c', 'sleep 30; echo finished > \"{}\"']\n",
            done.display()
        ),
    )
    .unwrap();
    let cmd = || {
        let mut cmd = std::process::Command::new(assert_cmd::cargo::cargo_bin!("lotar"));
        cmd.current_dir(&root)
            .env_remove("RUST_TEST_THREADS")
            .env("LOTAR_IGNORE_HOME_CONFIG", "1")
            .env("LOTAR_AGENT_QUEUE_DIR", &queue)
            .env("LOTAR_TEST_SILENT", "1");
        cmd
    };
    assert!(cmd().args(["add", "slow work"]).status().unwrap().success());

    let mut runner = cmd()
        .args(["agent", "run", "TEST-1", "go", "--agent", "slow", "--wait"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();

    // Another process sees the job with its id and ticket.
    let deadline = Instant::now() + Duration::from_secs(15);
    let job_id = loop {
        let out = cmd().args(["--format", "json", "agent", "list-running"]).output().unwrap();
        let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or(serde_json::Value::Null);
        if let Some(id) = json["jobs"].as_array().and_then(|jobs| jobs.iter().find(|job| job["ticket_id"] == "TEST-1")).and_then(|job| job["job_id"].as_str()) {
            break id.to_string();
        }
        assert!(Instant::now() < deadline, "job never showed up: {json}");
        std::thread::sleep(Duration::from_millis(200));
    };

    // The status guard doesn't disturb it: the change waits for its end
    // (DEV-320).
    let status = cmd().args(["status", "TEST-1", "Done"]).output().unwrap();
    assert!(status.status.success());
    assert!(
        String::from_utf8_lossy(&status.stdout).contains(&format!("Done will be set when job {job_id} ends")),
        "{}",
        String::from_utf8_lossy(&status.stdout)
    );

    // And it can be cancelled from here.
    let cancel = cmd().args(["agent", "cancel", &job_id]).output().unwrap();
    assert!(
        String::from_utf8_lossy(&cancel.stdout).contains("cancelled"),
        "{}{}",
        String::from_utf8_lossy(&cancel.stdout),
        String::from_utf8_lossy(&cancel.stderr)
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while runner.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline, "runner kept going after cancel");
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(!done.exists(), "the agent finished despite the cancel");
}
