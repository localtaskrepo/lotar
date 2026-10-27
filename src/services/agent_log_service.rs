//! Agent job log persistence.
//!
//! Writes job events to `<logs_dir>/<JOB_ID>.jsonl` files for later retrieval.
//! Uses JSON Lines format so events can be appended incrementally.
//!
//! Logs go to `<tasks dir>/@agent-logs` unless `agent_logs_dir` names another
//! folder (relative to the workspace root, or absolute) or turns logging off
//! with `off`. The folder gets a `.gitignore` so logs are never committed by
//! accident, and only the newest [`MAX_JOB_LOGS`] job logs are kept.

use crate::errors::{LoTaRError, LoTaRResult};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobLogEntry {
    pub kind: String,
    pub at: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobLogHeader {
    pub job_id: String,
    pub ticket_id: String,
    pub runner: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub worktree_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub worktree_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub effort: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum JobLogLine {
    #[serde(rename = "header")]
    Header(JobLogHeader),
    #[serde(rename = "event")]
    Event(JobLogEntry),
    #[serde(rename = "status")]
    Status(JobStatusLine),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobStatusLine {
    pub status: String,
    pub at: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub usage: Option<crate::api_types::AgentJobUsage>,
}

pub struct AgentLogService;

/// Default log folder inside the tasks directory.
pub const DEFAULT_LOGS_DIR: &str = "@agent-logs";
/// How many job logs a folder keeps; older ones are removed when a job starts.
pub const MAX_JOB_LOGS: usize = 500;

/// The log folder to use: the configured one, the default
/// `<tasks dir>/@agent-logs` when none is set, or `None` when logging is
/// turned off (`off`, `false` or `none`).
pub fn effective_logs_dir(tasks_dir: &Path, configured: Option<&str>) -> Option<String> {
    match configured.map(str::trim) {
        None | Some("") => Some(
            tasks_dir
                .join(DEFAULT_LOGS_DIR)
                .to_string_lossy()
                .to_string(),
        ),
        Some(value)
            if ["off", "false", "none"]
                .iter()
                .any(|off| value.eq_ignore_ascii_case(off)) =>
        {
            None
        }
        Some(value) => Some(value.to_string()),
    }
}

/// Keep logs out of version control and drop the oldest beyond the limit.
fn prepare_logs_dir(dir: &Path) {
    let ignore = dir.join(".gitignore");
    if !ignore.exists() {
        let _ = fs::write(&ignore, "# Agent job logs written by LoTaR\n*\n");
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut logs: Vec<(std::time::SystemTime, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "jsonl"))
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, entry.path()))
        })
        .collect();
    if logs.len() <= MAX_JOB_LOGS {
        return;
    }
    logs.sort();
    let excess = logs.len() - MAX_JOB_LOGS;
    for (_, path) in logs.into_iter().take(excess) {
        let _ = fs::remove_file(path);
    }
}

impl AgentLogService {
    /// Initialize log file for a new job. Writes header line.
    /// Only writes if logs_dir is provided.
    pub fn init_log(
        workspace_root: &Path,
        logs_dir: Option<&str>,
        header: JobLogHeader,
    ) -> LoTaRResult<()> {
        let Some(logs_dir) = logs_dir else {
            return Ok(());
        };

        let path = resolve_log_path(workspace_root, logs_dir, &header.job_id)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
            prepare_logs_dir(parent);
        }

        let header = JobLogLine::Header(header);

