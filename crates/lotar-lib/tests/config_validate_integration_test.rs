use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn get_test_binary_path() -> std::path::PathBuf {
    crate::common::lotar_bin()
}

#[test]
fn test_config_validate_global_valid() {
    let temp_dir = TempDir::new().unwrap();
    let tasks_dir = temp_dir.path().join(".tasks");

    // Create a valid global config
    fs::create_dir_all(&tasks_dir).unwrap();
    fs::write(
        tasks_dir.join("config.yml"),
        r#"
server.port: 8080
default.project: "TEST"
issue.states:
    - Todo
    - InProgress  
    - Done
issue.types:
    - Feature
    - Bug
issue.priorities:
    - Low
    - Medium
    - High
default.priority: Medium
default.status: Todo
"#,
    )
    .unwrap();

    let output = Command::new(get_test_binary_path())
        .args(["config", "validate", "--global"])
        .arg("--tasks-dir")
        .arg(&tasks_dir)
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success(), "Command failed with stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Validating global configuration"));
    assert!(stdout.contains("✅ All configurations are valid"));
}

#[test]
fn test_config_validate_global_with_warnings() {
    let temp_dir = TempDir::new().unwrap();
    let tasks_dir = temp_dir.path().join(".tasks");

    // Create a config with privileged port (should trigger warning)
    fs::create_dir_all(&tasks_dir).unwrap();
    fs::write(
        tasks_dir.join("config.yml"),
        r#"
server.port: 80
default.project: "TEST"
issue.states:
    - Todo
    - Done
issue.types:
    - Feature
issue.priorities:
    - Medium
default.priority: Medium
default.status: Todo
"#,
    )
    .unwrap();

    let output = Command::new(get_test_binary_path())
        .args(["config", "validate", "--global"])
        .arg("--tasks-dir")
        .arg(&tasks_dir)
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Global Config Validation Results"));
    assert!(stdout.contains("⚠️"));
    assert!(stdout.contains("Port 80 may require elevated privileges"));
    assert!(stdout.contains("Consider using a port >= 1024"));
    assert!(stdout.contains("✅ All configurations are valid"));
}

#[test]
fn test_config_validate_global_errors_only() {
    let temp_dir = TempDir::new().unwrap();
    let tasks_dir = temp_dir.path().join(".tasks");

    // Create a config with warnings but no errors
    fs::create_dir_all(&tasks_dir).unwrap();
    fs::write(
        tasks_dir.join("config.yml"),
        r#"
server.port: 80
issue.states:
    - Todo
issue.types:
    - Feature
issue.priorities:
    - Medium
default.priority: Medium
"#,
    )
    .unwrap();

    let output = Command::new(get_test_binary_path())
        .args(["config", "validate", "--global", "--errors-only"])
        .arg("--tasks-dir")
        .arg(&tasks_dir)
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    // Should not show warnings when --errors-only is used
    assert!(!stdout.contains("⚠️"));
    assert!(!stdout.contains("Port 80 may require elevated privileges"));
    assert!(stdout.contains("✅ All configurations are valid"));
}

#[test]
fn test_config_validate_project_valid() {
    let temp_dir = TempDir::new().unwrap();
    let tasks_dir = temp_dir.path().join(".tasks");
    let project_dir = tasks_dir.join("TEST");

    // Create a valid project config
    fs::create_dir_all(&project_dir).unwrap();
    fs::write(
        project_dir.join("config.yml"),
        r#"
project.name: "Test Project"
issue.states:
    - Todo
    - InProgress
    - Done
issue.types:
    - Feature
    - Bug
issue.priorities:
    - Low
    - Medium
    - High
default.priority: Medium
default.status: Todo
default.assignee: "user@example.com"
"#,
    )
    .unwrap();

    let output = Command::new(get_test_binary_path())
        .args(["config", "validate", "--project=TEST"])
        .arg("--tasks-dir")
        .arg(&tasks_dir)
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Validating project configuration for 'Test Project (TEST)'"));
    assert!(stdout.contains("✅ All configurations are valid"));
}

#[test]
fn test_config_validate_project_not_found() {
    let temp_dir = TempDir::new().unwrap();
    let tasks_dir = temp_dir.path().join(".tasks");
    fs::create_dir_all(&tasks_dir).unwrap();

    let output = Command::new(get_test_binary_path())
        .args(["config", "validate", "--project=NONEXISTENT"])
        .arg("--tasks-dir")
        .arg(&tasks_dir)
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success());

    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("Project config file not found"));
}

