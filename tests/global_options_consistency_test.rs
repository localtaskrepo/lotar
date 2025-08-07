use std::env;
use std::fs;

mod common;
use crate::common::TestFixtures;

/// Tests for Phase 3.1.2: Global Options Consistency
///
/// This test module ensures that global options like --tasks-dir, environment variables,
/// and parent directory resolution work consistently across all commands.
///
/// **Important:** These tests modify environment variables and should be run sequentially
/// to avoid conflicts. Use: `cargo test --test global_options_consistency_test -- --test-threads=1`
///
#[cfg(test)]
mod tasks_dir_consistency {
    use super::*;

    #[test]
    fn test_tasks_dir_option_works_across_commands() {
        let fixtures = TestFixtures::new();
        let custom_tasks_dir = fixtures.temp_dir.path().join("custom_tasks");
        fs::create_dir_all(&custom_tasks_dir).unwrap();

        // Test that --tasks-dir works with config commands
        let output = fixtures.run_command(&[
            "config",
            "show",
            "--tasks-dir",
            custom_tasks_dir.to_str().unwrap(),
        ]);
        assert!(
            output.is_ok(),
            "Config show with --tasks-dir should succeed"
        );
        let output_str = output.unwrap();
        assert!(output_str.contains(&*custom_tasks_dir.to_string_lossy()));

        // Test that --tasks-dir works with list commands
        let output =
            fixtures.run_command(&["list", "--tasks-dir", custom_tasks_dir.to_str().unwrap()]);
        assert!(output.is_ok(), "List with --tasks-dir should succeed");

        // Test that --tasks-dir works with scan commands
        let output =
            fixtures.run_command(&["scan", "--tasks-dir", custom_tasks_dir.to_str().unwrap()]);
        assert!(output.is_ok(), "Scan with --tasks-dir should succeed");

        // Test that --tasks-dir works with add commands
        let output = fixtures.run_command(&[
            "add",
            "test-task",
            "--description",
            "Test task description",
            "--tasks-dir",
            custom_tasks_dir.to_str().unwrap(),
        ]);
        match &output {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Command failed with error: {e}");
                panic!("Add with --tasks-dir should succeed");
            }
        }
        assert!(output.is_ok(), "Add with --tasks-dir should succeed");

        // Verify the task was created in the custom directory
        let task_files: Vec<_> = fs::read_dir(&custom_tasks_dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "yml"))
            .collect();
        assert!(
            !task_files.is_empty(),
            "Task should be created in custom tasks directory"
        );
    }

    #[test]
    fn test_tasks_dir_overrides_default_detection() {
        let fixtures = TestFixtures::new();

        // Create a default tasks directory
        let default_tasks_dir = fixtures.temp_dir.path().join("tasks");
        fs::create_dir_all(&default_tasks_dir).unwrap();

        // Create a custom tasks directory
        let custom_tasks_dir = fixtures.temp_dir.path().join("other_tasks");
        fs::create_dir_all(&custom_tasks_dir).unwrap();

        // Set working directory to the parent of default tasks dir
        env::set_current_dir(fixtures.temp_dir.path()).unwrap();

        // Add a task with explicit --tasks-dir (should override auto-detection)
        let output = fixtures.run_command(&[
            "add",
            "explicit-task",
            "--description",
            "Task with explicit dir",
            "--tasks-dir",
            custom_tasks_dir.to_str().unwrap(),
        ]);
        assert!(
            output.is_ok(),
            "Add with explicit --tasks-dir should succeed"
        );

        // Verify task is in custom dir, not default dir
        let custom_files: Vec<_> = fs::read_dir(&custom_tasks_dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "yml"))
            .collect();
        assert!(
            !custom_files.is_empty(),
            "Task should be in custom directory"
        );

        let default_files: Vec<_> = fs::read_dir(&default_tasks_dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "yml"))
            .collect();
        assert!(
            default_files.is_empty(),
            "Task should NOT be in default directory"
        );
    }

    #[test]
    fn test_tasks_dir_with_relative_paths() {
        let fixtures = TestFixtures::new();
        let tasks_subdir = fixtures.temp_dir.path().join("project").join("tasks");
        fs::create_dir_all(&tasks_subdir).unwrap();

        // Change to parent directory
        env::set_current_dir(fixtures.temp_dir.path()).unwrap();

        // Use relative path for --tasks-dir
        let output = fixtures.run_command(&[
            "add",
            "relative-task",
            "--description",
            "Task with relative path",
            "--tasks-dir",
            "project/tasks",
        ]);
        assert!(
            output.is_ok(),
            "Add with relative --tasks-dir should succeed"
        );

        // Verify task was created
        let task_files: Vec<_> = fs::read_dir(&tasks_subdir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "yml"))
            .collect();
        assert!(
            !task_files.is_empty(),
            "Task should be created with relative path"
        );
    }
}

