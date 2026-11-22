//! DEV-786: `lotar agent queue` starts tickets by priority, then kind, then
//! arrival, shows each entry's kind, and `agent queue move` reorders it.

use crate::common;
use common::TestFixtures;

#[test]
fn the_queue_lists_by_priority_with_kinds_and_moves_an_entry() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path().to_path_buf();
    std::fs::write(
        fixtures.tasks_root.join("config.yml"),
        "default:\n  project: TEST\nagents:\n  fix:\n    runner: command\n    command: 'true'\n  review:\n    runner: command\n    command: 'true'\n",
    )
    .unwrap();
    let cmd = || {
        let mut cmd = std::process::Command::new(crate::common::lotar_bin());
        cmd.current_dir(&root)
            .env_remove("RUST_TEST_THREADS")
            .env("LOTAR_IGNORE_HOME_CONFIG", "1")
            .env("LOTAR_AGENT_QUEUE_DIR", root.join("queue"))
            .env("LOTAR_AGENT_QUEUE_DISABLE_WORKER", "1")
            .env("LOTAR_TRUST_ALL", "1")
            .env("LOTAR_TEST_SILENT", "1");
        cmd
    };
    let run = |args: &[&str]| {
        let out = cmd().args(args).output().unwrap();
        assert!(out.status.success(), "{args:?}: {}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).to_string()
    };
    run(&["add", "low fix", "--priority", "Low"]);
    run(&["add", "medium review", "--priority", "Medium"]);
    run(&["add", "high fix", "--priority", "High"]);
    run(&["agent", "run", "TEST-1", "fix TEST-1", "--agent", "fix"]);
    run(&["agent", "run", "TEST-2", "review TEST-2", "--agent", "review"]);
    run(&["agent", "run", "TEST-3", "fix TEST-3", "--agent", "fix"]);

    let order = || {
        let json: serde_json::Value = serde_json::from_str(&run(&["--format", "json", "agent", "queue"])).unwrap();
        json["pending"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| format!("{} {} {}", e["ticket_id"].as_str().unwrap(), e["kind"].as_str().unwrap(), e["priority"].as_str().unwrap()))
            .collect::<Vec<_>>()
    };
    assert_eq!(order(), ["TEST-3 fix High", "TEST-2 review Medium", "TEST-1 fix Low"]);

    let text = run(&["agent", "queue"]);
    assert!(text.contains("\tkind\tpriority\t"), "{text}");
    assert!(text.contains("TEST-2\treview\tMedium\treview\t"), "{text}");

    let moved = run(&["agent", "queue", "move", "TEST-1", "--first"]);
    assert!(moved.contains("Moved TEST-1 (fix @fix) to place 1 of 3."), "{moved}");
    assert_eq!(order(), ["TEST-1 fix Low", "TEST-3 fix High", "TEST-2 review Medium"]);

    let missing = cmd().args(["agent", "queue", "move", "TEST-9", "--up"]).output().unwrap();
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("TEST-9 is not in the queue"), "{}", String::from_utf8_lossy(&missing.stderr));
}
