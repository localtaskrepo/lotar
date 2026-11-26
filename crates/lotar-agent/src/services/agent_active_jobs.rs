//! Running agent jobs, visible to every process (DEV-279).
//!
//! Jobs live in the memory of the process that runs them (`lotar serve`,
//! the queue worker, an MCP session, `agent run --wait`). Each one also
//! leaves a small record next to the agent queue, so other processes can
//! see it (status/assignee guards, `agent list-running`, worktree cleanup)
//! and ask for it to be cancelled, without depending on the
//! `lotar-agent-wrapper` binary. A record whose owning process is gone is
//! an orphan: it no longer counts as running, and stays until the next
//! worker or server [`claim`]s the job and finishes it (DEV-378).

use crate::api_types::JobOutcomeField;
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
    /// The runner's session (a T3 Code thread id), so a job whose process
    /// died can be finished from the thread's state (DEV-378).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// The job started as a merge job, so a takeover runs it as one, also
    /// when the ticket has moved on since (DEV-544).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub merge: bool,
    /// The ticket's assignee when the job started (empty: none), so a
    /// process that takes the job over sees whether the agent handed the
    /// ticket back (DEV-516) or something moved it on (DEV-566).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee_at_start: Option<String>,
    /// The job sets up its worktree and has no job log yet, so a takeover
    /// fails it for that and writes the log (DEV-598).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub in_setup: bool,
    /// What the job was started with, so a takeover whose session can't be
    /// resumed runs the job again instead of failing it (DEV-734).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<crate::api_types::AgentJobCreateRequest>,
}

/// What a ticket should be when its job ends, set while the job ran
/// (DEV-320). An empty assignee clears it.
pub type JobOutcome = crate::api_types::JobOutcome;

fn dir(tasks_dir: &Path) -> PathBuf {
    let canonical = crate::services::agent_queue_service::canonical_tasks_dir(tasks_dir);
    crate::services::agent_queue_service::queue_dir().join(format!("active-{}", crate::services::agent_queue_service::queue_hash(&canonical)))
}

/// A record as written: the job and the tasks folder it belongs to, so
/// `agent list-running --all` can name it (DEV-763). Records written
/// before have no folder.
#[derive(Serialize)]
struct StampedRef<'a> {
    #[serde(flatten)]
    job: &'a ActiveJob,
    tasks_dir: String,
}

#[derive(Deserialize)]
struct Stamped {
    #[serde(flatten)]
    job: ActiveJob,
    #[serde(default)]
    tasks_dir: Option<String>,
}

fn record_json(tasks_dir: &Path, job: &ActiveJob) -> serde_json::Result<String> {
    let canonical = tasks_dir.canonicalize().unwrap_or_else(|_| tasks_dir.to_path_buf());
    serde_json::to_string(&StampedRef { job, tasks_dir: canonical.to_string_lossy().into_owned() })
}

fn safe_name(job_id: &str) -> Option<&str> {
    (!job_id.is_empty() && job_id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))).then_some(job_id)
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
    if let Ok(json) = record_json(tasks_dir, job) {
        let _ = crate::storage::safety::atomic_write_file(&dir.join(format!("{name}.json")), &json);
    }
}

/// Write a `command` runner's prompt to a file only this user can read
/// (`LOTAR_AGENT_PROMPT_FILE`, DEV-542). It stays while the job's program
/// runs, also when this process exits first, and goes with the record.
pub fn write_prompt(tasks_dir: &Path, job_id: &str, prompt: &str) -> std::io::Result<PathBuf> {
    let name = safe_name(job_id).ok_or_else(|| std::io::Error::other(format!("Invalid job id '{job_id}'")))?;
    let dir = dir(tasks_dir);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{name}.prompt"));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    std::io::Write::write_all(&mut options.open(&path)?, prompt.as_bytes())?;
    Ok(path)
}

