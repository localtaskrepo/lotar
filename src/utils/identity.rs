use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

use super::identity_detectors as detectors_mod;
pub use detectors_mod::{DetectContext, IdentityDetection, IdentitySource};

// Identity cache keyed by normalized tasks_root path; stores the resolved identity (or None)
static IDENTITY_CACHE: OnceLock<RwLock<HashMap<String, Option<String>>>> = OnceLock::new();
// Explain cache keyed by tasks_root + env + git HEAD/config mtimes; stores detection details
static IDENTITY_EXPLAIN_CACHE: OnceLock<RwLock<HashMap<String, Option<IdentityDetection>>>> =
    OnceLock::new();

fn identity_cache() -> &'static RwLock<HashMap<String, Option<String>>> {
    IDENTITY_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn identity_explain_cache() -> &'static RwLock<HashMap<String, Option<IdentityDetection>>> {
    IDENTITY_EXPLAIN_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
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

fn explain_cache_key(tasks_root: Option<&Path>) -> String {
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

    // Include git config and HEAD mtimes so key changes on updates/branch switches
    let start = tasks_root
        .and_then(|r| r.parent().map(|p| p.to_path_buf()))
        .or_else(|| std::env::current_dir().ok());
    let (git_cfg_mtime, head_mtime) = if let Some(start) = start {
        if let Some(repo_root) = crate::utils::git::find_repo_root(&start) {
            let git_cfg = repo_root.join(".git").join("config");
            let head = repo_root.join(".git").join("HEAD");
            let cfg_m = std::fs::metadata(&git_cfg)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let head_m = std::fs::metadata(&head)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            (cfg_m, head_m)
        } else {
            (0, 0)
        }
    } else {
        (0, 0)
    };
    format!(
        "{}|DEF_REP={}|GCFG_M={}|HEAD_M={}",
        root_str, env_def_reporter, git_cfg_mtime, head_mtime
    )
}

/// Resolve current user identity used for reporter/assignee detection.
/// Order: config.default_reporter -> git config user.name/email -> system username
pub fn resolve_current_user(tasks_root: Option<&Path>) -> Option<String> {
    // Fast path: check cache first
    let key = id_cache_key(tasks_root);
    if let Ok(guard) = identity_cache().read()
        && let Some(cached) = guard.get(&key)
    {
        return cached.clone();
    }

    // Config default_reporter (global/project merged)
    if let Some(root) = tasks_root {
        if let Ok(cfg) = crate::config::resolution::load_and_merge_configs(Some(root))
            && let Some(rep_raw) = cfg.default_reporter
        {
            let trimmed = rep_raw.trim();
            if !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("@me") {
                let rep = trimmed.to_string();
                if let Ok(mut guard) = identity_cache().write() {
                    guard.insert(key.clone(), Some(rep.clone()));
                }
                return Some(rep);
            }
        }
    } else if let Ok(cfg) = crate::config::resolution::load_and_merge_configs(None)
        && let Some(rep_raw) = cfg.default_reporter
    {
        let trimmed = rep_raw.trim();
        if !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("@me") {
            let rep = trimmed.to_string();
            if let Ok(mut guard) = identity_cache().write() {
                guard.insert(key.clone(), Some(rep.clone()));
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
        if git_config.exists()
            && let Ok(contents) = std::fs::read_to_string(&git_config)
        {
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

/// Resolve with explain - returns detection details instead of just the string.
/// Falls back to the same precedence as resolve_current_user.
pub fn resolve_current_user_explain(tasks_root: Option<&Path>) -> Option<IdentityDetection> {
    let key = explain_cache_key(tasks_root);
    if let Ok(guard) = identity_explain_cache().read()
        && let Some(cached) = guard.get(&key)
    {
        return cached.clone();
    }

    let ctx = DetectContext { tasks_root };
    let found = detectors_mod::detect_identity(&ctx);
    if let Ok(mut guard) = identity_explain_cache().write() {
        guard.insert(key, found.clone());
    }
    found
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

/// Invalidate explain cache (clear all; conservative and safe)
pub fn invalidate_identity_explain_cache() {
    if let Ok(mut guard) = identity_explain_cache().write() {
        guard.clear();
    }
}
