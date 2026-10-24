use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Output;

mod common;

fn run(root: &Path, args: &[&str]) -> Output {
    common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .env("LOTAR_TEST_MODE", "1")
        .env_remove("LOTAR_TASKS_DIR")
        .args(args)
        .output()
        .unwrap()
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    if root.is_dir() {
        let mut entries: Vec<_> = std::fs::read_dir(root)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                files.insert(path.clone(), Vec::new());
                files.extend(snapshot(&path));
            } else {
                files.insert(path.clone(), std::fs::read(path).unwrap());
            }
        }
    }
    files
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{error}: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn create(root: &Path, title: &str) -> String {
    let output = run(root, &["--project", "CLI", "task", "add", title]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| {
            line.split_once("Created task: ")
                .map(|(_, id)| id.trim().to_string())
        })
        .expect("created task id")
}

#[test]
fn dev151_cli_graph_projection_flags_are_optional_and_strict() {
    let sandbox = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let workspace = tempfile::tempdir_in(sandbox.path()).unwrap();
    let id = create(workspace.path(), "Isolated");
    let graph = run(
        workspace.path(),
        &[
            "--format",
            "json",
            "task",
            "relationships",
            "graph",
            "--relationship-kind",
            "dependency",
            "--include-isolated",
            "false",
        ],
    );
    assert!(
        graph.status.success(),
        "{}",
        String::from_utf8_lossy(&graph.stderr)
    );
    assert!(json(&graph)["nodes"].as_array().unwrap().is_empty());
    let focused = run(
        workspace.path(),
        &[
            "--format",
            "json",
            "task",
            "relationships",
            "graph",
            "--relationship-kind",
            "dependency",
            "--include-isolated",
            "false",
            "--root",
            &id,
        ],
    );
    assert!(focused.status.success());
    assert_eq!(json(&focused)["nodes"].as_array().unwrap().len(), 1);
    let legacy = run(
        workspace.path(),
        &["--format", "json", "task", "relationships", "graph"],
    );
    assert!(legacy.status.success());
    assert_eq!(json(&legacy)["nodes"].as_array().unwrap().len(), 1);
    let bad = run(
        workspace.path(),
        &[
            "task",
            "relationships",
            "graph",
            "--relationship-kind",
            "blocks",
        ],
    );
    assert!(!bad.status.success());
    let bad_id = run(
        workspace.path(),
        &["task", "relationships", "repair", "--issue-id", "unknown"],
    );
    assert!(!bad_id.status.success());
    assert!(String::from_utf8_lossy(&bad_id.stderr).contains("Unknown"));
}

#[test]
fn explicit_missing_tasks_directory_is_not_created_by_check() {
    let sandbox = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let workspace = tempfile::tempdir_in(sandbox.path()).unwrap();
    let missing = workspace.path().join("absent");
    let output = run(
        workspace.path(),
        &[
            "--tasks-dir",
            missing.to_str().unwrap(),
            "task",
            "relationships",
            "check",
        ],
    );
    assert!(!output.status.success());
    assert!(!missing.exists());
}

