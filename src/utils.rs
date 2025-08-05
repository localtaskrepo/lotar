/// Shared project prefix generation utility
///
/// This module provides a consistent way to generate project prefixes
/// across the entire application. The algorithm produces intuitive
/// prefixes from project names.

/// Generate a project prefix from a project name
///
/// This is the smart algorithm that:
/// - For names <= 4 chars: use the name as-is (uppercase)
/// - For hyphenated/underscored names: take first letter of each word
/// - For single words: take first 4 characters
///
/// Examples:
/// - "test" -> "TEST"
/// - "my_cool_project" -> "MCP"
/// - "super-awesome-tool" -> "SAT"
/// - "longprojectname" -> "LONG"
pub fn generate_project_prefix(project_name: &str) -> String {
    if project_name.len() <= 4 {
        project_name.to_uppercase()
    } else {
        let normalized = project_name.to_uppercase();
        if normalized.contains('-') || normalized.contains('_') {
            // For hyphenated/underscored names, take first letters of each word
            normalized
                .split(&['-', '_'][..])
                .filter_map(|word| word.chars().next())
                .take(4)
                .collect::<String>()
        } else {
            // For single words, take first 4 characters
            normalized.chars().take(4).collect::<String>()
        }
    }
}

/// Smart resolver that accepts either a project name or prefix and returns the appropriate prefix
/// for storage operations. This allows users to use either format in --project parameters.
pub fn resolve_project_input(input: &str, tasks_dir: &std::path::PathBuf) -> String {
    // First, check if input is already a valid prefix by looking for an exact directory match
    let input_as_prefix_dir = tasks_dir.join(input);
    if input_as_prefix_dir.exists() && input_as_prefix_dir.is_dir() {
        // Input is a valid prefix (directory exists)
        return input.to_string();
    }

    // If not found as prefix, look through all project directories to find one with matching project_name
    if let Ok(entries) = std::fs::read_dir(tasks_dir) {
        for entry in entries.flatten() {
            if entry.file_type().map_or(false, |ft| ft.is_dir()) {
                let dir_name = entry.file_name().to_string_lossy().to_string();

                // Skip hidden directories
                if dir_name.starts_with('.') {
                    continue;
                }

                // Check if this directory has a config file with matching project_name
                let config_path = entry.path().join("config.yml");
                if config_path.exists() {
                    if let Ok(config_content) = std::fs::read_to_string(&config_path) {
                        // Simple YAML parsing to look for project_name field
                        for line in config_content.lines() {
                            if let Some(project_name) = line.strip_prefix("project_name: ") {
                                let project_name = project_name.trim().trim_matches('"');
                                if project_name == input {
                                    return dir_name;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // If not found as prefix, treat input as a potential full project name
    // Generate what the prefix would be for this project name
    let generated_prefix = generate_project_prefix(input);

    // Check if a directory exists for the generated prefix
    let generated_prefix_dir = tasks_dir.join(&generated_prefix);
    if generated_prefix_dir.exists() && generated_prefix_dir.is_dir() {
        // Full project name was provided, return its prefix
        return generated_prefix;
    }

    // Neither direct prefix nor full project name found
    // This could be a new project being created, so return the generated prefix
    return generated_prefix;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_names() {
        assert_eq!(generate_project_prefix("test"), "TEST");
        assert_eq!(generate_project_prefix("a"), "A");
        assert_eq!(generate_project_prefix("ab"), "AB");
        assert_eq!(generate_project_prefix("abc"), "ABC");
        assert_eq!(generate_project_prefix("abcd"), "ABCD");
    }

    #[test]
    fn test_hyphenated_names() {
        assert_eq!(generate_project_prefix("my-cool-project"), "MCP");
        assert_eq!(generate_project_prefix("super-awesome-tool"), "SAT");
        assert_eq!(generate_project_prefix("a-b-c-d"), "ABCD");
        assert_eq!(generate_project_prefix("my-project"), "MP");
    }

    #[test]
    fn test_underscored_names() {
        assert_eq!(generate_project_prefix("my_cool_project"), "MCP");
        assert_eq!(generate_project_prefix("local_task_repo"), "LTR");
        assert_eq!(generate_project_prefix("test_project"), "TP");
    }

    #[test]
    fn test_long_single_words() {
        assert_eq!(generate_project_prefix("longprojectname"), "LONG");
        assert_eq!(generate_project_prefix("verylongname"), "VERY");
        assert_eq!(generate_project_prefix("project"), "PROJ");
    }

    #[test]
    fn test_mixed_cases() {
        assert_eq!(generate_project_prefix("MyProject"), "MYPR");
        assert_eq!(generate_project_prefix("my-Cool-Project"), "MCP");
        assert_eq!(generate_project_prefix("Test_Project"), "TP");
    }

    #[test]
    fn test_resolve_project_input_with_existing_prefix() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let tasks_dir = temp_dir.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();

        // Create a prefix directory
        let prefix_dir = tasks_dir.join("FRON");
        std::fs::create_dir_all(&prefix_dir).unwrap();

        // Test with existing prefix - should return the prefix as-is
        assert_eq!(resolve_project_input("FRON", &tasks_dir), "FRON");
    }

    #[test]
    fn test_resolve_project_input_with_full_project_name() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let tasks_dir = temp_dir.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();

        // Create a prefix directory that matches generated prefix
        let prefix_dir = tasks_dir.join("FRON");
        std::fs::create_dir_all(&prefix_dir).unwrap();

        // Test with full project name - should return the generated prefix
        assert_eq!(resolve_project_input("FRONTEND", &tasks_dir), "FRON");
    }

    #[test]
    fn test_resolve_project_input_new_project() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let tasks_dir = temp_dir.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();

        // Test with new project - should return generated prefix
        assert_eq!(resolve_project_input("NEW-PROJECT", &tasks_dir), "NP");
        assert_eq!(resolve_project_input("BACKEND", &tasks_dir), "BACK");
    }

    #[test]
    fn test_resolve_project_input_hyphenated_names() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let tasks_dir = temp_dir.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();

        // Create prefix directory for API-BACKEND -> AB
        let prefix_dir = tasks_dir.join("AB");
        std::fs::create_dir_all(&prefix_dir).unwrap();

        // Test various forms of the same project
        assert_eq!(resolve_project_input("AB", &tasks_dir), "AB");
        assert_eq!(resolve_project_input("API-BACKEND", &tasks_dir), "AB");
    }

    #[test]
    fn test_resolve_project_input_prefix_generation_over_case_mismatch() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let tasks_dir = temp_dir.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();

        // Create an uppercase prefix directory
        let uppercase_dir = tasks_dir.join("DEMO");
        std::fs::create_dir_all(&uppercase_dir).unwrap();

        // Test exact match
        assert_eq!(resolve_project_input("DEMO", &tasks_dir), "DEMO");

        // Test that full project name generates correct prefix and finds directory
        // "demo-project" should generate "DP" prefix
        let dp_dir = tasks_dir.join("DP");
        std::fs::create_dir_all(&dp_dir).unwrap();

        assert_eq!(resolve_project_input("demo-project", &tasks_dir), "DP");

        // Test that non-existent input generates prefix
        assert_eq!(resolve_project_input("new-project", &tasks_dir), "NP");
    }
}
