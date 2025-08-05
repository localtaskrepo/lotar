use assert_cmd::Command;
use predicates::prelude::*;
use serde_yaml;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use lotar::config::types::{GlobalConfig, ProjectConfig, StringConfigField, ConfigurableField};
use lotar::types::{Priority, TaskStatus, TaskType};

/// Comprehensive test utilities for the new CLI system
pub struct CliTestHarness {
    pub temp_dir: TempDir,
    pub tasks_dir: PathBuf,
}

impl CliTestHarness {
    /// Create a new test harness with isolated temporary directory
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let tasks_dir = temp_dir.path().join(".tasks");
        fs::create_dir_all(&tasks_dir).expect("Failed to create tasks directory");

        Self {
            temp_dir,
            tasks_dir,
        }
    }

    /// Get the root path for the test environment
    pub fn root_path(&self) -> &Path {
        self.temp_dir.path()
    }

    /// Get the tasks directory path
    pub fn tasks_path(&self) -> &Path {
        &self.tasks_dir
    }

    /// Create a CLI command with the test environment set up
    pub fn cmd(&self) -> Command {
        let mut cmd = Command::cargo_bin("lotar").expect("Failed to get lotar binary");
        cmd.current_dir(self.root_path());
        cmd
    }

    /// Set up global configuration with custom settings
    pub fn setup_global_config(&self, config: GlobalConfig) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = self.tasks_dir.join("config.yml");
        let config_yaml = serde_yaml::to_string(&config)?;
        fs::write(config_path, config_yaml)?;
        Ok(())
    }

    /// Set up a project-specific configuration
    pub fn setup_project_config(&self, project: &str, config: ProjectConfig) -> Result<(), Box<dyn std::error::Error>> {
        let project_dir = self.tasks_dir.join(project);
        fs::create_dir_all(&project_dir)?;
        let config_path = project_dir.join("config.yml");
        let config_yaml = serde_yaml::to_string(&config)?;
        fs::write(config_path, config_yaml)?;
        Ok(())
    }

    /// Create a default global configuration for testing
    pub fn default_global_config(&self) -> GlobalConfig {
        GlobalConfig {
            server_port: 8080,
            default_prefix: "TEST".to_string(),
            issue_states: ConfigurableField {
                values: vec![TaskStatus::Todo, TaskStatus::InProgress, TaskStatus::Done],
            },
            issue_types: ConfigurableField {
                values: vec![TaskType::Feature, TaskType::Bug, TaskType::Chore],
            },
            issue_priorities: ConfigurableField {
                values: vec![Priority::Low, Priority::Medium, Priority::High],
            },
            categories: StringConfigField::new_wildcard(),
            tags: StringConfigField::new_wildcard(),
            default_assignee: None,
            default_priority: Priority::Medium,
            custom_fields: StringConfigField::new_wildcard(),
        }
    }

    /// Create a strict project configuration for testing validation
    pub fn strict_project_config(&self, project_name: &str, custom_fields: Vec<String>) -> ProjectConfig {
        ProjectConfig {
            project_name: project_name.to_string(),
            issue_states: Some(ConfigurableField {
                values: vec![TaskStatus::Todo, TaskStatus::Done],
            }),
            issue_types: Some(ConfigurableField {
                values: vec![TaskType::Feature, TaskType::Bug],
            }),
            issue_priorities: Some(ConfigurableField {
                values: vec![Priority::Low, Priority::High],
            }),
            categories: Some(StringConfigField::new_strict(vec![
                "frontend".to_string(),
                "backend".to_string(),
            ])),
            tags: Some(StringConfigField::new_wildcard()),
            default_assignee: None,
            default_priority: Some(Priority::Low), // Use an allowed priority
            custom_fields: Some(StringConfigField::new_strict(custom_fields)),
        }
    }

    /// Set up a complete test environment with global + project configs
    pub fn setup_test_environment(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Set up global config
        self.setup_global_config(self.default_global_config())?;

        // Set up a strict project for validation testing
        let strict_config = self.strict_project_config("PROJ", vec![
            "epic".to_string(),
            "sprint".to_string(),
            "story_points".to_string(),
        ]);
        self.setup_project_config("PROJ", strict_config)?;

        Ok(())
    }

    /// Execute an add command and return the result
    pub fn add_task(&self, title: &str) -> assert_cmd::assert::Assert {
        self.cmd()
            .args(["add", title])
            .assert()
    }

    /// Execute an add command with project override
    pub fn add_task_to_project(&self, project: &str, title: &str) -> assert_cmd::assert::Assert {
        self.cmd()
            .args(["--project", project, "add", title])
            .assert()
    }

    /// Execute an add command with custom fields
    /// This is a utility method designed for future test expansion
    #[allow(dead_code)]
    pub fn add_task_with_fields(&self, title: &str, fields: &[(&str, &str)]) -> assert_cmd::assert::Assert {
        let mut cmd = self.cmd();
        cmd.args(["add", title]);
        for (key, value) in fields {
            cmd.args(["--field", &format!("{}={}", key, value)]);
        }
        cmd.assert()
    }

    /// Execute an add command with project and custom fields
    pub fn add_task_to_project_with_fields(&self, project: &str, title: &str, fields: &[(&str, &str)]) -> assert_cmd::assert::Assert {
        let mut cmd = self.cmd();
        cmd.args(["--project", project, "add", title]);
        for (key, value) in fields {
            cmd.args(["--field", &format!("{}={}", key, value)]);
        }
        cmd.assert()
    }

    /// Execute a list command
    pub fn list_tasks(&self) -> assert_cmd::assert::Assert {
        self.cmd()
            .args(["list"])
            .assert()
    }

    /// Execute a list command with project filter
    pub fn list_tasks_for_project(&self, project: &str) -> assert_cmd::assert::Assert {
        self.cmd()
            .args(["--project", project, "list"])
            .assert()
    }

    /// Execute a status change command
    pub fn change_status(&self, task_id: &str, status: &str) -> assert_cmd::assert::Assert {
        self.cmd()
            .args(["status", task_id, status])
            .assert()
    }

    /// Execute a status change command with project override
    pub fn change_status_for_project(&self, project: &str, task_id: &str, status: &str) -> assert_cmd::assert::Assert {
        self.cmd()
            .args(["--project", project, "status", task_id, status])
            .assert()
    }

    /// Get the contents of a task file
    pub fn get_task_contents(&self, project: &str, task_id: &str) -> Result<String, std::io::Error> {
        let task_path = self.tasks_dir.join(project).join(format!("{}.yml", task_id));
        fs::read_to_string(task_path)
    }

    /// Check if a task file exists
    pub fn task_exists(&self, project: &str, task_id: &str) -> bool {
        let task_path = self.tasks_dir.join(project).join(format!("{}.yml", task_id));
        task_path.exists()
    }

    /// Count tasks in a project directory
    pub fn count_tasks(&self, project: &str) -> usize {
        let project_dir = self.tasks_dir.join(project);
        if !project_dir.exists() {
            return 0;
        }
        
        fs::read_dir(project_dir)
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok())
                    .filter(|entry| {
                        entry.path().extension()
                            .and_then(|ext| ext.to_str())
                            .map(|ext| ext == "yml")
                            .unwrap_or(false)
                    })
                    .count()
            })
            .unwrap_or(0)
    }

    /// Count all tasks across all projects
    pub fn count_all_tasks(&self) -> usize {
        fs::read_dir(&self.tasks_dir)
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok())
                    .filter(|entry| entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
                    .map(|project_dir| {
                        let project_name = project_dir.file_name().to_string_lossy().to_string();
                        self.count_tasks(&project_name)
                    })
                    .sum()
            })
            .unwrap_or(0)
    }

    /// Get list of all projects (directories in .tasks)
    pub fn get_projects(&self) -> Vec<String> {
        fs::read_dir(&self.tasks_dir)
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok())
                    .filter(|entry| entry.path().is_dir())
                    .filter_map(|entry| {
                        entry.file_name().to_str().map(|s| s.to_string())
                    })
                    .collect()
            })
            .unwrap_or_else(|_| vec![])
    }
}