pub fn unregister(tasks_dir: &Path, job_id: &str) {
    let Some(name) = safe_name(job_id) else {
        return;
    };
    let dir = dir(tasks_dir);
    let _ = std::fs::remove_file(dir.join(format!("{name}.json")));
    let _ = std::fs::remove_file(dir.join(format!("{name}.cancel")));
    let _ = std::fs::remove_file(dir.join(format!("{name}.prompt")));
    let _lock = lock_outcome(&dir, name);
    mark_ended(&dir, name);
    let _ = std::fs::remove_file(dir.join(format!("{name}.outcome.json")));
}

/// How long an ended job's marker stays: far longer than any update that
/// checked for the job before it ended takes to record its outcome.
const ENDED_MARKER_TTL: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

fn lock_outcome(dir: &Path, name: &str) -> std::io::Result<std::fs::File> {
    crate::storage::safety::acquire_storage_lock_within(dir, &format!("{name}.outcome"), std::time::Duration::from_secs(5))
}

/// Marks the job as ended, so [`record_outcome`] refuses it (DEV-350).
fn mark_ended(dir: &Path, name: &str) {
    if std::fs::create_dir_all(dir).is_ok() {
        let _ = std::fs::write(dir.join(format!("{name}.ended")), b"");
    }
}

/// Record a status and/or assignee change made while the job runs; it is
/// applied when the job ends instead of the end rules' `set` (DEV-320).
/// Later values replace earlier ones per field; a field in `cleared` drops
/// its recorded value (DEV-340). Returns the change as recorded: `cleared`
/// keeps only fields that held a value, or `None` when the job has
/// already ended: nobody would apply the outcome, so the caller applies the
/// change directly (DEV-350).
pub fn record_outcome(tasks_dir: &Path, outcome: &JobOutcome) -> std::io::Result<Option<JobOutcome>> {
    let Some(name) = safe_name(&outcome.job_id) else {
        return Err(std::io::Error::other(format!("invalid job id '{}'", outcome.job_id)));
    };
    let dir = dir(tasks_dir);
    std::fs::create_dir_all(&dir)?;
    let _lock = lock_outcome(&dir, name)?;
    if dir.join(format!("{name}.ended")).exists() {
        return Ok(None);
    }
    let path = dir.join(format!("{name}.outcome.json"));
    let mut merged = read_outcome(&path).unwrap_or_else(|| JobOutcome { job_id: outcome.job_id.clone(), ..JobOutcome::default() });
    let mut recorded = outcome.clone();
    recorded.cleared.retain(|field| {
        let slot = match field {
            JobOutcomeField::Status => &mut merged.status,
            JobOutcomeField::Assignee => &mut merged.assignee,
        };
        slot.take().is_some()
    });
    if outcome.status.is_some() {
        merged.status = outcome.status.clone();
    }
    if outcome.assignee.is_some() {
        merged.assignee = outcome.assignee.clone();
    }
    if merged.is_empty() {
        match std::fs::remove_file(&path) {
            Err(err) if err.kind() != std::io::ErrorKind::NotFound => return Err(err),
            _ => {}
        }
    } else {
        let json = serde_json::to_string(&merged).map_err(std::io::Error::other)?;
        crate::storage::safety::atomic_write_file(&path, &json)?;
    }
    Ok(Some(recorded))
}

/// The outcome recorded so far, without consuming it.
pub fn peek_outcome(tasks_dir: &Path, job_id: &str) -> Option<JobOutcome> {
    let name = safe_name(job_id)?;
    read_outcome(&dir(tasks_dir).join(format!("{name}.outcome.json")))
}

/// End the job's guard and read its recorded outcome, which stays for the
/// job's end: a paused job's retry carries the outcome, and a change made
/// from now on, while the paused job still ends, applies directly instead
/// of being recorded for a job that no longer reads it (DEV-752).
pub fn close_outcome(tasks_dir: &Path, job_id: &str) -> Option<JobOutcome> {
    let name = safe_name(job_id)?;
    let dir = dir(tasks_dir);
    let _lock = lock_outcome(&dir, name);
    mark_ended(&dir, name);
    read_outcome(&dir.join(format!("{name}.outcome.json")))
}