#[test]
fn check_is_read_only_and_fails_closed_for_missing_and_corrupt_data() {
    let sandbox = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let workspace = tempfile::tempdir_in(sandbox.path()).unwrap();
    let missing = run(workspace.path(), &["task", "relationships", "check"]);
    assert!(!missing.status.success());
    assert!(snapshot(workspace.path()).is_empty());
    let id = create(workspace.path(), "Check fixture");
    let before = snapshot(workspace.path());
    let clean = run(
        workspace.path(),
        &["--format", "json", "task", "relationships", "check"],
    );
    assert!(
        clean.status.success(),
        "{}",
        String::from_utf8_lossy(&clean.stderr)
    );
    assert_eq!(json(&clean)["complete"], true);
    assert_eq!(snapshot(workspace.path()), before);
    let missing_root = run(
        workspace.path(),
        &[
            "--format",
            "json",
            "task",
            "relationships",
            "graph",
            "--root",
            "CLI-999999",
        ],
    );
    assert!(!missing_root.status.success());
    assert_eq!(snapshot(workspace.path()), before);

    let mut storage = lotar::Storage::try_open(&workspace.path().join(".tasks")).unwrap();
    let mut task = storage.get(&id, "CLI").unwrap();
    task.relationships.depends_on = vec![id.clone()];
    storage.edit(&id, &task).unwrap();
    drop(storage);
    let before = snapshot(workspace.path());
    let blocking = run(
        workspace.path(),
        &["--format", "json", "task", "relationships", "check"],
    );
    assert!(!blocking.status.success());
    let report = json(&blocking);
    assert_eq!(report["complete"], true);
    assert!(
        report["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue["code"] == "self_link")
    );
    assert_eq!(snapshot(workspace.path()), before);

    let path = workspace
        .path()
        .join(".tasks/CLI")
        .join(format!("{}.yml", id.rsplit('-').next().unwrap()));
    std::fs::write(path, "title: [broken").unwrap();
    let before = snapshot(workspace.path());
    let failing = run(
        workspace.path(),
        &["--format", "json", "task", "relationships", "check"],
    );
    assert!(!failing.status.success());
    let report = json(&failing);
    assert_eq!(report["complete"], false);
    assert!(
        report["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue["code"] == "unreadable")
    );
    assert_eq!(snapshot(workspace.path()), before);
}

#[test]
fn repair_preview_revision_guard_and_relationship_field_conversion() {
    let sandbox = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let workspace = tempfile::tempdir_in(sandbox.path()).unwrap();
    let source = create(workspace.path(), "Prerequisite");
    let target = create(workspace.path(), "Dependent");
    let tasks_root = workspace.path().join(".tasks");
    let mut storage = lotar::Storage::try_open(&tasks_root).unwrap();
    let mut task = storage.get(&source, "CLI").unwrap();
    task.relationships.blocks = vec![target.clone()];
    storage.edit(&source, &task).unwrap();
    drop(storage);

    let before = snapshot(workspace.path());
    let preview = run(
        workspace.path(),
        &["--format", "json", "task", "relationships", "repair"],
    );
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let report = json(&preview);
    assert_eq!(report["applied"], false);
    let revision = report["revision"].as_str().unwrap();
    assert_eq!(snapshot(workspace.path()), before);
    create(workspace.path(), "Invalidate preview");
    let before = snapshot(workspace.path());
    let stale = run(
        workspace.path(),
        &[
            "task",
            "relationships",
            "repair",
            "--apply",
            "--expected-revision",
            revision,
        ],
    );
    assert!(!stale.status.success());
    assert_eq!(snapshot(workspace.path()), before);
    let preview = run(
        workspace.path(),
        &[
            "--format",
            "json",
            "task",
            "relationships",
            "repair",
            "--dry-run",
        ],
    );
    assert!(preview.status.success());
    let fresh = json(&preview);
    let revision = fresh["revision"].as_str().unwrap();
    assert_ne!(revision, report["revision"].as_str().unwrap());
    assert_eq!(snapshot(workspace.path()), before);
    let applied = run(
        workspace.path(),
        &[
            "--format",
            "json",
            "task",
            "relationships",
            "repair",
            "--apply",
            "--expected-revision",
            revision,
        ],
    );
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    assert_eq!(json(&applied)["applied"], true);

    let legacy = run(
        workspace.path(),
        &[
            "--format",
            "json",
            "task",
            "relationships",
            &target,
            "--kind",
            "depends-on",
        ],
    );
    assert!(legacy.status.success());
    let listed = json(&legacy);
    assert_eq!(listed["status"], "success");
    assert_eq!(
        listed["relationships"]["depends_on"],
        serde_json::json!([source])
    );
    assert_eq!(
        listed["filters"]["kinds"],
        serde_json::json!(["depends-on"])
    );

    let moved = run(
        workspace.path(),
        &[
            "task",
            "relationships",
            "move",
            &source,
            &target,
            "--from",
            "blocks",
            "--to",
            "related",
        ],
    );
    assert!(
        moved.status.success(),
        "{}",
        String::from_utf8_lossy(&moved.stderr)
    );
    let before = snapshot(workspace.path());
    let graph = run(
        workspace.path(),
        &[
            "--format",
            "json",
            "task",
            "relationships",
            "graph",
            "--root",
            &source,
        ],
    );
    assert!(
        graph.status.success(),
        "{}",
        String::from_utf8_lossy(&graph.stderr)
    );
    let graph = json(&graph);
    assert_eq!(graph["edges"].as_array().unwrap().len(), 1);
    assert_eq!(graph["edges"][0]["kind"], "related");
    let bounded = run(
        workspace.path(),
        &[
            "--format",
            "json",
            "task",
            "relationships",
            "graph",
            "--root",
            &source,
            "--max-nodes",
            "1",
        ],
    );
    assert!(!bounded.status.success());
    assert_eq!(json(&bounded)["truncated"], true);
    assert_eq!(snapshot(workspace.path()), before);
}

#[test]
fn readonly_relationship_commands_refuse_pending_recovery_without_writes() {
    let sandbox = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let workspace = tempfile::tempdir_in(sandbox.path()).unwrap();
    create(workspace.path(), "Recovery fixture");
    let journal = workspace.path().join(".tasks/.txn-pending.json");
    std::fs::write(journal, "invalid journal must remain untouched").unwrap();
    let before = snapshot(workspace.path());
    for command in ["check", "graph", "repair"] {
        let output = run(workspace.path(), &["task", "relationships", command]);
        assert!(!output.status.success(), "{command}");
        assert_eq!(snapshot(workspace.path()), before, "{command} wrote files");
    }
}

#[test]
fn numeric_mutations_preserve_reciprocals_and_guard_occupied_parent() {
    let sandbox = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let workspace = tempfile::tempdir_in(sandbox.path()).unwrap();
    let first = create(workspace.path(), "First parent");
    let child = create(workspace.path(), "Child");
    let second = create(workspace.path(), "Second parent");
    let first_number = first.rsplit('-').next().unwrap();
    let child_number = child.rsplit('-').next().unwrap();
    let added = run(
        workspace.path(),
        &[
            "--project",
            "CLI",
            "task",
            "relationships",
            "add",
            child_number,
            first_number,
            "--kind",
            "parent",
        ],
    );
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let before = snapshot(workspace.path());
    let unguarded = run(
        workspace.path(),
        &[
            "task",
            "relationships",
            "add",
            &child,
            &second,
            "--kind",
            "parent",
        ],
    );
    assert!(!unguarded.status.success());
    assert_eq!(snapshot(workspace.path()), before);
    let guarded = run(
        workspace.path(),
        &[
            "task",
            "relationships",
            "add",
            &child,
            &second,
            "--kind",
            "parent",
            "--expected-current",
            &first,
        ],
    );
    assert!(
        guarded.status.success(),
        "{}",
        String::from_utf8_lossy(&guarded.stderr)
    );
    let storage = lotar::Storage::try_open(&workspace.path().join(".tasks")).unwrap();
    assert!(
        storage
            .get(&first, "CLI")
            .unwrap()
            .relationships
            .children
            .is_empty()
    );
    assert_eq!(
        storage.get(&second, "CLI").unwrap().relationships.children,
        vec![child.clone()]
    );
    assert_eq!(
        storage.get(&child, "CLI").unwrap().relationships.parent,
        Some(second.clone())
    );
    drop(storage);
    let removed = run(
        workspace.path(),
        &[
            "task",
            "relationships",
            "remove",
            &child,
            &second,
            "--kind",
            "parent",
        ],
    );
    assert!(
        removed.status.success(),
        "{}",
        String::from_utf8_lossy(&removed.stderr)
    );
    let storage = lotar::Storage::try_open(&workspace.path().join(".tasks")).unwrap();
    assert!(
        storage
            .get(&second, "CLI")
            .unwrap()
            .relationships
            .children
            .is_empty()
    );
    assert_eq!(
        storage.get(&child, "CLI").unwrap().relationships.parent,
        None
    );
}

#[test]
fn move_depends_on_to_blocks_reverses_both_assertions() {
    let sandbox = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let workspace = tempfile::tempdir_in(sandbox.path()).unwrap();
    let source = create(workspace.path(), "Focused source");
    let target = create(workspace.path(), "Linked target");
    let added = run(
        workspace.path(),
        &[
            "task",
            "relationships",
            "add",
            &source,
            &target,
            "--kind",
            "depends-on",
        ],
    );
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let storage = lotar::Storage::try_open(&workspace.path().join(".tasks")).unwrap();
    assert_eq!(
        storage
            .get(&source, "CLI")
            .unwrap()
            .relationships
            .depends_on,
        vec![target.clone()]
    );
    assert_eq!(
        storage.get(&target, "CLI").unwrap().relationships.blocks,
        vec![source.clone()]
    );
    drop(storage);
    let moved = run(
        workspace.path(),
        &[
            "task",
            "relationships",
            "move",
            &source,
            &target,
            "--from",
            "depends-on",
            "--to",
            "blocks",
        ],
    );
    assert!(
        moved.status.success(),
        "{}",
        String::from_utf8_lossy(&moved.stderr)
    );
    let storage = lotar::Storage::try_open(&workspace.path().join(".tasks")).unwrap();
    let source_task = storage.get(&source, "CLI").unwrap();
    let target_task = storage.get(&target, "CLI").unwrap();
    assert!(source_task.relationships.depends_on.is_empty());
    assert!(target_task.relationships.blocks.is_empty());
    assert_eq!(source_task.relationships.blocks, vec![target.clone()]);
    assert_eq!(target_task.relationships.depends_on, vec![source.clone()]);
}

#[test]
fn nested_relationship_parser_enforces_safe_command_shapes() {
    use clap::Parser;
    let sandbox = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let workspace = tempfile::tempdir_in(sandbox.path()).unwrap();
    for kind in [
        "depends-on",
        "blocks",
        "related",
        "parent",
        "children",
        "fixes",
        "duplicate-of",
    ] {
        assert!(
            lotar::cli::Cli::try_parse_from([
                "lotar",
                "task",
                "relationships",
                "add",
                "CLI-1",
                "CLI-2",
                "--kind",
                kind,
            ])
            .is_ok(),
            "{kind}"
        );
    }
    for args in [
        vec!["task", "relationships"],
        vec!["task", "relationships", "repair", "--apply"],
        vec![
            "task",
            "relationships",
            "repair",
            "--expected-revision",
            "x",
        ],
        vec![
            "task",
            "relationships",
            "repair",
            "--apply",
            "--dry-run",
            "--expected-revision",
            "x",
        ],
        vec!["task", "relationships", "DEV-1", "check"],
        vec!["task", "relationships", "--kind", "parent", "check"],
        vec![
            "task",
            "relationships",
            "move",
            "DEV-1",
            "DEV-2",
            "--to",
            "related",
        ],
        vec!["task", "relationships", "graph", "--max-nodes", "0"],
    ] {
        let output = run(workspace.path(), &args);
        assert!(!output.status.success(), "args={args:?}");
    }
    for command in ["check", "repair", "graph", "add", "remove", "move"] {
        use clap::CommandFactory;
        let mut cli = lotar::cli::Cli::command();
        let nested = cli
            .find_subcommand_mut("task")
            .unwrap()
            .find_subcommand_mut("relationships")
            .unwrap()
            .find_subcommand_mut(command)
            .expect("dedicated nested command");
        assert!(!nested.render_long_help().to_string().is_empty());
    }
    assert!(snapshot(workspace.path()).is_empty());
}
