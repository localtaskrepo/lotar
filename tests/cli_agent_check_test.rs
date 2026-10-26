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