/// Assertion helpers for CLI testing
pub struct CliAssertions;

impl CliAssertions {
    /// Assert that command succeeded and created a task with expected ID pattern
    pub fn assert_task_created(assert: assert_cmd::assert::Assert, expected_project: &str) -> assert_cmd::assert::Assert {
        assert
            .success()
            .stdout(predicate::str::contains("✅ Created task:"))
            .stdout(predicate::str::contains(expected_project))
    }

    /// Assert that command failed with validation error
    pub fn assert_validation_error(assert: assert_cmd::assert::Assert, field_name: &str) -> assert_cmd::assert::Assert {
        assert
            .failure()
            .stdout(predicate::str::contains("validation failed"))
            .stdout(predicate::str::contains(field_name))
    }

    /// Assert that command failed with custom field validation error
    pub fn assert_custom_field_error(assert: assert_cmd::assert::Assert, field_name: &str) -> assert_cmd::assert::Assert {
        assert
            .failure()
            .stdout(predicate::str::contains("Custom field validation failed"))
            .stdout(predicate::str::contains(field_name))
    }

    /// Assert that list command shows expected number of tasks
    pub fn assert_task_count(assert: assert_cmd::assert::Assert, count: usize) -> assert_cmd::assert::Assert {
        if count == 0 {
            assert
                .success()
                .stdout(predicate::str::contains("No tasks found"))
        } else {
            // With the current output format, tasks appear as "  TASKID - Title [STATUS] (Priority)"
            // We look for lines that start with "  " and contain " - " and "[" and "]"
            let assert = assert.success();
            
            // Get the stdout to count tasks
            let output = assert.get_output();
            let stdout = String::from_utf8(output.stdout.clone()).unwrap();
            let task_lines = stdout.lines()
                .filter(|line| line.trim_start().starts_with(char::is_uppercase) && 
                              line.contains(" - ") && 
                              line.contains("[") && 
                              line.contains("]"))
                .count();
            
            if task_lines != count {
                panic!("Expected {} tasks but found {} task lines in output:\n{}", count, task_lines, stdout);
            }
            
            assert
        }
    }

