mod common;

use common::TestFixtures;
use common::env_mutex::EnvVarGuard;
use predicates::prelude::*;

#[test]
fn agent_check_flags_in_progress_tasks() {
    let fixtures = TestFixtures::new();
    let _tasks = EnvVarGuard::set(
        "LOTAR_TASKS_DIR",
        fixtures.tasks_root.to_string_lossy().as_ref(),
    );
    let _silent = EnvVarGuard::set("LOTAR_TEST_SILENT", "1");

    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(fixtures.get_temp_path())
        .args(["add", "Check task", "--project=TEST"])
        .assert()
        .success();

    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(fixtures.get_temp_path())
        .args(["status", "TEST-1", "InProgress"])
        .assert()
        .success();

    // Explicit --status since there's no default automation configured
    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(fixtures.get_temp_path())
        .args(["-p", "TEST", "agent", "check", "--status", "InProgress"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Agent check failed"));
}

#[test]
fn agent_check_passes_when_no_match() {
    let fixtures = TestFixtures::new();
    let _tasks = EnvVarGuard::set(
        "LOTAR_TASKS_DIR",
        fixtures.tasks_root.to_string_lossy().as_ref(),
    );
    let _silent = EnvVarGuard::set("LOTAR_TEST_SILENT", "1");

    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(fixtures.get_temp_path())
        .args(["add", "Done task", "--project=TEST"])
        .assert()
        .success();

    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(fixtures.get_temp_path())
        .args(["status", "TEST-1", "Done"])
        .assert()
        .success();

    // Explicit --status since there's no default automation configured
    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(fixtures.get_temp_path())
        .args(["-p", "TEST", "agent", "check", "--status", "InProgress"])
        .assert()
        .success();
}

/// `lotar agent run --model/--effort` overrides the profile for that run;
/// values that look like flags are refused before anything starts (DEV-200).
#[cfg(unix)]
#[test]
fn agent_run_takes_model_and_effort() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path();
    let _tasks = EnvVarGuard::set(
        "LOTAR_TASKS_DIR",
        fixtures.tasks_root.to_string_lossy().as_ref(),
    );
    let _silent = EnvVarGuard::set("LOTAR_TEST_SILENT", "1");
    let seen = root.join("settings.txt");
    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        &format!(
            "default:\n  project: TEST\nagents:\n  stub:\n    runner: command\n    model: profile-model\n    command: sh\n    args: ['-c', 'printf \"%s|%s\" \"$LOTAR_AGENT_MODEL\" \"$LOTAR_AGENT_EFFORT\" > \"{}\"']\n",
            seen.display()
        ),
    );
    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .args(["add", "Run settings"])
        .assert()
        .success();

    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .args([
            "agent",
            "run",
            "TEST-1",
            "go",
            "--agent",
            "stub",
            "--model=--yolo",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid agent model"));
    assert!(!seen.exists(), "a refused run must not start");

    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .args([
            "agent", "run", "TEST-1", "go", "--agent", "stub", "--effort", "high", "--wait",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("model profile-model, effort high"));
    assert_eq!(
        std::fs::read_to_string(&seen).unwrap(),
        "profile-model|high"
    );
}

/// Agents call the CLI themselves; the job tells them which build started it
/// so a different `lotar` on PATH can't answer instead (DEV-206).
#[test]
fn agent_jobs_get_the_running_lotar_binary() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path();
    let _tasks = EnvVarGuard::set(
        "LOTAR_TASKS_DIR",
        fixtures.tasks_root.to_string_lossy().as_ref(),
    );
    let _silent = EnvVarGuard::set("LOTAR_TEST_SILENT", "1");
    let seen = root.join("bin.txt");
    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        &format!(
            "default:\n  project: TEST\nagents:\n  stub:\n    runner: command\n    command: sh\n    args: ['-c', 'printf \"%s|%s|%s\" \"$LOTAR_BIN\" \"$LOTAR_TICKET_PROJECT\" \"${{PATH%%:*}}\" > \"{}\"']\n",
            seen.display()
        ),
    );
    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .args(["add", "Which binary"])
        .assert()
        .success();
    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .args(["agent", "run", "TEST-1", "go", "--agent", "stub", "--wait"])
        .assert()
        .success();
    let seen = std::fs::read_to_string(&seen).unwrap();
    let parts: Vec<&str> = seen.split('|').collect();
    let expected = std::fs::canonicalize(env!("CARGO_BIN_EXE_lotar")).unwrap();
    assert_eq!(std::fs::canonicalize(parts[0]).unwrap(), expected);
    assert_eq!(parts[1], "TEST");
    assert_eq!(
        std::fs::canonicalize(parts[2]).unwrap(),
        expected.parent().unwrap(),
        "the running build's directory comes first on PATH"
    );
}

/// The runner's final answer is the job summary even when a note (denied
/// tools) is logged just before it. The stub runner is a shell script.
#[cfg(unix)]
#[test]
fn agent_job_summary_is_the_final_answer() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path();
    let _tasks = EnvVarGuard::set(
        "LOTAR_TASKS_DIR",
        fixtures.tasks_root.to_string_lossy().as_ref(),
    );
    let _silent = EnvVarGuard::set("LOTAR_TEST_SILENT", "1");
    let stub = root.join("claude-stub.sh");
    std::fs::write(
        &stub,
        r#"#!/bin/sh
cat > /dev/null &
echo '{"type":"system","subtype":"init","session_id":"s1"}'
echo '{"type":"assistant","message":{"content":[{"type":"text","text":"Fixed it."}]},"session_id":"s1"}'
echo '{"type":"result","subtype":"success","is_error":false,"result":"Fixed it.","permission_denials":[{"tool_name":"Bash","tool_input":{"command":"git push"}}],"session_id":"s1"}'
"#,
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        &format!(
            "default:\n  project: TEST\nagents:\n  fixer:\n    runner: claude\n    command: \"{}\"\n",
            stub.display()
        ),
    );
    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .args(["add", "Summary"])
        .assert()
        .success();
    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .args(["agent", "run", "TEST-1", "go", "--agent", "fixer", "--wait"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Fixed it."))
        .stdout(predicate::str::contains("denied").not());
}

/// Job logs are kept by default (DEV-211), so a finished job can be looked up
/// from another process without configuring agent.logs_dir.
#[test]
fn finished_jobs_can_be_read_back_without_configuring_logs() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path();
    let _tasks = EnvVarGuard::set(
        "LOTAR_TASKS_DIR",
        fixtures.tasks_root.to_string_lossy().as_ref(),
    );
    let _silent = EnvVarGuard::set("LOTAR_TEST_SILENT", "1");
    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        "default:\n  project: TEST\nagents:\n  stub:\n    runner: command\n    command: sh\n    args: ['-c', 'echo done']\n",
    );
    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .args(["add", "Logged job"])
        .assert()
        .success();
    let run = crate::common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .args(["agent", "run", "TEST-1", "go", "--agent", "stub", "--wait"])
        .output()
        .unwrap();
    assert!(run.status.success());
    let stdout = String::from_utf8_lossy(&run.stdout);
    let job_id = stdout
        .split_whitespace()
        .find(|word| word.starts_with("job-"))
        .expect("job id in output")
        .to_string();

    let logs = fixtures.tasks_root.join("@agent-logs");
    assert!(logs.join(format!("{job_id}.jsonl")).exists());
    assert!(logs.join(".gitignore").exists());
    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .args(["agent", "status", &job_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("completed"));
}
