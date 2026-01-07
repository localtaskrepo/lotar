use predicates::prelude::*;
use std::fs;

mod common;
use common::TestFixtures;

fn assert_yaml_contains(anchor_list: &str, needle: &str) {
    let normalized = anchor_list.replace('\\', "/");
    assert!(
        normalized.contains(needle),
        "expected YAML to include `{needle}` but was: {normalized}"
    );
}

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
fn scan_adds_reference_for_existing_key_and_reanchors_on_move() {
    let tf = TestFixtures::new();
    let root = tf.temp_dir.path();

    // Create an initial task via CLI add (id will be <PROJECT>-1)
    let mut add = crate::common::lotar_cmd().unwrap();
    add.current_dir(root)
        .arg("add")
        .arg("First task for reanchor")
        .assert()
        .success();

    // Discover project and id
    let (project, task_file) = project_and_task_paths(root);
    assert!(task_file.exists(), "expected first task file to exist");
    let id = format!("{project}-1");

    // Create source file with an existing key on line 1
    let src_path = root.join("main.rs");
    fs::write(&src_path, format!("// TODO {id}: do thing\n")).unwrap();

    // Run scan; should not create a new task, but should append a code reference to 1.yml
    let mut scan = crate::common::lotar_cmd().unwrap();
    scan.current_dir(root)
        .arg("scan")
        .assert()
        .success()
        .stdout(predicate::str::contains("Found 1 TODO comment(s):"));

    let yaml = fs::read_to_string(&task_file).unwrap();
    assert!(
        yaml.contains("references:"),
        "expected references section in YAML after scan"
    );
    assert_yaml_contains(&yaml, "code: main.rs#1");

    // Move the comment down by inserting a blank line on top -> line becomes 2
    fs::write(&src_path, format!("\n// TODO {id}: do thing\n")).unwrap();

    // Scan again; should add a second anchor for the new line
    let mut scan2 = crate::common::lotar_cmd().unwrap();
    scan2.current_dir(root).arg("scan").assert().success();

    let yaml2 = fs::read_to_string(&task_file).unwrap();
    assert_yaml_contains(&yaml2, "code: main.rs#2");
}

#[test]
fn scan_adds_reference_when_file_is_renamed() {
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

    // Create file and scan once for baseline anchor
    let src1 = root.join("a.rs");
    fs::write(&src1, format!("// TODO {id}: alpha\n")).unwrap();
    let mut scan = crate::common::lotar_cmd().unwrap();
    scan.current_dir(root).arg("scan").assert().success();
    let yaml = fs::read_to_string(&task_file).unwrap();
    assert_yaml_contains(&yaml, "code: a.rs#1");

    // Rename file; keep the same TODO line
    let src2 = root.join("src").join("b.rs");
    fs::create_dir_all(src2.parent().unwrap()).unwrap();
    fs::rename(&src1, &src2).unwrap();

    // Scan again; should add a new anchor for the new path
    let mut scan2 = crate::common::lotar_cmd().unwrap();
    scan2.current_dir(root).arg("scan").assert().success();
    let yaml2 = fs::read_to_string(&task_file).unwrap();
    assert_yaml_contains(&yaml2, "code: src/b.rs#1");
}
