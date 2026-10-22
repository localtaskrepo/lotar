use std::fs;

mod common;
use common::TestFixtures;

fn project_and_task_paths(root: &std::path::Path) -> (String, std::path::PathBuf) {
    let tasks_dir = root.join(".tasks");
    let mut projects = std::fs::read_dir(&tasks_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().to_string())
        // Internal reserved folders (@sprints, created by the coordinated
        // creation transaction) are not project directories.
        .filter(|name| !name.starts_with('@') && !name.starts_with('.'))
        .collect::<Vec<_>>();
    projects.sort();
    assert!(
        !projects.is_empty(),
        "expected a project folder under .tasks"
    );
    let project = projects[0].clone();
    let task_file = tasks_dir.join(&project).join("1.yml");
    (project, task_file)
}

#[test]
fn scan_does_not_add_reference_when_mentions_disabled() {
    let tf = TestFixtures::new();
    let root = tf.temp_dir.path();

    // Initialize config via CLI to ensure tasks root exists
    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .args(["config", "show"])
        .assert()
        .success();
    // Disable mentions at project level: we need the actual detected project prefix directory
    // Resolve it by creating a project via CLI add first (creates .tasks/<PREFIX>)
    let mut add_tmp = crate::common::lotar_cmd().unwrap();
    add_tmp
        .current_dir(root)
        .arg("add")
        .arg("bootstrap")
        .assert()
        .success();
    let (project, task_file_tmp) = project_and_task_paths(root);
    // Now write project-level config
    let proj_dir = root.join(".tasks").join(&project);
    fs::write(proj_dir.join("config.yml"), "scan_enable_mentions: false\n").unwrap();
    // cleanup bootstrap task file to avoid interference
    fs::remove_file(task_file_tmp).unwrap();

    // Create a task
    let mut add = crate::common::lotar_cmd().unwrap();
    add.current_dir(root)
        .arg("add")
        .arg("Task")
        .assert()
        .success();
    let (_project, task_file) = project_and_task_paths(root);
    assert!(task_file.exists());

    // Create file with existing key
    let id = format!("{_project}-1");
    let src = root.join("main.rs");
    fs::write(&src, format!("// TODO {id}: mention\n")).unwrap();

    // Run scan; mentions disabled should not add references section
    let mut scan = crate::common::lotar_cmd().unwrap();
    scan.current_dir(root).arg("scan").assert().success();
    let yaml = fs::read_to_string(&task_file).unwrap();
    assert!(
        !yaml.contains("references:"),
        "mentions disabled should not add references; got: {yaml}"
    );
}

fn count_task_files(project_dir: &std::path::Path) -> usize {
    std::fs::read_dir(project_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.ends_with(".yml") && name.chars().next().is_some_and(|c| c.is_ascii_digit())
        })
        .count()
}

#[test]
fn scan_tracks_bare_known_ticket_mentions_without_signal_words() {
    let tf = TestFixtures::new();
    let root = tf.temp_dir.path();

    let mut add = crate::common::lotar_cmd().unwrap();
    add.current_dir(root)
        .arg("add")
        .arg("Mentioned task")
        .assert()
        .success();
    let (project, task_file) = project_and_task_paths(root);
    let id = format!("{project}-1");
    assert!(task_file.exists());

    // Bare known-ID mention: no TODO/signal word anywhere on the line.
    let src = root.join("main.rs");
    let source_text = format!("// {id} introduced this behavior\n");
    fs::write(&src, &source_text).unwrap();

    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .arg("scan")
        .assert()
        .success();

    let yaml = fs::read_to_string(&task_file).unwrap().replace('\\', "/");
    assert!(
        yaml.contains("code: main.rs#1"),
        "bare mention should anchor the known task: {yaml}"
    );

    // Mention tracking never rewrites source text.
    assert_eq!(fs::read_to_string(&src).unwrap(), source_text);

    // The bare mention must not create an additional task.
    let project_dir = root.join(".tasks").join(&project);
    assert_eq!(count_task_files(&project_dir), 1);
}

