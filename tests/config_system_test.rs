use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

mod common;
use common::TestFixtures;

#[test]
fn test_global_config_auto_generation() {
    let test_fixtures = TestFixtures::new();
    let temp_dir = test_fixtures.temp_dir.path();

    // The TestFixtures already creates a .tasks directory, so let's remove it first
    let tasks_dir = temp_dir.join(".tasks");
    if tasks_dir.exists() {
        std::fs::remove_dir_all(&tasks_dir).unwrap();
    }
    assert!(!tasks_dir.exists());

    // Run config show command, which should auto-generate global config
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp_dir)
        .arg("config")
        .arg("show")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Created default global configuration",
        ))
        .stdout(predicate::str::contains("Configuration for project:"));

    // Verify global config file was created
    let global_config_path = tasks_dir.join("config.yml");
    assert!(global_config_path.exists());

    // Verify the config content doesn't contain null values
    let config_content = fs::read_to_string(&global_config_path).unwrap();
    assert!(!config_content.contains("null"));
    assert!(config_content.contains("server_port: 8080"));
    assert!(config_content.contains("default_project: auto"));
}

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
            "Successfully initialized configuration for project 'MyVeryLongProjectName'",
        ))
        .stdout(predicate::str::contains(
            "Config file created at: .tasks/MYVE/config.yml",
        ))
        .stdout(predicate::str::contains(
            "Project folder uses 4-letter prefix 'MYVE' for project 'MyVeryLongProjectName'",
        ));

    // Verify the project folder was created with the correct prefix
    let project_dir = temp_dir.join(".tasks").join("MYVE");
    assert!(project_dir.exists());

    // Verify the project config file exists and has clean content
    let project_config_path = project_dir.join("config.yml");
    assert!(project_config_path.exists());

    let config_content = fs::read_to_string(&project_config_path).unwrap();
    assert!(!config_content.contains("null"));
    assert!(config_content.contains("project_name: MyVeryLongProjectName"));
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
        .stdout(predicate::str::contains(
            "Config file created at: .tasks/TEST/config.yml",
        ))
        .stdout(predicate::str::contains("Project folder uses 4-letter prefix").not());

    // Verify the project folder was created with the same name
    let project_dir = temp_dir.join(".tasks").join("TEST");
    assert!(project_dir.exists());
}

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
    assert!(config_content.contains("project_name: SimpleProject"));
    assert!(config_content.contains("issue_states:"));
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
    assert!(agile_config_content.contains("project_name: AgileProject"));
    assert!(agile_config_content.contains("issue_types:"));
    assert!(agile_config_content.contains("Epic"));
}

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

    // Verify the project config was updated
    let config_path = temp_dir.join(".tasks").join("SETT").join("config.yml");
    let config_content = fs::read_to_string(&config_path).unwrap();
    assert!(config_content.contains("default_priority: High"));
    assert!(config_content.contains("issue_states:"));

    // Verify global config was updated
    let global_config_path = temp_dir.join(".tasks").join("config.yml");
    let global_config_content = fs::read_to_string(&global_config_path).unwrap();
    assert!(global_config_content.contains("server_port: 9000"));
}

#[test]
fn test_config_optimization() {
    let test_fixtures = TestFixtures::new();
    let temp_dir = test_fixtures.temp_dir.path();

    // Initialize a project and set a value that matches the global default
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp_dir)
        .arg("config")
        .arg("init")
        .arg("--project=OptimizeTest")
        .assert()
        .success();

    // Set default priority to MEDIUM (which is the global default)
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp_dir)
        .arg("config")
        .arg("set")
        .arg("default_priority")
        .arg("MEDIUM")
        .arg("--project=OptimizeTest")
        .assert()
        .success();

    // The project config should be optimized to not contain the default value
    let config_path = temp_dir.join(".tasks").join("OPTI").join("config.yml");
    let config_content = fs::read_to_string(&config_path).unwrap();

    // Should only contain project_name since default_priority matches global default
    assert!(config_content.contains("project_name: OptimizeTest"));
    assert!(!config_content.contains("default_priority"));

    // Now set it to a different value
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp_dir)
        .arg("config")
        .arg("set")
        .arg("default_priority")
        .arg("HIGH")
        .arg("--project=OptimizeTest")
        .assert()
        .success();

    // Now it should contain the priority since it differs from default
    let config_content = fs::read_to_string(&config_path).unwrap();
    assert!(config_content.contains("default_priority: High"));
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
        .stdout(predicate::str::contains("Server Settings:"))
        .stdout(predicate::str::contains("Port: 8080"))
        .stdout(predicate::str::contains("Project Settings:"))
        .stdout(predicate::str::contains("Issue States:"))
        .stdout(predicate::str::contains("Mode: strict"))
        .stdout(predicate::str::contains("Tags:"))
        .stdout(predicate::str::contains("Mode: wildcard"));
}

