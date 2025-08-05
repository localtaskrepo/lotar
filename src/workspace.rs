use std::path::{Path, PathBuf};
use std::env;
use std::fs;

#[derive(Debug, Clone)]
pub struct TasksDirectoryResolver {
    pub path: PathBuf,
    #[allow(dead_code)]
    pub source: TasksDirectorySource,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum TasksDirectorySource {
    CommandLineFlag,
    FoundInParent(PathBuf), // The parent directory where it was found
    CurrentDirectory,
}

impl TasksDirectoryResolver {
    /// Resolve the tasks directory based on command line args, environment, config, and discovery
    pub fn resolve(
        explicit_path: Option<&str>,
        global_config_tasks_folder: Option<&str>,
    ) -> Result<Self, String> {
        Self::resolve_internal(explicit_path, global_config_tasks_folder, None)
    }

    /// Function to resolve with home directory override (primarily for testing)
    /// 
    /// This function allows tests to specify a custom home directory path for configuration
    /// loading, enabling isolated testing of home config functionality.
    #[allow(dead_code)] // Used in integration tests
    pub fn resolve_with_home_override(
        explicit_path: Option<&str>,
        global_config_tasks_folder: Option<&str>,
        home_config_override: Option<PathBuf>,
    ) -> Result<Self, String> {
        Self::resolve_internal(explicit_path, global_config_tasks_folder, home_config_override)
    }

    /// Internal resolve function with optional home config override for testing
    fn resolve_internal(
        explicit_path: Option<&str>,
        global_config_tasks_folder: Option<&str>,
        home_config_override: Option<PathBuf>,
    ) -> Result<Self, String> {
        // 1. Command line flag takes highest priority
        if let Some(path) = explicit_path {
            let path_buf = PathBuf::from(path);
            if !path_buf.exists() {
                return Err(format!("Specified tasks directory does not exist: {}", path));
            }
            return Ok(TasksDirectoryResolver {
                path: path_buf,
                source: TasksDirectorySource::CommandLineFlag,
            });
        }

        // 2. Load home config to get tasks folder preference
        let home_tasks_folder = Self::get_home_config_tasks_folder(&home_config_override)?;
        
        // 3. Determine the folder name to search for (home config takes precedence over parameter)
        let folder_name = Self::get_tasks_folder_name(home_tasks_folder.as_deref().or(global_config_tasks_folder));

        // 4. Search up the directory tree
        if let Some((found_path, parent_dir)) = Self::find_tasks_folder_in_parents(&folder_name)? {
            return Ok(TasksDirectoryResolver {
                path: found_path.clone(),
                source: if Self::is_current_directory(&parent_dir) {
                    TasksDirectorySource::CurrentDirectory
                } else {
                    TasksDirectorySource::FoundInParent(parent_dir)
                },
            });
        }

        // 5. Default to current directory + folder name
        let current_dir = env::current_dir()
            .map_err(|e| format!("Failed to get current directory: {}", e))?;
        let tasks_path = current_dir.join(&folder_name);

        Ok(TasksDirectoryResolver {
            path: tasks_path,
            source: TasksDirectorySource::CurrentDirectory,
        })
    }

    /// Get the tasks folder name from environment, config, or default
    fn get_tasks_folder_name(global_config_tasks_folder: Option<&str>) -> String {
        // 1. Environment variable
        if let Ok(env_folder) = env::var("LOTAR_TASKS_FOLDER") {
            if !env_folder.trim().is_empty() {
                return env_folder;
            }
        }

        // 2. Global config setting
        if let Some(config_folder) = global_config_tasks_folder {
            if !config_folder.trim().is_empty() {
                return config_folder.to_string();
            }
        }

        // 3. Default
        ".tasks".to_string()
    }

    /// Get tasks folder preference from home configuration
    pub fn get_home_config_tasks_folder(home_config_override: &Option<PathBuf>) -> Result<Option<String>, String> {
        let home_config_path = match home_config_override {
            Some(override_path) => override_path.clone(),
            None => Self::get_default_home_config_path()?,
        };

        if !home_config_path.exists() {
            return Ok(None);
        }

        // Try to load the home config file
        match fs::read_to_string(&home_config_path) {
            Ok(content) => {
                // Simple YAML parsing to extract tasks_folder setting
                // This is a basic implementation - could be enhanced with full YAML parsing
                for line in content.lines() {
                    let line = line.trim();
                    if line.starts_with("tasks_folder:") {
                        if let Some(value) = line.split(':').nth(1) {
                            let value = value.trim().trim_matches('"').trim_matches('\'');
                            if !value.is_empty() {
                                return Ok(Some(value.to_string()));
                            }
                        }
                    }
                }
                Ok(None)
            }
            Err(_) => Ok(None), // Ignore read errors for home config
        }
    }

    /// Get the default home configuration path (cross-platform)
    pub fn get_default_home_config_path() -> Result<PathBuf, String> {
        // Use dirs crate for cross-platform home directory support
        let home_dir = dirs::home_dir()
            .ok_or("Unable to determine home directory")?;
        
        // On Windows, we might want to use a different location
        #[cfg(windows)]
        {
            // Try Windows-specific config location first
            if let Some(config_dir) = dirs::config_dir() {
                return Ok(config_dir.join("lotar").join("config.yml"));
            }
        }
        
        // Default to ~/.lotar for Unix-like systems and fallback for Windows
        Ok(home_dir.join(".lotar"))
    }

    /// Search up the directory tree for a tasks folder
    fn find_tasks_folder_in_parents(folder_name: &str) -> Result<Option<(PathBuf, PathBuf)>, String> {
        let mut current_dir = env::current_dir()
            .map_err(|e| format!("Failed to get current directory: {}", e))?;

        loop {
            let tasks_path = current_dir.join(folder_name);
            if tasks_path.exists() && tasks_path.is_dir() {
                return Ok(Some((tasks_path, current_dir)));
            }

            // Move up one directory
            match current_dir.parent() {
                Some(parent) => current_dir = parent.to_path_buf(),
                None => break, // Reached filesystem root
            }

            // Safety check: don't go above home directory
            if let Ok(home_dir) = env::var("HOME") {
                if current_dir == PathBuf::from(home_dir).parent().unwrap_or(Path::new("/")) {
                    break;
                }
            }
        }

        Ok(None)
    }

    /// Check if a path is in the current working directory
    fn is_current_directory(path: &Path) -> bool {
        if let Ok(current_dir) = env::current_dir() {
            path == current_dir
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_get_tasks_folder_name() {
        // Test environment variable
        unsafe {
            env::set_var("LOTAR_TASKS_FOLDER", ".tickets");
        }
        assert_eq!(TasksDirectoryResolver::get_tasks_folder_name(None), ".tickets");

        // Test config fallback
        unsafe {
            env::remove_var("LOTAR_TASKS_FOLDER");
        }
        assert_eq!(TasksDirectoryResolver::get_tasks_folder_name(Some(".issues")), ".issues");

        // Test default
        assert_eq!(TasksDirectoryResolver::get_tasks_folder_name(None), ".tasks");
    }

    #[test]
    fn test_explicit_path_resolution() {
        let temp_dir = TempDir::new().unwrap();
        let tasks_dir = temp_dir.path().join("custom_tasks");
        fs::create_dir_all(&tasks_dir).unwrap();

        let resolver = TasksDirectoryResolver::resolve(
            Some(tasks_dir.to_str().unwrap()),
            None,
        ).unwrap();

        assert_eq!(resolver.path, tasks_dir);
        matches!(resolver.source, TasksDirectorySource::CommandLineFlag);
    }
}
