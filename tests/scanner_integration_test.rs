//! Scanner and indexing system tests
//!
//! This module consolidates all scanner-related tests including:
//! - File system scanning and indexing
//! - Project discovery and analysis
//! - Basic scanning functionality

use predicates::prelude::*;
use std::fs;

mod common;
use common::{TestFixtures, cargo_bin_in};

// =============================================================================
// Basic Scanner Functionality
// =============================================================================

mod basic_scanning {
    use super::*;

    #[test]
    fn test_basic_file_scanning() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create a basic project structure with TODO comments
        fs::write(temp_dir.join("README.md"), "# Test Project").unwrap();
        fs::write(
            temp_dir.join("main.py"),
            "# TODO: implement main logic\nprint('Hello World')",
        )
        .unwrap();
        fs::create_dir_all(temp_dir.join("src")).unwrap();
        fs::write(
            temp_dir.join("src/lib.py"),
            "# TODO: add library code\n# Library code",
        )
        .unwrap();

        // Test basic scan command
        let _cmd = cargo_bin_in(&test_fixtures)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Scanning"))
            .stdout(predicate::str::contains("Found 2 TODO comment(s):"));
    }

    #[test]
    fn test_scan_with_mixed_files() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create mixed file types with TODO comments
        fs::write(
            temp_dir.join("script.py"),
            "# TODO: Python script improvement",
        )
        .unwrap();
        fs::write(
            temp_dir.join("script.js"),
            "// TODO: JavaScript enhancement",
        )
        .unwrap();
        fs::write(temp_dir.join("style.css"), "/* CSS file */").unwrap();
        fs::write(temp_dir.join("data.json"), r#"{"key": "value"}"#).unwrap();

        // Test basic scan
        let _cmd = cargo_bin_in(&test_fixtures)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Found 2 TODO comment(s):"));
    }

    #[test]
    fn test_scan_recursive() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create nested directory structure with TODO comments
        fs::create_dir_all(temp_dir.join("level1/level2/level3")).unwrap();
        fs::write(temp_dir.join("root.rs"), "// TODO: Root level task").unwrap();
        fs::write(temp_dir.join("level1/level1.rs"), "// TODO: Level 1 task").unwrap();
        fs::write(
            temp_dir.join("level1/level2/level2.rs"),
            "// TODO: Level 2 task",
        )
        .unwrap();
        fs::write(
            temp_dir.join("level1/level2/level3/level3.rs"),
            "// TODO: Level 3 task",
        )
        .unwrap();

        // Test recursive scan
        let _cmd = cargo_bin_in(&test_fixtures)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Found 4 TODO comment(s):"));
    }

    #[test]
    fn test_scan_no_todos() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create files without TODO comments
        fs::write(
            temp_dir.join("clean.rs"),
            "fn main() { println!(\"Hello\"); }",
        )
        .unwrap();
        fs::write(temp_dir.join("clean.py"), "print('Hello World')").unwrap();

        // Test scan with no TODOs
        let _cmd = cargo_bin_in(&test_fixtures)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("✅ No TODO comments found."));
    }

    #[test]
    fn test_scan_with_detailed_flag() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create file with TODO
        fs::write(
            temp_dir.join("test.rs"),
            "// TODO: implement feature\nfn main() {}",
        )
        .unwrap();

        // Test scan with detailed output
        let _cmd = cargo_bin_in(&test_fixtures)
            .arg("scan")
            .arg("--detailed")
            .assert()
            .success()
            .stdout(predicate::str::contains("Found 1 TODO comment(s):"))
            .stdout(predicate::str::contains("📄"));
    }
}

// =============================================================================
// Project Discovery
// =============================================================================

mod project_discovery {
    use super::*;

    #[test]
    fn test_rust_project_discovery() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create Rust project structure
        fs::write(
            temp_dir.join("Cargo.toml"),
            "[package]\nname = \"test\"\nversion = \"0.1.0\"",
        )
        .unwrap();
        fs::create_dir_all(temp_dir.join("src")).unwrap();
        fs::write(
            temp_dir.join("src/main.rs"),
            "// TODO: implement main\nfn main() {}",
        )
        .unwrap();

        // Test scan
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Found 1 TODO comment(s):"));
    }

    #[test]
    fn test_python_project_discovery() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create Python project structure
        fs::write(temp_dir.join("setup.py"), "from setuptools import setup").unwrap();
        fs::write(
            temp_dir.join("main.py"),
            "# TODO: implement main\nprint('hello')",
        )
        .unwrap();

        // Test scan
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Found 1 TODO comment(s):"));
    }

    #[test]
    fn test_javascript_project_discovery() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create JavaScript project structure
        fs::write(
            temp_dir.join("package.json"),
            r#"{"name": "test", "version": "1.0.0"}"#,
        )
        .unwrap();
        fs::write(
            temp_dir.join("index.js"),
            "// TODO: implement main\nconsole.log('hello');",
        )
        .unwrap();

        // Test scan
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Found 1 TODO comment(s):"));
    }

    #[test]
    fn test_multiple_project_types() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create mixed project structure
        fs::write(
            temp_dir.join("Cargo.toml"),
            "[package]\nname = \"test\"\nversion = \"0.1.0\"",
        )
        .unwrap();
        fs::write(
            temp_dir.join("package.json"),
            r#"{"name": "test", "version": "1.0.0"}"#,
        )
        .unwrap();
        fs::write(temp_dir.join("setup.py"), "from setuptools import setup").unwrap();

        fs::write(temp_dir.join("main.rs"), "// TODO: Rust TODO").unwrap();
        fs::write(temp_dir.join("main.js"), "// TODO: JavaScript TODO").unwrap();
        fs::write(temp_dir.join("main.py"), "# TODO: Python TODO").unwrap();

        // Test scan
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Found 3 TODO comment(s):"));
    }
}

