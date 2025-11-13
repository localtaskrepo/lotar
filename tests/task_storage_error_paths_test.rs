#![cfg(unix)]

use predicates::prelude::*;

mod common;
use common::{TestFixtures, extract_task_id_from_bytes};

#[test]
fn status_update_reports_storage_failure() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    let add_output = crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp_dir)
        .env("LOTAR_TEST_SILENT", "1")
        .arg("add")
        .arg("Permission failure guard")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let task_id = extract_task_id_from_bytes(&add_output).expect("task id present in output");
    let project_prefix = task_id.split('-').next().expect("project prefix available");
    let numeric_id = task_id.split('-').nth(1).expect("numeric id available");

    let task_file = fixtures
        .tasks_root
        .join(project_prefix)
        .join(format!("{numeric_id}.yml"));

    let metadata = std::fs::metadata(&task_file).expect("task file metadata");
    let mut perms = metadata.permissions();
    let original_perms = perms.clone();
    perms.set_readonly(true);
    std::fs::set_permissions(&task_file, perms).expect("set read-only");

    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp_dir)
        .env("LOTAR_TEST_SILENT", "1")
        .arg("status")
        .arg(&task_id)
        .arg("done")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Storage error while updating task",
        ))
        .stderr(predicate::str::contains(&task_id));

    std::fs::set_permissions(&task_file, original_perms).expect("restore permissions");
}

#[test]
fn task_delete_reports_storage_failure() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    let add_output = crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp_dir)
        .env("LOTAR_TEST_SILENT", "1")
        .arg("add")
        .arg("Delete permission guard")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let task_id = extract_task_id_from_bytes(&add_output).expect("task id present in output");
    let project_prefix = task_id.split('-').next().expect("project prefix available");

    let project_dir = fixtures.tasks_root.join(project_prefix);
    let metadata = std::fs::metadata(&project_dir).expect("project dir metadata");
    let mut perms = metadata.permissions();
    let original_perms = perms.clone();
    perms.set_readonly(true);
    std::fs::set_permissions(&project_dir, perms).expect("set dir read-only");

    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp_dir)
        .env("LOTAR_TEST_SILENT", "1")
        .args(["task", "delete", &task_id, "--force"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Storage error while deleting task",
        ))
        .stderr(predicate::str::contains(&task_id));

    std::fs::set_permissions(&project_dir, original_perms).expect("restore permissions");
}
