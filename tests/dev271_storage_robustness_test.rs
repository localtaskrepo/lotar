//! DEV-271: runtime files are git-ignored, attachment blobs are published
//! atomically and repaired when truncated, hand-written tasks without
//! `created` are readable, and removed statuses can still be filtered.
#![cfg(unix)]

use crate::common;
use common::TestFixtures;
use lotar::services::attachment_service::AttachmentService;

fn run(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    common::lotar_cmd()
        .unwrap()
        .current_dir(dir)
        .env("LOTAR_DEFAULT_REPORTER", "tester")
        .args(args)
        .output()
        .unwrap()
}

fn ok(dir: &std::path::Path, args: &[&str]) -> String {
    let out = run(dir, args);
    assert!(
        out.status.success(),
        "{args:?}: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn lock_files_stay_out_of_git_status() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path();
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .current_dir(root)
            .env_remove("GIT_DIR")
            .args(args)
            .output()
            .unwrap()
    };
    git(&["init", "-q"]);
    ok(root, &["add", "t", "-p", "WEB"]);
    ok(root, &["sprint", "create", "--label", "One"]);
    let status =
        String::from_utf8_lossy(&git(&["status", "--porcelain", "-uall"]).stdout).into_owned();
    assert!(!status.contains(".lock"), "{status}");
    assert!(status.contains(".tasks/WEB/1.yml"), "{status}");
    assert!(status.contains(".tasks/.gitignore"), "{status}");

    // An existing file is left as the workspace wrote it.
    std::fs::write(root.join(".tasks/.gitignore"), "# mine\n").unwrap();
    ok(root, &["add", "u", "-p", "WEB"]);
    assert_eq!(
        std::fs::read_to_string(root.join(".tasks/.gitignore")).unwrap(),
        "# mine\n"
    );
}

#[test]
fn attachment_store_repairs_a_truncated_blob() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = b"complete attachment content".to_vec();
    let (name, created) = AttachmentService::store_bytes(dir.path(), "a.txt", &bytes).unwrap();
    assert!(created);
    assert_eq!(std::fs::read(dir.path().join(&name)).unwrap(), bytes);
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
        .collect();
    assert!(leftovers.is_empty());

    // Simulate a write cut short: the next upload of the same bytes fixes it.
    std::fs::write(dir.path().join(&name), b"compl").unwrap();
    let (again, created) = AttachmentService::store_bytes(dir.path(), "a.txt", &bytes).unwrap();
    assert_eq!(again, name);
    assert!(!created);
    assert_eq!(std::fs::read(dir.path().join(&name)).unwrap(), bytes);
}

#[test]
fn hand_written_task_without_created_is_readable() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path();
    ok(root, &["add", "first", "-p", "DEMO"]);
    std::fs::write(
        root.join(".tasks/DEMO/7.yml"),
        "title: Minimal hand-written\nstatus: todo\n",
    )
    .unwrap();
    let listed = run(root, &["list"]);
    assert!(String::from_utf8_lossy(&listed.stdout).contains("DEMO-7"));
    assert!(!String::from_utf8_lossy(&listed.stderr).contains("Skipping"));
    assert!(ok(root, &["show", "DEMO-7"]).contains("Minimal hand-written"));
    ok(root, &["comment", "DEMO-7", "-m", "seen"]);
    let stored = std::fs::read_to_string(root.join(".tasks/DEMO/7.yml")).unwrap();
    assert!(stored.contains("created:"), "{stored}");
    assert!(!stored.contains("1970"), "{stored}");
}

#[test]
fn removed_status_can_still_be_filtered() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path();
    ok(root, &["add", "a", "-p", "ALPH"]);
    ok(root, &["add", "b", "-p", "ALPH"]);
    ok(root, &["status", "ALPH-1", "InProgress"]);
    std::fs::write(
        root.join(".tasks/ALPH/config.yml"),
        "project:\n  name: ALPH\nissue:\n  states: [Open, Closed]\n",
    )
    .unwrap();
    let out = ok(root, &["list", "status:InProgress", "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    let ids: Vec<_> = value["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids, ["ALPH-1"]);
    // A status no task holds is still a typo.
    let typo = run(root, &["list", "status:InProgres"]);
    assert!(!typo.status.success());
}
