use assert_cmd::Command;
use predicates::prelude::*;
use serde_json;

mod common;

#[cfg(test)]
mod tag_filtering_tests {
    use super::*;
    use common::TestFixtures;

    #[test]
    fn test_tag_filtering_or_logic() {
        let fixtures = TestFixtures::new();
        let temp_dir = fixtures.temp_dir.path();

        // Create tasks with different tags
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Backend task")
            .arg("--tag=urgent")
            .arg("--tag=backend")
            .arg("--tag=feature")
            .assert()
            .success();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Frontend task")
            .arg("--tag=frontend")
            .arg("--tag=testing")
            .assert()
            .success();

        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Database task")
            .arg("--tag=database")
            .arg("--tag=backend")
            .assert()
            .success();

        // Test single tag filtering
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let output = cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--tag=urgent")
            .arg("--format=json")
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let stdout = String::from_utf8_lossy(&output);
        let json: serde_json::Value = serde_json::from_str(&stdout).expect("Valid JSON");
        let tasks = json["tasks"].as_array().expect("Tasks array");
        assert_eq!(tasks.len(), 1, "Should find exactly one task with 'urgent' tag");
        assert!(tasks[0]["tags"].as_array().unwrap().contains(&serde_json::Value::String("urgent".to_string())));

        // Test multiple tag filtering (OR logic) - should return tasks with ANY of the tags
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let output = cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--tag=urgent")
            .arg("--tag=frontend")
            .arg("--format=json")
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let stdout = String::from_utf8_lossy(&output);
        let json: serde_json::Value = serde_json::from_str(&stdout).expect("Valid JSON");
        let tasks = json["tasks"].as_array().expect("Tasks array");
        assert_eq!(tasks.len(), 2, "Should find two tasks: one with 'urgent' and one with 'frontend' tag");
        
        // Verify tasks have expected tags
        let task_titles: Vec<String> = tasks.iter()
            .map(|t| t["title"].as_str().unwrap().to_string())
            .collect();
        assert!(task_titles.contains(&"Backend task".to_string()));
        assert!(task_titles.contains(&"Frontend task".to_string()));

        // Test overlapping tags
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let output = cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--tag=backend")
            .arg("--tag=testing")
            .arg("--format=json")
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let stdout = String::from_utf8_lossy(&output);
        let json: serde_json::Value = serde_json::from_str(&stdout).expect("Valid JSON");
        let tasks = json["tasks"].as_array().expect("Tasks array");
        assert_eq!(tasks.len(), 3, "Should find three tasks: two with 'backend' and one with 'testing' tag");
    }

    #[test]
    fn test_task_display_completeness() {
        let fixtures = TestFixtures::new();
        let temp_dir = fixtures.temp_dir.path();

        // Create a task with comprehensive properties
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Complete task")
            .arg("--type=feature")
            .arg("--priority=high")
            .arg("--tag=urgent")
            .arg("--tag=frontend")
            .arg("--description=This is a detailed description")
            .arg("--category=web")
            .arg("--assignee=developer@example.com")
            .arg("--effort=5d")
            .arg("--field=custom=value")
            .assert()
            .success();

        // Test that all fields are displayed in JSON output
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let output = cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--format=json")
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let stdout = String::from_utf8_lossy(&output);
        let json: serde_json::Value = serde_json::from_str(&stdout).expect("Valid JSON");
        let tasks = json["tasks"].as_array().expect("Tasks array");
        assert_eq!(tasks.len(), 1, "Should find exactly one task");
        
        let task = &tasks[0];
        
        // Verify all standard fields are present
        assert!(task.get("id").is_some(), "Should have id field");
        assert!(task.get("title").is_some(), "Should have title field");
        assert!(task.get("status").is_some(), "Should have status field");
        assert!(task.get("priority").is_some(), "Should have priority field");
        assert!(task.get("task_type").is_some(), "Should have task_type field");
        assert!(task.get("description").is_some(), "Should have description field");
        assert!(task.get("assignee").is_some(), "Should have assignee field");
        assert!(task.get("project").is_some(), "Should have project field");
        assert!(task.get("due_date").is_some(), "Should have due_date field");
        assert!(task.get("effort").is_some(), "Should have effort field");
        assert!(task.get("category").is_some(), "Should have category field");
        assert!(task.get("tags").is_some(), "Should have tags field");
        assert!(task.get("created").is_some(), "Should have created field");
        assert!(task.get("modified").is_some(), "Should have modified field");
        assert!(task.get("custom_fields").is_some(), "Should have custom_fields field");

        // Verify specific values
        assert_eq!(task["title"], "Complete task");
        assert_eq!(task["priority"], "HIGH");
        assert_eq!(task["task_type"], "feature");
        assert_eq!(task["description"], "This is a detailed description");
        assert_eq!(task["category"], "web");
        assert_eq!(task["assignee"], "developer@example.com");
        assert_eq!(task["effort"], "5d");
        
        // Verify tags array
        let tags = task["tags"].as_array().expect("Tags should be an array");
        assert_eq!(tags.len(), 2, "Should have 2 tags");
        assert!(tags.contains(&serde_json::Value::String("urgent".to_string())));
        assert!(tags.contains(&serde_json::Value::String("frontend".to_string())));

        // Verify project is extracted from task ID
        assert!(task["project"].as_str().is_some(), "Project should be a string");
        assert!(!task["project"].is_null(), "Project should not be null");

        // Verify custom fields
        let custom_fields = task["custom_fields"].as_object().expect("Custom fields should be an object");
        assert!(custom_fields.contains_key("custom"), "Should have custom field");
    }

    #[test]
    fn test_text_format_includes_project() {
        let fixtures = TestFixtures::new();
        let temp_dir = fixtures.temp_dir.path();

        // Create a task
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("add")
            .arg("Test task")
            .arg("--tag=test")
            .assert()
            .success();

        // Test text format output
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("list")
            .arg("--tag=test")
            .assert()
            .success()
            .stdout(predicate::str::contains("Found 1 task(s)"))
            .stdout(predicate::str::contains("Test task"));
    }
}
