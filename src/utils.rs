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
            normalized.split(&['-', '_'][..])
                .filter_map(|word| word.chars().next())
                .take(4)
                .collect::<String>()
        } else {
            // For single words, take first 4 characters
            normalized.chars().take(4).collect::<String>()
        }
    }
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
}
