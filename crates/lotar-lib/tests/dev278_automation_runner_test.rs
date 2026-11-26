//! DEV-278: waited-for run commands don't touch the CLI's stdio and time
//! out, cooldowns hold across processes, and one failing rule doesn't stop
//! the rules after it.
#![cfg(unix)]

use crate::common;
use common::TestFixtures;

fn with_rules(rules: &str) -> TestFixtures {
    with_rules_in(TestFixtures::new(), rules)
}

fn with_rules_in(fixtures: TestFixtures, rules: &str) -> TestFixtures {
    std::fs::write(fixtures.tasks_root.join("config.yml"), "default:\n  project: AR\n").unwrap();
    std::fs::write(fixtures.tasks_root.join("automation.yml"), format!("automation:\n  rules:\n{rules}")).unwrap();
    fixtures
}

fn lotar(fixtures: &TestFixtures, args: &[&str], stdin: &str) -> std::process::Output {
    common::lotar_cmd().unwrap().current_dir(fixtures.get_temp_path()).write_stdin(stdin).args(args).output().unwrap()
}

#[test]
fn run_commands_keep_off_stdout_and_stdin() {
    let fixtures = with_rules("    - name: chatty\n      on:\n        updated:\n          run: \"echo automation-says-hi; cat > STOLEN\"\n");
    lotar(&fixtures, &["add", "one"], "");
    let out = lotar(&fixtures, &["--format", "json", "priority", "AR-1", "High"], "secret input\n");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str::<serde_json::Value>(stdout.trim()).unwrap_or_else(|e| panic!("stdout must stay JSON ({e}): {stdout}"));
    assert!(String::from_utf8_lossy(&out.stderr).contains("automation-says-hi"));
    let stolen = std::fs::read_to_string(fixtures.get_temp_path().join("STOLEN")).unwrap();
    assert!(stolen.is_empty(), "the command read our stdin: {stolen}");
}

#[test]
fn run_commands_time_out() {
    let fixtures = with_rules(
        "    - name: slow\n      on:\n        updated:\n          run:\n            command: sleep\n            args: ['20']\n            timeout: 1s\n",
    );
    lotar(&fixtures, &["add", "one"], "");
    let started = std::time::Instant::now();
    let out = lotar(&fixtures, &["priority", "AR-1", "High"], "");
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
    assert!(String::from_utf8_lossy(&out.stderr).contains("did not finish within 1s"), "{}", String::from_utf8_lossy(&out.stderr));
    // The owner sees it on the ticket, not only in a server log (DEV-808).
    let task = std::fs::read_to_string(fixtures.tasks_root.join("AR").join("1.yml")).unwrap();
    assert!(task.contains("stopped its run on AR-1: it did not finish within 1s."), "{task}");
}

/// A run whose command starts a process group of its own, as a nested
/// `lotar` call's run does: the timeout stops that group too, not only the
/// run's own group (DEV-808).
#[test]
fn run_timeout_stops_nested_process_groups() {
    let fixtures = TestFixtures::new();
    let pid_file = fixtures.get_temp_path().join("nested.pid");
    let script = format!("perl -e 'setpgrp(0, 0); sleep 600' & echo $! > {}; wait", pid_file.display());
    let fixtures = with_rules_in(
        fixtures,
        &format!(
            "    - name: nested\n      on:\n        updated:\n          run:\n            command: sh\n            args: ['-c', {script:?}]\n            timeout: 1s\n"
        ),
    );
    lotar(&fixtures, &["add", "one"], "");
    let out = lotar(&fixtures, &["priority", "AR-1", "High"], "");
    assert!(String::from_utf8_lossy(&out.stderr).contains("did not finish within 1s"), "{}", String::from_utf8_lossy(&out.stderr));
    let pid = std::fs::read_to_string(&pid_file).unwrap();
    assert!(stops_running(pid.trim()), "{} in the nested process group is still running", pid.trim());
}

/// Whether `pid` is gone or a zombie within a few seconds.
fn stops_running(pid: &str) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let ps = std::process::Command::new("ps").args(["-o", "stat=", "-p", pid]).output().unwrap();
        if !ps.status.success() || String::from_utf8_lossy(&ps.stdout).trim_start().starts_with('Z') {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            let _ = std::process::Command::new("kill").args(["-9", pid]).status();
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[test]
fn a_failing_rule_does_not_stop_later_rules() {
    let fixtures = with_rules(
        "    - name: broken\n      on:\n        updated:\n          run: \"exit 3\"\n    - name: second\n      on:\n        updated:\n          comment: second rule ran\n",
    );
    lotar(&fixtures, &["add", "one"], "");
    let out = lotar(&fixtures, &["priority", "AR-1", "High"], "");
    assert!(String::from_utf8_lossy(&out.stderr).contains("Automation rule 'broken' failed"));
    let task = std::fs::read_to_string(fixtures.tasks_root.join("AR").join("1.yml")).unwrap();
    assert!(task.contains("second rule ran"), "{task}");
}

#[test]
fn cooldowns_hold_across_processes() {
    let fixtures = with_rules("    - name: Nag\n      cooldown: 1d\n      on:\n        manual:\n          comment: Daily nag\n");
    lotar(&fixtures, &["add", "one"], "");
    for _ in 0..2 {
        let out = lotar(&fixtures, &["automation", "run", "Nag"], "");
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    }
    let task = std::fs::read_to_string(fixtures.tasks_root.join("AR").join("1.yml")).unwrap();
    assert_eq!(task.matches("Daily nag").count(), 1, "{task}");
}
