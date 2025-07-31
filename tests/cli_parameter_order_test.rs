use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

mod common;

/// Tests for parameter order flexibility, particularly with --tasks-dir flag
#[cfg(test)]
mod parameter_order_tests {
    use super::*;

    struct TestFixtures {
        temp_dir: TempDir,
    }

    impl TestFixtures {
        fn new() -> Self {
            TestFixtures {
                temp_dir: TempDir::new().expect("Failed to create temp dir"),
            }
        }
    }

    #[test]
    fn test_tasks_dir_flag_first_position() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();
        let custom_tasks_dir = temp_dir.join("custom-tasks");
        
        // Create the custom directory first (system requires it to exist)
        fs::create_dir_all(&custom_tasks_dir).unwrap();

        // Test --tasks-dir as first argument: lotar --tasks-dir=/path config init --project=Test
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("--tasks-dir")
            .arg(custom_tasks_dir.to_str().unwrap())
            .arg("config")
            .arg("init")
            .arg("--project=FirstPosition")
            .assert()
            .success()
            .stdout(predicate::str::contains("Config file created at"))
            .stdout(predicate::str::contains("custom-tasks"));

        // Verify the config was created in the custom directory
        let config_path = custom_tasks_dir.join("FIRS").join("config.yml");
        assert!(config_path.exists());
    }

    #[test]
    fn test_tasks_dir_flag_middle_position() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();
        let custom_tasks_dir = temp_dir.join("middle-tasks");
        
        // Create the custom directory first
        fs::create_dir_all(&custom_tasks_dir).unwrap();

        // Test --tasks-dir in middle: lotar config --tasks-dir=/path init --project=Test
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("--tasks-dir")
            .arg(custom_tasks_dir.to_str().unwrap())
            .arg("init")
            .arg("--project=MiddlePosition")
            .assert()
            .success()
            .stdout(predicate::str::contains("Config file created at"))
            .stdout(predicate::str::contains("middle-tasks"));

        // Verify the config was created in the custom directory
        let config_path = custom_tasks_dir.join("MIDD").join("config.yml");
        assert!(config_path.exists());
    }

    #[test]
    fn test_tasks_dir_flag_last_position() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();
        let custom_tasks_dir = temp_dir.join("last-tasks");
        
        // Create the custom directory first
        fs::create_dir_all(&custom_tasks_dir).unwrap();

        // Test --tasks-dir as last argument: lotar config init --project=Test --tasks-dir=/path
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=LastPosition")
            .arg("--tasks-dir")
            .arg(custom_tasks_dir.to_str().unwrap())
            .assert()
            .success()
            .stdout(predicate::str::contains("Config file created at"))
            .stdout(predicate::str::contains("last-tasks"));

        // Verify the config was created in the custom directory
        let config_path = custom_tasks_dir.join("LAST").join("config.yml");
        assert!(config_path.exists());
    }

    #[test]
    fn test_tasks_dir_equals_syntax_different_positions() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();
        let custom_tasks_dir1 = temp_dir.join("equals1");
        let custom_tasks_dir2 = temp_dir.join("equals2");
        let custom_tasks_dir3 = temp_dir.join("equals3");

        // Create all custom directories first
        fs::create_dir_all(&custom_tasks_dir1).unwrap();
        fs::create_dir_all(&custom_tasks_dir2).unwrap();
        fs::create_dir_all(&custom_tasks_dir3).unwrap();

        // Test equals syntax in first position
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg(&format!("--tasks-dir={}", custom_tasks_dir1.to_str().unwrap()))
            .arg("config")
            .arg("init")
            .arg("--project=EqualsFirst")
            .assert()
            .success();

        let config_path1 = custom_tasks_dir1.join("EQUA").join("config.yml");
        assert!(config_path1.exists());

        // Test equals syntax in middle position
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg(&format!("--tasks-dir={}", custom_tasks_dir2.to_str().unwrap()))
            .arg("init")
            .arg("--project=EqualsMiddle")
            .assert()
            .success();

        let config_path2 = custom_tasks_dir2.join("EQUA").join("config.yml");
        assert!(config_path2.exists());

        // Test equals syntax in last position
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=EqualsLast")
            .arg(&format!("--tasks-dir={}", custom_tasks_dir3.to_str().unwrap()))
            .assert()
            .success();

        let config_path3 = custom_tasks_dir3.join("EQUA").join("config.yml");
        assert!(config_path3.exists());
    }

    #[test]
    fn test_tasks_dir_with_multiple_project_flags() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();
        let custom_tasks_dir = temp_dir.join("multi-flags");
        
        // Create the custom directory first
        fs::create_dir_all(&custom_tasks_dir).unwrap();

        // Test with multiple flags in different orders
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("--tasks-dir")
            .arg(custom_tasks_dir.to_str().unwrap())
            .arg("config")
            .arg("init")
            .arg("--project=MultiFlags")
            .arg("--template=simple")
            .assert()
            .success();

        let config_path = custom_tasks_dir.join("MULT").join("config.yml");
        assert!(config_path.exists());

        // Verify template was applied
        let config_content = fs::read_to_string(&config_path).unwrap();
        assert!(config_content.contains("project_name: MultiFlags"));
        assert!(config_content.contains("issue_states:"));
    }

    #[test]
    fn test_tasks_dir_with_different_commands() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();
        let custom_tasks_dir = temp_dir.join("diff-commands");
        
        // Create the custom directory first
        fs::create_dir_all(&custom_tasks_dir).unwrap();

        // Create a test file for scanning
        let test_file = temp_dir.join("test.js");
        fs::write(&test_file, "// TODO: Test todo comment\nfunction test() {}").unwrap();

        // Test with scan command
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("--tasks-dir")
            .arg(custom_tasks_dir.to_str().unwrap())
            .arg("scan")
            .assert()
            .success()
            .stdout(predicate::str::contains("Scanning"));

        // Test with config set command
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("--tasks-dir")
            .arg(custom_tasks_dir.to_str().unwrap())
            .arg("set")
            .arg("server_port")
            .arg("9000")
            .assert()
            .success()
            .stdout(predicate::str::contains("Successfully updated server_port"));

        // Verify the global config was created in custom directory
        let global_config_path = custom_tasks_dir.join("config.yml");
        assert!(global_config_path.exists());
    }

    #[test]
    fn test_tasks_dir_error_cases() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();

        // Test missing value after --tasks-dir flag (it treats 'config' as the directory name)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("--tasks-dir")
            .arg("config")
            .arg("init")
            .arg("--project=ErrorTest")
            .assert()
            .failure()
            .stderr(predicate::str::contains("Specified tasks directory does not exist: config"));

        // Test empty value with equals syntax
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("--tasks-dir=")
            .arg("config")
            .arg("init")
            .arg("--project=ErrorTest")
            .assert()
            .failure()
            .stderr(predicate::str::contains("Specified tasks directory does not exist:"));

        // Test just the flag with no other arguments
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("--tasks-dir")
            .assert()
            .failure()
            .stderr(predicate::str::contains("Invalid command '--tasks-dir'"));
    }

    #[test]
    fn test_multiple_tasks_dir_flags() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();
        let custom_tasks_dir1 = temp_dir.join("first-dir");
        let custom_tasks_dir2 = temp_dir.join("second-dir");
        
        // Create both custom directories
        fs::create_dir_all(&custom_tasks_dir1).unwrap();
        fs::create_dir_all(&custom_tasks_dir2).unwrap();

        // Test with multiple --tasks-dir flags (last one should win)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("--tasks-dir")
            .arg(custom_tasks_dir1.to_str().unwrap())
            .arg("--tasks-dir")
            .arg(custom_tasks_dir2.to_str().unwrap())
            .arg("config")
            .arg("init")
            .arg("--project=MultipleFlags")
            .assert()
            .success()
            .stdout(predicate::str::contains("second-dir"));

        // Verify the config was created in the second directory (last wins)
        let config_path = custom_tasks_dir2.join("MULT").join("config.yml");
        assert!(config_path.exists());

        // Verify the first directory was not used
        let wrong_config_path = custom_tasks_dir1.join("MULT").join("config.yml");
        assert!(!wrong_config_path.exists());
    }

    #[test]
    fn test_tasks_dir_mixed_syntax() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();
        let custom_tasks_dir1 = temp_dir.join("space-syntax");
        let custom_tasks_dir2 = temp_dir.join("equals-syntax");
        
        // Create both custom directories
        fs::create_dir_all(&custom_tasks_dir1).unwrap();
        fs::create_dir_all(&custom_tasks_dir2).unwrap();

        // Test mixing space and equals syntax (last one should win)
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("--tasks-dir")
            .arg(custom_tasks_dir1.to_str().unwrap())
            .arg(&format!("--tasks-dir={}", custom_tasks_dir2.to_str().unwrap()))
            .arg("config")
            .arg("init")
            .arg("--project=MixedSyntax")
            .assert()
            .success()
            .stdout(predicate::str::contains("equals-syntax"));

        // Verify the config was created in the equals-syntax directory
        let config_path = custom_tasks_dir2.join("MIXE").join("config.yml");
        assert!(config_path.exists());
    }

    #[test]
    fn test_tasks_dir_with_complex_command_chains() {
        let test_fixtures = TestFixtures::new();
        let temp_dir = test_fixtures.temp_dir.path();
        let custom_tasks_dir = temp_dir.join("complex-chain");
        
        // Create the custom directory first
        fs::create_dir_all(&custom_tasks_dir).unwrap();

        // Test with complex command with multiple subcommands and flags
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("init")
            .arg("--project=ComplexChain")
            .arg("--template=agile")
            .arg("--tasks-dir")
            .arg(custom_tasks_dir.to_str().unwrap())
            .assert()
            .success();

        let config_path = custom_tasks_dir.join("COMP").join("config.yml");
        assert!(config_path.exists());

        // Verify template was applied correctly
        let config_content = fs::read_to_string(&config_path).unwrap();
        assert!(config_content.contains("project_name: ComplexChain"));
        assert!(config_content.contains("Epic")); // From agile template

        // Test subsequent command with same directory
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(temp_dir)
            .arg("config")
            .arg("set")
            .arg("default_priority")
            .arg("HIGH")
            .arg("--project=ComplexChain")
            .arg("--tasks-dir")
            .arg(custom_tasks_dir.to_str().unwrap())
            .assert()
            .success();

        // Verify the setting was applied
        let updated_content = fs::read_to_string(&config_path).unwrap();
        assert!(updated_content.contains("default_priority: High"));
    }
}