#[test]
fn scan_bare_mentions_never_create_unknown_tasks() {
    let tf = TestFixtures::new();
    let root = tf.temp_dir.path();

    // Bootstrap a project (and its tasks root), then remove the bootstrap task.
    let mut add = crate::common::lotar_cmd().unwrap();
    add.current_dir(root)
        .arg("add")
        .arg("bootstrap")
        .assert()
        .success();
    let (project, bootstrap_task) = project_and_task_paths(root);
    fs::remove_file(bootstrap_task).unwrap();

    // Unknown/external ticket key on a comment line without signal words.
    let src = root.join("main.rs");
    let source_text = "// OTHER-99 external ticket mention\n".to_string();
    fs::write(&src, &source_text).unwrap();

    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .arg("scan")
        .assert()
        .success();

    assert!(!root.join(".tasks").join("OTHER").exists());
    let project_dir = root.join(".tasks").join(&project);
    assert_eq!(count_task_files(&project_dir), 0);
    assert_eq!(fs::read_to_string(&src).unwrap(), source_text);
}

#[test]
fn scan_does_not_track_bare_mentions_when_mentions_disabled() {
    let tf = TestFixtures::new();
    let root = tf.temp_dir.path();

    let mut add = crate::common::lotar_cmd().unwrap();
    add.current_dir(root)
        .arg("add")
        .arg("Mentioned task")
        .assert()
        .success();
    let (project, task_file) = project_and_task_paths(root);
    let id = format!("{project}-1");

    let proj_dir = root.join(".tasks").join(&project);
    fs::write(proj_dir.join("config.yml"), "scan_enable_mentions: false\n").unwrap();

    let src = root.join("main.rs");
    fs::write(&src, format!("// {id} bare mention while disabled\n")).unwrap();

    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .arg("scan")
        .assert()
        .success();

    let yaml = fs::read_to_string(&task_file).unwrap();
    assert!(
        !yaml.contains("references:"),
        "bare mentions must not anchor while scan_enable_mentions is false; got: {yaml}"
    );
}

#[test]
fn scan_bare_mentions_ignore_keys_inside_quoted_strings() {
    let tf = TestFixtures::new();
    let root = tf.temp_dir.path();

    let mut add = crate::common::lotar_cmd().unwrap();
    add.current_dir(root)
        .arg("add")
        .arg("Mentioned task")
        .assert()
        .success();
    let (project, task_file) = project_and_task_paths(root);
    let id = format!("{project}-1");

    let src = root.join("main.rs");
    let source_text =
        format!("let quoted = \"{id} inside a string\";\n// {id} real comment mention\n");
    fs::write(&src, &source_text).unwrap();

    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .arg("scan")
        .assert()
        .success();

    let yaml = fs::read_to_string(&task_file).unwrap().replace('\\', "/");
    assert!(
        yaml.contains("code: main.rs#2"),
        "comment mention should anchor line 2: {yaml}"
    );
    assert!(
        !yaml.contains("code: main.rs#1"),
        "quoted string must not be treated as a mention: {yaml}"
    );
    assert_eq!(fs::read_to_string(&src).unwrap(), source_text);
}