/// Reopen the guard [`close_outcome`] ended: the job's pause or retry
/// wasn't saved, so it fails and its end applies the outcome after all
/// (DEV-756). `keep` decides what of the outcome still applies (a field the
/// ticket changed in since the close stays as it is); `None` drops it.
pub fn reopen_outcome(tasks_dir: &Path, job_id: &str, keep: impl FnOnce(JobOutcome) -> Option<JobOutcome>) {
    let Some(name) = safe_name(job_id) else {
        return;
    };
    let dir = dir(tasks_dir);
    let _lock = lock_outcome(&dir, name);
    let _ = std::fs::remove_file(dir.join(format!("{name}.ended")));
    let path = dir.join(format!("{name}.outcome.json"));
    let Some(outcome) = read_outcome(&path) else {
        return;
    };
    match keep(outcome).filter(|outcome| !outcome.is_empty()) {
        Some(outcome) => {
            if let Ok(json) = serde_json::to_string(&outcome) {
                let _ = crate::storage::safety::atomic_write_file(&path, &json);
            }
        }
        None => {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// End the job's guard and hand back its recorded outcome. The record goes
/// first, so a change made from now on applies directly instead of being
/// recorded for a job that no longer reads it. A change that checked for
/// the job before that and records its outcome after the read below is
/// refused by [`record_outcome`] and applied directly by its caller.
///
/// The outcome stays on disk as pending until [`settle_outcome`] (DEV-467):
/// if its end can't apply it, or its process dies first, a later worker or
/// server applies it ([`pending_outcomes`]). `keep` decides what of it
/// still applies before it is kept (DEV-528's agent-reporter hand-back), so a
/// later worker or server applies only that; `None` drops it.
pub fn end_and_take_outcome(tasks_dir: &Path, job_id: &str, ticket_id: &str, keep: impl FnOnce(JobOutcome) -> Option<JobOutcome>) -> Option<JobOutcome> {
    let name = safe_name(job_id)?;
    let dir = dir(tasks_dir);
    let _ = std::fs::remove_file(dir.join(format!("{name}.json")));
    let _lock = lock_outcome(&dir, name);
    mark_ended(&dir, name);
    let path = dir.join(format!("{name}.outcome.json"));
    let Some(outcome) = read_outcome(&path).and_then(keep) else {
        let _ = std::fs::remove_file(&path);
        return None;
    };
    let pending = PendingOutcome { ticket_id: ticket_id.to_string(), outcome: outcome.clone(), owner_pid: std::process::id(), deferred: false };
    if write_pending(&dir, name, &pending).is_ok() {
        let _ = std::fs::remove_file(&path);
    }
    Some(outcome)
}

/// An outcome its job's end took but hasn't applied yet (DEV-467).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingOutcome {
    pub ticket_id: String,
    pub outcome: JobOutcome,
    /// The process that ended the job and applies the outcome.
    pub owner_pid: u32,
    /// The owner couldn't apply it and left it to a later worker or server.
    #[serde(default)]
    pub deferred: bool,
}

fn pending_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.pending.json"))
}

fn write_pending(dir: &Path, name: &str, pending: &PendingOutcome) -> std::io::Result<()> {
    let json = serde_json::to_string(pending).map_err(std::io::Error::other)?;
    crate::storage::safety::atomic_write_file(&pending_path(dir, name), &json)
}

/// The outcome was applied, or can never be: forget it.
pub fn settle_outcome(tasks_dir: &Path, job_id: &str) {
    if let Some(name) = safe_name(job_id) {
        let _ = std::fs::remove_file(pending_path(&dir(tasks_dir), name));
    }
}

/// The outcome couldn't be applied yet: leave it to the next worker or server.
pub fn defer_outcome(tasks_dir: &Path, job_id: &str) {
    let Some(name) = safe_name(job_id) else {
        return;
    };
    let dir = dir(tasks_dir);
    let path = pending_path(&dir, name);
    if let Some(mut pending) = std::fs::read_to_string(&path).ok().and_then(|json| serde_json::from_str::<PendingOutcome>(&json).ok()) {
        pending.deferred = true;
        let _ = write_pending(&dir, name, &pending);
    }
}

/// Outcomes left to this process: deferred, or their owner exited first.
pub fn pending_outcomes(tasks_dir: &Path) -> Vec<PendingOutcome> {
    let Ok(entries) = std::fs::read_dir(dir(tasks_dir)) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| entry.file_name().to_str().is_some_and(|name| name.ends_with(".pending.json")))
        .filter_map(|entry| std::fs::read_to_string(entry.path()).ok().and_then(|json| serde_json::from_str::<PendingOutcome>(&json).ok()))
        .filter(|pending| pending.deferred || !process_alive(pending.owner_pid))
        .collect()
}

