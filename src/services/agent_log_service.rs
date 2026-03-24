//! Agent job log persistence.
//!
//! Writes job events to `<logs_dir>/<JOB_ID>.jsonl` files for later retrieval.
//! Uses JSON Lines format so events can be appended incrementally.
//!
//! Logging is only enabled when `agent_logs_dir` is configured. The path can be
//! relative (resolved against workspace root) or absolute.

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
}

pub struct AgentLogService;

impl AgentLogService {
    /// Initialize log file for a new job. Writes header line.
    /// Only writes if logs_dir is provided.
    #[allow(clippy::too_many_arguments)]
    pub fn init_log(
        workspace_root: &Path,
        logs_dir: Option<&str>,
        job_id: &str,
        ticket_id: &str,
        runner: &str,
        created_at: &str,
        worktree_path: Option<&str>,
        worktree_branch: Option<&str>,
    ) -> LoTaRResult<()> {
        let Some(logs_dir) = logs_dir else {
            return Ok(());
        };

        let path = resolve_log_path(workspace_root, logs_dir, job_id)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let header = JobLogLine::Header(JobLogHeader {
            job_id: job_id.to_string(),
            ticket_id: ticket_id.to_string(),
            runner: runner.to_string(),
            created_at: created_at.to_string(),
            worktree_path: worktree_path.map(|s| s.to_string()),
            worktree_branch: worktree_branch.map(|s| s.to_string()),
        });

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
        let Some(logs_dir) = logs_dir else {
            return Ok(());
        };

        let path = resolve_log_path(workspace_root, logs_dir, job_id)?;
        if !path.exists() {
            return Ok(());
        }

        let status_line = JobLogLine::Status(JobStatusLine {
            status: status.to_string(),
            at: at.to_string(),
            exit_code,
            summary,
        });

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
    fn test_log_lifecycle() {
        let dir = tempdir().unwrap();
        let workspace_root = dir.path();
        let logs_dir = ".agent-logs";

        // Init log
        AgentLogService::init_log(
            workspace_root,
            Some(logs_dir),
            "job-test-1",
            "DICE-1",
            "claude",
            "2026-02-03T12:00:00Z",
            Some("/tmp/worktree"),
            Some("agent/DICE-1"),
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
            "job-test-2",
            "DICE-2",
            "claude",
            "2026-02-03T12:00:00Z",
            None,
            None,
        )
        .unwrap();

        // Verify no log file was created
        assert!(!workspace_root.join(".agent-logs").exists());
    }
}
