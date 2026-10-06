//! DEV-92 CLI soft-deletion lifecycle: default soft delete, explicit
//! `--hard` physical deletion with structured warnings, `--deleted` /
//! `--include-deleted` list views, `task restore` (incl. dry-run), and the
//! confirmation-only semantics of `--yes`.

mod common;
use common::TestFixtures;

use predicates::prelude::*;
use serde_json::Value;

fn run_json(fixtures: &TestFixtures, args: &[&str]) -> Value {
    let mut cmd = crate::common::lotar_cmd().unwrap();
    let output = cmd
        .current_dir(fixtures.temp_dir.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).unwrap_or_else(|e| {
        panic!(
            "valid JSON expected from `lotar {}`: {e}; raw: {}",
            args.join(" "),
            String::from_utf8_lossy(&output)
        )
    })
}

fn add_task(fixtures: &TestFixtures, title: &str) {
    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(fixtures.temp_dir.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args(["add", title, "--project=test-project"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created task:"));
}

fn listed_ids(fixtures: &TestFixtures, extra: &[&str]) -> Vec<String> {
    let mut args = vec!["task", "list", "--project=test-project", "--format=json"];
    args.extend_from_slice(extra);
    let json = run_json(fixtures, &args);
    json["tasks"]
        .as_array()
        .map(|tasks| {
            tasks
                .iter()
                .map(|t| t["id"].as_str().unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Split "PREFIX-12" into ("PREFIX", 12); the CLI-supplied project name may
/// map to a normalized prefix, so derive it from the listed canonical id.
fn split_id(id: &str) -> (String, u64) {
    let (prefix, number) = id.rsplit_once('-').expect("canonical task id");
    (prefix.to_string(), number.parse().expect("numeric suffix"))
}

#[test]
fn normal_cli_reads_and_edits_reject_tombstones_without_numeric_redirect_or_side_effects() {
    let fixtures = TestFixtures::new();
    add_task(&fixtures, "Hidden primary task");
    let primary_id = listed_ids(&fixtures, &[])[0].clone();
    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(fixtures.temp_dir.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args(["add", "Visible foreign task", "--project=other-project"])
        .assert()
        .success();
    let foreign = run_json(
        &fixtures,
        &["list", "--project=other-project", "--format=json"],
    );
    let foreign_id = foreign["tasks"][0]["id"].as_str().unwrap();
    let (primary_prefix, primary_number) = split_id(&primary_id);
    let (foreign_prefix, foreign_number) = split_id(foreign_id);
    assert_eq!(primary_number, foreign_number);
    run_json(
        &fixtures,
        &["task", "delete", &primary_id, "--yes", "--format=json"],
    );
    let paths = [
        fixtures.tasks_root.join("config.yml"),
        fixtures.tasks_root.join(&primary_prefix).join("config.yml"),
        fixtures.tasks_root.join(&foreign_prefix).join("config.yml"),
        fixtures
            .tasks_root
            .join(&primary_prefix)
            .join(format!("{primary_number}.yml")),
        fixtures
            .tasks_root
            .join(&foreign_prefix)
            .join(format!("{foreign_number}.yml")),
    ];
    let before: Vec<_> = paths.iter().map(|path| std::fs::read(path).ok()).collect();
    for args in [
        vec!["priority", primary_id.as_str()],
        vec!["priority", "1", "High", "--project=test-project"],
        vec!["assignee", primary_id.as_str(), "New Member"],
    ] {
        crate::common::lotar_cmd()
            .unwrap()
            .current_dir(fixtures.temp_dir.path())
            .env("LOTAR_TEST_SILENT", "1")
            .args(args)
            .assert()
            .failure()
            .stderr(predicate::str::contains("soft-deleted"));
    }
    let after: Vec<_> = paths.iter().map(|path| std::fs::read(path).ok()).collect();
    assert_eq!(
        before, after,
        "hidden reads/edits must not change task/config files"
    );
    assert_eq!(
        listed_ids(&fixtures, &["--deleted"]),
        vec![primary_id],
        "explicit trash lookup still works"
    );
}

#[test]
fn task_delete_defaults_to_soft_delete_and_hides_from_list() {
    let fixtures = TestFixtures::new();
    add_task(&fixtures, "DEV92 soft target");

    let active = listed_ids(&fixtures, &[]);
    assert_eq!(active.len(), 1);
    let full_id = active[0].clone();
    let (prefix, number) = split_id(&full_id);
    let task_file = fixtures
        .tasks_root
        .join(&prefix)
        .join(format!("{number}.yml"));
    assert!(task_file.exists(), "task file seeded at {task_file:?}");

    // Default delete is soft: JSON reports hard=false, no warnings, and the
    // file remains on disk as a tombstone.
    let json = run_json(
        &fixtures,
        &[
            "task",
            "delete",
            "1",
            "--project=test-project",
            "--yes",
            "--format=json",
        ],
    );
    assert_eq!(json["status"], "success");
    assert_eq!(json["hard"], false);
    assert_eq!(json["warnings"].as_array().map(Vec::len), Some(0));
    assert!(
        json["message"].as_str().unwrap_or("").contains("restore"),
        "soft-delete message should point at restore: {json}"
    );
    assert!(task_file.exists(), "soft delete keeps the task file");

    // Deletion views: default hides, --deleted shows only tombstones,
    // --include-deleted shows everything.
    assert!(listed_ids(&fixtures, &[]).is_empty(), "active view hides");
    assert_eq!(
        listed_ids(&fixtures, &["--deleted"]),
        vec![full_id.clone()],
        "deleted view shows the tombstone"
    );
    assert_eq!(
        listed_ids(&fixtures, &["--include-deleted"]),
        vec![full_id],
        "all view shows the tombstone"
    );

    // The two flags are mutually exclusive.
    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(fixtures.temp_dir.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args([
            "task",
            "list",
            "--project=test-project",
            "--deleted",
            "--include-deleted",
        ])
        .assert()
        .failure();
}

#[test]
fn task_restore_roundtrip_including_dry_run() {
    let fixtures = TestFixtures::new();
    add_task(&fixtures, "DEV92 restore target");
    add_task(&fixtures, "DEV92 survivor");

    let full_id = listed_ids(&fixtures, &[])
        .into_iter()
        .find(|id| id.ends_with("-1"))
        .expect("first created task");
    run_json(
        &fixtures,
        &[
            "task",
            "delete",
            &full_id,
            "--project=test-project",
            "--yes",
            "--format=json",
        ],
    );
    assert_eq!(listed_ids(&fixtures, &["--deleted"]).len(), 1);

    // Dry-run previews the restore without mutating.
    let preview = run_json(
        &fixtures,
        &[
            "task",
            "restore",
            &full_id,
            "--project=test-project",
            "--dry-run",
            "--format=json",
        ],
    );
    assert_eq!(preview["status"], "preview");
    assert_eq!(preview["action"], "restore");
    assert!(
        preview["deleted_at"].as_str().is_some(),
        "preview should expose the tombstone timestamp: {preview}"
    );
    assert_eq!(
        listed_ids(&fixtures, &["--deleted"]).len(),
        1,
        "dry-run must not restore"
    );

    // Real restore returns the task to the active view.
    let restored = run_json(
        &fixtures,
        &[
            "task",
            "restore",
            &full_id,
            "--project=test-project",
            "--format=json",
        ],
    );
    assert_eq!(restored["status"], "success");
    assert_eq!(restored["task_id"], full_id);
    assert!(
        restored["deleted_at"].is_null(),
        "restored task must have no deleted_at: {restored}"
    );
    assert!(listed_ids(&fixtures, &[]).contains(&full_id));
    assert!(
        listed_ids(&fixtures, &["--deleted"]).is_empty(),
        "no tombstones remain after restore"
    );
}

#[test]
fn task_delete_hard_removes_file_and_reports_incoming_relationships() {
    let fixtures = TestFixtures::new();
    add_task(&fixtures, "DEV92 hard target");
    add_task(&fixtures, "DEV92 dependent");

    let ids = listed_ids(&fixtures, &[]);
    assert_eq!(ids.len(), 2);
    let target = ids
        .iter()
        .find(|id| id.ends_with("-1"))
        .expect("target task")
        .clone();
    let dependent = ids
        .iter()
        .find(|id| id.ends_with("-2"))
        .expect("dependent task")
        .clone();
    let (prefix, target_number) = split_id(&target);
    let (_, dependent_number) = split_id(&dependent);
    let target_file = fixtures
        .tasks_root
        .join(&prefix)
        .join(format!("{target_number}.yml"));
    let dependent_file = fixtures
        .tasks_root
        .join(&prefix)
        .join(format!("{dependent_number}.yml"));

    // Seed an incoming relationship: dependent depends_on target.
    let mut yaml = std::fs::read_to_string(&dependent_file).unwrap();
    assert!(!yaml.contains("relationships:"), "fresh task has none");
    yaml.push_str(&format!("relationships:\n  depends_on:\n    - {target}\n"));
    std::fs::write(&dependent_file, yaml).unwrap();

    // Hard delete warns about the incoming relationship in the stable JSON
    // warnings array (not mixed into human output).
    let json = run_json(
        &fixtures,
        &[
            "task",
            "delete",
            &target,
            "--project=test-project",
            "--yes",
            "--hard",
            "--format=json",
        ],
    );
    assert_eq!(json["status"], "success");
    assert_eq!(json["hard"], true);
    let warnings = json["warnings"].as_array().expect("warnings array");
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().is_some_and(|text| text.contains(&dependent))),
        "warnings must name the incoming relationship from {dependent}: {warnings:?}"
    );
    assert!(!target_file.exists(), "hard delete removes the task file");
    assert!(dependent_file.exists(), "dependent untouched");

    // Hard deletion is final: nothing to list under any view, and restore
    // fails cleanly.
    assert!(
        listed_ids(&fixtures, &["--include-deleted"])
            .iter()
            .all(|id| *id != target),
        "no tombstone after hard delete"
    );
    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(fixtures.temp_dir.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args([
            "task",
            "restore",
            &target,
            "--project=test-project",
            "--format=json",
        ])
        .assert()
        .failure();
}

#[test]
fn task_delete_repeat_soft_delete_is_successful_noop() {
    let fixtures = TestFixtures::new();
    add_task(&fixtures, "DEV92 repeat target");
    let full_id = listed_ids(&fixtures, &[])[0].clone();
    let (prefix, number) = split_id(&full_id);
    let task_file = fixtures
        .tasks_root
        .join(&prefix)
        .join(format!("{number}.yml"));

    run_json(
        &fixtures,
        &[
            "task",
            "delete",
            &full_id,
            "--project=test-project",
            "--yes",
            "--format=json",
        ],
    );
    assert!(task_file.exists(), "first delete is soft");
    let bytes_after_delete = std::fs::read(&task_file).unwrap();

    // Repeated soft deletion is a successful no-op: exit 0, tombstone bytes
    // (timestamp + history) untouched, and no escalation to a hard delete.
    let repeat = run_json(
        &fixtures,
        &[
            "task",
            "delete",
            &full_id,
            "--project=test-project",
            "--yes",
            "--format=json",
        ],
    );
    assert_eq!(repeat["status"], "success");
    assert_eq!(repeat["hard"], false, "repeat must never escalate to hard");
    assert!(
        task_file.exists(),
        "repeat delete must not physically remove the file"
    );
    assert_eq!(
        std::fs::read(&task_file).unwrap(),
        bytes_after_delete,
        "no-op repeat must not rewrite the tombstone"
    );

    // The tombstone is still restorable afterwards.
    let restored = run_json(
        &fixtures,
        &[
            "task",
            "restore",
            &full_id,
            "--project=test-project",
            "--format=json",
        ],
    );
    assert_eq!(restored["status"], "success");
}

#[test]
fn list_views_mark_deleted_rows_in_json_and_text() {
    let fixtures = TestFixtures::new();
    add_task(&fixtures, "marker active task");
    add_task(&fixtures, "marker deleted task");
    let ids = listed_ids(&fixtures, &[]);
    let deleted_id = ids
        .iter()
        .find(|id| id.ends_with("-2"))
        .expect("second task")
        .clone();
    run_json(
        &fixtures,
        &[
            "task",
            "delete",
            &deleted_id,
            "--project=test-project",
            "--yes",
            "--format=json",
        ],
    );

    // Mixed JSON view: tombstone carries deleted_at, active row omits the
    // field entirely, and both keep their stored titles.
    let mixed = run_json(
        &fixtures,
        &[
            "task",
            "list",
            "--project=test-project",
            "--include-deleted",
            "--format=json",
        ],
    );
    let tasks = mixed["tasks"].as_array().expect("tasks array");
    let deleted_row = tasks
        .iter()
        .find(|t| t["id"].as_str() == Some(deleted_id.as_str()))
        .expect("tombstone listed");
    assert!(
        deleted_row["deleted_at"].as_str().is_some(),
        "tombstone row must expose deleted_at: {deleted_row}"
    );
    assert!(
        deleted_row["title"]
            .as_str()
            .is_some_and(|t| t == "marker deleted task"),
        "stored title unchanged"
    );
    let active_row = tasks
        .iter()
        .find(|t| t["id"].as_str() != Some(deleted_id.as_str()))
        .expect("active row listed");
    assert!(
        active_row.get("deleted_at").is_none(),
        "active row must omit deleted_at: {active_row}"
    );

    // Active-only JSON never carries the field.
    let active_json = run_json(
        &fixtures,
        &["task", "list", "--project=test-project", "--format=json"],
    );
    for row in active_json["tasks"].as_array().expect("active tasks") {
        assert!(row.get("deleted_at").is_none());
    }

    // Mixed human output marks the deleted row with its timestamp without
    // rewriting the title.
    let mut cmd = crate::common::lotar_cmd().unwrap();
    let text = cmd
        .current_dir(fixtures.temp_dir.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args([
            "task",
            "list",
            "--project=test-project",
            "--include-deleted",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8_lossy(&text);
    assert!(
        text.contains("marker deleted task"),
        "title visible: {text}"
    );
    assert!(text.contains("marker active task"));
    assert!(
        text.contains("deleted: "),
        "deleted row must carry a deleted marker with timestamp: {text}"
    );
}

#[test]
fn restore_dry_run_fails_closed_when_config_rejects_stored_values() {
    let fixtures = TestFixtures::new();
    add_task(&fixtures, "DEV92 invalid tombstone");
    let full_id = listed_ids(&fixtures, &[])[0].clone();
    let (prefix, number) = split_id(&full_id);
    let task_file = fixtures
        .tasks_root
        .join(&prefix)
        .join(format!("{number}.yml"));

    run_json(
        &fixtures,
        &[
            "task",
            "delete",
            &full_id,
            "--project=test-project",
            "--yes",
            "--format=json",
        ],
    );

    // Restrict the project's issue states AFTER the tombstone exists: the
    // stored "Todo" is now invalid under the current config, so the real
    // restore would be rejected -- the dry run must fail closed too,
    // without touching the tombstone.
    std::fs::write(
        fixtures.tasks_root.join("config.yml"),
        b"default:\n  project: test-project\nissue:\n  states: [Open, Closed]\n",
    )
    .unwrap();
    let bytes_before = std::fs::read(&task_file).unwrap();

    let mut cmd = crate::common::lotar_cmd().unwrap();
    let stderr = cmd
        .current_dir(fixtures.temp_dir.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args([
            "task",
            "restore",
            &full_id,
            "--project=test-project",
            "--dry-run",
            "--format=json",
        ])
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8_lossy(&stderr);
    assert!(
        stderr.to_lowercase().contains("status"),
        "failure should explain the config-invalid stored status: {stderr}"
    );
    assert_eq!(
        std::fs::read(&task_file).unwrap(),
        bytes_before,
        "dry run must not rewrite the tombstone"
    );

    // The real restore is blocked by the same gate, proving the preview
    // was honest rather than pessimistic.
    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(fixtures.temp_dir.path())
        .env("LOTAR_TEST_SILENT", "1")
        .args([
            "task",
            "restore",
            &full_id,
            "--project=test-project",
            "--format=json",
        ])
        .assert()
        .failure();
    assert!(task_file.exists(), "tombstone survives the refused restore");
}

#[test]
fn task_delete_dry_run_previews_without_mutating() {
    let fixtures = TestFixtures::new();
    add_task(&fixtures, "DEV92 dry run target");
    let full_id = listed_ids(&fixtures, &[])[0].clone();
    let (prefix, number) = split_id(&full_id);
    let task_file = fixtures
        .tasks_root
        .join(&prefix)
        .join(format!("{number}.yml"));

    let soft = run_json(
        &fixtures,
        &[
            "task",
            "delete",
            &full_id,
            "--project=test-project",
            "--dry-run",
            "--format=json",
        ],
    );
    assert_eq!(soft["status"], "preview");
    assert_eq!(soft["mode"], "soft");
    assert!(
        soft["warnings"].as_array().is_some(),
        "preview JSON must carry a warnings array: {soft}"
    );
    assert!(task_file.exists(), "dry-run must not delete");
    assert_eq!(listed_ids(&fixtures, &[]).len(), 1, "still active");

    let hard = run_json(
        &fixtures,
        &[
            "task",
            "delete",
            &full_id,
            "--project=test-project",
            "--dry-run",
            "--hard",
            "--format=json",
        ],
    );
    assert_eq!(hard["status"], "preview");
    assert_eq!(hard["mode"], "hard");
    assert!(
        hard["warnings"].as_array().is_some(),
        "hard preview must carry a warnings array: {hard}"
    );
    assert!(task_file.exists(), "hard dry-run must not delete");
}