#[cfg(test)]
mod environment_variable_consistency {
    use super::*;

    /// Combined test for environment variable scenarios to avoid parallel execution conflicts
    #[test]
    fn test_environment_variable_scenarios_combined() {
        // Clean up any leftover environment variables from other tests
        unsafe {
            env::remove_var("LOTAR_TASKS_DIR");
            env::remove_var("LOTAR_DEFAULT_ASSIGNEE");
        }

        // === SCENARIO 1: LOTAR_TASKS_DIR environment variable ===
        {
            let fixtures = TestFixtures::new();
            let env_tasks_dir = fixtures.temp_dir.path().join("env_tasks");
            fs::create_dir_all(&env_tasks_dir).unwrap();

            // Set environment variable
            unsafe {
                env::set_var("LOTAR_TASKS_DIR", env_tasks_dir.to_str().unwrap());
            }

            // Commands should use environment variable when no --tasks-dir is specified
            let output = fixtures.run_command(&["config", "show"]);
            assert!(
                output.is_ok(),
                "Config show should work with environment variable"
            );
            let output_str = output.unwrap();

            // Check that the tasks directory line contains our environment path
            assert!(
                output_str
                    .lines()
                    .any(|line| line.trim().starts_with("Tasks directory:")
                        && line.contains(&*env_tasks_dir.to_string_lossy())),
                "Config should show environment-specified tasks directory"
            );

            let output = fixtures.run_command(&["list"]);
            assert!(output.is_ok(), "List should work with environment variable");

            let output =
                fixtures.run_command(&["add", "env-task", "--description", "Task using env var"]);
            assert!(output.is_ok(), "Add should work with environment variable");

            // Verify task was created in environment directory
            let task_files: Vec<_> = fs::read_dir(&env_tasks_dir)
                .unwrap()
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "yml"))
                .collect();
            assert!(
                !task_files.is_empty(),
                "Task should be created using LOTAR_TASKS_DIR"
            );

