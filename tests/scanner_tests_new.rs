//! Scanner and indexing system tests
//! 
//! This module consolidates all scanner-related tests including:
//! - File system scanning and indexing
//! - Project discovery and analysis
//! - Basic scanning functionality

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

mod common;
use common::TestFixtures;

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
        fs::write(temp_dir.join("main.py"), "# TODO: implement main logic\nprint('Hello World')").unwrap();
        fs::create_dir_all(temp_dir.join("src")).unwrap();
        fs::write(temp_dir.join("src/lib.py"), "# TODO: add library code\n# Library code").unwrap();

        // Test basic scan command
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("🔍 Scanning"))
            .stdout(predicate::str::contains("📝 Found 2 TODO comment(s):"));
    }

    #[test]
    fn test_scan_with_mixed_files() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create mixed file types with TODO comments
        fs::write(temp_dir.join("script.py"), "# TODO: Python script improvement").unwrap();
        fs::write(temp_dir.join("script.js"), "// TODO: JavaScript enhancement").unwrap();
        fs::write(temp_dir.join("style.css"), "/* CSS file */").unwrap();
        fs::write(temp_dir.join("data.json"), r#"{"key": "value"}"#).unwrap();

        // Test basic scan
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("📝 Found 2 TODO comment(s):"));
    }

    #[test]
    fn test_scan_recursive() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create nested directory structure with TODO comments
        fs::create_dir_all(temp_dir.join("level1/level2/level3")).unwrap();
        fs::write(temp_dir.join("root.rs"), "// TODO: Root level task").unwrap();
        fs::write(temp_dir.join("level1/level1.rs"), "// TODO: Level 1 task").unwrap();
        fs::write(temp_dir.join("level1/level2/level2.rs"), "// TODO: Level 2 task").unwrap();
        fs::write(temp_dir.join("level1/level2/level3/level3.rs"), "// TODO: Level 3 task").unwrap();

        // Test recursive scan
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("📝 Found 4 TODO comment(s):"));
    }

    #[test]
    fn test_scan_no_todos() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create files without TODO comments
        fs::write(temp_dir.join("clean.rs"), "fn main() { println!(\"Hello\"); }").unwrap();
        fs::write(temp_dir.join("clean.py"), "print('Hello World')").unwrap();

        // Test scan with no TODOs
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
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
        fs::write(temp_dir.join("test.rs"), "// TODO: implement feature\nfn main() {}").unwrap();

        // Test scan with detailed output
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("scan")
            .arg("--detailed")
            .assert()
            .success()
            .stdout(predicate::str::contains("📝 Found 1 TODO comment(s):"))
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
        fs::write(temp_dir.join("Cargo.toml"), "[package]\nname = \"test\"\nversion = \"0.1.0\"").unwrap();
        fs::create_dir_all(temp_dir.join("src")).unwrap();
        fs::write(temp_dir.join("src/main.rs"), "// TODO: implement main\nfn main() {}").unwrap();

        // Test scan
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("📝 Found 1 TODO comment(s):"));
    }

    #[test]
    fn test_python_project_discovery() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create Python project structure
        fs::write(temp_dir.join("setup.py"), "from setuptools import setup").unwrap();
        fs::write(temp_dir.join("main.py"), "# TODO: implement main\nprint('hello')").unwrap();

        // Test scan
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("📝 Found 1 TODO comment(s):"));
    }

    #[test]
    fn test_javascript_project_discovery() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create JavaScript project structure
        fs::write(temp_dir.join("package.json"), r#"{"name": "test", "version": "1.0.0"}"#).unwrap();
        fs::write(temp_dir.join("index.js"), "// TODO: implement main\nconsole.log('hello');").unwrap();

        // Test scan
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("📝 Found 1 TODO comment(s):"));
    }

    #[test]
    fn test_multiple_project_types() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create mixed project structure
        fs::write(temp_dir.join("Cargo.toml"), "[package]\nname = \"test\"\nversion = \"0.1.0\"").unwrap();
        fs::write(temp_dir.join("package.json"), r#"{"name": "test", "version": "1.0.0"}"#).unwrap();
        fs::write(temp_dir.join("setup.py"), "from setuptools import setup").unwrap();
        
        fs::write(temp_dir.join("main.rs"), "// TODO: Rust TODO").unwrap();
        fs::write(temp_dir.join("main.js"), "// TODO: JavaScript TODO").unwrap();
        fs::write(temp_dir.join("main.py"), "# TODO: Python TODO").unwrap();

        // Test scan
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("📝 Found 3 TODO comment(s):"));
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
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("--tasks-dir=custom-tasks")
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("📝 Found 1 TODO comment(s):"));
    }

    #[test]
    fn test_comprehensive_scanner_workflow() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create comprehensive test structure
        fs::create_dir_all(temp_dir.join("src")).unwrap();
        fs::create_dir_all(temp_dir.join("tests")).unwrap();
        
        fs::write(temp_dir.join("src/main.rs"), "// TODO: implement main logic").unwrap();
        fs::write(temp_dir.join("src/lib.rs"), "// TODO: add library functionality").unwrap();
        fs::write(temp_dir.join("tests/test.rs"), "// TODO: add more tests").unwrap();

        // Test verbose scan
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("scan")
            .arg("--detailed")
            .assert()
            .success()
            .stdout(predicate::str::contains("📝 Found 3 TODO comment(s):"));
    }

    #[test]
    fn test_scan_integration_with_task_system() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create source files with TODOs
        fs::write(temp_dir.join("main.rs"), "// TODO: integrate with task system").unwrap();

        // Test scan
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("📝 Found 1 TODO comment(s):"));
    }
}
