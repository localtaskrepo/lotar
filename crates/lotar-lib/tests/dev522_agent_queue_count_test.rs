//! DEV-522: `lotar agent queue` counts the pending entries it lists, not only
//! the jobs this process queued in memory.

use crate::common;
use common::TestFixtures;

#[test]
fn queued_counts_the_listed_pending_entries() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    std::fs::write(fixtures.tasks_root.join("config.yml"), "default:\n  project: TEST\nagents:\n  quick:\n    runner: command\n    command: 'true'\n").unwrap();
    let cmd = || {
        let mut cmd = std::process::Command::new(crate::common::lotar_bin());
        cmd.current_dir(&root)
            .env_remove("RUST_TEST_THREADS")
            .env("LOTAR_IGNORE_HOME_CONFIG", "1")
            .env("LOTAR_AGENT_QUEUE_DIR", root.join("queue"))
            .env("LOTAR_AGENT_QUEUE_DISABLE_WORKER", "1")
            .env("LOTAR_TEST_SILENT", "1");
        cmd
    };
    for title in ["first", "second"] {
        assert!(cmd().args(["add", title]).status().unwrap().success());
    }
    for ticket in ["TEST-1", "TEST-2"] {
        let run = cmd().args(["agent", "run", ticket, "go", "--agent", "quick"]).output().unwrap();
        assert!(run.status.success(), "{}{}", String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&run.stderr));
    }

    let out = cmd().args(["--format", "json", "agent", "queue"]).output().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["pending"].as_array().map(Vec::len), Some(2), "{json}");
    assert_eq!(json["queued_jobs"], 2, "{json}");

    let out = cmd().args(["agent", "queue"]).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Queued: 2"), "{text}");
}
