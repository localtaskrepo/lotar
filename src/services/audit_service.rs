use chrono::{DateTime, Datelike, Duration, Utc};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::storage::task::Task;
use crate::types::{TaskChange, TaskChangeLogEntry};

#[derive(Debug, Serialize, Clone)]
pub struct FileCommitEvent {
    pub commit: String,
    pub author: String,
    pub email: String,
    pub date: DateTime<Utc>,
    pub message: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct ActivityFeedChange {
    pub field: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct ActivityFeedHistoryEntry {
    pub at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    pub changes: Vec<ActivityFeedChange>,
}

#[derive(Debug, Serialize, Clone)]
pub struct ActivityFeedItem {
    pub commit: String,
    pub author: String,
    pub email: String,
    pub date: DateTime<Utc>,
    pub message: String,
    pub task_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_title: Option<String>,
    pub history: Vec<ActivityFeedHistoryEntry>,
}

#[derive(Debug, Serialize, Clone)]
pub struct PropertyChange<T> {
    pub field: String,
    pub old: Option<T>,
    pub new: Option<T>,
    pub at: DateTime<Utc>,
    pub commit: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct TaskHistory<T> {
    pub id: String,
    pub file: PathBuf,
    pub changes: Vec<PropertyChange<T>>,
}

#[derive(Debug, Clone)]
struct ParsedCommit {
    sha: String,
    author: String,
    email: String,
    date: DateTime<Utc>,
    message: String,
    files: Vec<PathBuf>,
}

pub struct AuditService;

fn classify_change(field: &str) -> &'static str {
    let lower = field.to_lowercase();
    if lower == "created" {
        return "created";
    }
    if lower.starts_with("comment") {
        return "comment";
    }
    if lower.contains("status") {
        return "status";
    }
    if lower.contains("assignee") || lower.contains("reporter") || lower.contains("owner") {
        return "assignment";
    }
    if lower.contains("tag") {
        return "tags";
    }
    if lower.contains("relationship") {
        return "relationships";
    }
    if lower.contains("custom") {
        return "custom";
    }
    if lower.contains("description") || lower.contains("title") || lower.contains("subtitle") {
        return "content";
    }
    if lower.contains("due") || lower.contains("effort") {
        return "planning";
    }
    "other"
}

fn history_signature(entry: &TaskChangeLogEntry) -> String {
    let mut key = String::new();
    key.push_str(entry.at.as_str());
    key.push('|');
    if let Ok(json) = serde_json::to_string(&entry.changes) {
        key.push_str(json.as_str());
    }
    key
}

fn extract_new_history_entries(
    previous: &[TaskChangeLogEntry],
    current: &[TaskChangeLogEntry],
) -> Vec<TaskChangeLogEntry> {
    let mut seen: HashSet<String> = HashSet::new();
    for entry in previous {
        seen.insert(history_signature(entry));
    }
    current
        .iter()
        .filter(|entry| !seen.contains(&history_signature(entry)))
        .cloned()
        .collect()
}

fn to_feed_history_entry(entry: TaskChangeLogEntry) -> ActivityFeedHistoryEntry {
    ActivityFeedHistoryEntry {
        at: entry.at,
        actor: entry.actor,
        changes: entry
            .changes
            .into_iter()
            .map(|change: TaskChange| {
                let TaskChange { field, old, new } = change;
                ActivityFeedChange {
                    kind: classify_change(&field).to_string(),
                    field,
                    old,
                    new,
                }
            })
            .collect(),
    }
}

impl AuditService {
    fn git_path_str(path: &Path) -> String {
        #[cfg(windows)]
        {
            path.to_string_lossy().replace('\\', "/")
        }
        #[cfg(not(windows))]
        {
            path.to_string_lossy().to_string()
        }
    }

    fn git_path_arg(path: &Path) -> OsString {
        OsString::from(Self::git_path_str(path))
    }

    /// List commits touching a specific file (relative to repo root)
    pub fn list_commits_for_file(
        repo_root: &Path,
        file_rel: &Path,
    ) -> Result<Vec<FileCommitEvent>, String> {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(repo_root);
        cmd.arg("log");
        cmd.arg("--no-merges");
        cmd.arg("--pretty=format:%H%x00%an%x00%ae%x00%cI%x00%s");
        cmd.arg("--");
        cmd.arg(Self::git_path_arg(file_rel));

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run git: {}", e))?;
        if !output.status.success() {
            return Err(format!(
                "git log failed (status {}): {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut items = Vec::new();
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if !trimmed.contains('\u{0000}') {
                continue;
            }
            let parts: Vec<&str> = trimmed.split('\u{0000}').collect();
            if parts.len() < 5 {
                continue;
            }
            let date = match chrono::DateTime::parse_from_rfc3339(parts[3]) {
                Ok(dt) => dt.with_timezone(&Utc),
                Err(_) => continue,
            };
            items.push(FileCommitEvent {
                commit: parts[0].to_string(),
                author: parts[1].to_string(),
                email: parts[2].to_string(),
                date,
                message: parts[4].to_string(),
            });
        }

        Ok(items)
    }

    pub fn list_activity_feed(
        repo_root: &Path,
        tasks_rel: &Path,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
        project_filter: Option<&str>,
        max_commits: usize,
    ) -> Result<Vec<ActivityFeedItem>, String> {
        let tasks_root_abs = repo_root.join(tasks_rel);
        let tasks_tracked = Self::tasks_are_tracked(repo_root, tasks_rel);
        let resolved_project_path = project_filter
            .and_then(|project| Self::resolve_project_path(&tasks_root_abs, tasks_rel, project));

        let path_filters = if tasks_tracked {
            let mut filters = Vec::new();
            if let Some(path) = resolved_project_path.clone() {
                filters.push(path);
            } else {
                filters.push(tasks_rel.to_path_buf());
            }
            Some(filters)
        } else {
            None
        };

        let commits = Self::collect_commits(repo_root, since, until, max_commits, path_filters)?;

        if tasks_tracked {
            Self::build_tracked_activity_feed(repo_root, tasks_rel, commits, project_filter)
        } else {
            Self::build_untracked_activity_feed(
                &tasks_root_abs,
                tasks_rel,
                commits,
                since,
                until,
                project_filter,
            )
        }
    }

    fn tasks_are_tracked(repo_root: &Path, tasks_rel: &Path) -> bool {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(repo_root);
        cmd.arg("ls-files");
        cmd.arg(Self::git_path_arg(tasks_rel));
        match cmd.output() {
            Ok(output) if output.status.success() => !output.stdout.is_empty(),
            _ => false,
        }
    }

    fn resolve_project_path(
        tasks_root_abs: &Path,
        tasks_rel: &Path,
        project_filter: &str,
    ) -> Option<PathBuf> {
        if let Ok(entries) = fs::read_dir(tasks_root_abs) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let name = match path.file_name().and_then(|s| s.to_str()) {
                    Some(name) => name,
                    None => continue,
                };
                if name.eq_ignore_ascii_case(project_filter) {
                    return Some(tasks_rel.join(name));
                }
            }
        }
        None
    }

    fn collect_commits(
        repo_root: &Path,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
        max_commits: usize,
        path_filters: Option<Vec<PathBuf>>,
    ) -> Result<Vec<ParsedCommit>, String> {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(repo_root);
        cmd.arg("log");
        cmd.arg("--no-merges");
        cmd.arg(format!("--since={}", since.to_rfc3339()));
        cmd.arg(format!("--until={}", until.to_rfc3339()));
        if max_commits > 0 {
            cmd.arg(format!("--max-count={}", max_commits));
        }
        cmd.arg("--name-only");
        cmd.arg("--pretty=format:%H%x00%an%x00%ae%x00%cI%x00%s");
        cmd.arg("--");
        if let Some(filters) = path_filters {
            for filter in filters {
                cmd.arg(Self::git_path_arg(&filter));
            }
        }

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run git: {}", e))?;
        if !output.status.success() {
            return Err(format!(
                "git log failed (status {}): {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut commits: Vec<ParsedCommit> = Vec::new();
        let mut current: Option<ParsedCommit> = None;
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                if let Some(commit) = current.take() {
                    commits.push(commit);
                }
                continue;
            }
            if trimmed.contains('\u{0000}') {
                if let Some(commit) = current.take() {
                    commits.push(commit);
                }
                let parts: Vec<&str> = trimmed.split('\u{0000}').collect();
                if parts.len() < 5 {
                    continue;
                }
                let date = match chrono::DateTime::parse_from_rfc3339(parts[3]) {
                    Ok(dt) => dt.with_timezone(&Utc),
                    Err(_) => continue,
                };
                current = Some(ParsedCommit {
                    sha: parts[0].to_string(),
                    author: parts[1].to_string(),
                    email: parts[2].to_string(),
                    date,
                    message: parts[4].to_string(),
                    files: Vec::new(),
                });
                continue;
            }
            if let Some(commit) = current.as_mut() {
                commit.files.push(PathBuf::from(trimmed));
            }
        }
        if let Some(commit) = current.take() {
            commits.push(commit);
        }
        Ok(commits)
    }

    fn build_tracked_activity_feed(
        repo_root: &Path,
        tasks_rel: &Path,
        commits: Vec<ParsedCommit>,
        project_filter: Option<&str>,
    ) -> Result<Vec<ActivityFeedItem>, String> {
        let mut feed: Vec<ActivityFeedItem> = Vec::new();

        for commit in commits.into_iter() {
            if commit.files.is_empty() {
                continue;
            }
            let mut per_task: HashMap<String, ActivityFeedItem> = HashMap::new();
            for file_rel in commit.files.iter() {
                if !file_rel.starts_with(tasks_rel) {
                    continue;
                }
                if file_rel.extension().and_then(|e| e.to_str()) != Some("yml") {
                    continue;
                }
                let (task_id, project_prefix) = match Self::task_id_from_path(tasks_rel, file_rel) {
                    Some(data) => data,
                    None => continue,
                };
                if let Some(filter) = project_filter
                    && !project_prefix.eq_ignore_ascii_case(filter)
                {
                    continue;
                }

                let current_task = Self::load_task_version(repo_root, &commit.sha, file_rel);
                let previous_task =
                    Self::load_task_version(repo_root, &format!("{}^", commit.sha), file_rel);

                let current_history = current_task
                    .as_ref()
                    .map(|task| task.history.clone())
                    .unwrap_or_default();
                let previous_history = previous_task
                    .as_ref()
                    .map(|task| task.history.clone())
                    .unwrap_or_default();

                let mut new_entries =
                    extract_new_history_entries(&previous_history, &current_history);
                if new_entries.is_empty() {
                    continue;
                }
                new_entries.sort_by(|a, b| a.at.cmp(&b.at));

                let title = current_task
                    .as_ref()
                    .or(previous_task.as_ref())
                    .map(|task| task.title.clone());

                let entry = per_task
                    .entry(task_id.clone())
                    .or_insert_with(|| ActivityFeedItem {
                        commit: commit.sha.clone(),
                        author: commit.author.clone(),
                        email: commit.email.clone(),
                        date: commit.date,
                        message: commit.message.clone(),
                        task_id: task_id.clone(),
                        task_title: title.clone(),
                        history: Vec::new(),
                    });
                if entry.task_title.is_none() {
                    entry.task_title = title;
                }

                entry
                    .history
                    .extend(new_entries.into_iter().map(to_feed_history_entry));
            }

            for mut item in per_task.into_values() {
                item.history.sort_by(|a, b| a.at.cmp(&b.at));
                if !item.history.is_empty() {
                    feed.push(item);
                }
            }
        }

        feed.sort_by(|a, b| {
            b.date
                .cmp(&a.date)
                .then_with(|| b.commit.cmp(&a.commit))
                .then_with(|| b.task_id.cmp(&a.task_id))
        });

        Ok(feed)
    }

    fn build_untracked_activity_feed(
        tasks_root_abs: &Path,
        tasks_rel: &Path,
        commits: Vec<ParsedCommit>,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
        project_filter: Option<&str>,
    ) -> Result<Vec<ActivityFeedItem>, String> {
        if !tasks_root_abs.exists() {
            return Ok(Vec::new());
        }

        let commit_refs = commits;
        let mut feed: HashMap<(String, String), ActivityFeedItem> = HashMap::new();
        let project_filter_lower = project_filter.map(|p| p.to_string());

        let projects = fs::read_dir(tasks_root_abs).map_err(|e| {
            format!(
                "Failed to read tasks directory {}: {}",
                tasks_root_abs.display(),
                e
            )
        })?;

        for project_entry in projects.flatten() {
            let project_path = project_entry.path();
            if !project_path.is_dir() {
                continue;
            }
            let project_name = match project_path.file_name().and_then(|s| s.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };
            if let Some(filter) = project_filter_lower.as_ref()
                && !project_name.eq_ignore_ascii_case(filter)
            {
                continue;
            }

            let tasks = fs::read_dir(&project_path).map_err(|e| {
                format!(
                    "Failed to read project directory {}: {}",
                    project_path.display(),
                    e
                )
            })?;

            for task_entry in tasks.flatten() {
                let file_path = task_entry.path();
                if file_path.extension().and_then(|ext| ext.to_str()) != Some("yml") {
                    continue;
                }

                let rel = match file_path.strip_prefix(tasks_root_abs) {
                    Ok(rel) => rel,
                    Err(_) => continue,
                };
                let repo_rel = tasks_rel.join(rel);
                let (task_id, _project_prefix) = match Self::task_id_from_path(tasks_rel, &repo_rel)
                {
                    Some(info) => info,
                    None => continue,
                };

                let raw = match fs::read_to_string(&file_path) {
                    Ok(content) => content,
                    Err(_) => continue,
                };
                let task = match Self::parse_task_yaml(&raw) {
                    Some(task) => task,
                    None => continue,
                };

                let mut entries: Vec<TaskChangeLogEntry> = task
                    .history
                    .into_iter()
                    .filter(|entry| match Self::parse_history_timestamp(entry) {
                        Some(ts) => ts >= since && ts <= until,
                        None => false,
                    })
                    .collect();
                if entries.is_empty() {
                    continue;
                }
                entries.sort_by(|a, b| a.at.cmp(&b.at));

                let task_title = task.title.clone();

                for entry in entries.into_iter() {
                    let entry_time = match Self::parse_history_timestamp(&entry) {
                        Some(ts) => ts,
                        None => continue,
                    };
                    let actor = entry.actor.clone();
                    let matched_commit = Self::match_commit_for_entry(
                        &commit_refs,
                        &task_id,
                        entry_time,
                        actor.as_deref(),
                    );
                    let (commit_id, author, email, message, date) =
                        if let Some(commit) = matched_commit {
                            (
                                commit.sha.clone(),
                                commit.author.clone(),
                                commit.email.clone(),
                                commit.message.clone(),
                                commit.date,
                            )
                        } else {
                            (
                                format!("history-{}-{}", task_id, entry_time.timestamp()),
                                actor.unwrap_or_else(|| "Unknown".to_string()),
                                String::new(),
                                format!("Task update for {}", task_id),
                                entry_time,
                            )
                        };

                    let key = (commit_id.clone(), task_id.clone());
                    let item = feed.entry(key).or_insert_with(|| ActivityFeedItem {
                        commit: commit_id.clone(),
                        author,
                        email,
                        date,
                        message,
                        task_id: task_id.clone(),
                        task_title: Some(task_title.clone()),
                        history: Vec::new(),
                    });
                    if item.task_title.is_none() {
                        item.task_title = Some(task_title.clone());
                    }
                    item.history.push(to_feed_history_entry(entry));
                }
            }
        }

        let mut items: Vec<ActivityFeedItem> = feed
            .into_values()
            .map(|mut item| {
                item.history.sort_by(|a, b| a.at.cmp(&b.at));
                item
            })
            .collect();

        items.sort_by(|a, b| {
            b.date
                .cmp(&a.date)
                .then_with(|| b.commit.cmp(&a.commit))
                .then_with(|| b.task_id.cmp(&a.task_id))
        });

        Ok(items)
    }

    fn parse_history_timestamp(entry: &TaskChangeLogEntry) -> Option<DateTime<Utc>> {
        chrono::DateTime::parse_from_rfc3339(entry.at.as_str())
            .map(|dt| dt.with_timezone(&Utc))
            .ok()
    }

    fn match_commit_for_entry<'a>(
        commits: &'a [ParsedCommit],
        task_id: &str,
        entry_time: DateTime<Utc>,
        actor: Option<&str>,
    ) -> Option<&'a ParsedCommit> {
        if commits.is_empty() {
            return None;
        }

        let id_upper = task_id.to_uppercase();
        let mut best: Option<(&ParsedCommit, i64)> = None;
        for commit in commits {
            if commit.message.to_uppercase().contains(&id_upper) {
                let diff = (commit.date - entry_time).num_seconds().abs();
                if best.is_none_or(|(_, best_diff)| diff < best_diff) {
                    best = Some((commit, diff));
                }
            }
        }
        if let Some((commit, _)) = best {
            return Some(commit);
        }

        if let Some(actor_value) = actor {
            let actor_lower = actor_value.to_lowercase();
            let mut best_actor: Option<(&ParsedCommit, i64)> = None;
            for commit in commits {
                if commit.author.to_lowercase().contains(&actor_lower) {
                    let diff = (commit.date - entry_time).num_seconds().abs();
                    if diff <= Duration::minutes(10).num_seconds()
                        && best_actor.is_none_or(|(_, best_diff)| diff < best_diff)
                    {
                        best_actor = Some((commit, diff));
                    }
                }
            }
            if let Some((commit, _)) = best_actor {
                return Some(commit);
            }
        }

        let mut best_time: Option<(&ParsedCommit, i64)> = None;
        for commit in commits {
            let diff = (commit.date - entry_time).num_seconds().abs();
            if diff <= Duration::minutes(5).num_seconds()
                && best_time.is_none_or(|(_, best_diff)| diff < best_diff)
            {
                best_time = Some((commit, diff));
            }
        }
        best_time.map(|(commit, _)| commit)
    }

    fn task_id_from_path(tasks_rel: &Path, file_rel: &Path) -> Option<(String, String)> {
        let rel = file_rel.strip_prefix(tasks_rel).ok()?;
        let project = rel.parent()?.file_name()?.to_str()?.to_string();
        let stem = rel.file_stem()?.to_str()?;
        let _numeric: u64 = stem.parse().ok()?;
        let id = format!("{}-{}", project, stem);
        Some((id, project))
    }

    fn parse_task_yaml(content: &str) -> Option<Task> {
        serde_yaml::from_str::<Task>(content).ok()
    }

    fn load_task_version(repo_root: &Path, commit: &str, file_rel: &Path) -> Option<Task> {
        match Self::show_file_at(repo_root, commit, file_rel) {
            Ok(content) => Self::parse_task_yaml(&content),
            Err(_) => None,
        }
    }

    /// Compute last change info for each task across full history (no time filter)
    pub fn list_last_change_per_task(
        repo_root: &Path,
        tasks_rel: &Path,
        project_filter: Option<&str>,
    ) -> Result<Vec<ChangedTaskSummary>, String> {
        fn visit_dir_collect<F: FnMut(&Path)>(dir: &Path, f: &mut F) {
            if let Ok(read) = std::fs::read_dir(dir) {
                for entry in read.flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        visit_dir_collect(&p, f);
                    } else {
                        f(&p);
                    }
                }
            }
        }

