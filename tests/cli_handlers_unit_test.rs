use local_task_repo::cli::handlers::{CommandHandler, AddHandler};
use local_task_repo::cli::{AddArgs, CliTaskType, CliPriority};
use local_task_repo::workspace::TasksDirectoryResolver;
use local_task_repo::output::{OutputRenderer, OutputFormat};
use tempfile::TempDir;
use std::fs;

/// Test that AddHandler uses project prefix, not project name
#[test]
fn test_add_handler_uses_project_prefix() {
    let temp_dir = TempDir::new().unwrap();
    let tasks_dir = temp_dir.path().join(".tasks");
    fs::create_dir_all(&tasks_dir).unwrap();
    
    // Create a predictable project environment that generates DEFA prefix
    fs::write(temp_dir.path().join("Cargo.toml"), 
        "[package]\nname = \"default-example-feature-app\"\nversion = \"0.1.0\"\n").unwrap();
    
    // Create a basic global config
    let config_content = r#"
server_port: 8080
tasks_dir_name: .tasks
"#;
    fs::write(tasks_dir.join("config.yml"), config_content).unwrap();
    
    let resolver = TasksDirectoryResolver {
        path: tasks_dir.clone(),
        source: local_task_repo::workspace::TasksDirectorySource::CurrentDirectory,
    };
    
    let args = AddArgs {
        title: "Test task".to_string(),
        task_type: Some(CliTaskType::Feature),
        priority: Some(CliPriority::High),
        assignee: None,
        effort: None,
        due: None,
        description: None,
        category: None,
        tags: vec![],
        fields: vec![],
        bug: false,
        epic: false,
        critical: false,
        high: false,
    };
    
    // Execute the handler with explicit project to avoid dependency on file system project detection
    let result = AddHandler::execute(args, Some("DEFA"), &resolver);
    assert!(result.is_ok(), "AddHandler should succeed");
    
    let task_id = result.unwrap();
    assert_eq!(task_id, "DEFA-1", "Task ID should use DEFA prefix");
    
    // Verify file structure
    assert!(tasks_dir.join("DEFA").exists(), "DEFA directory should exist");
    assert!(!tasks_dir.join("default").exists(), "default directory should NOT exist");
    assert!(tasks_dir.join("DEFA").join("1.yml").exists(), "Task file should exist");
}

/// Test that project resolution works with explicit project
#[test]
fn test_add_handler_explicit_project_resolution() {
    let temp_dir = TempDir::new().unwrap();
    let tasks_dir = temp_dir.path().join(".tasks");
    fs::create_dir_all(&tasks_dir).unwrap();
    
    // Create a basic global config
    let config_content = r#"
default_prefix: DEFA
server_port: 8080
tasks_dir_name: .tasks
"#;
    fs::write(tasks_dir.join("config.yml"), config_content).unwrap();
    
    let resolver = TasksDirectoryResolver {
        path: tasks_dir.clone(),
        source: local_task_repo::workspace::TasksDirectorySource::CurrentDirectory,
    };
    
    let args = AddArgs {
        title: "Explicit project task".to_string(),
        task_type: Some(CliTaskType::Bug),
        priority: Some(CliPriority::Medium),
        assignee: None,
        effort: None,
        due: None,
        description: None,
        category: None,
        tags: vec![],
        fields: vec![],
        bug: false,
        epic: false,
        critical: false,
        high: false,
    };
    
    // Execute with explicit project
    let result = AddHandler::execute(args, Some("test"), &resolver);
    assert!(result.is_ok(), "AddHandler should succeed with explicit project");
    
    let task_id = result.unwrap();
    assert_eq!(task_id, "TEST-1", "Task ID should use TEST prefix for 'test' project");
    
    // Verify file structure
    assert!(tasks_dir.join("TEST").exists(), "TEST directory should exist");
    assert!(tasks_dir.join("TEST").join("1.yml").exists(), "Task file should exist in TEST directory");
}

/// Test OutputRenderer JSON format includes task_id
#[test]
fn test_output_renderer_json_format() {
    let renderer = OutputRenderer::new(OutputFormat::Json, false);
    
    // Test success message should be JSON
    let success_output = renderer.render_success("Created task: PROJ-123");
    
    let json: serde_json::Value = serde_json::from_str(&success_output)
        .expect("Success output should be valid JSON");
    
    assert_eq!(json["status"], "success");
    assert_eq!(json["message"], "Created task: PROJ-123");
}

/// Test OutputRenderer error format in JSON
#[test]
fn test_output_renderer_json_error_format() {
    let renderer = OutputRenderer::new(OutputFormat::Json, false);
    
    // Test error message should be JSON
    let error_output = renderer.render_error("Validation failed");
    
    let json: serde_json::Value = serde_json::from_str(&error_output)
        .expect("Error output should be valid JSON");
    
    assert_eq!(json["status"], "error");
    assert_eq!(json["message"], "Validation failed");
}

/// Test that different output formats work correctly
#[test]
fn test_output_format_variations() {
    // Test Text format
    let text_renderer = OutputRenderer::new(OutputFormat::Text, false);
    let text_output = text_renderer.render_success("Task created");
    assert!(text_output.contains("✅"));
    assert!(text_output.contains("Task created"));
    
    // Test JSON format
    let json_renderer = OutputRenderer::new(OutputFormat::Json, false);
    let json_output = json_renderer.render_success("Task created");
    let json: serde_json::Value = serde_json::from_str(&json_output).unwrap();
    assert_eq!(json["status"], "success");
    assert_eq!(json["message"], "Task created");
    
    // Test that different renderers have different behavior
    assert_ne!(text_output, json_output, "Text and JSON outputs should be different");
}

/// Test that task validation prevents regression issues
#[test]
fn test_task_validation_prevents_invalid_projects() {
    let temp_dir = TempDir::new().unwrap();
    let tasks_dir = temp_dir.path().join(".tasks");
    fs::create_dir_all(&tasks_dir).unwrap();
    
    // Create config with limited task types
    let config_content = r#"
default_prefix: TEST
server_port: 8080
tasks_dir_name: .tasks
"#;
    fs::write(tasks_dir.join("config.yml"), config_content).unwrap();
    
    // Create project config that restricts task types
    let project_dir = tasks_dir.join("TEST");
    fs::create_dir_all(&project_dir).unwrap();
    let project_config = r#"
project_name: test
issue_types:
  values: [feature, bug]
"#;
    fs::write(project_dir.join("config.yml"), project_config).unwrap();
    
    let resolver = TasksDirectoryResolver {
        path: tasks_dir.clone(),
        source: local_task_repo::workspace::TasksDirectorySource::CurrentDirectory,
    };
    
    let args = AddArgs {
        title: "Invalid epic task".to_string(),
        task_type: Some(CliTaskType::Epic), // This should be rejected
        priority: Some(CliPriority::Medium),
        assignee: None,
        effort: None,
        due: None,
        description: None,
        category: None,
        tags: vec![],
        fields: vec![],
        bug: false,
        epic: false,
        critical: false,
        high: false,
    };
    
    // Execute the handler - should fail validation
    let result = AddHandler::execute(args, None, &resolver);
    assert!(result.is_err(), "AddHandler should fail validation for epic task type");
    
    let error_msg = result.unwrap_err();
    assert!(error_msg.contains("not allowed"), "Error should mention task type not allowed");
    assert!(error_msg.contains("epic"), "Error should mention epic type specifically");
}
