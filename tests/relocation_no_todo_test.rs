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
fn reanchor_runs_even_when_no_todos_found() {
    let tf = TestFixtures::new();
    let root = tf.temp_dir.path();

    // Create task
    let mut add = crate::common::lotar_cmd().unwrap();
    add.current_dir(root)
        .arg("add")
        .arg("Task")
        .assert()
        .success();
    let (project, task_file) = project_and_task_paths(root);
    let id = format!("{project}-1");

    // Initial TODO with key on line 1
    let src = root.join("file.rs");
    fs::write(&src, format!("// TODO {id}: do it\n")).unwrap();
    let mut scan = crate::common::lotar_cmd().unwrap();
    scan.current_dir(root).arg("scan").assert().success();
    let yaml = fs::read_to_string(&task_file).unwrap();
    assert!(
        yaml.contains("code: file.rs#L1"),
        "expected initial anchor at L1"
    );

    // Move line down and remove signal word, keeping only the key marker
    fs::write(&src, format!("\n// do it ({id})\n")).unwrap();

    // Scan again; should find no TODOs but still re-anchor the reference automatically
    let mut scan2 = crate::common::lotar_cmd().unwrap();
    scan2.current_dir(root).arg("scan").assert().success();

    let yaml2 = fs::read_to_string(&task_file).unwrap();
    assert!(
        yaml2.contains("code: file.rs#L2"),
        "expected anchor updated to L2 after move without TODO"
    );
}