        let tasks_abs = repo_root.join(tasks_rel);
        let mut files: Vec<PathBuf> = Vec::new();
        if let Some(project) = project_filter {
            visit_dir_collect(&tasks_abs.join(project), &mut |p| {
                files.push(p.to_path_buf())
            });
        } else {
            visit_dir_collect(&tasks_abs, &mut |p| files.push(p.to_path_buf()));
        }

        let mut items: Vec<ChangedTaskSummary> = Vec::new();
        for abs_path in files {
            if abs_path.extension().and_then(|e| e.to_str()) != Some("yml") {
                continue;
            }

            let rel_path = match abs_path.strip_prefix(repo_root) {
                Ok(r) => r.to_path_buf(),
                Err(_) => continue,
            };
            if !rel_path.starts_with(tasks_rel) {
                continue;
            }

            let file_name = match rel_path.file_stem().and_then(|s| s.to_str()) {
                Some(name) => name,
                None => continue,
            };
            let numeric: u64 = match file_name.parse() {
                Ok(n) => n,
                Err(_) => continue,
            };
            let project = match rel_path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
            {
                Some(p) => p.to_string(),
                None => continue,
            };
            let id = format!("{}-{}", project, numeric);

            let mut log1 = Command::new("git");
            log1.arg("-C").arg(repo_root);
            log1.arg("log");
            log1.arg("-1");
            log1.arg("--no-merges");
            log1.arg("--pretty=format:%H%x00%an%x00%ae%x00%cI");
            log1.arg("--");
            log1.arg(&rel_path);
            let log_out = log1
                .output()
                .map_err(|e| format!("Failed to run git log -1: {}", e))?;
            if !log_out.status.success() {
                continue;
            }
            let header = String::from_utf8_lossy(&log_out.stdout);
            let parts: Vec<&str> = header.trim().split('\u{0000}').collect();
            if parts.len() < 4 {
                continue;
            }
            let last_commit = parts[0].to_string();
            let last_author = parts[1].to_string();
            let last_date = match chrono::DateTime::parse_from_rfc3339(parts[3]) {
                Ok(dt) => dt.with_timezone(&Utc),
                Err(_) => continue,
            };

            let commits: usize = {
                let mut rev = Command::new("git");
                rev.arg("-C").arg(repo_root);
                rev.arg("rev-list");
                rev.arg("--count");
                rev.arg("HEAD");
                rev.arg("--");
                rev.arg(&rel_path);
                match rev.output() {
                    Ok(output) if output.status.success() => {
                        String::from_utf8_lossy(&output.stdout)
                            .trim()
                            .parse::<usize>()
                            .unwrap_or(1)
                    }
                    _ => 1,
                }
            };

            items.push(ChangedTaskSummary {
                id,
                project,
                file: rel_path.to_string_lossy().to_string(),
                last_commit,
                last_author,
                last_date,
                commits,
            });
        }
        items.sort_by(|a, b| b.last_date.cmp(&a.last_date));
        Ok(items)
    }

    /// Show file contents at a specific commit (binary-safe as String lossily)
    pub fn show_file_at(repo_root: &Path, commit: &str, file_rel: &Path) -> Result<String, String> {
        let spec = format!("{}:{}", commit, Self::git_path_str(file_rel));
        let output = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .arg("show")
            .arg(spec)
            .output()
            .map_err(|e| format!("Failed to run git: {}", e))?;
        if !output.status.success() {
            return Err(format!(
                "git show failed (status {}): {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Get raw patch for a specific commit restricted to file
    pub fn show_file_diff(
        repo_root: &Path,
        commit: &str,
        file_rel: &Path,
    ) -> Result<String, String> {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .arg("show")
            .arg(commit)
            .arg("--")
            .arg(Self::git_path_arg(file_rel))
            .output()
            .map_err(|e| format!("Failed to run git: {}", e))?;
        if !output.status.success() {
            return Err(format!(
                "git show failed (status {}): {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// List changed task files within a time window under the tasks directory, grouped by ticket.
    /// - repo_root: path to the git repository root
    /// - tasks_rel: path to the tasks directory relative to repo_root (e.g., ".tasks")
    /// - since/until: time window in UTC
    /// - author_filter: optional substring (case-insensitive) to filter commit authors
    /// - project_filter: if Some(prefix), restrict to .tasks/<prefix>/ only
    pub fn list_changed_tasks(
        repo_root: &Path,
        tasks_rel: &Path,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
        author_filter: Option<&str>,
        project_filter: Option<&str>,
    ) -> Result<Vec<ChangedTaskSummary>, String> {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(repo_root);
        cmd.arg("log");
        cmd.arg(format!("--since={}", since.to_rfc3339()));
        cmd.arg(format!("--until={}", until.to_rfc3339()));
        cmd.arg("--name-only");
        cmd.arg("--no-merges");
        // Use NUL-separated header for reliable parsing
        cmd.arg("--pretty=format:%H%x00%an%x00%ae%x00%cI");
        // Limit to tasks directory changes
        cmd.arg("--");
        cmd.arg(Self::git_path_arg(tasks_rel));

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run git: {}", e))?;
        if !output.status.success() {
            return Err(format!(
                "git log failed (status {}): {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut summaries: HashMap<String, ChangedTaskSummary> = HashMap::new();
        let mut current_commit: Option<(String, String, String, DateTime<Utc>)> = None; // (sha, author, email, date)

        // We'll parse by lines: header lines contain three NULs; following non-empty lines are file paths; blank line separates commits
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                current_commit = None;
                continue;
            }

            if trimmed.contains('\u{0000}') {
                let parts: Vec<&str> = trimmed.split('\u{0000}').collect();
                if parts.len() >= 4 {
                    let sha = parts[0].to_string();
                    let author = parts[1].to_string();
                    let email = parts[2].to_string();
                    let date = match chrono::DateTime::parse_from_rfc3339(parts[3]) {
                        Ok(dt) => dt.with_timezone(&Utc),
                        Err(_) => continue,
                    };

                    // If author filter provided, skip this commit if it doesn't match
                    if let Some(filter) = author_filter {
                        let f = filter.to_lowercase();
                        if !author.to_lowercase().contains(&f) && !email.to_lowercase().contains(&f)
                        {
                            current_commit = None;
                            continue;
                        }
                    }

                    current_commit = Some((sha, author, email, date));
                }
                continue;
            }

            // If we have a current commit, this line is a file path
            if let Some((ref sha, ref author, ref _email, date)) = current_commit {
                let rel_path = Path::new(trimmed);
                // Ensure it's under tasks_rel
                if !rel_path.starts_with(tasks_rel) {
                    continue;
                }
                // Optional project filter
                if let Some(project) = project_filter {
                    let expect = tasks_rel.join(project);
                    if !rel_path.starts_with(&expect) {
                        continue;
                    }
                }
                // Must be a YAML file with numeric stem under a project folder
                if rel_path.extension().and_then(|e| e.to_str()) != Some("yml") {
                    continue;
                }
                let file_name = match rel_path.file_stem().and_then(|s| s.to_str()) {
                    Some(s) => s,
                    None => continue,
                };
                let numeric: u64 = match file_name.parse() {
                    Ok(n) => n,
                    Err(_) => continue,
                };
                let project = match rel_path
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|s| s.to_str())
                {
                    Some(p) => p.to_string(),
                    None => continue,
                };
                let id = format!("{}-{}", project, numeric);

                let entry = summaries
                    .entry(id.clone())
                    .or_insert_with(|| ChangedTaskSummary {
                        id: id.clone(),
                        project: project.clone(),
                        file: rel_path.to_string_lossy().to_string(),
                        last_commit: sha.clone(),
                        last_author: author.clone(),
                        last_date: date,
                        commits: 0,
                    });
                // Update last info if newer
                if date > entry.last_date {
                    entry.last_date = date;
                    entry.last_commit = sha.clone();
                    entry.last_author = author.clone();
                }
                entry.commits += 1;
            }
        }

        let mut items: Vec<ChangedTaskSummary> = summaries.into_values().collect();
        items.sort_by(|a, b| b.last_date.cmp(&a.last_date));
        Ok(items)
    }

    /// Aggregate commits by author for changes under the tasks directory within a window
    pub fn list_authors_activity(
        repo_root: &Path,
        tasks_rel: &Path,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
        project_filter: Option<&str>,
    ) -> Result<Vec<AuthorActivity>, String> {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(repo_root);
        cmd.arg("log");
        cmd.arg(format!("--since={}", since.to_rfc3339()));
        cmd.arg(format!("--until={}", until.to_rfc3339()));
        cmd.arg("--no-merges");
        cmd.arg("--pretty=format:%H%x00%an%x00%ae%x00%cI");
        // Limit to tasks directory (and optionally project) changes via pathspec
        cmd.arg("--");
        if let Some(project) = project_filter {
            cmd.arg(tasks_rel.join(project));
        } else {
            cmd.arg(Self::git_path_arg(tasks_rel));
        }

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run git: {}", e))?;
        if !output.status.success() {
            return Err(format!(
                "git log failed (status {}): {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut map: HashMap<(String, String), AuthorActivity> = HashMap::new();
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.contains('\u{0000}') {
                let parts: Vec<&str> = trimmed.split('\u{0000}').collect();
                if parts.len() >= 4 {
                    let _sha = parts[0];
                    let author = parts[1].to_string();
                    let email = parts[2].to_string();
                    let date = match chrono::DateTime::parse_from_rfc3339(parts[3]) {
                        Ok(dt) => dt.with_timezone(&Utc),
                        Err(_) => continue,
                    };

                    let key = (author.clone(), email.clone());
                    let entry = map.entry(key).or_insert_with(|| AuthorActivity {
                        author: author.clone(),
                        email: email.clone(),
                        commits: 0,
                        last_date: date,
                    });
                    entry.commits += 1;
                    if date > entry.last_date {
                        entry.last_date = date;
                    }
                }
            }
        }

        let mut items: Vec<AuthorActivity> = map.into_values().collect();
        items.sort_by(|a, b| {
            b.commits
                .cmp(&a.commits)
                .then(b.last_date.cmp(&a.last_date))
        });
        Ok(items)
    }

    /// Group commits touching the tasks directory by author|day|week|project
    pub fn list_activity(
        repo_root: &Path,
        tasks_rel: &Path,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
        group_by: GroupBy,
        project_filter: Option<&str>,
    ) -> Result<Vec<ActivityItem>, String> {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(repo_root);
        cmd.arg("log");
        cmd.arg(format!("--since={}", since.to_rfc3339()));
        cmd.arg(format!("--until={}", until.to_rfc3339()));
        cmd.arg("--no-merges");
        // If grouping by project we need names; otherwise headers suffice
        if matches!(group_by, GroupBy::Project) {
            cmd.arg("--pretty=format:%H%x00%an%x00%ae%x00%cI");
            cmd.arg("--name-only");
        } else {
            cmd.arg("--pretty=format:%H%x00%an%x00%ae%x00%cI");
        }
        cmd.arg("--");
        if let Some(project) = project_filter {
            cmd.arg(tasks_rel.join(project));
        } else {
            cmd.arg(Self::git_path_arg(tasks_rel));
        }

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run git: {}", e))?;
        if !output.status.success() {
            return Err(format!(
                "git log failed (status {}): {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut map: std::collections::HashMap<String, ActivityItem> =
            std::collections::HashMap::new();
        let mut current: Option<(String, String, String, DateTime<Utc>)> = None;
        let mut touched_projects: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                // Flush project-level counts per commit
                if matches!(group_by, GroupBy::Project)
                    && let Some((_, _, _, date)) = &current
                {
                    for proj in touched_projects.drain() {
                        let entry = map.entry(proj.clone()).or_insert_with(|| ActivityItem {
                            key: proj.clone(),
                            count: 0,
                            last_date: *date,
                        });
                        entry.count += 1;
                        if *date > entry.last_date {
                            entry.last_date = *date;
                        }
                    }
                }
                current = None;
                continue;
            }
            if trimmed.contains('\u{0000}') {
                let parts: Vec<&str> = trimmed.split('\u{0000}').collect();
                if parts.len() >= 4 {
                    let _sha = parts[0].to_string();
                    let author = parts[1].to_string();
                    let _email = parts[2].to_string();
                    let date = match chrono::DateTime::parse_from_rfc3339(parts[3]) {
                        Ok(dt) => dt.with_timezone(&Utc),
                        Err(_) => continue,
                    };
                    current = Some((_sha, author, _email, date));

                    // For non-project groups, bump immediately per commit
                    match group_by {
                        GroupBy::Author => {
                            let (_, author, _, date) = current.as_ref().unwrap();
                            let key = author.clone();
                            let entry = map.entry(key.clone()).or_insert_with(|| ActivityItem {
                                key: key.clone(),
                                count: 0,
                                last_date: *date,
                            });
                            entry.count += 1;
                            if *date > entry.last_date {
                                entry.last_date = *date;
                            }
                        }
                        GroupBy::Day => {
                            let (_, _, _, date) = current.as_ref().unwrap();
                            let key = date.format("%Y-%m-%d").to_string();
                            let entry = map.entry(key.clone()).or_insert_with(|| ActivityItem {
                                key: key.clone(),
                                count: 0,
                                last_date: *date,
                            });
                            entry.count += 1;
                            if *date > entry.last_date {
                                entry.last_date = *date;
                            }
                        }
                        GroupBy::Week => {
                            let (_, _, _, date) = current.as_ref().unwrap();
                            // ISO week key: YYYY-Www
                            let iso_week = date.iso_week();
                            let key = format!("{}-W{:02}", iso_week.year(), iso_week.week());
                            let entry = map.entry(key.clone()).or_insert_with(|| ActivityItem {
                                key: key.clone(),
                                count: 0,
                                last_date: *date,
                            });
                            entry.count += 1;
                            if *date > entry.last_date {
                                entry.last_date = *date;
                            }
                        }
                        GroupBy::Project => {
                            // defer until we gather file paths
                        }
                    }
                }
                continue;
            }

            if matches!(group_by, GroupBy::Project) {
                // collect project from file path lines
                let rel_path = std::path::Path::new(trimmed);
                if rel_path.extension().and_then(|e| e.to_str()) == Some("yml")
                    && let Some(project) = rel_path
                        .parent()
                        .and_then(|p| p.file_name())
                        .and_then(|s| s.to_str())
                {
                    touched_projects.insert(project.to_string());
                }
            }
        }

        // In case the log doesn't end with a blank line, flush remaining projects
        if matches!(group_by, GroupBy::Project)
            && let Some((_, _, _, date)) = &current
        {
            for proj in touched_projects.drain() {
                let entry = map.entry(proj.clone()).or_insert_with(|| ActivityItem {
                    key: proj.clone(),
                    count: 0,
                    last_date: *date,
                });
                entry.count += 1;
                if *date > entry.last_date {
                    entry.last_date = *date;
                }
            }
        }

        let mut items: Vec<ActivityItem> = map.into_values().collect();
        items.sort_by(|a, b| b.count.cmp(&a.count).then(b.last_date.cmp(&a.last_date)));
        Ok(items)
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct ChangedTaskSummary {
    pub id: String,
    pub project: String,
    pub file: String,
    pub last_commit: String,
    pub last_author: String,
    pub last_date: DateTime<Utc>,
    pub commits: usize,
}

#[derive(Debug, Serialize, Clone)]
pub struct AuthorActivity {
    pub author: String,
    pub email: String,
    pub commits: usize,
    pub last_date: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy)]
pub enum GroupBy {
    Author,
    Day,
    Week,
    Project,
}

#[derive(Debug, Serialize, Clone)]
pub struct ActivityItem {
    pub key: String,
    pub count: usize,
    pub last_date: DateTime<Utc>,
}