        let mut file = File::create(&path)?;
        let line = serde_json::to_string(&header)
            .map_err(|e| LoTaRError::SerializationError(e.to_string()))?;
        writeln!(file, "{}", line)?;
        Ok(())
    }

    /// Append an event line to the log file.
    /// Only writes if logs_dir is provided.
    pub fn append_event(
        workspace_root: &Path,
        logs_dir: Option<&str>,
        job_id: &str,
        kind: &str,
        at: &str,
        message: Option<String>,
    ) -> LoTaRResult<()> {
        let Some(logs_dir) = logs_dir else {
            return Ok(());
        };

        let path = resolve_log_path(workspace_root, logs_dir, job_id)?;
        if !path.exists() {
            // Log file not initialized, skip
            return Ok(());
        }

        let entry = JobLogLine::Event(JobLogEntry {
            kind: kind.to_string(),
            at: at.to_string(),
            message,
        });

        let mut file = OpenOptions::new().append(true).open(&path)?;
        let line = serde_json::to_string(&entry)
            .map_err(|e| LoTaRError::SerializationError(e.to_string()))?;
        writeln!(file, "{}", line)?;
        Ok(())
    }

    /// Write final status line to the log file.
    /// Only writes if logs_dir is provided.
    pub fn write_status(
        workspace_root: &Path,
        logs_dir: Option<&str>,
        job_id: &str,
        status: &str,
        at: &str,
        exit_code: Option<i32>,
        summary: Option<String>,
    ) -> LoTaRResult<()> {
        Self::write_status_with_usage(
            workspace_root,
            logs_dir,
            job_id,
            JobStatusLine {
                status: status.to_string(),
                at: at.to_string(),
                exit_code,
                summary,
                usage: None,
            },
        )
    }

    /// Write the final status line, including what the job used.
    pub fn write_status_with_usage(
        workspace_root: &Path,
        logs_dir: Option<&str>,
        job_id: &str,
        status_line: JobStatusLine,
    ) -> LoTaRResult<()> {
        let Some(logs_dir) = logs_dir else {
            return Ok(());
        };

        let path = resolve_log_path(workspace_root, logs_dir, job_id)?;
        if !path.exists() {
            return Ok(());
        }

        let status_line = JobLogLine::Status(status_line);

        let mut file = OpenOptions::new().append(true).open(&path)?;
        let line = serde_json::to_string(&status_line)
            .map_err(|e| LoTaRError::SerializationError(e.to_string()))?;
        writeln!(file, "{}", line)?;
        Ok(())
    }

    /// Load all events from a log file.
    pub fn load_events(
        workspace_root: &Path,
        logs_dir: &str,
        job_id: &str,
    ) -> LoTaRResult<Vec<JobLogEntry>> {
        let path = resolve_log_path(workspace_root, logs_dir, job_id)?;
        if !path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&path)?;
        let reader = BufReader::new(file);
        let mut events = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(JobLogLine::Event(entry)) = serde_json::from_str::<JobLogLine>(&line) {
                events.push(entry);
            }
        }

        Ok(events)
    }

    /// Load header from a log file.
    pub fn load_header(
        workspace_root: &Path,
        logs_dir: &str,
        job_id: &str,
    ) -> LoTaRResult<Option<JobLogHeader>> {
        let path = resolve_log_path(workspace_root, logs_dir, job_id)?;
        if !path.exists() {
            return Ok(None);
        }

        let file = File::open(&path)?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(JobLogLine::Header(header)) = serde_json::from_str::<JobLogLine>(&line) {
                return Ok(Some(header));
            }
            // Header should be first line
            break;
        }

        Ok(None)
    }

    /// Load final status from a log file (last status line).
    pub fn load_status(
        workspace_root: &Path,
        logs_dir: &str,
        job_id: &str,
    ) -> LoTaRResult<Option<JobStatusLine>> {
        let path = resolve_log_path(workspace_root, logs_dir, job_id)?;
        if !path.exists() {
            return Ok(None);
        }

        let file = File::open(&path)?;
        let reader = BufReader::new(file);
        let mut last_status: Option<JobStatusLine> = None;

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(JobLogLine::Status(status)) = serde_json::from_str::<JobLogLine>(&line) {
                last_status = Some(status);
            }
        }

        Ok(last_status)
    }

    /// List all job log files in the logs directory.
    pub fn list_logs(workspace_root: &Path, logs_dir: &str) -> LoTaRResult<Vec<String>> {
        let resolved_dir = resolve_logs_dir(workspace_root, logs_dir);
        if !resolved_dir.exists() {
            return Ok(Vec::new());
        }

        let mut job_ids = Vec::new();
        for entry in fs::read_dir(&resolved_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("jsonl")
                && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
            {
                job_ids.push(stem.to_string());
            }
        }

        job_ids.sort();
        job_ids.reverse(); // Newest first
        Ok(job_ids)
    }
}

/// Resolve the logs directory path. Supports both relative and absolute paths.
fn resolve_logs_dir(workspace_root: &Path, logs_dir: &str) -> PathBuf {
    let logs_path = Path::new(logs_dir);
    if logs_path.is_absolute() {
        logs_path.to_path_buf()
    } else {
        workspace_root.join(logs_dir)
    }
}

