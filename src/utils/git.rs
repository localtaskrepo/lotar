use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Anchor Git to the command's working directory, not the invoking process's repo.
/// Keep SSH/askpass and author/committer identity variables; injected Git config
/// is removed because it can override core.worktree and repository discovery.
pub fn clear_repository_env(command: &mut Command) {
    const LOCAL_KEYS: &[&str] = &[
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_NAMESPACE",
        "GIT_GRAFT_FILE",
        "GIT_SHALLOW_FILE",
        "GIT_REPLACE_REF_BASE",
        "GIT_NO_REPLACE_OBJECTS",
        "GIT_PREFIX",
        "GIT_INTERNAL_SUPER_PREFIX",
        "GIT_SUPER_PREFIX",
        "GIT_IMPLICIT_WORK_TREE",
        "GIT_CEILING_DIRECTORIES",
        "GIT_DISCOVERY_ACROSS_FILESYSTEM",
    ];
    let config_keys: Vec<_> = std::env::vars_os()
        .map(|(key, _)| key)
        .chain(command.get_envs().map(|(key, _)| key.to_os_string()))
        .filter(|key| {
            key.to_str()
                .is_some_and(|key| key.to_ascii_uppercase().starts_with("GIT_CONFIG"))
        })
        .collect();
    for key in LOCAL_KEYS {
        command.env_remove(key);
    }
    for key in config_keys {
        command.env_remove(key);
    }
}

pub fn git_command(root: &Path) -> Command {
    let mut command = Command::new("git");
    command.current_dir(root);
    clear_repository_env(&mut command);
    command
}

/// Verify discovery did not select a parent/foreign checkout and identify its repo.
pub fn verified_common_dir(root: &Path) -> std::io::Result<PathBuf> {
    let root = fs::canonicalize(root)?;
    let output = git_command(&root)
        .args([
            "rev-parse",
            "--path-format=absolute",
            "--show-toplevel",
            "--git-common-dir",
        ])
        .output()?;
    if !output.status.success() {
        return Err(std::io::Error::other("Cannot verify Git working directory"));
    }
    let text = String::from_utf8(output.stdout).map_err(std::io::Error::other)?;
    let mut lines = text.lines();
    let top = lines
        .next()
        .ok_or_else(|| std::io::Error::other("Missing Git top-level"))?;
    let common = lines
        .next()
        .ok_or_else(|| std::io::Error::other("Missing Git common directory"))?;
    if lines.next().is_some() || fs::canonicalize(top)? != root {
        return Err(std::io::Error::other(
            "Git top-level differs from intended checkout",
        ));
    }
    fs::canonicalize(common)
}

/// Find the git repo root by walking up from start until a .git directory or file is found.
pub fn find_repo_root(start: &Path) -> Option<PathBuf> {
    let mut cur = start;
    loop {
        let candidate = cur.join(".git");
        if candidate.is_dir() || candidate.is_file() {
            return Some(cur.to_path_buf());
        }
        cur = cur.parent()?;
    }
}

/// Read current branch from .git/HEAD (expects 'ref: refs/heads/<branch>')
pub fn read_current_branch(repo_root: &Path) -> Option<String> {
    let head = repo_root.join(".git").join("HEAD");
    let contents = fs::read_to_string(head).ok()?;
    let line = contents.lines().next()?.trim();
    if let Some(rest) = line.strip_prefix("ref: ")
        && let Some(branch) = rest.strip_prefix("refs/heads/")
    {
        return Some(branch.to_string());
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
