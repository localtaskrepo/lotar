use crate::config::env_overrides::{self, EnvOverrideReport};
use crate::config::types::*;
use std::fs;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::sync::OnceLock;

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

/// Whether the home config layer is honored right now.
///
/// In test environments the user's home config is ignored to keep behavior
/// deterministic: `RUST_TEST_THREADS` is set by cargo test, and
/// `LOTAR_TEST_MODE`/`LOTAR_IGNORE_HOME_CONFIG` can force-disable reading
/// the home config. Shared by the loaders below and by callers that need
/// source presence (not just a load result) for the home layer.
pub fn home_config_honored() -> bool {
    !(std::env::var("RUST_TEST_THREADS").is_ok()
        || std::env::var("LOTAR_TEST_MODE")
            .map(|v| v == "1")
            .unwrap_or(false)
        || std::env::var("LOTAR_IGNORE_HOME_CONFIG")
            .map(|v| v == "1")
            .unwrap_or(false))
}

/// Load home configuration from ~/.lotar
pub fn load_home_config() -> Result<GlobalConfig, ConfigError> {
    if !home_config_honored() {
        return Err(ConfigError::FileNotFound(
            "Home config ignored in test mode".to_string(),
        ));
    }
    let home_dir =
        dirs::home_dir().ok_or(ConfigError::IoError("Home directory not found".to_string()))?;
    let path = home_dir.join(".lotar");
    warn_insecure_home_config_permissions(&path);
    load_config_file(&path)
}

/// Load home configuration with optional path override
pub fn load_home_config_with_override(
    home_config_path: Option<&Path>,
) -> Result<GlobalConfig, ConfigError> {
    if !home_config_honored() {
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
    warn_insecure_home_config_permissions(&path);
    load_config_file(&path)
}

/// Whether config content sets the server port under any accepted spelling
/// (`server_port`, `server.port`, nested `server: {port: ...}`). Key
/// presence is the authority — never the parsed value — so an explicit
/// `8080` stays distinguishable from "not configured" for every consumer
/// (serve port selection, generic resolution, source labels).
pub(crate) fn config_sets_server_port(content: &str) -> bool {
    use serde_yaml_ng::Value;
    let Ok(Value::Mapping(mapping)) = serde_yaml_ng::from_str::<Value>(content) else {
        return false;
    };
    if mapping.contains_key(Value::String("server_port".to_string())) {
        return true;
    }
    if mapping.contains_key(Value::String("server.port".to_string())) {
        return true;
    }
    match mapping.get(Value::String("server".to_string())) {
        Some(Value::Mapping(nested)) => nested.contains_key(Value::String("port".to_string())),
        _ => false,
    }
}

/// Pure form of [`config_file_server_port`] on raw file content.
pub(crate) fn config_content_server_port(content: &str) -> Result<Option<u16>, String> {
    let parsed = crate::config::normalization::parse_global_from_yaml_str(content)
        .map_err(|err| format!("{}", err))?;
    if config_sets_server_port(content) {
        Ok(Some(parsed.server_port))
    } else {
        Ok(None)
    }
}

/// A config file layer's explicit server port: `Ok(Some(port))` when the
/// file sets the server port, `Ok(None)` when the file is absent or sets no
/// port, and `Err` when the file exists but cannot be parsed (its intended
/// port is unknowable, so callers must not guess).
pub(crate) fn config_file_server_port(path: &Path, what: &str) -> Result<Option<u16>, String> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(format!(
                "Failed to read {} at {}: {}",
                what,
                path.display(),
                err
            ));
        }
    };
    config_content_server_port(&content)
        .map_err(|err| format!("Invalid {} at {}: {}", what, path.display(), err))
}

/// Whether the global config FILE under a tasks root explicitly sets the
/// server port (any accepted spelling). Presence-only: false for missing or
/// unparseable files.
pub(crate) fn global_config_sets_server_port(tasks_root: &Path) -> bool {
    config_file_server_port(
        &crate::utils::paths::global_config_path(tasks_root),
        "global config",
    )
    .ok()
    .flatten()
    .is_some()
}

/// The home config layer's explicit server port, honoring the shared ignore
/// gates: an ignored or missing home layer contributes nothing; a present
/// but unparseable one is an error so no caller silently guesses.
pub(crate) fn home_config_server_port() -> Result<Option<u16>, String> {
    if !home_config_honored() {
        return Ok(None);
    }
    let Some(home_dir) = dirs::home_dir() else {
        return Ok(None);
    };
    config_file_server_port(&home_dir.join(".lotar"), "home config")
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
    crate::config::normalization::parse_project_from_yaml_str(project_name, &content)
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
    crate::config::normalization::parse_global_from_yaml_str(&content)
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
    if let Some(parent) = config_path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent).map_err(|e| {
            ConfigError::IoError(format!("Failed to create tasks directory: {}", e))
        })?;
    }

    // Create default config with auto-detected prefix
    let mut default_config = GlobalConfig::default();

    // Auto-detect the default prefix from the tasks directory structure
    // This only happens during initial global config creation
    if let Some(tasks_dir_path) = tasks_dir
        && let Some(detected_prefix) = auto_detect_prefix(tasks_dir_path)
    {
        default_config.default_project = detected_prefix;
    }
    // If no existing projects found, leave default_project empty
    // It will be set when the first project is created

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
        renderer.log_warn(format_args!(
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
    // Return None so default_project remains empty until first project is created
    None
}

fn warn_insecure_home_config_permissions(path: &Path) {
    #[cfg(unix)]
    {
        static WARNED: OnceLock<()> = OnceLock::new();
        if WARNED.get().is_some() {
            return;
        }
        let quiet = std::env::var("LOTAR_TEST_SILENT").unwrap_or_default() == "1";
        if quiet || !path.exists() {
            return;
        }
        if let Ok(meta) = fs::metadata(path) {
            let mode = meta.permissions().mode() & 0o777;
            if mode & 0o077 != 0 {
                let renderer = crate::output::OutputRenderer::new(
                    crate::output::OutputFormat::Text,
                    crate::output::LogLevel::Warn,
                );
                renderer.emit_warning(format!(
                    "Home config permissions are too open (mode {:o}). Run chmod 600 {}",
                    mode,
                    path.display()
                ));
                let _ = WARNED.set(());
            }
        }
    }
}

#[cfg(test)]
mod server_port_tests {
    use super::{config_content_server_port, config_sets_server_port};

    #[test]
    fn detects_every_accepted_port_spelling() {
        for content in [
            "server:\n  port: 9000\n",
            "server.port: 9100\n",
            "server_port: 9200\n",
            "default:\n  project: A\nserver:\n  port: 8080\n",
            "server_port: 8080\n",
            "server:\n  port: 0\n",
        ] {
            assert!(
                config_sets_server_port(content),
                "must detect port key in: {content}"
            );
        }
    }

    #[test]
    fn content_without_port_key_is_not_detected() {
        for content in [
            "",
            "default:\n  project: A\n",
            "issue:\n  states: [Todo, Done]\n",
        ] {
            assert!(
                !config_sets_server_port(content),
                "must not detect port key in: {content}"
            );
        }
    }

    #[test]
    fn extracts_values_and_fails_closed() {
        assert_eq!(
            config_content_server_port("server:\n  port: 9000\n").unwrap(),
            Some(9000)
        );
        assert_eq!(
            config_content_server_port("server_port: 8080\n").unwrap(),
            Some(8080)
        );
        assert_eq!(
            config_content_server_port("default:\n  project: A\n").unwrap(),
            None
        );
        assert!(config_content_server_port("server:\n  port: [not, a, number]\n").is_err());
    }
}
