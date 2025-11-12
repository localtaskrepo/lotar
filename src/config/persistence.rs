use crate::config::env_overrides::{self, EnvOverrideReport};
use crate::config::types::*;
use std::fs;
use std::path::Path;

/// Ensure global config exists, creating default if necessary
pub fn ensure_global_config_exists(tasks_dir: Option<&Path>) -> Result<(), ConfigError> {
    let config_path = match tasks_dir {
        Some(dir) => crate::utils::paths::global_config_path(dir),
        None => crate::utils::paths::global_config_path(&crate::utils::paths::tasks_root_from(
            Path::new("."),
        )),
    };

    if !config_path.exists() {
        create_default_global_config(tasks_dir)?;
    }

    Ok(())
}

/// Load global configuration from tasks_dir/config.yml
pub fn load_global_config(tasks_dir: Option<&Path>) -> Result<GlobalConfig, ConfigError> {
    let path = match tasks_dir {
        Some(dir) => crate::utils::paths::global_config_path(dir),
        None => crate::utils::paths::global_config_path(&crate::utils::paths::tasks_root_from(
            Path::new("."),
        )),
    };
    load_config_file(&path)
}

/// Load home configuration from ~/.lotar
pub fn load_home_config() -> Result<GlobalConfig, ConfigError> {
    // In test environments, ignore the user's home config to keep behavior deterministic
    // Heuristics: RUST_TEST_THREADS is set by cargo test; LOTAR_TEST_MODE/LOTAR_IGNORE_HOME_CONFIG
    // can be used to force-disable reading home config.
    if std::env::var("RUST_TEST_THREADS").is_ok()
        || std::env::var("LOTAR_TEST_MODE")
            .map(|v| v == "1")
            .unwrap_or(false)
        || std::env::var("LOTAR_IGNORE_HOME_CONFIG")
            .map(|v| v == "1")
            .unwrap_or(false)
    {
        return Err(ConfigError::FileNotFound(
            "Home config ignored in test mode".to_string(),
        ));
    }
    let home_dir =
        dirs::home_dir().ok_or(ConfigError::IoError("Home directory not found".to_string()))?;
    let path = home_dir.join(".lotar");
    load_config_file(&path)
}

/// Load home configuration with optional path override
pub fn load_home_config_with_override(
    home_config_path: Option<&Path>,
) -> Result<GlobalConfig, ConfigError> {
    // In test environments, ignore the user's home config to keep behavior deterministic
    if std::env::var("RUST_TEST_THREADS").is_ok()
        || std::env::var("LOTAR_TEST_MODE")
            .map(|v| v == "1")
            .unwrap_or(false)
        || std::env::var("LOTAR_IGNORE_HOME_CONFIG")
            .map(|v| v == "1")
            .unwrap_or(false)
    {
        return Err(ConfigError::FileNotFound(
            "Home config ignored in test mode".to_string(),
        ));
    }
    let path = match home_config_path {
        Some(override_path) => override_path.to_path_buf(),
        None => {
            let home_dir = dirs::home_dir()
                .ok_or(ConfigError::IoError("Home directory not found".to_string()))?;
            home_dir.join(".lotar")
        }
    };
    load_config_file(&path)
}

/// Load project configuration from .tasks/{project}/config.yml
pub fn load_project_config(project_name: &str) -> Result<ProjectConfig, ConfigError> {
    load_project_config_from_dir(project_name, Path::new(".tasks"))
}

/// Load project configuration from specified directory
pub fn load_project_config_from_dir(
    project_name: &str,
    tasks_dir: &Path,
) -> Result<ProjectConfig, ConfigError> {
    let path = crate::utils::paths::project_config_path(tasks_dir, project_name);
    if !path.exists() {
        return Ok(ProjectConfig::new(project_name.to_string()));
    }

    let content = fs::read_to_string(&path)
        .map_err(|e| ConfigError::IoError(format!("Failed to read project config: {}", e)))?;

    // Prefer normalization-aware parse so dotted/nested canonical YAML is supported everywhere
    match crate::config::normalization::parse_project_from_yaml_str(project_name, &content) {
        Ok(cfg) => Ok(cfg),
        Err(_e) => serde_yaml::from_str::<ProjectConfig>(&content)
            .map_err(|e| ConfigError::ParseError(format!("Failed to parse project config: {}", e))),
    }
}

