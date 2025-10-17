//! Configuration system tests
//!
//! This module consolidates all configuration-related tests including:
//! - Global and project configuration
//! - Configuration templates and inheritance
//! - Settings operations (show, set, init)
//! - Custom tasks directory handling
//! - Environment variables and command-line overrides

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

mod common;
use common::TestFixtures;

// =============================================================================
// Global Configuration
// =============================================================================

mod global_config {
    use super::*;

    #[test]
    fn test_global_config_read_only_behavior() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // The TestFixtures already creates a .tasks directory, so let's remove it first
        let tasks_dir = temp_dir.join(".tasks");
        if tasks_dir.exists() {
            std::fs::remove_dir_all(&tasks_dir).unwrap();
        }
        assert!(!tasks_dir.exists());

        // Run config show command, which should NOT create any files (read-only operation)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("show")
            .assert()
            .success()
            .stdout(predicate::str::contains("Configuration for project:"))
            .stdout(predicate::str::contains(
                "(none set - will auto-detect on first task creation)",
            ));

        // Verify no files were created by the read-only operation
        assert!(
            !tasks_dir.exists(),
            "Config show should not create any directories"
        );

        // Now test that write operations DO create config files when needed
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("task")
            .arg("add")
            .arg("Test task")
            .assert()
            .success();

        // Now verify the config was created by the write operation
        let global_config_path = tasks_dir.join("config.yml");
        assert!(
            global_config_path.exists(),
            "Task creation should create global config"
        );

        // Verify the config content doesn't contain null values
        let config_content = fs::read_to_string(&global_config_path).unwrap();
        assert!(!config_content.contains("null"));
        // Canonical nested YAML output
        assert!(config_content.contains("server:"));
        assert!(config_content.contains("port: 8080"));
    }

    #[test]
    fn test_global_config_operations() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Set global server port
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("set")
            .arg("server_port")
            .arg("9000")
            .assert()
            .success()
            .stdout(predicate::str::contains("Successfully updated server_port"));

        // Verify global config was updated
        let global_config_path = temp_dir.join(".tasks").join("config.yml");
        let global_config_content = fs::read_to_string(&global_config_path).unwrap();
        assert!(global_config_content.contains("server:"));
        assert!(global_config_content.contains("port: 9000"));

        // Test config show displays the updated value
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("show")
            .assert()
            .success()
            .stdout(predicate::str::contains("Port: 9000"));
    }
}

// =============================================================================
// Project Configuration
// =============================================================================

mod project_config {
    use super::*;

    #[test]
    fn test_project_config_creation_with_prefix() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Initialize a project with a long name to test prefix generation
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=MyVeryLongProjectName")
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "Initializing configuration with template 'default'",
            ))
            .stdout(predicate::str::contains("✅ Configuration initialized at:"))
            .stdout(predicate::str::contains(".tasks/MYVE/config.yml"));

        // Verify the project folder was created with the correct prefix
        let project_dir = temp_dir.join(".tasks").join("MYVE");
        assert!(project_dir.exists());

        // Verify the project config file exists and has clean content
        let project_config_path = project_dir.join("config.yml");
        assert!(project_config_path.exists());

        let config_content = fs::read_to_string(&project_config_path).unwrap();
        assert!(!config_content.contains("null"));
        // Canonical nested project id
        assert!(config_content.contains("project:"));
        assert!(config_content.contains("name: MyVeryLongProjectName"));
        assert!(!config_content.contains("project.id"));
        // Template-based configs contain all template fields, not just project name
        let lines: Vec<&str> = config_content.lines().collect();
        assert!(lines.len() > 1); // Should contain multiple lines from template
    }

    #[test]
    fn test_project_config_short_name() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Test with a 4-character project name (should remain the same)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=TEST")
            .assert()
            .success()
            .stdout(predicate::str::contains("✅ Configuration initialized at:"))
            .stdout(predicate::str::contains(".tasks/TEST/config.yml"));

        // Verify the project folder was created with the same name
        let project_dir = temp_dir.join(".tasks").join("TEST");
        assert!(project_dir.exists());
    }

    #[test]
    fn test_project_prefix_generation_edge_cases() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Test hyphenated project name
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=my-awesome-project")
            .assert()
            .success()
            .stdout(predicate::str::contains(".tasks/MAP/config.yml"));

        let hyphen_dir = temp_dir.join(".tasks").join("MAP");
        assert!(hyphen_dir.exists());

        // Test underscored project name
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=my_cool_project")
            .assert()
            .success()
            .stdout(predicate::str::contains(".tasks/MCP/config.yml"));

        let underscore_dir = temp_dir.join(".tasks").join("MCP");
        assert!(underscore_dir.exists());

        // Test short project name (should remain unchanged)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=ABC")
            .assert()
            .success()
            .stdout(predicate::str::contains(".tasks/ABC/config.yml"));

        let short_dir = temp_dir.join(".tasks").join("ABC");
        assert!(short_dir.exists());
    }
}