fn read_outcome(path: &Path) -> Option<JobOutcome> {
    std::fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok())
}

/// Every record, live or orphaned; expired end markers are removed on the way.
fn records(tasks_dir: &Path) -> Vec<ActiveJob> {
    records_in(&dir(tasks_dir)).into_iter().map(|record| record.job).collect()
}

fn records_in(dir: &Path) -> Vec<Stamped> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut jobs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "ended") {
            let expired = entry.metadata().and_then(|meta| meta.modified()).ok().and_then(|at| at.elapsed().ok()).is_some_and(|age| age > ENDED_MARKER_TTL);
            if expired {
                let _ = std::fs::remove_file(&path);
            }
            continue;
        }
        if let Some(record) = read_stamped(&path) {
            jobs.push(record);
        }
    }
    jobs.sort_by(|a, b| a.job.started_at.cmp(&b.job.started_at));
    jobs
}

/// Holds a ticket's lock for its T3 Code threads that run again after their
/// job ended (DEV-626), so one process at a time adopts or reports them.
pub fn lock_resumed_threads(tasks_dir: &Path, ticket_id: &str) -> Option<std::fs::File> {
    let name = safe_name(ticket_id)?;
    let dir = dir(tasks_dir);
    std::fs::create_dir_all(&dir).ok()?;
    crate::storage::safety::acquire_storage_lock_within(&dir, &format!("{name}.resumed"), std::time::Duration::from_secs(5)).ok()
}

/// Whether this is the first look at the run `run` of the T3 Code thread
/// `thread` since its job ended (DEV-626); marks it as seen.
pub fn first_look_at_run(tasks_dir: &Path, thread: &str, run: &str) -> bool {
    let dir = dir(tasks_dir);
    let hash = blake3::hash(format!("{thread}\n{run}").as_bytes()).to_hex();
    let path = dir.join(format!("{}.resumed", &hash[..32]));
    std::fs::create_dir_all(&dir).is_ok() && std::fs::OpenOptions::new().write(true).create_new(true).open(path).is_ok()
}

fn read_record(path: &Path) -> Option<ActiveJob> {
    read_stamped(path).map(|record| record.job)
}

fn read_stamped(path: &Path) -> Option<Stamped> {
    let is_record =
        path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with(".json") && !n.ends_with(".outcome.json") && !n.ends_with(".pending.json"));
    if !is_record {
        return None;
    }
    std::fs::read_to_string(path).ok().and_then(|text| serde_json::from_str::<Stamped>(&text).ok())
}

/// Records of running jobs: their owning process is alive.
pub fn list(tasks_dir: &Path) -> Vec<ActiveJob> {
    records(tasks_dir).into_iter().filter(|job| process_alive(job.owner_pid)).collect()
}

/// Running jobs of every tasks folder on this machine, oldest first, each
/// with its tasks folder (`None`: written before DEV-763).
pub fn list_all() -> Vec<(Option<PathBuf>, ActiveJob)> {
    let Ok(entries) = std::fs::read_dir(crate::services::agent_queue_service::queue_dir()) else {
        return Vec::new();
    };
    let mut jobs: Vec<(Option<PathBuf>, ActiveJob)> = entries
        .flatten()
        .filter(|entry| entry.file_name().to_str().is_some_and(|name| name.starts_with("active-")) && entry.path().is_dir())
        .flat_map(|entry| records_in(&entry.path()))
        .filter(|record| process_alive(record.job.owner_pid))
        .map(|record| (record.tasks_dir.map(PathBuf::from), record.job))
        .collect();
    jobs.sort_by(|a, b| a.1.started_at.cmp(&b.1.started_at));
    jobs
}

