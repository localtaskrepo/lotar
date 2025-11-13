use predicates::prelude::*;
use std::fs;

mod common;
use common::TestFixtures;

#[test]
fn scan_creates_task_with_source_reference() {
    let tf = TestFixtures::new();
    let root = tf.temp_dir.path();

    // Create a simple source file with a TODO missing a key
    let src = r#"// TODO: connect bi-dir link test"#;
    let file_path = root.join("main.rs");
    fs::write(&file_path, src).unwrap();
    let canon_path = fs::canonicalize(&file_path).unwrap();
    let canon_str = canon_path.display().to_string();

    // Run scan (apply-by-default)
    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(root)
        .arg("scan")
        .assert()
        .success()
        .stdout(predicate::str::contains("Found 1 TODO comment(s):"));

    // Determine project folder (default)
    let tasks_dir = root.join(".tasks");
    // The effective default project folder is derived; list dirs under .tasks and pick one
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
    let project = &projects[0];

    // Find the created task file (1.yml)
    let task_file = tasks_dir.join(project).join("1.yml");
    assert!(
        task_file.exists(),
        "expected {} to exist",
        task_file.display()
    );
    let yaml = fs::read_to_string(&task_file).unwrap();

    // Verify references contains a code entry with file path and #L1 anchor
    assert!(
        yaml.contains("references:"),
        "expected references in YAML: {yaml}"
    );
    // Path with anchor: accept canonical absolute form or just main.rs
    let anchor1 = format!("code: {canon_str}#L1");
    assert!(
        yaml.contains(&anchor1) || yaml.contains("code: main.rs#L1"),
        "expected code reference with #L1 in YAML: {yaml}"
    );
}
