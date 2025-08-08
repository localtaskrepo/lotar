//! Project-related utilities: prefix generation, validation and resolution
//! This module provides a consistent way to generate and resolve project
//! prefixes across the entire application.

// Generate a project prefix from a project name with conflict detection
//
// This is the smart algorithm that:
// - For names <= 4 chars: use the name as-is (uppercase)
// - For hyphenated/underscored names: take first letter of each word
// - For single words: take first 4 characters
// - Ensures no conflicts with existing projects
//
// Examples:
// - "test" -> "TEST"
// - "my_cool_project" -> "MCP"
// - "super-awesome-tool" -> "SAT"
// - "longprojectname" -> "LONG"
pub fn generate_project_prefix(project_name: &str) -> String {
    // Strip leading dots to avoid hidden directory issues
    let clean_name = project_name.trim_start_matches('.');

    if clean_name.len() <= 4 {
        clean_name.to_uppercase()
    } else {
        let normalized = clean_name.to_uppercase();
        if normalized.contains('-') || normalized.contains('_') || normalized.contains(' ') {
            // For hyphenated/underscored/spaced names, take first letters of each word
            normalized
                .split(&['-', '_', ' '][..])
                .filter_map(|word| word.chars().next())
                .take(4)
                .collect::<String>()
        } else {
            // For single words, take first 4 characters
            normalized.chars().take(4).collect::<String>()
        }
    }
}

/// Generate a unique project prefix from a project name, ensuring no conflicts
///
/// This function checks for conflicts between:
/// 1. Generated prefixes that match existing project names
/// 2. Project names that match existing prefixes
/// 3. Generated prefixes that match existing prefixes (with different project names)
///
/// Returns an error if a conflict is detected that cannot be resolved.
pub fn generate_unique_project_prefix(
    project_name: &str,
    tasks_dir: &std::path::Path,
) -> Result<String, String> {
    let generated_prefix = generate_project_prefix(project_name);

    // Check if tasks directory exists - if not, no conflicts possible
    if !tasks_dir.exists() {
        return Ok(generated_prefix);
    }

    // Collect all existing projects for conflict checking
    let mut existing_projects = Vec::new();
    let mut existing_prefixes = Vec::new();

    for (dir_name, _dir_path) in crate::utils::filesystem::list_visible_subdirs(tasks_dir) {
        existing_prefixes.push(dir_name.clone());

        // Try to read project name from config
        let config_path = crate::utils::paths::project_config_path(tasks_dir, &dir_name);
        if let Some(existing_project_name) =
            crate::utils::config::read_project_name_from_config(&config_path)
        {
            existing_projects.push((existing_project_name, dir_name.clone()));
        }
    }

    // Check for conflicts

    // 1. Check if our project name matches an existing prefix
    if existing_prefixes.contains(&project_name.to_uppercase()) {
        return Err(format!(
            "Cannot create project '{}': A project with prefix '{}' already exists. Project names cannot match existing prefixes.",
            project_name,
            project_name.to_uppercase()
        ));
    }

    // 2. Check if our generated prefix matches an existing project name
    for (existing_project_name, _) in &existing_projects {
        if generated_prefix.eq_ignore_ascii_case(existing_project_name) {
            return Err(format!(
                "Cannot create project '{}' with prefix '{}': This prefix conflicts with existing project name '{}'. Choose a different project name.",
                project_name, generated_prefix, existing_project_name
            ));
        }
    }

    // 3. Check if our generated prefix matches an existing prefix for a different project
    if existing_prefixes.contains(&generated_prefix) {
        // Find which project uses this prefix
        for (existing_project_name, existing_prefix) in &existing_projects {
            if existing_prefix == &generated_prefix && existing_project_name != project_name {
                return Err(format!(
                    "Cannot create project '{}' with prefix '{}': This prefix is already used by project '{}'. Choose a different project name or use explicit --prefix argument.",
                    project_name, generated_prefix, existing_project_name
                ));
            }
        }

        // If we reach here, the prefix exists but we couldn't find the project name
        // This could happen if the config file is missing or malformed
        return Err(format!(
            "Cannot create project '{}' with prefix '{}': This prefix is already in use. Choose a different project name or use explicit --prefix argument.",
            project_name, generated_prefix
        ));
    }

    // No conflicts detected
    Ok(generated_prefix)
}

/// Validate an explicit prefix provided by the user
///
/// Checks for conflicts between:
/// 1. Explicit prefix matching existing project names  
/// 2. Explicit prefix matching existing prefixes (with different project names)
pub fn validate_explicit_prefix(
    explicit_prefix: &str,
    project_name: &str,
    tasks_dir: &std::path::Path,
) -> Result<(), String> {
    // Check if tasks directory exists - if not, no conflicts possible
    if !tasks_dir.exists() {
        return Ok(());
    }

    // Collect all existing projects for conflict checking
    let mut existing_projects = Vec::new();
    let mut existing_prefixes = Vec::new();

    for (dir_name, _dir_path) in crate::utils::filesystem::list_visible_subdirs(tasks_dir) {
        existing_prefixes.push(dir_name.clone());

        // Try to read project name from config
        let config_path = crate::utils::paths::project_config_path(tasks_dir, &dir_name);
        if let Some(existing_project_name) =
            crate::utils::config::read_project_name_from_config(&config_path)
        {
            existing_projects.push((existing_project_name, dir_name.clone()));
        }
    }

    // Check for conflicts

    // 1. Check if our explicit prefix matches an existing project name (case insensitive)
    for (existing_project_name, _) in &existing_projects {
        if explicit_prefix.eq_ignore_ascii_case(existing_project_name) {
            return Err(format!(
                "Cannot use prefix '{}': This prefix conflicts with existing project name '{}'. Choose a different prefix.",
                explicit_prefix, existing_project_name
            ));
        }
    }

    // 2. Check if our explicit prefix matches an existing prefix for a different project (case insensitive)
    for (existing_project_name, existing_prefix) in &existing_projects {
        if explicit_prefix.eq_ignore_ascii_case(existing_prefix)
            && project_name != existing_project_name
        {
            return Err(format!(
                "Cannot use prefix '{}': This prefix is already used by project '{}'. Choose a different prefix.",
                explicit_prefix, existing_project_name
            ));
        }
    }

    // No conflicts detected
    Ok(())
}

