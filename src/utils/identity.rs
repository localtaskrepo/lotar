use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

// Identity cache keyed by normalized tasks_root path; stores the resolved identity (or None)
static IDENTITY_CACHE: OnceLock<RwLock<HashMap<String, Option<String>>>> = OnceLock::new();

fn identity_cache() -> &'static RwLock<HashMap<String, Option<String>>> {
    IDENTITY_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn id_cache_key(tasks_root: Option<&Path>) -> String {
    let root: PathBuf = match tasks_root {
        Some(p) => p.to_path_buf(),
        None => crate::utils::paths::tasks_root_from(Path::new(".")),
    };
    let root_str = root
        .canonicalize()
        .unwrap_or(root)
        .to_string_lossy()
        .to_string();
    let env_def_reporter = std::env::var("LOTAR_DEFAULT_REPORTER").unwrap_or_default();
    format!("{}|DEF_REP={}", root_str, env_def_reporter)
}

/// Resolve current user identity used for reporter/assignee detection.
/// Order: config.default_reporter -> git config user.name/email -> system username
pub fn resolve_current_user(tasks_root: Option<&Path>) -> Option<String> {
    // Fast path: check cache first
    let key = id_cache_key(tasks_root);
    if let Ok(guard) = identity_cache().read() {
        if let Some(cached) = guard.get(&key) {
            return cached.clone();
        }
    }

    // Config default_reporter (global/project merged)
    if let Some(root) = tasks_root {
        if let Ok(cfg) = crate::config::resolution::load_and_merge_configs(Some(root)) {
            if let Some(rep) = cfg.default_reporter.and_then(|s| {
                let t = s.trim().to_string();
                if t.is_empty() { None } else { Some(t) }
            }) {
                // Cache and return
                if let Ok(mut guard) = identity_cache().write() {
                    guard.insert(key, Some(rep.clone()));
                }
                return Some(rep);
            }
        }
    } else if let Ok(cfg) = crate::config::resolution::load_and_merge_configs(None) {
        if let Some(rep) = cfg.default_reporter.and_then(|s| {
            let t = s.trim().to_string();
            if t.is_empty() { None } else { Some(t) }
        }) {
            if let Ok(mut guard) = identity_cache().write() {
                guard.insert(key, Some(rep.clone()));
            }
            return Some(rep);
        }
    }

    // Try git config user.name then user.email (best-effort, no git dependency in build)
    // Prefer repo inferred from tasks_root (its parent) when provided; else use cwd.
    let repo_root = tasks_root
        .and_then(|root| root.parent().map(|p| p.to_path_buf()))
        .or_else(|| std::env::current_dir().ok());
    if let Some(repo_root) = repo_root {
        let git_config = repo_root.join(".git").join("config");
        if git_config.exists() {
            if let Ok(contents) = std::fs::read_to_string(&git_config) {
                // Find user.name first
                for line in contents.lines() {
                    let line = line.trim();
                    if line.starts_with("name = ") {
                        let name = line.trim_start_matches("name = ").trim();
                        if !name.is_empty() {
                            let v = Some(name.to_string());
                            if let Ok(mut guard) = identity_cache().write() {
                                guard.insert(key.clone(), v.clone());
                            }
                            return v;
                        }
                    }
                }
                // Then user.email
                for line in contents.lines() {
                    let line = line.trim();
                    if line.starts_with("email = ") {
                        let email = line.trim_start_matches("email = ").trim();
                        if !email.is_empty() {
                            let v = Some(email.to_string());
                            if let Ok(mut guard) = identity_cache().write() {
                                guard.insert(key.clone(), v.clone());
                            }
                            return v;
                        }
                    }
                }
            }
        }
    }

    // System username via env
    if let Ok(user) = std::env::var("USER") {
        let t = user.trim();
        if !t.is_empty() {
            let v = Some(t.to_string());
            if let Ok(mut guard) = identity_cache().write() {
                guard.insert(key.clone(), v.clone());
            }
            return v;
        }
    }
    if let Ok(user) = std::env::var("USERNAME") {
        let t = user.trim();
        if !t.is_empty() {
            let v = Some(t.to_string());
            if let Ok(mut guard) = identity_cache().write() {
                guard.insert(key.clone(), v.clone());
            }
            return v;
        }
    }

    if let Ok(mut guard) = identity_cache().write() {
        guard.insert(key, None);
    }
    None
}

/// Resolve "@me" alias to the actual current user if present, otherwise
/// return the input unchanged. Returns None if resolving @me fails.
pub fn resolve_me_alias(input: &str, tasks_root: Option<&Path>) -> Option<String> {
    if input == "@me" {
        resolve_current_user(tasks_root)
    } else {
        Some(input.to_string())
    }
}

/// Invalidate cached identity for a given tasks_root
pub fn invalidate_identity_cache(tasks_root: Option<&Path>) {
    let key = id_cache_key(tasks_root);
    if let Ok(mut guard) = identity_cache().write() {
        guard.remove(&key);
    }
}
