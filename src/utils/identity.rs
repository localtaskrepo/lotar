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
    let git_stamp = if let Some(start) = start
        && let Some(repo_root) = crate::utils::git::find_repo_root(&start)
    {
        git_cache_fingerprint(&repo_root.join(".git"))
    } else {
        String::new()
    };

    format!(
        "{}|DEF_REP={}|GIT={}",
        root_str, env_def_reporter, git_stamp
    )
}

pub(crate) fn git_cache_fingerprint(git_entry: &Path) -> String {
    let Some((git_dir, common_dir)) = super::git::metadata_dirs(git_entry) else {
        return String::new();
    };
    let mtime_of = |path: &Path| {
        std::fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok()
    };
    let config_modified = mtime_of(&common_dir.join("config"));
    let head_modified = mtime_of(&git_dir.join("HEAD"));
    // Include target paths as well as stable timestamps: routing can change
    // without a timestamp change, and file age must not churn cache keys.
    format!(
        "{:?}",
        (git_dir, common_dir, config_modified, head_modified)
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
