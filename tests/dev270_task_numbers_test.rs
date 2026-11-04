//! DEV-270: hard delete leaves no sprint memberships for a reused number,
//! nested workspaces sharing a prefix don't allocate one ID twice, and a
//! number stored twice can be renumbered.
#![cfg(unix)]

mod common;

use common::TestFixtures;

fn lotar_in(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    common::lotar_cmd()
        .unwrap()
        .current_dir(dir)
        .env("LOTAR_DEFAULT_REPORTER", "tester")
        .args(args)
        .output()
        .unwrap()
}

fn ok(dir: &std::path::Path, args: &[&str]) -> String {
    let out = lotar_in(dir, args);
    assert!(
        out.status.success(),
        "{args:?}: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn hard_delete_drops_sprint_memberships_and_warns_about_reuse() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path();
    for title in ["a", "b", "c"] {
        ok(root, &["add", title, "-p", "HD"]);
    }
    ok(root, &["sprint", "create", "--label", "One"]);
    ok(root, &["sprint", "add", "--sprint", "1", "HD-3"]);
    let out = ok(root, &["task", "delete", "HD-3", "--hard", "--yes"]);
    assert!(out.contains("removed from sprint(s) #1"), "{out}");
    assert!(out.contains("gets this ID again"), "{out}");
    let sprint = std::fs::read_to_string(root.join(".tasks/@sprints/1.yml")).unwrap();
    assert!(!sprint.contains("HD-3"), "{sprint}");

    ok(root, &["add", "brand new", "-p", "HD"]);
    let shown = ok(root, &["show", "HD-3", "--format", "json"]);
    let value: serde_json::Value = serde_json::from_str(shown.trim()).unwrap();
    let sprints = value["task"]["sprints"]
        .as_array()
        .or_else(|| value["sprints"].as_array())
        .cloned()
        .unwrap_or_default();
    assert!(sprints.is_empty(), "{shown}");
}

#[test]
fn nested_workspaces_sharing_a_prefix_get_distinct_ids() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path();
    ok(root, &["add", "root task", "-p", "DEV"]);
    let pkg = root.join("pkg");
    std::fs::create_dir_all(pkg.join(".tasks")).unwrap();
    let out = ok(
        &pkg,
        &["--tasks-dir", ".tasks", "add", "pkg task", "-p", "DEV"],
    );
    assert!(out.contains("DEV-2"), "{out}");
    let out = ok(root, &["add", "root again", "-p", "DEV"]);
    assert!(out.contains("DEV-3"), "{out}");
    // From the root, every ID resolves to exactly one task.
    for id in ["DEV-1", "DEV-2", "DEV-3"] {
        ok(root, &["show", id]);
    }
}

#[test]
fn renumber_resolves_a_number_stored_twice() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path();
    ok(root, &["add", "first", "-p", "DUP"]);
    ok(root, &["add", "second", "-p", "DUP"]);
    let copy = root.join(".tasks/DUP/merged/1.yml");
    std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
    let original = std::fs::read_to_string(root.join(".tasks/DUP/1.yml")).unwrap();
    std::fs::write(
        &copy,
        original.replace("title: first", "title: their first"),
    )
    .unwrap();

    // A single copy is not renumbered; a duplicate needs --file.
    let single = lotar_in(root, &["task", "renumber", "DUP-2"]);
    assert!(!single.status.success());
    assert!(String::from_utf8_lossy(&single.stderr).contains("one file"));
    let unnamed = lotar_in(root, &["task", "renumber", "DUP-1"]);
    assert!(!unnamed.status.success());
    assert!(String::from_utf8_lossy(&unnamed.stderr).contains("--file"));

    let preview = ok(
        root,
        &[
            "task",
            "renumber",
            "DUP-1",
            "--file",
            ".tasks/DUP/merged/1.yml",
            "-n",
        ],
    );
    assert!(preview.contains("DUP-3"), "{preview}");
    assert!(copy.exists());

    ok(
        root,
        &[
            "task",
            "renumber",
            "DUP-1",
            "--file",
            ".tasks/DUP/merged/1.yml",
        ],
    );
    assert!(!copy.exists());
    let renumbered = std::fs::read_to_string(root.join(".tasks/DUP/merged/3.yml")).unwrap();
    assert!(renumbered.contains("their first"), "{renumbered}");
    assert!(renumbered.contains("renumbered"), "{renumbered}");

    let shown = ok(root, &["show", "DUP-1"]);
    assert!(
        shown.contains("first") && !shown.contains("their first"),
        "{shown}"
    );
    ok(root, &["status", "DUP-1", "InProgress"]);
    let shown = ok(root, &["show", "DUP-3"]);
    assert!(shown.contains("their first"), "{shown}");
}
