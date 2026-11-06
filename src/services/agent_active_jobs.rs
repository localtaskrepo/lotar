//! Running agent jobs, visible to every process (DEV-279).
//!
//! Jobs live in the memory of the process that runs them (`lotar serve`,
//! the queue worker, an MCP session, `agent run --wait`). Each one also
//! leaves a small record next to the agent queue, so other processes can
//! see it (status/assignee guards, `agent list-running`, worktree cleanup)
//! and ask for it to be cancelled, without depending on the
//! `lotar-agent-wrapper` binary. A record whose owning process is gone is
//! treated as stale and removed.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActiveJob {
    pub job_id: String,
    pub ticket_id: String,
    pub runner: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// Process that runs the job and holds its in-memory state.
    pub owner_pid: u32,
    /// The runner process (its own process group on Unix).
    pub child_pid: u32,
    pub started_at: String,
}

fn dir(tasks_dir: &Path) -> PathBuf {
    let canonical = tasks_dir
        .canonicalize()
        .unwrap_or_else(|_| tasks_dir.to_path_buf());
    crate::services::agent_queue_service::queue_dir().join(format!(
        "active-{}",
        crate::services::agent_queue_service::queue_hash(&canonical)
    ))
}

fn safe_name(job_id: &str) -> Option<&str> {
    (!job_id.is_empty()
        && job_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
    .then_some(job_id)
}

pub fn process_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(unix)]
    {
        // SAFETY: signal 0 only checks that the process exists.
        let result = unsafe { libc::kill(pid as i32, 0) };
        result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
    #[cfg(not(unix))]
    {
        // If the process list cannot be read, assume the job still runs:
        // a live job is never treated as gone and started twice.
        let Ok(mut system) = sysinfo::System::new() else {
            return true;
        };
        let pid = sysinfo::Pid::from_u32(pid);
        system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
        system.process(pid).is_some()
    }
}

pub fn register(tasks_dir: &Path, job: &ActiveJob) {
    let Some(name) = safe_name(&job.job_id) else {
        return;
    };
    let dir = dir(tasks_dir);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    if let Ok(json) = serde_json::to_string(job) {
        let _ = crate::storage::safety::atomic_write_file(&dir.join(format!("{name}.json")), &json);
    }
}

pub fn unregister(tasks_dir: &Path, job_id: &str) {
    let Some(name) = safe_name(job_id) else {
        return;
    };
    let dir = dir(tasks_dir);
    let _ = std::fs::remove_file(dir.join(format!("{name}.json")));
    let _ = std::fs::remove_file(dir.join(format!("{name}.cancel")));
}

/// Live records; stale ones (owner gone) are removed on the way.
pub fn list(tasks_dir: &Path) -> Vec<ActiveJob> {
    let dir = dir(tasks_dir);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut jobs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(job) = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<ActiveJob>(&text).ok())
        else {
            continue;
        };
        if process_alive(job.owner_pid) {
            jobs.push(job);
        } else {
            unregister(tasks_dir, &job.job_id);
        }
    }
    jobs.sort_by(|a, b| a.started_at.cmp(&b.started_at));
    jobs
}

pub fn for_ticket(tasks_dir: &Path, ticket_id: &str) -> Option<ActiveJob> {
    list(tasks_dir)
        .into_iter()
        .find(|job| crate::storage::identity::aliases_match(&job.ticket_id, ticket_id))
}

/// A job another process runs for this ticket. This process's own jobs are
/// tracked in memory, which knows when one is finishing.
pub fn for_ticket_elsewhere(tasks_dir: &Path, ticket_id: &str) -> Option<ActiveJob> {
    list(tasks_dir).into_iter().find(|job| {
        job.owner_pid != std::process::id()
            && crate::storage::identity::aliases_match(&job.ticket_id, ticket_id)
    })
}

pub fn find(tasks_dir: &Path, job_id: &str) -> Option<ActiveJob> {
    list(tasks_dir).into_iter().find(|job| job.job_id == job_id)
}

/// Ask the owning process to cancel the job; it checks while it waits for
/// the runner and then finalizes the job as cancelled (automation included).
pub fn request_cancel(tasks_dir: &Path, job_id: &str) -> bool {
    let Some(name) = safe_name(job_id) else {
        return false;
    };
    if find(tasks_dir, job_id).is_none() {
        return false;
    }
    std::fs::write(dir(tasks_dir).join(format!("{name}.cancel")), b"").is_ok()
}

pub fn cancel_requested(tasks_dir: &Path, job_id: &str) -> bool {
    safe_name(job_id).is_some_and(|name| dir(tasks_dir).join(format!("{name}.cancel")).exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_are_visible_until_unregistered_and_stale_ones_vanish() {
        let _queue = tempfile::tempdir().unwrap();
        // SAFETY: test-local override of the queue directory.
        unsafe { std::env::set_var("LOTAR_AGENT_QUEUE_DIR", _queue.path()) };
        let tasks = tempfile::tempdir().unwrap();
        let job = ActiveJob {
            job_id: "job-1".into(),
            ticket_id: "T-1".into(),
            runner: "command".into(),
            agent: None,
            owner_pid: std::process::id(),
            child_pid: std::process::id(),
            started_at: "2026-01-01T00:00:00Z".into(),
        };
        register(tasks.path(), &job);
        assert_eq!(for_ticket(tasks.path(), "T-1"), Some(job.clone()));
        assert!(request_cancel(tasks.path(), "job-1"));
        assert!(cancel_requested(tasks.path(), "job-1"));
        unregister(tasks.path(), "job-1");
        assert!(list(tasks.path()).is_empty());
        assert!(!cancel_requested(tasks.path(), "job-1"));

        let stale = ActiveJob {
            job_id: "job-2".into(),
            owner_pid: 0,
            ..job
        };
        register(tasks.path(), &stale);
        assert!(list(tasks.path()).is_empty(), "dead owner is stale");
        unsafe { std::env::remove_var("LOTAR_AGENT_QUEUE_DIR") };
    }
}