fn resolve_log_path(workspace_root: &Path, logs_dir: &str, job_id: &str) -> LoTaRResult<PathBuf> {
    let trimmed = job_id.trim();
    if trimmed.is_empty() {
        return Err(LoTaRError::ValidationError("Missing job id".to_string()));
    }
    // Job IDs have format: job-<timestamp>-<counter>
    // Validate no path traversal
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err(LoTaRError::ValidationError("Invalid job id".to_string()));
    }

    let resolved_dir = resolve_logs_dir(workspace_root, logs_dir);
    let path = resolved_dir.join(format!("{}.jsonl", trimmed));

    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(LoTaRError::ValidationError("Invalid log path".to_string()));
    }

    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn logs_default_to_the_tasks_dir_and_can_be_turned_off() {
        let tasks = Path::new("/work/.tasks");
        assert_eq!(
            effective_logs_dir(tasks, None).as_deref(),
            Some("/work/.tasks/@agent-logs")
        );
        assert_eq!(
            effective_logs_dir(tasks, Some(" ")).as_deref(),
            Some("/work/.tasks/@agent-logs")
        );
        assert_eq!(
            effective_logs_dir(tasks, Some(".logs")).as_deref(),
            Some(".logs")
        );
        assert_eq!(effective_logs_dir(tasks, Some("off")), None);
        assert_eq!(effective_logs_dir(tasks, Some("None")), None);
    }

    #[test]
    fn log_folder_is_git_ignored_and_keeps_the_newest_logs() {
        let dir = tempdir().unwrap();
        let logs = dir.path().join("logs");
        fs::create_dir_all(&logs).unwrap();
        for index in 0..MAX_JOB_LOGS + 3 {
            let path = logs.join(format!("job-{index:04}.jsonl"));
            fs::write(&path, "{}\n").unwrap();
            let time = std::time::SystemTime::UNIX_EPOCH
                + std::time::Duration::from_secs(1_000_000 + index as u64);
            File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(time)
                .unwrap();
        }
        let header = JobLogHeader {
            job_id: "job-new".to_string(),
            ticket_id: "T-1".to_string(),
            runner: "claude".to_string(),
            created_at: "now".to_string(),
            worktree_path: None,
            worktree_branch: None,
            model: None,
            effort: None,
        };
        AgentLogService::init_log(dir.path(), Some("logs"), header).unwrap();
        assert!(
            fs::read_to_string(logs.join(".gitignore"))
                .unwrap()
                .contains('*')
        );
        let remaining = fs::read_dir(&logs)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .path()
                    .extension()
                    .is_some_and(|x| x == "jsonl")
            })
            .count();
        assert!(remaining <= MAX_JOB_LOGS + 1, "{remaining}");
        assert!(!logs.join("job-0000.jsonl").exists(), "oldest log removed");
        assert!(logs.join("job-new.jsonl").exists());
    }

    #[test]
    fn test_log_lifecycle() {
        let dir = tempdir().unwrap();
        let workspace_root = dir.path();
        let logs_dir = ".agent-logs";

        // Init log
        AgentLogService::init_log(
            workspace_root,
            Some(logs_dir),
            JobLogHeader {
                job_id: "job-test-1".to_string(),
                ticket_id: "DICE-1".to_string(),
                runner: "claude".to_string(),
                created_at: "2026-02-03T12:00:00Z".to_string(),
                worktree_path: Some("/tmp/worktree".to_string()),
                worktree_branch: Some("agent/DICE-1".to_string()),
                model: Some("sonnet".to_string()),
                effort: None,
            },
        )
        .unwrap();

        // Append events
        AgentLogService::append_event(
            workspace_root,
            Some(logs_dir),
            "job-test-1",
            "agent_job_started",
            "2026-02-03T12:00:01Z",
            None,
        )
        .unwrap();

        AgentLogService::append_event(
            workspace_root,
            Some(logs_dir),
            "job-test-1",
            "agent_job_progress",
            "2026-02-03T12:00:02Z",
            Some("Reading files...".to_string()),
        )
        .unwrap();

        // Write final status
        AgentLogService::write_status(
            workspace_root,
            Some(logs_dir),
            "job-test-1",
            "completed",
            "2026-02-03T12:00:10Z",
            Some(0),
            Some("Created dice roller".to_string()),
        )
        .unwrap();

        // Load and verify
        let header = AgentLogService::load_header(workspace_root, logs_dir, "job-test-1")
            .unwrap()
            .unwrap();
        assert_eq!(header.ticket_id, "DICE-1");
        assert_eq!(header.runner, "claude");
        assert_eq!(header.model.as_deref(), Some("sonnet"));

        let events = AgentLogService::load_events(workspace_root, logs_dir, "job-test-1").unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, "agent_job_started");
        assert_eq!(events[1].kind, "agent_job_progress");
        assert_eq!(events[1].message, Some("Reading files...".to_string()));

        let status = AgentLogService::load_status(workspace_root, logs_dir, "job-test-1")
            .unwrap()
            .unwrap();
        assert_eq!(status.status, "completed");
        assert_eq!(status.exit_code, Some(0));

        // List logs
        let logs = AgentLogService::list_logs(workspace_root, logs_dir).unwrap();
        assert_eq!(logs, vec!["job-test-1".to_string()]);
    }

    #[test]
    fn test_no_logs_when_disabled() {
        let dir = tempdir().unwrap();
        let workspace_root = dir.path();

        // Init log with None logs_dir - should do nothing
        AgentLogService::init_log(
            workspace_root,
            None,
            JobLogHeader {
                job_id: "job-test-2".to_string(),
                ticket_id: "DICE-2".to_string(),
                runner: "claude".to_string(),
                created_at: "2026-02-03T12:00:00Z".to_string(),
                worktree_path: None,
                worktree_branch: None,
                model: None,
                effort: None,
            },
        )
        .unwrap();

        // Verify no log file was created
        assert!(!workspace_root.join(".agent-logs").exists());
    }
}
