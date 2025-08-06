// Simple test to verify type filtering fix
#[cfg(test)]
mod type_filter_test {
    use std::process::Command;
    use tempfile::TempDir;
    use assert_cmd::prelude::*;

    #[test]
    fn test_type_filtering_works() {
        let temp_dir = TempDir::new().unwrap();
        
        // Create a bug task
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["add", "--title=Bug task", "--type=bug"])
            .assert()
            .success();

        // Create a feature task  
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        cmd.current_dir(&temp_dir)
            .args(&["add", "--title=Feature task", "--type=feature"])
            .assert()
            .success();

        // Test filtering by bug type
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let output = cmd.current_dir(&temp_dir)
            .args(&["list", "--type=bug"])
            .output()
            .unwrap();
        
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(stdout.contains("Bug task"));
        assert!(!stdout.contains("Feature task"));
        assert!(stdout.contains("Found 1 task(s)"));

        // Test filtering by feature type
        let mut cmd = Command::cargo_bin("lotar").unwrap();
        let output = cmd.current_dir(&temp_dir)
            .args(&["list", "--type=feature"])
            .output()
            .unwrap();
        
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(stdout.contains("Feature task"));
        assert!(!stdout.contains("Bug task"));
        assert!(stdout.contains("Found 1 task(s)"));
    }
}
