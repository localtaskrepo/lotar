use std::collections::HashMap;
use std::path::Path;
use std::sync::{OnceLock, RwLock};

use super::identity_detectors as detectors_mod;
pub use detectors_mod::{DetectContext, IdentityDetection, IdentitySource};

// Detection cache keyed by tasks_root + env + git HEAD/config mtimes.
// A single cache backs both resolve_current_user and resolve_current_user_explain
// so the two APIs can never disagree.
static IDENTITY_CACHE: OnceLock<RwLock<HashMap<String, Option<IdentityDetection>>>> =
    OnceLock::new();

fn identity_cache() -> &'static RwLock<HashMap<String, Option<IdentityDetection>>> {
    IDENTITY_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn cache_key(tasks_root: Option<&Path>) -> String {
    let root_str = match tasks_root {
        Some(p) => p
            .canonicalize()
            .unwrap_or_else(|_| p.to_path_buf())
            .to_string_lossy()
            .to_string(),
        None => crate::utils::paths::tasks_root_from(std::path::Path::new("."))
            .to_string_lossy()
            .to_string(),
    };
    let env_def_reporter = std::env::var("LOTAR_DEFAULT_REPORTER").unwrap_or_default();

    // Include git config and HEAD mtimes so the key changes on updates/branch switches
    let start = tasks_root
        .and_then(|r| r.parent().map(|p| p.to_path_buf()))
        .or_else(|| std::env::current_dir().ok());
    let (git_cfg_mtime, head_mtime) = if let Some(start) = start
        && let Some(repo_root) = crate::utils::git::find_repo_root(&start)
    {
        let mtime_of = |rel: &str| {
            std::fs::metadata(repo_root.join(".git").join(rel))
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .map(|d| d.as_secs())
                .unwrap_or(0)
        };
        (mtime_of("config"), mtime_of("HEAD"))
    } else {
        (0, 0)
    };

    format!(
        "{}|DEF_REP={}|GCFG_M={}|HEAD_M={}",
        root_str, env_def_reporter, git_cfg_mtime, head_mtime
    )
}

/// Resolve current user identity used for reporter/assignee detection.
/// Delegates to the shared detector chain (see `identity_detectors`), so the
/// answer is always consistent with `resolve_current_user_explain`.
pub fn resolve_current_user(tasks_root: Option<&Path>) -> Option<String> {
    resolve_current_user_explain(tasks_root).map(|d| d.user)
}

/// Resolve with explain - returns detection details instead of just the string.
pub fn resolve_current_user_explain(tasks_root: Option<&Path>) -> Option<IdentityDetection> {
    let key = cache_key(tasks_root);
    if let Ok(guard) = identity_cache().read()
        && let Some(cached) = guard.get(&key)
    {
        return cached.clone();
    }

    let ctx = DetectContext { tasks_root };
    let found = detectors_mod::detect_identity(&ctx);
    if let Ok(mut guard) = identity_cache().write() {
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

/// Invalidate cached identity (clear all entries; conservative and safe)
pub fn invalidate_identity_cache(_tasks_root: Option<&Path>) {
    if let Ok(mut guard) = identity_cache().write() {
        guard.clear();
    }
}

/// Invalidate explain cache (kept for API compatibility; same cache)
pub fn invalidate_identity_explain_cache() {
    invalidate_identity_cache(None);
}