/// Jobs whose owning process exited before they ended (DEV-378).
pub fn orphaned(tasks_dir: &Path) -> Vec<ActiveJob> {
    records(tasks_dir).into_iter().filter(|job| !process_alive(job.owner_pid)).collect()
}

/// Take over an orphaned job: this process becomes its owner, so no other
/// process finishes it too. `None` when its owner is alive (another
/// process claimed it first) or the record is gone.
pub fn claim(tasks_dir: &Path, job_id: &str) -> Option<ActiveJob> {
    let name = safe_name(job_id)?;
    let dir = dir(tasks_dir);
    let _lock = crate::storage::safety::acquire_storage_lock_within(&dir, &format!("{name}.claim"), std::time::Duration::from_secs(5)).ok()?;
    let mut job = read_record(&dir.join(format!("{name}.json")))?;
    if process_alive(job.owner_pid) {
        return None;
    }
    job.owner_pid = std::process::id();
    let json = record_json(tasks_dir, &job).ok()?;
    crate::storage::safety::atomic_write_file(&dir.join(format!("{name}.json")), &json).ok()?;
    Some(job)
}

pub fn for_ticket(tasks_dir: &Path, ticket_id: &str) -> Option<ActiveJob> {
    list(tasks_dir).into_iter().find(|job| crate::storage::identity::aliases_match(&job.ticket_id, ticket_id))
}