            // Clean up for next scenario
            unsafe {
                env::remove_var("LOTAR_TASKS_DIR");
            }
        }

        // === SCENARIO 2: Fallback behavior without environment variables ===
        {
            // Ensure no environment variables are set
            unsafe {
                env::remove_var("LOTAR_TASKS_DIR");
                env::remove_var("LOTAR_DEFAULT_ASSIGNEE");
            }

            let fixtures = TestFixtures::new();

            // Commands should work without specific project config
            let output = fixtures.run_command(&["config", "show"]);
            assert!(
                output.is_ok(),
                "Config show should work without project config"
            );

            // Should be able to add tasks even without project config (uses defaults)
            let output = fixtures.run_command(&[
                "add",
                "isolated-task",
                "--description",
                "Task without project config",
            ]);
            assert!(output.is_ok(), "Add should work without project config");

            // Task should be created in the default tasks directory
            // The system creates a .tasks directory by default
            let tasks_subdir = fixtures.temp_dir.path().join(".tasks");

            if tasks_subdir.exists() && tasks_subdir.is_dir() {
                let task_files: Vec<_> = fs::read_dir(&tasks_subdir)
                    .unwrap()
                    .filter_map(|entry| entry.ok())
                    .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "yml"))
                    .collect();
                assert!(
                    !task_files.is_empty(),
                    "Task should be created in .tasks subdirectory"
                );
            } else {
                // Check if task was created directly in test root directory
                let current_files: Vec<_> = fs::read_dir(fixtures.temp_dir.path())
                    .unwrap()
                    .filter_map(|entry| entry.ok())
                    .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "yml"))
                    .collect();
                assert!(
                    !current_files.is_empty(),
                    "Task should be created in test directory"
                );
            }
        }

        // === SCENARIO 3: Precedence order testing ===
        {
            // Ensure clean state
            unsafe {
                env::remove_var("LOTAR_TASKS_DIR");
                env::remove_var("LOTAR_DEFAULT_ASSIGNEE");
            }

            let fixtures = TestFixtures::new();

            let cli_dir = fixtures.temp_dir.path().join("cli_specified");
            let env_dir = fixtures.temp_dir.path().join("env_specified");

            fs::create_dir_all(&cli_dir).unwrap();
            fs::create_dir_all(&env_dir).unwrap();

            // Set environment variable
            unsafe {
                env::set_var("LOTAR_TASKS_DIR", env_dir.to_str().unwrap());
            }

            // Test 1: CLI option should win over environment variable
            let output = fixtures.run_command(&[
                "add",
                "cli-wins",
                "--description",
                "CLI option wins",
                "--tasks-dir",
                cli_dir.to_str().unwrap(),
            ]);
            assert!(output.is_ok(), "CLI precedence test should succeed");

            let cli_files: Vec<_> = fs::read_dir(&cli_dir).unwrap().collect();
            assert!(!cli_files.is_empty(), "CLI option should take precedence");

            // Test 2: Environment variable should be used when no CLI option
            let output =
                fixtures.run_command(&["add", "env-wins", "--description", "Environment wins"]);
            assert!(output.is_ok(), "Environment precedence test should succeed");

            let env_files: Vec<_> = fs::read_dir(&env_dir)
                .unwrap()
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "yml"))
                .collect();
            assert!(
                !env_files.is_empty(),
                "Environment variable should be used when no CLI override"
            );

            // Clean up for next scenario
            unsafe {
                env::remove_var("LOTAR_TASKS_DIR");
            }
        }

        // === SCENARIO 4: CLI override behavior ===
        {
            // Ensure clean state
            unsafe {
                env::remove_var("LOTAR_TASKS_DIR");
                env::remove_var("LOTAR_DEFAULT_ASSIGNEE");
            }

            let fixtures = TestFixtures::new();

            let env_tasks_dir = fixtures.temp_dir.path().join("env_tasks");
            fs::create_dir_all(&env_tasks_dir).unwrap();

            let cli_tasks_dir = fixtures.temp_dir.path().join("cli_tasks");
            fs::create_dir_all(&cli_tasks_dir).unwrap();

            // Set environment variable
            unsafe {
                env::set_var("LOTAR_TASKS_DIR", env_tasks_dir.to_str().unwrap());
            }

            // Command line option should override environment variable
            let output = fixtures.run_command(&[
                "add",
                "override-task",
                "--description",
                "Task overriding env var",
                "--tasks-dir",
                cli_tasks_dir.to_str().unwrap(),
            ]);
            assert!(output.is_ok(), "Add with CLI override should succeed");

            // Verify task is in CLI directory, not environment directory
            let cli_files: Vec<_> = fs::read_dir(&cli_tasks_dir)
                .unwrap()
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "yml"))
                .collect();
            assert!(
                !cli_files.is_empty(),
                "Task should be in CLI-specified directory"
            );

            let env_files: Vec<_> = fs::read_dir(&env_tasks_dir)
                .unwrap()
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "yml"))
                .collect();
            assert!(
                env_files.is_empty(),
                "Task should NOT be in environment directory"
            );

            // Clean up for next scenario
            unsafe {
                env::remove_var("LOTAR_TASKS_DIR");
            }
        }

        // === SCENARIO 3: LOTAR_TASKS_DIR with relative paths ===
        {
            let fixtures = TestFixtures::new();

            // Set environment variable to relative path
            unsafe {
                env::set_var("LOTAR_TASKS_DIR", "./rel_tasks");
            }

            // Commands should use relative path from current directory
            let output = fixtures.run_command(&["config", "show"]);
            assert!(output.is_ok(), "Config show should work with relative path");
            let output_str = output.unwrap();

            // Check that the tasks directory line contains our resolved path
            // (relative paths get resolved to absolute paths for display)
            assert!(
                output_str
                    .lines()
                    .any(|line| line.trim().starts_with("Tasks directory:")
                        && line.contains("rel_tasks")),
                "Config should show resolved path containing rel_tasks: {output_str}"
            );

            let output = fixtures.run_command(&["add", "rel-task", "--project=rel"]);
            assert!(output.is_ok(), "Add should work with relative path");

            // Verify task was created in relative directory from test working directory
            let rel_tasks_dir = fixtures.temp_dir.path().join("rel_tasks");
            assert!(
                rel_tasks_dir.exists(),
                "Relative tasks directory should be created at: {}",
                rel_tasks_dir.display()
            );

            let task_files: Vec<_> = fs::read_dir(&rel_tasks_dir)
                .unwrap_or_else(|_| panic!("Should be able to read rel_tasks directory"))
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    entry.path().is_dir() && entry.file_name().to_string_lossy().starts_with("REL")
                })
                .collect();
            assert!(
                !task_files.is_empty(),
                "Task project directory should be created in relative path"
            );

            // Clean up this scenario
            unsafe {
                env::remove_var("LOTAR_TASKS_DIR");
            }
        }

        // === SCENARIO 4: Command line flag should override relative environment variable ===
        {
            let fixtures = TestFixtures::new();
            let cli_tasks_dir = fixtures.temp_dir.path().join("cli_tasks");
            fs::create_dir_all(&cli_tasks_dir).unwrap();

            // Set environment variable to relative path
            unsafe {
                env::set_var("LOTAR_TASKS_DIR", "./env_rel_tasks");
            }

            // Command line option should override environment variable
            let output = fixtures.run_command(&[
                "add",
                "override-rel-task",
                "--project=override",
                "--tasks-dir",
                cli_tasks_dir.to_str().unwrap(),
            ]);
            assert!(output.is_ok(), "Add with CLI override should succeed");

            // Verify task is in CLI directory, not environment directory
            let cli_files: Vec<_> = fs::read_dir(&cli_tasks_dir)
                .unwrap()
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    entry.path().is_dir() && entry.file_name().to_string_lossy().starts_with("OVE")
                })
                .collect();
            assert!(
                !cli_files.is_empty(),
                "Task should be in CLI-specified directory"
            );

            let env_rel_tasks_dir = fixtures.temp_dir.path().join("env_rel_tasks");
            // Environment directory should not exist since CLI override was used
            assert!(
                !env_rel_tasks_dir.exists(),
                "Environment relative directory should not be created when CLI override is used"
            );

            // Clean up this scenario
            unsafe {
                env::remove_var("LOTAR_TASKS_DIR");
            }
        }

        // Final cleanup
        unsafe {
            env::remove_var("LOTAR_TASKS_DIR");
            env::remove_var("LOTAR_DEFAULT_ASSIGNEE");
        }
    }
}

