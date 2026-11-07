use crate::utils::project::generate_project_prefix;
use crate::workspace::TasksDirectoryResolver;
use std::fs;
use std::path::{Path, PathBuf};

/// Get the effective project name by checking global config first, then falling back to auto-detection
pub fn get_effective_project_name(resolver: &TasksDirectoryResolver) -> String {
    // Try to read from global config first
    if let Ok(config) = crate::config::persistence::load_global_config(Some(&resolver.path)) {
        // If default_project is set (not empty), use it
        if !config.default_project.is_empty() {
            return config.default_project;
        }
    }

    // Fall back to auto-detection (but generate prefix from detected name)
    if let Some(project_name) = get_project_name() {
        generate_project_prefix(&project_name)
    } else {
        "DEFAULT".to_string()
    }
}

pub fn get_project_name() -> Option<String> {
    detect_project_name()
}

pub fn detect_project_name() -> Option<String> {
    detect_project_name_with_source().map(|(name, _)| name)
}

/// [`detect_project_name`] plus where the name came from, for messages such
/// as `add --explain` ("package.json at /repo/site/package.json").
pub fn detect_project_name_with_source() -> Option<(String, String)> {
    // 1. Check environment variable first
    if let Ok(project) = std::env::var("LOTAR_PROJECT")
        && !project.is_empty()
    {
        return Some((
            project,
            "the LOTAR_PROJECT environment variable".to_string(),
        ));
    }

    // 2. Try to detect from project files (nearest manifest upwards)
    if let Some((name, path)) = detect_from_project_files_with_path() {
        return Some((name, format!("the name in {}", path.display())));
    }

    // 3. Use git repo name when available
    if let Some(repo_root) = crate::utils::git::find_repo_root(&std::env::current_dir().ok()?)
        && let Some(repo_name) = repo_root.file_name().and_then(|s| s.to_str())
    {
        return Some((
            repo_name.to_string(),
            "the git repository folder name".to_string(),
        ));
    }

    // 4. Use current folder name
    if let Some(name) = get_current_folder_name() {
        return Some((name, "the current folder name".to_string()));
    }

    // Final fallback
    Some(("default".to_string(), "the built-in fallback".to_string()))
}

/// The name in the nearest `package.json`, `Cargo.toml` or `go.mod`, walking
/// up to the repository root, with that file's path.
fn detect_from_project_files_with_path() -> Option<(String, PathBuf)> {
    let mut dir = std::env::current_dir().ok()?;
    // Walk up to repo root (if known) or filesystem root
    let repo_root = crate::utils::git::find_repo_root(&dir);
    loop {
        // Prefer package.json name
        if let Some(name) = read_package_json_name(&dir) {
            return Some((name, dir.join("package.json")));
        }
        // Then Cargo.toml package name
        if let Some(name) = read_cargo_toml_name(&dir) {
            return Some((name, dir.join("Cargo.toml")));
        }
        // Then go.mod module name (last path segment)
        if let Some(name) = read_go_mod_name(&dir) {
            return Some((name, dir.join("go.mod")));
        }

        // Stop at repo root if found
        if let Some(ref root) = repo_root
            && &dir == root
        {
            break;
        }
        // Move up
        match dir.parent() {
            Some(parent) => dir = parent.to_path_buf(),
            None => break,
        }
    }
    None
}

fn read_cargo_toml_name(dir: &Path) -> Option<String> {
    let cargo_toml_path = dir.join("Cargo.toml");
    if !cargo_toml_path.exists() {
        return None;
    }

    let content = fs::read_to_string(&cargo_toml_path).ok()?;
    // Only treat as a package file if it has a [package] section
    if !content.contains("[package]") {
        return None;
    }
    // Simple regex-based parsing for the name field within Cargo.toml
    let re = regex::Regex::new(r#"(?m)^\s*name\s*=\s*"([^"]+)""#).ok()?;
    let captures = re.captures(&content)?;
    captures
        .get(1)
        .map(|m| m.as_str().to_string())
        .filter(|s| !s.is_empty())
}

fn read_package_json_name(dir: &Path) -> Option<String> {
    let pj = dir.join("package.json");
    if !pj.exists() {
        return None;
    }
    let content = fs::read_to_string(&pj).ok()?;
    let v: serde_json::Value = serde_json::from_str(&content).ok()?;
    let name = v.get("name")?.as_str()?.trim();
    if name.is_empty() {
        return None;
    }
    // Strip scope if present (e.g., @scope/name -> name)
    let cleaned = name.rsplit('/').next().unwrap_or(name).to_string();
    Some(cleaned)
}

fn read_go_mod_name(dir: &Path) -> Option<String> {
    let gm = dir.join("go.mod");
    if !gm.exists() {
        return None;
    }
    let content = fs::read_to_string(&gm).ok()?;
    for line in content.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("module ") {
            let last = rest.trim().trim_end_matches('/').rsplit('/').next()?;
            if !last.is_empty() {
                return Some(last.to_string());
            }
        }
    }
    None
}

fn get_current_folder_name() -> Option<String> {
    std::env::current_dir()
        .ok()?
        .file_name()?
        .to_str()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty() && s != "/" && s != ".")
}

pub fn get_project_path() -> Option<PathBuf> {
    std::env::current_dir().ok()
}
