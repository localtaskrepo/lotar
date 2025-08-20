use chrono::{DateTime, Datelike, Utc};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Serialize, Clone)]
pub struct FileCommitEvent {
    pub commit: String,
    pub author: String,
    pub email: String,
    pub date: DateTime<Utc>,
    pub message: String,
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

pub struct AuditService;

impl AuditService {
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
        cmd.arg(file_rel);
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
            if trimmed.contains('\u{0000}') {
                let parts: Vec<&str> = trimmed.split('\u{0000}').collect();
                if parts.len() >= 5 {
                    let commit = parts[0].to_string();
                    let author = parts[1].to_string();
                    let email = parts[2].to_string();
                    let date = match chrono::DateTime::parse_from_rfc3339(parts[3]) {
                        Ok(dt) => dt.with_timezone(&Utc),
                        Err(_) => continue,
                    };
                    let message = parts[4].to_string();
                    items.push(FileCommitEvent {
                        commit,
                        author,
                        email,
                        date,
                        message,
                    });
                }
            }
        }
        Ok(items)
    }

    /// Compute last change info for each task across full history (no time filter)
    pub fn list_last_change_per_task(
        repo_root: &Path,
        tasks_rel: &Path,
        project_filter: Option<&str>,
    ) -> Result<Vec<ChangedTaskSummary>, String> {
        // Walk the filesystem under tasks_rel to find task YAML files
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
            // Compute path relative to repo root
            let rel_path = match abs_path.strip_prefix(repo_root) {
                Ok(r) => r.to_path_buf(),
                Err(_) => continue,
            };
            if !rel_path.starts_with(tasks_rel) {
                continue;
            }
            // ID derivation
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

            // Last commit info
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

            // Commit count for file
            let commits: usize = {
                let mut rev = Command::new("git");
                rev.arg("-C").arg(repo_root);
                rev.arg("rev-list");
                rev.arg("--count");
                rev.arg("HEAD");
                rev.arg("--");
                rev.arg(&rel_path);
                match rev.output() {
                    Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
                        .trim()
                        .parse::<usize>()
                        .unwrap_or(1),
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
        let spec = format!("{}:{}", commit, file_rel.to_string_lossy());
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
        // Use format: git show <sha> -- <file>
        let output = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .arg("show")
            .arg(commit)
            .arg("--")
            .arg(file_rel)
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
        cmd.arg(tasks_rel);

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
            cmd.arg(tasks_rel);
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
            cmd.arg(tasks_rel);
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
                if matches!(group_by, GroupBy::Project) {
                    if let Some((_, _, _, date)) = &current {
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
                if rel_path.extension().and_then(|e| e.to_str()) == Some("yml") {
                    if let Some(project) = rel_path
                        .parent()
                        .and_then(|p| p.file_name())
                        .and_then(|s| s.to_str())
                    {
                        touched_projects.insert(project.to_string());
                    }
                }
            }
        }

        // In case the log doesn't end with a blank line, flush remaining projects
        if matches!(group_by, GroupBy::Project) {
            if let Some((_, _, _, date)) = &current {
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
