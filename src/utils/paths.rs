use std::path::{Path, PathBuf};

/// Return the tasks root directory path from a base directory (usually cwd)
pub fn tasks_root_from(base: &Path) -> PathBuf {
    base.join(".tasks")
}

/// Return the project directory path under a tasks root
pub fn project_dir(tasks_root: &Path, project_prefix: &str) -> PathBuf {
    tasks_root.join(project_prefix)
}

/// Return the path to a project config.yml under a tasks root
pub fn project_config_path(tasks_root: &Path, project_prefix: &str) -> PathBuf {
    project_dir(tasks_root, project_prefix).join("config.yml")
}

/// Return the path to the global config.yml under a tasks root
pub fn global_config_path(tasks_root: &Path) -> PathBuf {
    tasks_root.join("config.yml")
}