#[test]
fn mixed_signal_and_bare_mentions_preserve_anchors_without_rewriting() {
    let fixture = TestFixtures::new();
    common::cargo_bin_in(&fixture)
        .args(["add", "Mentioned task"])
        .assert()
        .success();
    let (project, task_file) = project_and_task_paths(fixture.get_temp_path());
    let id = format!("{project}-1");
    let source = fixture.get_temp_path().join("main.rs");
    let text = format!("// TODO ({id}): signal\n// {id} bare mention\n");
    fs::write(&source, &text).unwrap();
    common::cargo_bin_in(&fixture)
        .arg("scan")
        .assert()
        .success();
    let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(946_684_800);
    for path in [&task_file, &source] {
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(old)
            .unwrap();
    }
    let before = fs::read(&task_file).unwrap();
    for args in [vec!["scan"], vec!["scan", "--reanchor"], vec!["scan"]] {
        common::cargo_bin_in(&fixture).args(args).assert().success();
        let task: lotar::storage::task::Task =
            serde_yaml_ng::from_slice(&fs::read(&task_file).unwrap()).unwrap();
        assert_eq!(task.references.len(), 2, "unchanged scan grew anchors");
        assert_eq!(fs::read(&task_file).unwrap(), before);
        assert_eq!(fs::metadata(&task_file).unwrap().modified().unwrap(), old);
        assert_eq!(fs::read_to_string(&source).unwrap(), text);
        assert_eq!(fs::metadata(&source).unwrap().modified().unwrap(), old);
    }
    let moved = format!("// TODO ({id}): signal\n\nlet fake = \"({id})\";\n// {id} bare mention\n");
    fs::write(&source, &moved).unwrap();
    common::cargo_bin_in(&fixture)
        .args(["scan", "--reanchor"])
        .assert()
        .success();
    let task: lotar::storage::task::Task =
        serde_yaml_ng::from_slice(&fs::read(&task_file).unwrap()).unwrap();
    let anchors: Vec<_> = task
        .references
        .iter()
        .filter_map(|entry| entry.code.as_deref())
        .collect();
    assert_eq!(anchors, vec!["main.rs#1", "main.rs#4"]);
    assert_eq!(fs::read_to_string(&source).unwrap(), moved);
}

#[test]
fn scan_reanchor_flag_preserves_cross_file_anchors() {
    let tf = TestFixtures::new();
    let root = tf.temp_dir.path();

    // Create a task
    let mut add = crate::common::lotar_cmd().unwrap();
    add.current_dir(root)
        .arg("add")
        .arg("Task")
        .assert()
        .success();
    let (project, task_file) = project_and_task_paths(root);
    let id = format!("{project}-1");

    // Create two files referencing same key; scan both to accumulate anchors
    let a = root.join("a.rs");
    let b = root.join("nested").join("b.rs");
    fs::create_dir_all(b.parent().unwrap()).unwrap();
    fs::write(&a, format!("// TODO {id}: A\n")).unwrap();
    fs::write(&b, format!("// TODO {id}: B\n")).unwrap();

    // First scan without --reanchor: both anchors should persist (latest per file only)
    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .arg("scan")
        .assert()
        .success();
    let yaml = fs::read_to_string(&task_file).unwrap();
    let yaml_norm = yaml.replace('\\', "/");
    assert!(yaml_norm.contains("code: a.rs#1"));
    assert!(yaml_norm.contains("code: nested/b.rs#1"));

    // Reanchor replaces stale locations within a file, not references in other files.
    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .args(["scan", "--reanchor"])
        .assert()
        .success();
    let yaml2 = fs::read_to_string(&task_file).unwrap().replace('\\', "/");
    let count = yaml2.matches("code:").count();
    assert_eq!(count, 2, "both live file anchors must remain: {yaml2}");
    assert!(yaml2.contains("code: a.rs#1"));
    assert!(yaml2.contains("code: nested/b.rs#1"));
}

// --- DEV-119: `--reanchor` removes dead anchors only ---------------------

fn set_references(task_file: &std::path::Path, refs: Vec<lotar::types::ReferenceEntry>) {
    let mut task: lotar::storage::task::Task =
        serde_yaml_ng::from_slice(&fs::read(task_file).unwrap()).unwrap();
    task.references = refs;
    fs::write(task_file, serde_yaml_ng::to_string(&task).unwrap()).unwrap();
}

fn read_references(task_file: &std::path::Path) -> Vec<lotar::types::ReferenceEntry> {
    let task: lotar::storage::task::Task =
        serde_yaml_ng::from_slice(&fs::read(task_file).unwrap()).unwrap();
    task.references
}

fn code(value: &str) -> lotar::types::ReferenceEntry {
    lotar::types::ReferenceEntry {
        code: Some(value.to_string()),
        ..Default::default()
    }
}