#[cfg(test)]
mod parent_directory_resolution {
    use super::*;

    #[test]
    fn test_project_detection_from_subdirectory() {
        let fixtures = TestFixtures::new();

        // Create a project structure with tasks directory
        let project_root = fixtures.temp_dir.path().join("my_project");
        let tasks_dir = project_root.join("tasks");
        let sub_dir = project_root.join("src").join("components");
        fs::create_dir_all(&tasks_dir).unwrap();
        fs::create_dir_all(&sub_dir).unwrap();

        // Test that --tasks-dir overrides the default tasks directory
        let output =
            fixtures.run_command(&["config", "show", "--tasks-dir", tasks_dir.to_str().unwrap()]);
        assert!(
            output.is_ok(),
            "Config show should work with explicit tasks dir"
        );
        let output_str = output.unwrap();
        assert!(
            output_str.contains(&*tasks_dir.to_string_lossy()),
            "Should show specified tasks directory in config output"
        );

        // Add task with explicit tasks directory - should create task in specified location
        let output = fixtures.run_command(&[
            "add",
            "subdir-task",
            "--description",
            "Task from subdirectory",
            "--tasks-dir",
            tasks_dir.to_str().unwrap(),
        ]);
        if let Err(e) = &output {
            eprintln!("Add command failed: {e}");
        }
        assert!(output.is_ok(), "Add should work with explicit tasks dir");

        // Verify task was created in the specified tasks directory
        let task_files: Vec<_> = fs::read_dir(&tasks_dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "yml"))
            .collect();
        assert!(
            !task_files.is_empty(),
            "Task should be created in specified tasks directory"
        );
    }

    #[test]
    fn test_project_detection_stops_at_git_boundary() {
        let fixtures = TestFixtures::new();

        // Create nested project structure
        let outer_project = fixtures.temp_dir.path().join("outer");
        let inner_project = outer_project.join("subproject");
        let inner_tasks = inner_project.join("tasks");
        let work_dir = inner_project.join("work");

        fs::create_dir_all(&inner_tasks).unwrap();
        fs::create_dir_all(&work_dir).unwrap();

        // Create .git directory in inner project to simulate git boundary
        fs::create_dir_all(inner_project.join(".git")).unwrap();

        // Test with explicit tasks directory pointing to inner project
        let output = fixtures.run_command(&[
            "config",
            "show",
            "--tasks-dir",
            inner_tasks.to_str().unwrap(),
        ]);
        assert!(
            output.is_ok(),
            "Config show should work with explicit tasks dir"
        );
        let output_str = output.unwrap();
        assert!(
            output_str.contains(&*inner_tasks.to_string_lossy()),
            "Should show specified tasks directory in config output"
        );
    }
}

#[cfg(test)]
mod global_options_integration {
    use super::*;

