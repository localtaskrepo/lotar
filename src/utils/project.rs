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
        if normalized.contains('-')
            || normalized.contains('_')
            || normalized.contains(' ')
            || normalized.contains('.')
        {
            // For hyphenated/underscored/spaced names, take first letters of each word
            normalized
                .split(&['-', '_', ' ', '.'][..])
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
        let mut conflict: Option<String> = None;

        for (existing_project_name, existing_prefix) in &existing_projects {
            if existing_prefix == &generated_prefix {
                if existing_project_name.eq_ignore_ascii_case(project_name) {
                    conflict = Some(format!(
                        "Cannot create project '{}' with prefix '{}': This prefix is already in use. Choose a different project name or use explicit --prefix argument.",
                        project_name, generated_prefix
                    ));
                    break;
                }

                if !existing_project_name.eq_ignore_ascii_case(&generated_prefix) {
                    conflict = Some(format!(
                        "Cannot create project '{}' with prefix '{}': This prefix is already used by project '{}'. Choose a different project name or use explicit --prefix argument.",
                        project_name, generated_prefix, existing_project_name
                    ));
                    break;
                }
                // Placeholder config (project name equals prefix) – allow caller to reuse.
            }
        }

        if let Some(message) = conflict {
            return Err(message);
        }

        return Ok(generated_prefix);
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

/// Format a human-friendly project label given a prefix and optional display name.
///
/// Rules mirror the web helpers:
/// - When a display name is present, show "{name} ({PREFIX})"
/// - When display name is absent but prefix exists, show the prefix alone
/// - When both are missing, fall back to the word "Project"
pub fn format_project_label(prefix: &str, display_name: Option<&str>) -> String {
    let trimmed_prefix = prefix.trim();
    let trimmed_name = display_name.unwrap_or("").trim();

    if !trimmed_name.is_empty() {
        if trimmed_prefix.is_empty() {
            trimmed_name.to_string()
        } else if trimmed_name.eq_ignore_ascii_case(trimmed_prefix) {
            trimmed_prefix.to_string()
        } else {
            format!("{} ({})", trimmed_name, trimmed_prefix)
        }
    } else if !trimmed_prefix.is_empty() {
        trimmed_prefix.to_string()
    } else {
        "Project".to_string()
    }
}

/// Attempt to load the display name for a project from its configuration file.
/// Returns `None` when no config is present or the name is blank.
pub fn project_display_name_from_config(
    tasks_dir: &std::path::Path,
    project_prefix: &str,
) -> Option<String> {
    let config_path = crate::utils::paths::project_config_path(tasks_dir, project_prefix);

    let explicit_name = std::fs::read_to_string(&config_path)
        .ok()
        .and_then(|content| serde_yaml::from_str::<serde_yaml::Value>(&content).ok())
        .and_then(|value| extract_project_name(&value));

    if let Some(name) = explicit_name {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    crate::config::persistence::load_project_config_from_dir(project_prefix, tasks_dir)
        .ok()
        .map(|cfg| cfg.project_name.trim().to_string())
        .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case(project_prefix))
}

fn extract_project_name(value: &serde_yaml::Value) -> Option<String> {
    use serde_yaml::Value;

    if let Value::Mapping(map) = value {
        let project_key = Value::String("project".to_string());
        if let Some(Value::Mapping(project)) = map.get(&project_key) {
            let project_name_key = Value::String("name".to_string());
            if let Some(Value::String(name)) = project.get(&project_name_key) {
                let trimmed = name.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
            let project_id_key = Value::String("id".to_string());
            if let Some(Value::String(id)) = project.get(&project_id_key) {
                let trimmed = id.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }

        let config_key = Value::String("config".to_string());
        if let Some(Value::Mapping(cfg)) = map.get(&config_key) {
            let cfg_project_name_key = Value::String("project_name".to_string());
            if let Some(Value::String(name)) = cfg.get(&cfg_project_name_key) {
                let trimmed = name.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }

        let project_name_key = Value::String("project_name".to_string());
        if let Some(Value::String(name)) = map.get(&project_name_key) {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }

        for (key, val) in map {
            if let Value::String(key_str) = key {
                match key_str.as_str() {
                    "project.name" | "project.id" | "config.project_name" | "project_name" => {
                        if let Value::String(name) = val {
                            let trimmed = name.trim();
                            if !trimmed.is_empty() {
                                return Some(trimmed.to_string());
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    None
}
