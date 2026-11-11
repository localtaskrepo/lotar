//! DEV-322: `lotar agent run --wait` returns only after the job's end
//! automation has run, not as soon as the job's status is final.
#![cfg(unix)]

use crate::common;
use common::TestFixtures;

#[test]
fn agent_run_wait_returns_after_the_job_completed_rule() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    let marker = root.join("RULE_DONE");
    std::fs::write(
        fixtures.tasks_root.join("config.yml"),
        "default:\n  project: WAIT\nagents:\n  quick:\n    runner: command\n    command: sh\n    args: ['-c', 'true']\n",
    )
    .unwrap();
    std::fs::write(
        fixtures.tasks_root.join("automation.yml"),
        format!(
            "automation:\n  rules:\n    - name: Slow wrap-up\n      on:\n        job_completed:\n          run:\n            command: sh\n            args: ['-c', 'sleep 1; touch \"{}\"']\n            wait: true\n",
            marker.display()
        ),
    )
    .unwrap();
    let cmd = || {
        let mut cmd = std::process::Command::new(assert_cmd::cargo::cargo_bin!("lotar"));
        cmd.current_dir(&root)
            .env_remove("RUST_TEST_THREADS")
            .env("LOTAR_IGNORE_HOME_CONFIG", "1")
            .env("LOTAR_AGENT_QUEUE_DIR", root.join("queue"))
            .env("LOTAR_TEST_SILENT", "1");
        cmd
    };
    assert!(
        cmd()
            .args(["add", "quick work"])
            .status()
            .unwrap()
            .success()
    );

    let run = cmd()
        .args(["agent", "run", "WAIT-1", "go", "--agent", "quick", "--wait"])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(marker.exists(), "--wait returned before the rule finished");
}