    /// Assert that status change succeeded
    pub fn assert_status_changed(assert: assert_cmd::assert::Assert, _task_id: &str) -> assert_cmd::assert::Assert {
        assert
            .success()
            .stdout(predicate::str::contains("✅ Status changed successfully"))
    }
}

/// Test data builders for common test scenarios
pub struct TestDataBuilder;

impl TestDataBuilder {
    /// Build test environment for basic functionality testing
    pub fn basic_environment() -> CliTestHarness {
        let harness = CliTestHarness::new();
        harness.setup_test_environment().expect("Failed to set up test environment");
        harness
    }

    /// Build test environment with multiple projects for complex testing
    pub fn multi_project_environment() -> Result<CliTestHarness, Box<dyn std::error::Error>> {
        let harness = CliTestHarness::new();
        
        // Global config
        harness.setup_global_config(harness.default_global_config())?;

        // Project A: Strict validation
        let project_a = harness.strict_project_config("PROJA", vec![
            "epic".to_string(),
            "feature".to_string(),
        ]);
        harness.setup_project_config("PROJA", project_a)?;

        // Project B: Different strict validation
        let project_b = harness.strict_project_config("PROJB", vec![
            "module".to_string(),
            "component".to_string(),
            "version".to_string(),
        ]);
        harness.setup_project_config("PROJB", project_b)?;

        // Project C: Wildcard (inherits global)
        let project_c = ProjectConfig {
            project_name: "PROJC".to_string(),
            issue_states: None,
            issue_types: None,
            issue_priorities: None,
            categories: None,
            tags: None,
            default_assignee: None,
            default_priority: None,
            custom_fields: Some(StringConfigField::new_wildcard()),
        };
        harness.setup_project_config("PROJC", project_c)?;

        Ok(harness)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_harness_creation() {
        let harness = CliTestHarness::new();
        assert!(harness.tasks_path().exists());
    }

    #[test]
    fn test_environment_setup() {
        let harness = CliTestHarness::new();
        harness.setup_test_environment().expect("Setup should succeed");
        
        // Check global config exists
        assert!(harness.tasks_path().join("config.yml").exists());
        
        // Check project config exists
        assert!(harness.tasks_path().join("PROJ/config.yml").exists());
    }

    #[test]
    fn test_multi_project_environment() {
        let harness = TestDataBuilder::multi_project_environment()
            .expect("Multi-project setup should succeed");
        
        let projects = harness.get_projects();
        assert!(projects.contains(&"PROJA".to_string()));
        assert!(projects.contains(&"PROJB".to_string()));
        assert!(projects.contains(&"PROJC".to_string()));
    }
}
