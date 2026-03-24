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