/// Task with: a live anchor, an anchor into a deleted file, an anchor into a
/// file that no longer mentions the ID, a link, and a mixed entry whose code
/// part is dead.
fn dead_anchor_fixture() -> (TestFixtures, String, std::path::PathBuf) {
    let tf = TestFixtures::new();
    common::cargo_bin_in(&tf)
        .args(["add", "Anchored task"])
        .assert()
        .success();
    let root = tf.get_temp_path().to_path_buf();
    let (project, task_file) = project_and_task_paths(&root);
    let id = format!("{project}-1");
    fs::write(root.join("live.rs"), format!("// TODO {id}: still here\n")).unwrap();
    fs::write(root.join("quiet.rs"), "fn unrelated() {}\n").unwrap();
    set_references(
        &task_file,
        vec![
            code("live.rs#1"),
            code("gone.rs#3"),
            code("quiet.rs#1"),
            lotar::types::ReferenceEntry {
                link: Some("https://example.com/spec".into()),
                ..Default::default()
            },
            lotar::types::ReferenceEntry {
                code: Some("gone.rs#9".into()),
                link: Some("https://example.com/mixed".into()),
                ..Default::default()
            },
        ],
    );
    (tf, id, task_file)
}

#[test]
fn reanchor_removes_dead_anchors_and_keeps_live_and_non_code_references() {
    let (tf, id, task_file) = dead_anchor_fixture();

    // A plain scan never removes anything.
    common::cargo_bin_in(&tf).arg("scan").assert().success();
    assert_eq!(read_references(&task_file).len(), 5);

    // Dry run lists removals without writing.
    let before = fs::read(&task_file).unwrap();
    let output = common::cargo_bin_in(&tf)
        .args(["scan", "--reanchor", "--dry-run"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr);
    for expected in [
        format!("Would remove dead anchor gone.rs#3 from {id}."),
        format!("Would remove dead anchor quiet.rs#1 from {id}."),
        format!("Would remove dead anchor gone.rs#9 from {id}."),
    ] {
        assert!(text.contains(&expected), "missing {expected:?} in {text}");
    }
    assert_eq!(
        fs::read(&task_file).unwrap(),
        before,
        "dry run must not write"
    );

    common::cargo_bin_in(&tf)
        .args(["scan", "--reanchor"])
        .assert()
        .success();
    let refs = read_references(&task_file);
    let codes: Vec<_> = refs.iter().filter_map(|r| r.code.as_deref()).collect();
    assert_eq!(codes, vec!["live.rs#1"]);
    assert!(
        refs.iter()
            .any(|r| r.link.as_deref() == Some("https://example.com/spec"))
    );
    // Mixed entry keeps its link, loses only the dead code part.
    assert!(
        refs.iter()
            .any(|r| r.link.as_deref() == Some("https://example.com/mixed") && r.code.is_none())
    );
    assert_eq!(refs.len(), 3);
}

#[test]
fn reanchor_with_paths_only_touches_anchors_under_those_paths() {
    let tf = TestFixtures::new();
    common::cargo_bin_in(&tf)
        .args(["add", "Scoped task"])
        .assert()
        .success();
    let root = tf.get_temp_path().to_path_buf();
    let (project, task_file) = project_and_task_paths(&root);
    let id = format!("{project}-1");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("other")).unwrap();
    fs::write(root.join("src/a.rs"), "fn nothing() {}\n").unwrap();
    // Drifted anchor outside the scanned path: the mention moved to line 3.
    fs::write(
        root.join("other/b.rs"),
        format!("// filler\n// filler\n// TODO {id}: moved\n"),
    )
    .unwrap();
    set_references(
        &task_file,
        vec![
            code("src/a.rs#1"),
            code("other/b.rs#1"),
            code("src/gone.rs#2"),
        ],
    );

    common::cargo_bin_in(&tf)
        .args(["scan", "src", "--reanchor"])
        .assert()
        .success();
    let codes: Vec<String> = read_references(&task_file)
        .into_iter()
        .filter_map(|r| r.code)
        .collect();
    assert_eq!(codes, vec!["other/b.rs#1".to_string()]);

    // A whole-workspace scan then relocates the drifted anchor.
    common::cargo_bin_in(&tf).arg("scan").assert().success();
    let codes: Vec<String> = read_references(&task_file)
        .into_iter()
        .filter_map(|r| r.code)
        .collect();
    assert_eq!(codes, vec!["other/b.rs#3".to_string()]);
}
