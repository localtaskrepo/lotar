use std::fs;
use std::path::{Path, PathBuf};

/// Find the git repo root by walking up from start until a .git directory or file is found.
pub fn find_repo_root(start: &Path) -> Option<PathBuf> {
    let mut cur = start;
    loop {
        let candidate = cur.join(".git");
        if candidate.is_dir() || candidate.is_file() {
            return Some(cur.to_path_buf());
        }
        match cur.parent() {
            Some(p) => cur = p,
            None => return None,
        }
    }
}

/// Read current branch from .git/HEAD (expects 'ref: refs/heads/<branch>')
pub fn read_current_branch(repo_root: &Path) -> Option<String> {
    let head = repo_root.join(".git").join("HEAD");
    let contents = fs::read_to_string(head).ok()?;
    let line = contents.lines().next()?.trim();
    if let Some(rest) = line.strip_prefix("ref: ") {
        if let Some(branch) = rest.strip_prefix("refs/heads/") {
            return Some(branch.to_string());
        }
    }
    None
}

/// Parse remotes from .git/config by looking for [remote "*"] sections and their url.
pub fn read_remotes(repo_root: &Path) -> Vec<String> {
    let config_path = repo_root.join(".git").join("config");
    let contents = match fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let mut remotes = Vec::new();
    let mut in_remote = false;
    for line in contents.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_remote = t.starts_with("[remote ");
            continue;
        }
        if in_remote && t.starts_with("url = ") {
            remotes.push(t.trim_start_matches("url = ").trim().to_string());
        }
    }
    remotes
}
