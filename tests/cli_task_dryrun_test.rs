use predicates::prelude::*;

mod common;
use common::TestFixtures;

#[test]
fn edit_dry_run_previews_without_write() {
    let tf = TestFixtures::new();
    let temp = tf.temp_dir.path();

    // Create a task first and extract ID from output
    let mut cmd = crate::common::lotar_cmd().unwrap();
    let add_out = cmd
        .current_dir(temp)
        .arg("task")
        .arg("add")
        .arg("Initial Task")
        .output()
        .unwrap();
    assert!(add_out.status.success());
    let stdout = String::from_utf8_lossy(&add_out.stdout);
    let id = stdout
        .lines()
        .find_map(|l| {
            l.strip_prefix("✅ Created task: ")
                .map(|s| s.trim().to_string())
        })
        .expect("expected created task id in output");

    // Dry-run edit change priority
    let mut edit = crate::common::lotar_cmd().unwrap();
    edit.current_dir(temp)
        .arg("task")
        .arg("edit")
        .arg(&id)
        .arg("--priority")
        .arg("HIGH")
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("DRY RUN: Would update"));
}

#[test]
fn delete_dry_run_previews_without_delete() {
    let tf = TestFixtures::new();
    let temp = tf.temp_dir.path();

    // Create a task first and extract ID from output
    let mut cmd = crate::common::lotar_cmd().unwrap();
    let add_out = cmd
        .current_dir(temp)
        .arg("task")
        .arg("add")
        .arg("Task To Delete")
        .output()
        .unwrap();
    assert!(add_out.status.success());
    let stdout = String::from_utf8_lossy(&add_out.stdout);
    let id = stdout
        .lines()
        .find_map(|l| {
            l.strip_prefix("✅ Created task: ")
                .map(|s| s.trim().to_string())
        })
        .expect("expected created task id in output");

    // Dry-run delete
    let mut del = crate::common::lotar_cmd().unwrap();
    del.current_dir(temp)
        .arg("task")
        .arg("delete")
        .arg(&id)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("DRY RUN: Would delete task"));

    // Verify the task still exists by attempting to delete for real (should succeed)
    let mut del2 = crate::common::lotar_cmd().unwrap();
    del2.current_dir(temp)
        .arg("task")
        .arg("delete")
        .arg(&id)
        .arg("--force")
        .assert()
        .success()
        .stdout(predicate::str::contains("deleted successfully"));
}