/// Smart resolver that accepts either a project name or prefix and returns the appropriate prefix
/// for storage operations. This allows users to use either format in --project parameters.
pub fn resolve_project_input(input: &str, tasks_dir: &std::path::Path) -> String {
    // First, check if input is already a valid prefix by looking for an exact directory match
    let input_as_prefix_dir = tasks_dir.join(input);
    if input_as_prefix_dir.exists() && input_as_prefix_dir.is_dir() {
        // Input is a valid prefix (directory exists)
        return input.to_string();
    }

    // If not found as prefix, look through all project directories to find one with matching project_name
    for (dir_name, _dir_path) in crate::utils::filesystem::list_visible_subdirs(tasks_dir) {
        // Check if this directory has a config file with matching project_name
        let config_path = crate::utils::paths::project_config_path(tasks_dir, &dir_name);
        if let Some(project_name) =
            crate::utils::config::read_project_name_from_config(&config_path)
        {
            if project_name == input {
                return dir_name;
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
    generated_prefix
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
    fn test_spaced_names() {
        assert_eq!(generate_project_prefix("My Test Project"), "MTP");
        assert_eq!(generate_project_prefix("Super Awesome Tool"), "SAT");
        assert_eq!(generate_project_prefix("A B C D"), "ABCD");
        assert_eq!(generate_project_prefix("my project"), "MP");
        assert_eq!(generate_project_prefix("Local Task Repository"), "LTR");
    }

    #[test]
    fn test_dot_prefixed_names() {
        assert_eq!(generate_project_prefix(".tmp"), "TMP");
        assert_eq!(generate_project_prefix(".tmpABC123"), "TMPA");
        assert_eq!(generate_project_prefix(".test_dot_prefix"), "TDP");
        assert_eq!(generate_project_prefix("..hidden"), "HIDD");
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
    fn test_generate_unique_project_prefix_no_conflicts() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let tasks_dir = temp_dir.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();

        // Test with no existing projects
        assert_eq!(
            generate_unique_project_prefix("frontend", &tasks_dir).unwrap(),
            "FRON"
        );
        assert_eq!(
            generate_unique_project_prefix("api-backend", &tasks_dir).unwrap(),
            "AB"
        );
    }

    #[test]
    fn test_generate_unique_project_prefix_conflicts() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let tasks_dir = temp_dir.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();

        // Create existing project "backend" with prefix "BACK"
        let existing_project_dir = tasks_dir.join("BACK");
        std::fs::create_dir_all(&existing_project_dir).unwrap();
        std::fs::write(
            crate::utils::paths::project_config_path(
                temp_dir.path().join(".tasks").as_path(),
                existing_project_dir.file_name().unwrap().to_str().unwrap(),
            ),
            "project_name: backend\n",
        )
        .unwrap();

        // Test conflict: project name matches existing prefix
        let result = generate_unique_project_prefix("BACK", &tasks_dir);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("Project names cannot match existing prefixes")
        );

        // Test conflict: generated prefix matches existing project name
        let result = generate_unique_project_prefix("backend", &tasks_dir);
        assert!(result.is_err());
        let error_msg = result.unwrap_err();
        // "backend" -> "BACK" prefix, which should conflict with existing prefix "BACK"
        assert!(error_msg.contains("prefix is already in use"));

        // Test conflict: generated prefix matches existing prefix for different project
        let result = generate_unique_project_prefix("backend-api", &tasks_dir); // Would generate "BA" but "BACK" exists
        // This one should actually work since "BA" != "BACK"
        assert!(result.is_ok());

        // Create a project that would actually conflict
        let result = generate_unique_project_prefix("back-end", &tasks_dir); // Would generate "BE" 
        assert!(result.is_ok()); // No conflict

        // Test a project that would generate "BACK" prefix (4-letter name)
        let result = generate_unique_project_prefix("back", &tasks_dir);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("Project names cannot match existing prefixes")
        );
    }

    #[test]
    fn test_validate_explicit_prefix_conflicts() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let tasks_dir = temp_dir.path().join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();

        // Create existing project "frontend" with prefix "FRON"
        let existing_project_dir = tasks_dir.join("FRON");
        std::fs::create_dir_all(&existing_project_dir).unwrap();
        std::fs::write(
            crate::utils::paths::project_config_path(
                temp_dir.path().join(".tasks").as_path(),
                existing_project_dir.file_name().unwrap().to_str().unwrap(),
            ),
            "project_name: frontend\n",
        )
        .unwrap();

        // Test conflict: explicit prefix matches existing project name
        let result = validate_explicit_prefix("frontend", "new-project", &tasks_dir);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("prefix conflicts with existing project name")
        );

        // Test conflict: explicit prefix matches existing prefix for a different project
        let result = validate_explicit_prefix("FRON", "backend", &tasks_dir);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("prefix is already used by project")
        );

        // Test no conflict: explicit prefix is unique
        let result = validate_explicit_prefix("BACK", "backend", &tasks_dir);
        assert!(result.is_ok());

        // Test no conflict: explicit prefix for same project (updating existing)
        let result = validate_explicit_prefix("FRON", "frontend", &tasks_dir);
        assert!(result.is_ok());
    }
}
