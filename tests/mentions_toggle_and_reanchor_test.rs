use assert_cmd::Command;
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
    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(root)
        .args(["config", "show"])
        .assert()
        .success();
    // Disable mentions at project level: we need the actual detected project prefix directory
    // Resolve it by creating a project via CLI add first (creates .tasks/<PREFIX>)
    let mut add_tmp = Command::cargo_bin("lotar").unwrap();
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
    let mut add = Command::cargo_bin("lotar").unwrap();
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
    let mut scan = Command::cargo_bin("lotar").unwrap();
    scan.current_dir(root).arg("scan").assert().success();
    let yaml = fs::read_to_string(&task_file).unwrap();
    assert!(
        !yaml.contains("references:"),
        "mentions disabled should not add references; got: {yaml}"
    );
}

#[test]
fn scan_reanchor_flag_prunes_cross_file_anchors() {
    let tf = TestFixtures::new();
    let root = tf.temp_dir.path();

    // Create a task
    let mut add = Command::cargo_bin("lotar").unwrap();
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
    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(root)
        .arg("scan")
        .assert()
        .success();
    let yaml = fs::read_to_string(&task_file).unwrap();
    assert!(yaml.contains("code: a.rs#L1"));
    assert!(yaml.contains("code: nested/b.rs#L1"));

    // Now run with --reanchor: only the newest occurrence should remain
    Command::cargo_bin("lotar")
        .unwrap()
        .current_dir(root)
        .args(["scan", "--reanchor"])
        .assert()
        .success();
    let yaml2 = fs::read_to_string(&task_file).unwrap();
    // We only assert that at most one anchor remains. It should be for whichever scan processed last
    let count = yaml2.matches("code:").count();
    assert!(
        count <= 1,
        "expected reanchor to prune to a single anchor, got {count}: {yaml2}"
    );
}