/// A job another process runs for this ticket. This process's own jobs are
/// tracked in memory, which knows when one is finishing.
pub fn for_ticket_elsewhere(tasks_dir: &Path, ticket_id: &str) -> Option<ActiveJob> {
    list(tasks_dir).into_iter().find(|job| job.owner_pid != std::process::id() && crate::storage::identity::aliases_match(&job.ticket_id, ticket_id))
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
            session_id: None,
            merge: false,
            assignee_at_start: None,
            in_setup: false,
            request: None,
        };
        register(tasks.path(), &job);
        assert_eq!(for_ticket(tasks.path(), "T-1"), Some(job.clone()));
        assert!(request_cancel(tasks.path(), "job-1"));
        assert!(cancel_requested(tasks.path(), "job-1"));
        let prompt = write_prompt(tasks.path(), "job-1", "Do it").unwrap();
        assert_eq!(std::fs::read_to_string(&prompt).unwrap(), "Do it");
        unregister(tasks.path(), "job-1");
        assert!(!prompt.exists(), "the prompt goes with the record");
        assert!(list(tasks.path()).is_empty());
        assert!(!cancel_requested(tasks.path(), "job-1"));

        let orphan = ActiveJob { job_id: "job-2".into(), owner_pid: 0, ..job };
        register(tasks.path(), &orphan);
        assert!(list(tasks.path()).is_empty(), "a job whose owner died is not running");
        assert_eq!(orphaned(tasks.path()), vec![orphan.clone()], "it stays until a process finishes it");
        let claimed = claim(tasks.path(), "job-2").expect("claims the orphan");
        assert_eq!((claimed.owner_pid, claimed.session_id.as_deref()), (std::process::id(), orphan.session_id.as_deref()));
        assert!(claim(tasks.path(), "job-2").is_none(), "a live owner keeps it");
        assert!(orphaned(tasks.path()).is_empty());
        unsafe { std::env::remove_var("LOTAR_AGENT_QUEUE_DIR") };
    }

    #[test]
    fn an_outcome_recorded_after_the_job_ended_is_refused() {
        let _queue = tempfile::tempdir().unwrap();
        // SAFETY: test-local override of the queue directory.
        unsafe { std::env::set_var("LOTAR_AGENT_QUEUE_DIR", _queue.path()) };
        let tasks = tempfile::tempdir().unwrap();
        let outcome = |job_id: &str| JobOutcome { job_id: job_id.into(), assignee: Some("owner".into()), ..JobOutcome::default() };

        assert_eq!(record_outcome(tasks.path(), &outcome("job-1")).unwrap(), Some(outcome("job-1")));
        assert_eq!(end_and_take_outcome(tasks.path(), "job-1", "T-1", Some), Some(outcome("job-1")));
        assert_eq!(record_outcome(tasks.path(), &outcome("job-1")).unwrap(), None, "after the end took the outcome");
        assert!(peek_outcome(tasks.path(), "job-1").is_none());
        settle_outcome(tasks.path(), "job-1");

        unregister(tasks.path(), "job-2");
        assert_eq!(record_outcome(tasks.path(), &outcome("job-2")).unwrap(), None, "after the job was unregistered");
        assert!(peek_outcome(tasks.path(), "job-2").is_none());

        // A pausing job closes its outcome for the retry; its end still reads it.
        record_outcome(tasks.path(), &outcome("job-3")).unwrap();
        assert_eq!(close_outcome(tasks.path(), "job-3"), Some(outcome("job-3")));
        assert_eq!(record_outcome(tasks.path(), &outcome("job-3")).unwrap(), None, "after the pause closed the outcome");
        assert_eq!(end_and_take_outcome(tasks.path(), "job-3", "T-1", Some), Some(outcome("job-3")));
        settle_outcome(tasks.path(), "job-3");

        // A pause that wasn't saved reopens it: the failed job records and applies it again.
        record_outcome(tasks.path(), &outcome("job-4")).unwrap();
        close_outcome(tasks.path(), "job-4");
        reopen_outcome(tasks.path(), "job-4", Some);
        let sam = JobOutcome { assignee: Some("sam".into()), ..outcome("job-4") };
        assert_eq!(record_outcome(tasks.path(), &sam).unwrap(), Some(sam.clone()), "after the reopen");
        assert_eq!(end_and_take_outcome(tasks.path(), "job-4", "T-1", Some), Some(sam));
        settle_outcome(tasks.path(), "job-4");
        unsafe { std::env::remove_var("LOTAR_AGENT_QUEUE_DIR") };
    }

    #[test]
    fn a_taken_outcome_stays_pending_until_settled() {
        let _queue = tempfile::tempdir().unwrap();
        // SAFETY: test-local override of the queue directory.
        unsafe { std::env::set_var("LOTAR_AGENT_QUEUE_DIR", _queue.path()) };
        let tasks = tempfile::tempdir().unwrap();
        let outcome = JobOutcome { job_id: "job-1".into(), status: Some("Done".into()), ..JobOutcome::default() };
        record_outcome(tasks.path(), &outcome).unwrap();
        assert_eq!(end_and_take_outcome(tasks.path(), "job-1", "T-1", Some), Some(outcome.clone()));
        unregister(tasks.path(), "job-1");
        assert!(pending_outcomes(tasks.path()).is_empty(), "its live owner still applies it");

        defer_outcome(tasks.path(), "job-1");
        let pending = PendingOutcome { ticket_id: "T-1".into(), outcome, owner_pid: std::process::id(), deferred: true };
        assert_eq!(pending_outcomes(tasks.path()), vec![pending.clone()], "kept past the job's end");
        assert!(list(tasks.path()).is_empty() && orphaned(tasks.path()).is_empty(), "not a job record");

        write_pending(&dir(tasks.path()), "job-1", &PendingOutcome { owner_pid: 0, deferred: false, ..pending }).unwrap();
        assert_eq!(pending_outcomes(tasks.path()).len(), 1, "its owner exited before applying it");
        settle_outcome(tasks.path(), "job-1");
        assert!(pending_outcomes(tasks.path()).is_empty());
        unsafe { std::env::remove_var("LOTAR_AGENT_QUEUE_DIR") };
    }
}
