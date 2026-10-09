use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Anchor Git to the command's working directory, not the invoking process's repo.
/// Keep SSH/askpass and author/committer identity variables; injected Git config
/// is removed because it can override core.worktree and repository discovery.
const REPOSITORY_ROUTING_KEYS: &[&str] = &[
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

pub fn clear_repository_env(command: &mut Command) {
    let config_keys: Vec<_> = std::env::vars_os()
        .map(|(key, _)| key)
        .chain(command.get_envs().map(|(key, _)| key.to_os_string()))
        .filter(|key| {
            key.to_str()
                .is_some_and(|key| key.to_ascii_uppercase().starts_with("GIT_CONFIG"))
        })
        .collect();
    for key in REPOSITORY_ROUTING_KEYS {
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

/// Repository routing keys removed by [`clear_repository_env`]: presence in
/// an environment can direct Git discovery and writes outside the command's
/// working directory, and injected `GIT_CONFIG*` keys can override
/// `core.worktree`. Transport/auth/identity variables (SSH, askpass,
/// author/committer) are deliberately NOT routing keys. Matching is
/// case-insensitive so Windows env folds cannot slip a variant through.
pub fn is_repository_routing_key(key: &str) -> bool {
    REPOSITORY_ROUTING_KEYS
        .iter()
        .any(|routing| routing.eq_ignore_ascii_case(key))
        || key.to_ascii_uppercase().starts_with("GIT_CONFIG")
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

/// Read the checkout's own HEAD, including gitdir-file linked worktrees.
pub fn read_current_branch(repo_root: &Path) -> Option<String> {
    read_branch_at(&repo_root.join(".git"))
}

fn read_branch_at(git_entry: &Path) -> Option<String> {
    let (git_dir, _) = metadata_dirs(git_entry)?;
    let head = git_dir.join("HEAD");
    let line = metadata_line(&head)?;
    let branch = line.strip_prefix("ref: refs/heads/")?;
    if branch.is_empty()
        || branch
            .chars()
            .any(|c| c.is_ascii_whitespace() || c.is_ascii_control())
    {
        return None;
    }
    Some(branch.to_string())
}

/// Parse local remotes from the shared Git config, not the worktree admin directory.
pub fn read_remotes(repo_root: &Path) -> Vec<String> {
    read_remotes_at(&repo_root.join(".git"))
}

fn read_remotes_at(git_entry: &Path) -> Vec<String> {
    let Some((_, common_dir)) = metadata_dirs(git_entry) else {
        return vec![];
    };
    let config_path = common_dir.join("config");
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

/// Read-only metadata location, not the ownership verification used for Git writes.
pub(crate) fn metadata_dirs(git_entry: &Path) -> Option<(PathBuf, PathBuf)> {
    let git_dir = if git_entry.is_dir() {
        fs::canonicalize(git_entry).ok()?
    } else {
        let marker = metadata_line(git_entry)?;
        metadata_directory(marker.strip_prefix("gitdir: ")?, git_entry.parent()?)?
    };
    let common_file = git_dir.join("commondir");
    let common_dir = match fs::symlink_metadata(&common_file) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => git_dir.clone(),
        Ok(_) => metadata_directory(&metadata_line(&common_file)?, &git_dir)?,
        Err(_) => return None,
    };
    Some((git_dir, common_dir))
}

fn metadata_line(path: &Path) -> Option<String> {
    if !fs::metadata(path).ok()?.is_file() {
        return None;
    }
    let contents = fs::read_to_string(path).ok()?;
    let mut lines = contents.lines();
    let line = lines.next()?;
    if lines.next().is_some() {
        return None;
    }
    Some(line.to_string())
}

fn metadata_directory(value: &str, base: &Path) -> Option<PathBuf> {
    if value.is_empty() || value.contains('\0') {
        return None;
    }
    let path = Path::new(value);
    let directory = fs::canonicalize(if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    })
    .ok()?;
    directory.is_dir().then_some(directory)
}

#[cfg(test)]
#[path = "../../tests/common/dev73_git_metadata_cases.rs"]
mod metadata_tests;

#[cfg(test)]
mod routing_tests {
    use super::is_repository_routing_key;

    /// The lib-test ctor scrubs routing variables once per process; nothing
    /// that matches the shared inventory may be present afterwards.
    #[test]
    fn lib_test_baseline_has_no_repository_routing_env() {
        for (key, _) in std::env::vars() {
            assert!(
                !is_repository_routing_key(&key),
                "routing variable {key} survived the lib-test baseline"
            );
        }
    }
}