#[test]
fn test_config_validate_invalid_yaml() {
    let temp_dir = TempDir::new().unwrap();
    let tasks_dir = temp_dir.path().join(".tasks");

    // Create invalid YAML
    fs::create_dir_all(&tasks_dir).unwrap();
    fs::write(
        tasks_dir.join("config.yml"),
        r#"
invalid_yaml: [unclosed bracket
server_port: 8080
"#,
    )
    .unwrap();

    let output = Command::new(get_test_binary_path())
        .args(["config", "validate", "--global"])
        .arg("--tasks-dir")
        .arg(&tasks_dir)
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success());

    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("Failed to parse global config"));
}

#[test]
fn test_config_validate_no_config_uses_defaults() {
    let temp_dir = TempDir::new().unwrap();
    let tasks_dir = temp_dir.path().join(".tasks");
    fs::create_dir_all(&tasks_dir).unwrap();
    // No config.yml file - should use defaults

    let output = Command::new(get_test_binary_path())
        .args(["config", "validate", "--global"])
        .arg("--tasks-dir")
        .arg(&tasks_dir)
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Validating global configuration"));
    assert!(stdout.contains("✅ All configurations are valid"));
}

#[test]
fn test_config_validate_both_global_and_project() {
    let temp_dir = TempDir::new().unwrap();
    let tasks_dir = temp_dir.path().join(".tasks");
    let project_dir = tasks_dir.join("TEST");

    // Create both global and project configs
    fs::create_dir_all(&project_dir).unwrap();

    fs::write(
        tasks_dir.join("config.yml"),
        r#"
server.port: 8080
issue.states: [Todo, Done]
issue.types: [Feature]
issue.priorities: [Medium]
default.priority: Medium
"#,
    )
    .unwrap();

    fs::write(
        project_dir.join("config.yml"),
        r#"
project.name: "Test Project"
default.assignee: "user@example.com"
"#,
    )
    .unwrap();

    let output = Command::new(get_test_binary_path())
        .args(["config", "validate"]) // No specific flags - should validate global
        .arg("--tasks-dir")
        .arg(&tasks_dir)
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Validating global configuration"));
    assert!(stdout.contains("✅ All configurations are valid"));
}

#[test]
fn test_config_validate_prefix_conflicts() {
    let temp_dir = TempDir::new().unwrap();
    let tasks_dir = temp_dir.path().join(".tasks");

    // Create existing project directories to cause conflicts
    fs::create_dir_all(tasks_dir.join("EXISTING")).unwrap();
    fs::create_dir_all(tasks_dir.join("TEST")).unwrap();

    // Create project config that might conflict
    let project_dir = tasks_dir.join("EXISTING");
    fs::write(
        project_dir.join("config.yml"),
        r#"
project.name: "Existing Project"
"#,
    )
    .unwrap();

    let output = Command::new(get_test_binary_path())
        .args(["config", "validate", "--project=EXISTING"])
        .arg("--tasks-dir")
        .arg(&tasks_dir)
        .output()
        .expect("Failed to execute command");

    // Should still succeed since EXISTING validates against itself
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Validating project configuration"));
}

/// DEV-883: a bad value in one agent profile of the home config skipped the whole home config, every profile with it,
/// while validate passed. Validate now fails naming the profile and key, without a contradicting success line (DEV-893),
/// and every load warns that it is ignored.
#[test]
fn test_config_validate_fails_on_unparsable_home_agent_profile() {
    let temp_dir = TempDir::new().unwrap();
    let tasks_dir = temp_dir.path().join(".tasks");
    let home = temp_dir.path().join("home");
    fs::create_dir_all(&tasks_dir).unwrap();
    fs::create_dir_all(&home).unwrap();
    fs::write(home.join(".lotar"), "agents:\n  good:\n    runner: claude\n  bad:\n    runner: claude\n    stall_timeout_seconds: notanumber\n").unwrap();
    let run = |args: &[&str]| {
        Command::new(get_test_binary_path())
            .args(args)
            .arg("--tasks-dir")
            .arg(&tasks_dir)
            .env("HOME", &home)
            .env_remove("RUST_TEST_THREADS")
            .env_remove("LOTAR_TEST_MODE")
            .env_remove("LOTAR_IGNORE_HOME_CONFIG")
            .env_remove("LOTAR_TEST_SILENT")
            .output()
            .expect("Failed to execute command")
    };

    let output = run(&["config", "validate"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "stdout: {stdout}\nstderr: {stderr}");
    assert!(format!("{stdout}{stderr}").contains("Invalid type for config field 'agents.bad.stall_timeout_seconds'"), "stdout: {stdout}\nstderr: {stderr}");
    assert!(!stdout.contains("All configurations are valid"), "{stdout}");
    // DEV-893: no success line right after the home config error.
    assert!(!format!("{stdout}{stderr}").contains("Global configuration is valid"), "stdout: {stdout}\nstderr: {stderr}");

    let output = run(&["--format", "json", "config", "show"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("is ignored: Invalid type for config field 'agents.bad.stall_timeout_seconds'"), "{stderr}");
}