// =============================================================================
// Configuration Templates
// =============================================================================

mod templates {
    use super::*;

    #[test]
    fn test_config_templates() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Test simple template
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--template=simple")
            .arg("--project=SimpleProject")
            .assert()
            .success()
            .stdout(predicate::str::contains("template 'simple'"));

        let config_path = temp_dir.join(".tasks").join("SIMP").join("config.yml");
        let config_content = fs::read_to_string(&config_path).unwrap();
        assert!(config_content.contains("project:"));
        assert!(config_content.contains("name: SimpleProject"));
        assert!(!config_content.contains("project.id"));
        assert!(config_content.contains("issue:"));
        assert!(config_content.contains("states:"));
        assert!(config_content.contains("Todo"));
        assert!(config_content.contains("InProgress"));
        assert!(config_content.contains("Done"));

        // Test agile template
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--template=agile")
            .arg("--project=AgileProject")
            .assert()
            .success()
            .stdout(predicate::str::contains("template 'agile'"));

        let agile_config_path = temp_dir.join(".tasks").join("AGIL").join("config.yml");
        let agile_config_content = fs::read_to_string(&agile_config_path).unwrap();
        assert!(agile_config_content.contains("project:"));
        assert!(agile_config_content.contains("name: AgileProject"));
        assert!(!agile_config_content.contains("project.id"));
        assert!(agile_config_content.contains("issue:"));
        assert!(agile_config_content.contains("types:"));
        assert!(agile_config_content.contains("Epic"));
    }

    #[test]
    fn test_config_templates_list() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // List available templates
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("templates")
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "Available Configuration Templates:",
            ))
            .stdout(predicate::str::contains("default"))
            .stdout(predicate::str::contains("simple"))
            .stdout(predicate::str::contains("agile"))
            .stdout(predicate::str::contains("kanban"));
    }
}

// =============================================================================
// Configuration Operations
// =============================================================================

mod config_operations {
    use super::*;

    #[test]
    fn test_config_set_operations() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // First initialize a project
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=SetTestProject")
            .assert()
            .success();

        // Ensure default project is set for project-scoped operations
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("set")
            .arg("default_project")
            .arg("SetTestProject")
            .assert()
            .success();

        // Set issue states
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("set")
            .arg("issue_states")
            .arg("TODO,IN_PROGRESS,VERIFY,DONE")
            .arg("--project=SetTestProject")
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "Successfully updated issue_states",
            ));

        // Set default priority
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("set")
            .arg("default_priority")
            .arg("HIGH")
            .arg("--project=SetTestProject")
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "Successfully updated default_priority",
            ));

        // Verify the project config was updated
        let config_path = temp_dir.join(".tasks").join("SETT").join("config.yml");
        let config_content = fs::read_to_string(&config_path).unwrap();
        assert!(config_content.contains("default:"));
        assert!(config_content.contains("priority: HIGH"));
        assert!(config_content.contains("issue:"));
        assert!(config_content.contains("states:"));
    }

    #[test]
    fn test_config_show_command() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Initialize a project with custom settings
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--template=simple")
            .arg("--project=ShowTest")
            .assert()
            .success();

        // Show the configuration
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("show")
            .arg("--project=ShowTest")
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "Configuration for project: ShowTest",
            ))
            .stdout(predicate::str::contains("Project Settings:"))
            .stdout(predicate::str::contains("Project prefix:"))
            .stdout(predicate::str::contains("Default Priority:"));
    }
}

// =============================================================================
// Custom Tasks Directory
// =============================================================================

mod custom_tasks_directory {
    use super::*;

    #[test]
    fn test_custom_tasks_directory_flag() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();
        let custom_tasks_dir = temp_dir.join("custom-tasks");

        // Ensure custom directory doesn't exist initially
        assert!(!custom_tasks_dir.exists());

        // Test that using --tasks-dir with non-existent directory automatically creates it
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("--tasks-dir=custom-tasks")
            .arg("config")
            .arg("show")
            .assert()
            .success()
            .stdout(predicate::str::contains("Tasks directory: custom-tasks"));

        // Verify the custom directory was created
        assert!(custom_tasks_dir.exists());

        // Test config set command with custom directory
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("--tasks-dir=custom-tasks")
            .arg("config")
            .arg("set")
            .arg("server_port")
            .arg("9001")
            .assert()
            .success()
            .stdout(predicate::str::contains("Successfully updated server_port"));

        // Verify config.yml was created in custom directory
        let custom_config_path = custom_tasks_dir.join("config.yml");
        assert!(custom_config_path.exists());