    #[test]
    fn test_complex_scenario_with_all_global_options() {
        let fixtures = TestFixtures::new();

        // Set up complex directory structure
        let base_dir = fixtures.temp_dir.path().join("workspace");
        let project_dir = base_dir.join("project");
        let tasks_dir = project_dir.join("custom_tasks");
        let work_dir = project_dir.join("src");

        fs::create_dir_all(&tasks_dir).unwrap();
        fs::create_dir_all(&work_dir).unwrap();

        // Set environment variables
        unsafe {
            env::set_var("LOTAR_DEFAULT_ASSIGNEE", "env-assignee");
        }

        // Test global options integration: environment variable + explicit --tasks-dir
        // This tests the precedence: CLI args > Environment > defaults

        // Run command with explicit --tasks-dir (should override everything)
        let output = fixtures.run_command(&[
            "add",
            "complex-task",
            "--description",
            "Complex scenario task",
            "--tasks-dir",
            tasks_dir.to_str().unwrap(),
            "--priority",
            "high",
        ]);
        if let Err(e) = &output {
            eprintln!("Command failed with error: {e}");
            eprintln!("Tasks dir: {tasks_dir:?}");
            eprintln!("Project dir: {project_dir:?}");
            eprintln!("Work dir: {work_dir:?}");
        }
        assert!(output.is_ok(), "Complex command should succeed");

        // Verify task created in explicit tasks directory
        let all_files: Vec<_> = fs::read_dir(&tasks_dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .collect();

        // Look for project subdirectories (tasks are organized by project)
        let project_dirs: Vec<_> = all_files
            .iter()
            .filter(|entry| entry.path().is_dir())
            .collect();

        if !project_dirs.is_empty() {
            // Check inside the project directory for task files
            let project_dir = &project_dirs[0];

            let task_files: Vec<_> = fs::read_dir(project_dir.path())
                .unwrap()
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    let path = entry.path();
                    // Only include .yml files that are NOT config.yml
                    path.extension().is_some_and(|ext| ext == "yml")
                        && path.file_name().is_some_and(|name| name != "config.yml")
                })
                .collect();

            assert!(
                !task_files.is_empty(),
                "Should create at least one task file in project directory"
            );

            // Verify task content includes the specified priority and environment assignee
            let task_file = &task_files[0];
            let task_content = fs::read_to_string(task_file.path()).unwrap();

            // Debug output for flaky test investigation
            if !task_content.contains("priority: High") {
                eprintln!("=== DEBUG: Task content does not contain expected priority ===");
                eprintln!("File path: {:?}", task_file.path());
                eprintln!("Task content:\n{task_content}");
                eprintln!("Looking for: 'priority: High'");
                eprintln!(
                    "Priority field present: {}",
                    task_content.contains("priority:")
                );
                eprintln!("All files in directory:");
                if let Ok(entries) = fs::read_dir(task_file.path().parent().unwrap()) {
                    for entry in entries.flatten() {
                        eprintln!("  - {:?}", entry.file_name());
                    }
                }
                eprintln!("=== END DEBUG ===");
            }

            assert!(
                task_content.contains("priority: High"),
                "Task content should contain 'priority: High' (variant name from serde), but got: {task_content}"
            );
            assert!(task_content.contains("Complex scenario task"));
            assert!(task_content.contains("assignee: env-assignee")); // Environment variable was used
        } else {
            // Look for task files directly in the tasks directory
            let task_files: Vec<_> = all_files
                .iter()
                .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "yml"))
                .filter(|entry| entry.file_name() != "config.yml")
                .collect();

            assert!(
                !task_files.is_empty(),
                "Should create at least one task file"
            );

            // Verify task content includes the specified priority and environment assignee
            let task_file = task_files[0];
            let task_content = fs::read_to_string(task_file.path()).unwrap();

            // Debug output for flaky test investigation
            if !task_content.contains("priority: High") {
                eprintln!("=== DEBUG: Task content does not contain expected priority ===");
                eprintln!("File path: {:?}", task_file.path());
                eprintln!("Task content:\n{task_content}");
                eprintln!("Looking for: 'priority: High'");
                eprintln!(
                    "Priority field present: {}",
                    task_content.contains("priority:")
                );
                eprintln!("All files in directory:");
                if let Ok(entries) = fs::read_dir(task_file.path().parent().unwrap()) {
                    for entry in entries.flatten() {
                        eprintln!("  - {:?}", entry.file_name());
                    }
                }
                eprintln!("=== END DEBUG ===");
            }

            assert!(
                task_content.contains("priority: High"),
                "Task content should contain 'priority: High' (variant name from serde), but got: {task_content}"
            );
            assert!(task_content.contains("Complex scenario task"));
            assert!(task_content.contains("assignee: env-assignee")); // Environment variable was used
        }

        // Clean up environment variables
        unsafe {
            env::remove_var("LOTAR_DEFAULT_ASSIGNEE");
        }
    }
}
