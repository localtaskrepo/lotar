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

/// Try to produce a path string relative to the git repo root, else to cwd, else absolute.
pub fn repo_relative_display(path: &Path) -> String {
    let repo_rel = path
        .parent()
        .and_then(crate::utils::git::find_repo_root)
        .and_then(|root| path.strip_prefix(&root).ok())
        .map(|p| p.to_path_buf());
    if let Some(rel) = repo_rel {
        return rel.display().to_string();
    }
    if let Ok(cwd) = std::env::current_dir() {
        if let Ok(rel) = path.strip_prefix(&cwd) {
            return rel.display().to_string();
        }
    }
    path.display().to_string()
}