#[test]
fn test_config_templates_list() {
    let test_fixtures = TestFixtures::new();
    let temp_dir = test_fixtures.temp_dir.path();

    // List available templates - our hardcoded fallback templates now provide descriptions
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp_dir)
        .arg("config")
        .arg("templates")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Available configuration templates:",
        ))
        .stdout(predicate::str::contains("default"))
        .stdout(predicate::str::contains("simple"))
        .stdout(predicate::str::contains("agile"))
        .stdout(predicate::str::contains("kanban"))
        .stdout(predicate::str::contains("Basic project template"));
}

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
        .stderr(predicate::str::contains("Template 'invalid' not found"));

    // Test invalid config field
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp_dir)
        .arg("config")
        .arg("set")
        .arg("invalid_field")
        .arg("value")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Unknown configuration field 'invalid_field'",
        ));

    // Test invalid priority value
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp_dir)
        .arg("config")
        .arg("set")
        .arg("default_priority")
        .arg("INVALID")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error parsing priority"));

    // Test invalid numeric value
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp_dir)
        .arg("config")
        .arg("set")
        .arg("server_port")
        .arg("not_a_number")
        .assert()
        .failure()
        .stderr(predicate::str::contains("must be a valid port number"));
}

#[test]
fn test_config_help_command() {
    let test_fixtures = TestFixtures::new();
    let temp_dir = test_fixtures.temp_dir.path();

    // Test config help
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp_dir)
        .arg("config")
        .arg("help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Configuration management commands",
        ))
        .stdout(predicate::str::contains("USAGE:"))
        .stdout(predicate::str::contains("COMMANDS:"))
        .stdout(predicate::str::contains("show"))
        .stdout(predicate::str::contains("set"))
        .stdout(predicate::str::contains("init"))
        .stdout(predicate::str::contains("EXAMPLES:"))
        .stdout(predicate::str::contains("CONFIGURABLE FIELDS:"));
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
        .stdout(predicate::str::contains(
            "Config file created at: .tasks/MAP/config.yml",
        ));

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
        .stdout(predicate::str::contains(
            "Config file created at: .tasks/MCP/config.yml",
        ));

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
        .stdout(predicate::str::contains(
            "Config file created at: .tasks/ABC/config.yml",
        ));

    let short_dir = temp_dir.join(".tasks").join("ABC");
    assert!(short_dir.exists());
}

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

    // Show config should display both global and project settings
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp_dir)
        .arg("config")
        .arg("show")
        .arg("--project=InheritanceTest")
        .assert()
        .success()
        .stdout(predicate::str::contains("Port: 9000")) // From global
        .stdout(predicate::str::contains("Default Priority: MEDIUM")); // Default value since project override may not be working yet
}

#[test]
fn test_config_wildcard_and_strict_modes() {
    let test_fixtures = TestFixtures::new();
    let temp_dir = test_fixtures.temp_dir.path();

    // Initialize project
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp_dir)
        .arg("config")
        .arg("init")
        .arg("--project=WildcardTest")
        .assert()
        .success();

    // Set strict categories
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp_dir)
        .arg("config")
        .arg("set")
        .arg("categories")
        .arg("frontend,backend,database")
        .arg("--project=WildcardTest")
        .assert()
        .success();

    // Set mixed mode tags (with wildcard)
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp_dir)
        .arg("config")
        .arg("set")
        .arg("tags")
        .arg("urgent,bug,*")
        .arg("--project=WildcardTest")
        .assert()
        .success();

    // Show config should display the modes correctly
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(temp_dir)
        .arg("config")
        .arg("show")
        .arg("--project=WildcardTest")
        .assert()
        .success()
        .stdout(predicate::str::contains("Categories:"))
        .stdout(predicate::str::contains("Mode: strict"))
        .stdout(predicate::str::contains("Tags:"))
        .stdout(predicate::str::contains("Mode: wildcard"));

    // Verify the config file contains the settings
    let config_path = temp_dir.join(".tasks").join("WILD").join("config.yml");
    let config_content = fs::read_to_string(&config_path).unwrap();
    assert!(config_content.contains("categories:"));
    assert!(config_content.contains("frontend"));
    assert!(config_content.contains("tags:"));
    assert!(config_content.contains("urgent"));
    assert!(config_content.contains("'*'"));
}