        // Verify the config contains our setting
        let config_content = fs::read_to_string(&custom_config_path).unwrap();
        assert!(config_content.contains("server:"));
        assert!(config_content.contains("port: 9001"));
    }

    #[test]
    fn test_tasks_directory_environment_variable() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();
        let env_tasks_dir = temp_dir.join("env-tasks");

        // Create the environment-specified directory
        fs::create_dir_all(&env_tasks_dir).unwrap();

        // Test with LOTAR_TASKS_DIR environment variable
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .env("LOTAR_TASKS_DIR", env_tasks_dir.to_str().unwrap())
            .arg("config")
            .arg("set")
            .arg("default_project")
            .arg("env-project")
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "Successfully updated default_project",
            ));

        // Verify config.yml was created in environment-specified directory
        let env_config_path = env_tasks_dir.join("config.yml");
        assert!(env_config_path.exists());

        // Verify the config contains our setting
        let config_content = fs::read_to_string(&env_config_path).unwrap();
        assert!(config_content.contains("default:"));
        // default_project is normalized to a prefix in canonical config; env-project -> EP
        assert!(config_content.contains("project: EP"));
    }

    #[test]
    fn test_tasks_directory_parent_search() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create a parent directory with .tasks
        let parent_tasks_dir = temp_dir.join(".tasks");
        fs::create_dir_all(&parent_tasks_dir).unwrap();

        // Create a config file in the parent tasks directory
        let parent_config_path = parent_tasks_dir.join("config.yml");
        fs::write(&parent_config_path, "server.port: 7777\n# task_file_extension and tasks_folder are legacy/no-op in current model\ndefault.project: parent-project\n").unwrap();

        // Create a subdirectory
        let sub_dir = temp_dir.join("subdir");
        fs::create_dir_all(&sub_dir).unwrap();

        // Test from subdirectory - should find parent .tasks directory
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&sub_dir)
            .arg("config")
            .arg("show")
            .assert()
            .success()
            .stdout(predicate::str::contains("Port: 7777"))
            .stdout(predicate::str::contains("Default Project: parent-project"));
    }

    #[test]
    fn test_task_commands_with_custom_directory() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();
        let custom_tasks_dir = temp_dir.join("project-tasks");

        // Create the custom directory
        fs::create_dir_all(&custom_tasks_dir).unwrap();

        // Test task creation with custom directory
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("--tasks-dir=project-tasks")
            .arg("task")
            .arg("add")
            .arg("Custom Task")
            .arg("--description=Task in custom directory")
            .arg("--project=test-project")
            .assert()
            .success()
            .stdout(predicate::str::contains("Created task:"));

        // Verify task files were created in custom directory
        // Note: index.yml no longer created in simplified architecture
        assert!(custom_tasks_dir.exists());

        // Check that project directory was created
        let project_dirs: Vec<_> = fs::read_dir(&custom_tasks_dir)
            .unwrap()
            .filter_map(|entry| {
                let entry = entry.ok()?;
                if entry.file_type().ok()?.is_dir() {
                    Some(entry.file_name().to_string_lossy().to_string())
                } else {
                    None
                }
            })
            .collect();

        assert!(
            !project_dirs.is_empty(),
            "At least one project directory should be created"
        );
    }
}

// =============================================================================
// Configuration Inheritance and Priority
// =============================================================================

mod inheritance {
    use super::*;

    #[test]
    fn test_config_inheritance_and_priority() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Create global config first
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("set")
            .arg("server_port")
            .arg("9000")
            .assert()
            .success();

        // Set a default project
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("set")
            .arg("default_project")
            .arg("InheritanceTest")
            .assert()
            .success();

        // Create a project config
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=InheritanceTest")
            .assert()
            .success();

        // Set project-specific configuration
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("set")
            .arg("default_priority")
            .arg("HIGH")
            .arg("--project=InheritanceTest")
            .assert()
            .success();

        // Show config should display project settings (not server settings)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("show")
            .arg("--project=InheritanceTest")
            .assert()
            .success()
            // With canonicalization, a provided project name is displayed as its prefix
            .stdout(predicate::str::contains("Project prefix: INHE"))
            .stdout(predicate::str::contains("Default Priority: HIGH")); // Project-specific
    }
}

// =============================================================================
// Error Handling
// =============================================================================

mod error_handling {
    use super::*;

    #[test]
    fn test_config_error_handling() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Test invalid template
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--template=invalid")
            .arg("--project=ErrorTest")
            .assert()
            .failure()
            .stderr(predicate::str::contains("Unknown template: invalid"));
    }

    #[test]
    fn test_config_help_command() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Test config help
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("help")
            .arg("config")
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "Manage project and global configuration settings",
            ))
            .stdout(predicate::str::contains("Usage"))
            .stdout(predicate::str::contains("Actions"))
            .stdout(predicate::str::contains("show"))
            .stdout(predicate::str::contains("set"))
            .stdout(predicate::str::contains("init"))
            .stdout(predicate::str::contains("Examples"))
            .stdout(predicate::str::contains("Templates"));
    }
}
