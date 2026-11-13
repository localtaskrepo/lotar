//! Tasks in another repository's LoTaR (DEV-386): `lotar:DEV-374` names
//! task DEV-374 in the repo registered as `lotar` under the global `repos`
//! setting. Only `depends_on` and `related` may hold such references. They
//! stay one-sided (the other repo is never written) and outside the local
//! relationship graph. A reference that cannot be read, for a repo that is
//! not registered, missing, or without the task, is unresolved: a
//! prerequisite stays open, as for a deleted local one.
use crate::config::types::{GlobalConfig, is_repo_name};
use crate::storage::TaskId;
use crate::storage::task::Task;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The repo name and canonical task ID of `repo:ID`, or `None` for anything else.
pub fn parse(raw: &str) -> Option<(String, String)> {
    let (repo, id) = raw.trim().split_once(':')?;
    if !is_repo_name(repo) {
        return None;
    }
    Some((repo.to_string(), TaskId::parse(id).ok()?.canonical()))
}

/// `raw` canonicalized when it is a cross-repo reference.
pub fn canonical(raw: &str) -> Option<String> {
    parse(raw).map(|(repo, id)| format!("{repo}:{id}"))
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExternalTask {
    /// `repo:ID` with the canonical ID.
    pub reference: String,
    pub title: String,
    pub status: String,
    /// Done by the other repo's done-state policy for its project.
    pub done: bool,
    /// The task's web link from the other repo's `server.public_url`.
    pub url: Option<String>,
}

/// Registered repos for the tasks folder `local_root`: the home config's,
/// then the workspace's own, which win for the same name.
pub fn registry(local_root: &Path) -> BTreeMap<String, String> {
    let mut repos = BTreeMap::new();
    if let Ok(home) = crate::config::persistence::load_home_config() {
        repos.extend(home.repos);
    }
    if let Ok(GlobalConfig { repos: local, .. }) = crate::config::persistence::load_global_config(Some(local_root)) {
        repos.extend(local);
    }
    repos
}

/// The tasks folder of registered repo `name`. Relative paths start at the
/// repository that holds `local_root`.
pub fn tasks_dir(local_root: &Path, name: &str) -> Result<PathBuf, String> {
    let registered = registry(local_root);
    let raw = registered.get(name).ok_or_else(|| format!("repo '{name}' is not registered in the repos setting"))?;
    let path = match raw.strip_prefix("~/") {
        Some(rest) => dirs::home_dir().ok_or("home directory not found")?.join(rest),
        None => PathBuf::from(raw),
    };
    let path = if path.is_relative() { local_root.parent().unwrap_or(local_root).join(path) } else { path };
    let nested = path.join(".tasks");
    if nested.is_dir() {
        return Ok(nested);
    }
    if path.is_dir() {
        return Ok(path);
    }
    Err(format!("repo '{name}' has no tasks folder at {}", path.display()))
}

/// Read the task `raw` (`repo:ID`) names. Moved tasks are followed to their
/// new ID; deleted ones are unresolved.
pub fn resolve(local_root: &Path, raw: &str) -> Result<ExternalTask, String> {
    let (repo, id) = parse(raw).ok_or_else(|| format!("'{raw}' is not a repo:ID reference"))?;
    let root = tasks_dir(local_root, &repo)?;
    let mut id = TaskId::parse(&id).map_err(|e| e.to_string())?;
    for _ in 0..=crate::storage::moves::MAX_MOVE_HOPS {
        let task = read_task(&root, &id).map_err(|reason| format!("{repo}:{} {reason}", id.canonical()))?;
        if let Some(next) = task.moved_to.as_deref().and_then(|next| TaskId::parse(next.trim()).ok()) {
            id = next;
            continue;
        }
        if task.deleted_at.is_some() {
            return Err(format!("{repo}:{} is deleted", id.canonical()));
        }
        let canonical = id.canonical();
        let config = crate::config::resolution::config_for_project(&root, Some(&id.project))
            .map_err(|e| format!("{repo}:{canonical} has an unreadable configuration: {e}"))?;
        let status = if task.status.is_empty() { config.effective_default_status().unwrap_or_default() } else { task.status.clone() };
        let done = crate::services::completion::effective_done_statuses(&config).contains(&status.as_str().to_ascii_lowercase());
        return Ok(ExternalTask {
            url: config.task_url(&canonical),
            reference: format!("{repo}:{canonical}"),
            title: task.title,
            status: status.to_string(),
            done,
        });
    }
    Err(format!("{repo}:{} moves too often", id.canonical()))
}

/// `raw` resolved for the REST API: the task, or why it is unresolved.
pub fn lookup(local_root: &Path, raw: &str) -> crate::api_types::CrossRepoTaskDTO {
    match resolve(local_root, raw) {
        Ok(task) => crate::api_types::CrossRepoTaskDTO {
            reference: task.reference,
            title: Some(task.title),
            status: Some(task.status),
            done: Some(task.done),
            url: task.url,
            unresolved: None,
        },
        Err(reason) => crate::api_types::CrossRepoTaskDTO {
            reference: canonical(raw).unwrap_or_else(|| raw.trim().to_string()),
            title: None,
            status: None,
            done: None,
            url: None,
            unresolved: Some(reason),
        },
    }
}

/// One line for a relationship entry: a cross-repo task with its status,
/// title and link (or why it is unresolved); any other value as it is.
pub fn describe(local_root: &Path, raw: &str) -> String {
    if parse(raw).is_none() {
        return raw.to_string();
    }
    match resolve(local_root, raw) {
        Ok(task) => {
            let mut line = format!("{} [{}] {}", task.reference, task.status, task.title);
            if let Some(url) = task.url {
                line.push_str(&format!(" {url}"));
            }
            line
        }
        Err(reason) => format!("{} (unresolved: {reason})", canonical(raw).unwrap_or_else(|| raw.to_string())),
    }
}

fn read_task(root: &Path, id: &TaskId) -> Result<Task, String> {
    // The other repo's sibling workspace roots count, as they do there.
    let locations = id.locate(root);
    let [location] = locations.as_slice() else {
        return Err(if locations.is_empty() { "does not exist".into() } else { "is stored more than once".into() });
    };
    let bytes = std::fs::read(&location.file).map_err(|e| format!("is unreadable: {e}"))?;
    serde_yaml_ng::from_slice(&bytes).map_err(|e| format!("is unreadable: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_repo_references_only() {
        assert_eq!(parse("lotar:DEV-0374"), Some(("lotar".into(), "DEV-374".into())));
        assert_eq!(canonical(" my.repo_2:STD-5 "), Some("my.repo_2:STD-5".into()));
        for raw in ["DEV-374", ":DEV-1", "-x:DEV-1", "lo tar:DEV-1", "lotar:DEV", "https://x/DEV-1", "a/b:DEV-1"] {
            assert_eq!(parse(raw), None, "{raw}");
        }
    }
}