// =============================================================================
// System Integration
// =============================================================================

mod system_integration {
    use super::*;

    #[test]
    fn test_scan_with_custom_tasks_directory() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create custom tasks directory
        fs::create_dir_all(temp_dir.join("custom-tasks")).unwrap();

        // Create file with TODO
        fs::write(temp_dir.join("test.rs"), "// TODO: custom dir test").unwrap();

        // Test scan with custom tasks directory
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("--tasks-dir=custom-tasks")
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Found 1 TODO comment(s):"));
    }

    #[test]
    fn test_comprehensive_scanner_workflow() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create comprehensive test structure
        fs::create_dir_all(temp_dir.join("src")).unwrap();
        fs::create_dir_all(temp_dir.join("tests")).unwrap();

        fs::write(
            temp_dir.join("src/main.rs"),
            "// TODO: implement main logic",
        )
        .unwrap();
        fs::write(
            temp_dir.join("src/lib.rs"),
            "// TODO: add library functionality",
        )
        .unwrap();
        fs::write(temp_dir.join("tests/test.rs"), "// TODO: add more tests").unwrap();

        // Test verbose scan
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("scan")
            .arg("--detailed")
            .assert()
            .success()
            .stdout(predicate::str::contains("Found 3 TODO comment(s):"));
    }

    #[test]
    fn test_scan_integration_with_task_system() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create source files with TODOs
        fs::write(
            temp_dir.join("main.rs"),
            "// TODO: integrate with task system",
        )
        .unwrap();

        // Test scan
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp_dir)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Found 1 TODO comment(s):"));
    }
}

// =============================================================================
// Consolidated additions
// =============================================================================

mod bidir_references {
    use crate::common::TestFixtures;
    use predicates::prelude::*;
    use std::fs;

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

        // Verify references contains a code entry with file path and #1 anchor
        assert!(
            yaml.contains("references:"),
            "expected references in YAML: {yaml}"
        );
        let anchor1 = format!("code: {canon_str}#1");
        assert!(
            yaml.contains(&anchor1) || yaml.contains("code: main.rs#1"),
            "expected code reference with #1 in YAML: {yaml}"
        );
    }
}

mod ignore_and_filters {
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;

    use tempfile::tempdir;

    use lotar::scanner::Scanner;

    fn write(path: &std::path::Path, content: &str) {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let mut f = fs::File::create(path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    fn filenames(results: &[lotar::scanner::Reference]) -> Vec<String> {
        let mut v: Vec<String> = results
            .iter()
            .map(|r| {
                r.file_path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            })
            .collect();
        v.sort();
        v
    }

    #[test]
    fn include_filter_limits_extensions() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        write(&root.join("a.rs"), "// TODO: in rust");
        write(&root.join("b.py"), "# TODO: in python");

        let mut scanner = Scanner::new(PathBuf::from(root)).with_include_ext(&["rs".into()]);
        let results = scanner.scan();

        assert_eq!(results.len(), 1, "only .rs should be scanned");
        assert_eq!(filenames(&results), vec!["a.rs".to_string()]);
    }

    #[test]
    fn exclude_overrides_include() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        write(&root.join("a.rs"), "// TODO: rust");
        write(&root.join("b.py"), "# TODO: py");

        let mut scanner = Scanner::new(PathBuf::from(root))
            .with_include_ext(&["rs".into(), "py".into()])
            .with_exclude_ext(&["py".into()]);
        let results = scanner.scan();

        assert_eq!(results.len(), 1, "py should be excluded");
        assert_eq!(filenames(&results), vec!["a.rs".to_string()]);
    }

    #[test]
    fn gitignore_is_respected_when_no_lotarignore() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        write(&root.join(".gitignore"), "nested/\n*.log\n");
        write(
            &root.join("nested/ignored.js"),
            "// TODO: hidden by gitignore",
        );
        write(&root.join("visible.rs"), "// TODO: visible");

        let mut scanner = Scanner::new(PathBuf::from(root));
        let results = scanner.scan();

        let names = filenames(&results);
        assert!(names.contains(&"visible.rs".to_string()));
        assert!(
            !names.contains(&"ignored.js".to_string()),
            ".gitignore should hide nested/"
        );
    }

    #[test]
    fn lotarignore_overrides_gitignore() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        write(&root.join(".gitignore"), "nested/\n");
        write(&root.join(".lotarignore"), "# custom rules (none)\n");
        write(
            &root.join("nested/scan.js"),
            "// TODO: should be scanned when .lotarignore present",
        );

        let mut scanner = Scanner::new(PathBuf::from(root));
        let results = scanner.scan();

        let names = filenames(&results);
        assert!(
            names.contains(&"scan.js".to_string()),
            ".lotarignore present => fallback to gitignore disabled"
        );
    }

    #[test]
    fn lotarignore_can_exclude() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        write(&root.join(".lotarignore"), "nested/\n");
        write(
            &root.join("nested/skip.ts"),
            "// TODO: should be excluded by .lotarignore",
        );
        write(&root.join("keep.rs"), "// TODO: keep");

        let mut scanner = Scanner::new(PathBuf::from(root));
        let results = scanner.scan();

        let names = filenames(&results);
        assert!(names.contains(&"keep.rs".to_string()));
        assert!(!names.contains(&"skip.ts".to_string()));
    }
}