/// Load configuration from a specific file path
fn load_config_file(path: &Path) -> Result<GlobalConfig, ConfigError> {
    if !path.exists() {
        return Err(ConfigError::FileNotFound(
            path.to_string_lossy().to_string(),
        ));
    }

    let content = fs::read_to_string(path)
        .map_err(|e| ConfigError::IoError(format!("Failed to read config: {}", e)))?;

    // Prefer normalization-aware parse so dotted/nested canonical YAML is supported everywhere
    match crate::config::normalization::parse_global_from_yaml_str(&content) {
        Ok(cfg) => Ok(cfg),
        Err(_e) => serde_yaml::from_str::<GlobalConfig>(&content)
            .map_err(|e| ConfigError::ParseError(format!("Failed to parse config: {}", e))),
    }
}

/// Apply environment variable overrides to configuration
pub fn apply_env_overrides(config: &mut GlobalConfig) -> EnvOverrideReport {
    env_overrides::apply_env_overrides(config)
}

/// Create default global configuration file
fn create_default_global_config(tasks_dir: Option<&Path>) -> Result<(), ConfigError> {
    let config_path = match tasks_dir {
        Some(dir) => crate::utils::paths::global_config_path(dir),
        None => crate::utils::paths::global_config_path(&crate::utils::paths::tasks_root_from(
            Path::new("."),
        )),
    };

    // Create tasks directory if it doesn't exist
    if let Some(parent) = config_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| {
                ConfigError::IoError(format!("Failed to create tasks directory: {}", e))
            })?;
        }
    }

    // Create default config with auto-detected prefix
    let mut default_config = GlobalConfig::default();

    // Auto-detect the default prefix from the tasks directory structure
    // This only happens during initial global config creation
    if let Some(tasks_dir_path) = tasks_dir {
        if let Some(detected_prefix) = auto_detect_prefix(tasks_dir_path) {
            default_config.default_prefix = detected_prefix;
        }
        // If no existing projects found, leave default_prefix empty
        // It will be set when the first project is created
    }

    // Write in canonical nested format
    let config_yaml = crate::config::normalization::to_canonical_global_yaml(&default_config);

    fs::write(&config_path, config_yaml).map_err(|e| {
        ConfigError::IoError(format!("Failed to write default global config: {}", e))
    })?;

    // Invalidate cache for this tasks_dir after creation
    if let Some(dir) = tasks_dir {
        crate::config::resolution::invalidate_config_cache_for(Some(dir));
        crate::utils::identity::invalidate_identity_cache(Some(dir));
    } else {
        crate::config::resolution::invalidate_config_cache_for(None);
        crate::utils::identity::invalidate_identity_cache(None);
    }

    // Be quiet by default during tests; only log when explicitly verbose
    let quiet = std::env::var("LOTAR_TEST_SILENT").unwrap_or_default() == "1";
    let verbose = std::env::var("LOTAR_VERBOSE").unwrap_or_default() == "1";
    if !quiet && verbose {
        // Use standard renderer path to ensure logs go to stderr
        let renderer = crate::output::OutputRenderer::new(
            crate::output::OutputFormat::Text,
            crate::output::LogLevel::Warn,
        );
        renderer.log_warn(&format!(
            "Created default global configuration at: {}",
            config_path.display()
        ));
    }
    Ok(())
}

/// Auto-detect the default prefix from existing project directories
/// This scans for existing project directories and their configurations
pub fn auto_detect_prefix(tasks_dir: &Path) -> Option<String> {
    let mut project_prefixes = Vec::new();

    // Look for project directories that exist and have config files
    for (prefix, path) in crate::utils::filesystem::list_visible_subdirs(tasks_dir) {
        let config_path = path.join("config.yml");
        if config_path.exists() {
            project_prefixes.push(prefix);
        }
    }

    if !project_prefixes.is_empty() {
        // If we have existing projects, sort them and return the first one alphabetically
        // This provides deterministic behavior
        project_prefixes.sort();
        return Some(project_prefixes[0].clone());
    }

    // No existing project directories found
    // Return None so default_prefix remains empty until first project is created
    None
}
