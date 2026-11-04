//! DEV-285: renaming a scanned file leaves one code reference, and blank
//! titles are refused.

mod common;

use common::TestFixtures;

#[test]
fn rename_keeps_a_single_code_reference() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path();
    let git = |args: &[&str]| {
        let mut full = vec![
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@e",
            "-c",
            "commit.gpgsign=false",
        ];
        full.extend_from_slice(args);
        std::process::Command::new("git")
            .current_dir(root)
            .args(&full)
            .output()
            .unwrap()
    };
    let lotar = |args: &[&str]| {
        common::lotar_cmd()
            .unwrap()
            .current_dir(root)
            .args(args)
            .output()
            .unwrap()
    };
    git(&["init", "-q"]);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/m.rs"), "fn a() {}\n// TODO: renamed item\n").unwrap();
    assert!(lotar(&["scan"]).status.success());
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "a"]);
    git(&["mv", "src/m.rs", "src/n.rs"]);
    assert!(lotar(&["scan", "src"]).status.success());
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "b"]);
    assert!(lotar(&["scan", "--reanchor"]).status.success());

    let task_dir = std::fs::read_dir(&fixtures.tasks_root)
        .unwrap()
        .flatten()
        .find(|e| e.path().is_dir() && !e.file_name().to_string_lossy().starts_with('@'))
        .unwrap()
        .path();
    let yaml = std::fs::read_to_string(task_dir.join("1.yml")).unwrap();
    assert_eq!(yaml.matches("src/n.rs#2").count(), 1, "{yaml}");
    assert!(!yaml.contains("src/m.rs"), "{yaml}");

    assert!(
        !lotar(&["add", "   "]).status.success(),
        "blank titles are refused"
    );
}
