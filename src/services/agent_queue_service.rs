use crate::api_types::AgentJobCreateRequest;
use crate::config::manager::ConfigManager;
use crate::errors::{LoTaRError, LoTaRResult};
use crate::services::agent_job_service::{AgentJobService, JobStartMode};
use chrono::Utc;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

const QUEUE_VERSION: u32 = 1;
const WORKER_POLL_MS: u64 = 300;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentQueueState {
    version: u32,
    tasks_dir: String,
    updated_at: String,
    pending: Vec<AgentQueueEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentQueueEntry {
    pub ticket_id: String,
    pub agent: String,
    pub prompt: String,
    pub queued_at: String,
    pub attempts: u32,
}

pub struct AgentQueueService;

impl AgentQueueService {
    pub fn enqueue(tasks_dir: &Path, req: AgentJobCreateRequest) -> LoTaRResult<bool> {
        let agent = req
            .agent
            .clone()
            .ok_or_else(|| LoTaRError::ValidationError("Agent profile required".to_string()))?;
        let ticket_id = req.ticket_id.trim().to_string();
        if ticket_id.is_empty() {
            return Err(LoTaRError::ValidationError(
                "Ticket id is required".to_string(),
            ));
        }
        let prompt = req.prompt.trim().to_string();
        if prompt.is_empty() {
            return Err(LoTaRError::ValidationError(
                "Prompt cannot be empty".to_string(),
            ));
        }

        let entry = AgentQueueEntry {
            ticket_id: ticket_id.clone(),
            agent: agent.clone(),
            prompt,
            queued_at: Utc::now().to_rfc3339(),
            attempts: 0,
        };

        let mut inserted = false;
        with_locked_queue(tasks_dir, |state| {
            if state
                .pending
                .iter()
                .any(|item| item.ticket_id == ticket_id && item.agent == agent)
            {
                return;
            }
            state.pending.push(entry.clone());
            inserted = true;
        })?;

        if inserted {
            Self::ensure_worker(tasks_dir)?;
        }

        Ok(inserted)
    }

    /// List all pending entries without modifying the queue.
    pub fn list_pending(tasks_dir: &Path) -> LoTaRResult<Vec<AgentQueueEntry>> {
        let queue_path = queue_file_path(tasks_dir);
        if !queue_path.exists() {
            return Ok(Vec::new());
        }
        let mut entries = Vec::new();
        with_locked_queue(tasks_dir, |state| {
            entries = state.pending.clone();
        })?;
        Ok(entries)
    }

    /// Remove all pending entries from the queue. Returns the number removed.
    pub fn flush(tasks_dir: &Path) -> LoTaRResult<usize> {
        let queue_path = queue_file_path(tasks_dir);
        if !queue_path.exists() {
            return Ok(0);
        }
        let mut count = 0;
        with_locked_queue(tasks_dir, |state| {
            count = state.pending.len();
            state.pending.clear();
        })?;
        Ok(count)
    }

    /// Remove a specific pending entry by ticket ID. Returns true if removed.
    pub fn remove(tasks_dir: &Path, ticket_id: &str) -> LoTaRResult<bool> {
        let mut removed = false;
        with_locked_queue(tasks_dir, |state| {
            let before = state.pending.len();
            state.pending.retain(|e| e.ticket_id != ticket_id);
            removed = state.pending.len() < before;
        })?;
        Ok(removed)
    }

    /// Check if the worker process is running.
    pub fn is_worker_running(tasks_dir: &Path) -> bool {
        let Some(lock_path) = worker_lock_path(tasks_dir) else {
            return false;
        };
        if !lock_path.exists() {
            return false;
        }
        let Ok(file) = OpenOptions::new().read(true).open(&lock_path) else {
            return false;
        };
        // If we can acquire exclusive lock, worker is NOT running
        if file.try_lock_exclusive().is_ok() {
            let _ = file.unlock();
            return false;
        }
        true
    }

    pub fn ensure_worker(tasks_dir: &Path) -> LoTaRResult<()> {
        if std::env::var("LOTAR_AGENT_QUEUE_DISABLE_WORKER").is_ok() {
            return Ok(());
        }
        if let Some(lock_path) = worker_lock_path(tasks_dir) {
            if let Some(parent) = lock_path.parent() {
                std::fs::create_dir_all(parent).map_err(LoTaRError::IoError)?;
            }
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&lock_path)
                .map_err(LoTaRError::IoError)?;

            if file.try_lock_exclusive().is_ok() {
                let _ = file.unlock();
                let _ = spawn_worker(tasks_dir);
            }
        }
        Ok(())
    }

    pub fn run_worker(tasks_dir: &Path) -> LoTaRResult<()> {
        let Some(lock_path) = worker_lock_path(tasks_dir) else {
            return Ok(());
        };
        if let Some(parent) = lock_path.parent() {
            std::fs::create_dir_all(parent).map_err(LoTaRError::IoError)?;
        }
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(LoTaRError::IoError)?;

        if lock_file.try_lock_exclusive().is_err() {
            return Ok(());
        }

        loop {
            let max_parallel = resolve_max_parallel(tasks_dir)?;
            let running = AgentJobService::queue_stats().running;
            let capacity = match max_parallel {
                Some(max) => max.saturating_sub(running),
                None => usize::MAX,
            };

            if capacity > 0 {
                let entries = dequeue_entries(tasks_dir, capacity)?;
                for entry in entries {
                    if start_entry(tasks_dir, entry.clone()).is_err() {
                        requeue_entry(tasks_dir, entry)?;
                    }
                }
            }

            if running == 0 && pending_count(tasks_dir)? == 0 {
                break;
            }

            thread::sleep(Duration::from_millis(WORKER_POLL_MS));
        }

        Ok(())
    }
}

fn start_entry(tasks_dir: &Path, entry: AgentQueueEntry) -> LoTaRResult<()> {
    let req = AgentJobCreateRequest {
        ticket_id: entry.ticket_id,
        prompt: entry.prompt,
        runner: None,
        agent: Some(entry.agent),
    };
    let _ =
        AgentJobService::start_job_with_tasks_dir_mode(req, tasks_dir, JobStartMode::Immediate)?;
    Ok(())
}

fn requeue_entry(tasks_dir: &Path, mut entry: AgentQueueEntry) -> LoTaRResult<()> {
    entry.attempts = entry.attempts.saturating_add(1);
    if entry.attempts > 3 {
        return Ok(());
    }

    with_locked_queue(tasks_dir, |state| {
        if !state
            .pending
            .iter()
            .any(|item| item.ticket_id == entry.ticket_id && item.agent == entry.agent)
        {
            state.pending.push(entry.clone());
        }
    })?;

    Ok(())
}

fn dequeue_entries(tasks_dir: &Path, limit: usize) -> LoTaRResult<Vec<AgentQueueEntry>> {
    let mut entries = Vec::new();
    with_locked_queue(tasks_dir, |state| {
        let count = limit.min(state.pending.len());
        entries.extend(state.pending.drain(0..count));
    })?;
    Ok(entries)
}

fn pending_count(tasks_dir: &Path) -> LoTaRResult<usize> {
    let mut count = 0;
    with_locked_queue(tasks_dir, |state| {
        count = state.pending.len();
    })?;
    Ok(count)
}

fn with_locked_queue<F>(tasks_dir: &Path, mut apply: F) -> LoTaRResult<()>
where
    F: FnMut(&mut AgentQueueState),
{
    let queue_path = queue_file_path(tasks_dir);
    if let Some(parent) = queue_path.parent() {
        std::fs::create_dir_all(parent).map_err(LoTaRError::IoError)?;
    }

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&queue_path)
        .map_err(LoTaRError::IoError)?;

    file.lock_exclusive().map_err(LoTaRError::IoError)?;
    let mut state = load_state(&mut file, tasks_dir)?;
    apply(&mut state);
    save_state(&mut file, &state)?;
    file.unlock().map_err(LoTaRError::IoError)?;
    Ok(())
}

fn load_state(file: &mut File, tasks_dir: &Path) -> LoTaRResult<AgentQueueState> {
    file.seek(SeekFrom::Start(0)).map_err(LoTaRError::IoError)?;
    let mut buf = String::new();
    file.read_to_string(&mut buf).map_err(LoTaRError::IoError)?;
    if buf.trim().is_empty() {
        return Ok(AgentQueueState {
            version: QUEUE_VERSION,
            tasks_dir: tasks_dir.to_string_lossy().to_string(),
            updated_at: Utc::now().to_rfc3339(),
            pending: Vec::new(),
        });
    }
    let mut state: AgentQueueState =
        serde_json::from_str(&buf).map_err(|e| LoTaRError::SerializationError(e.to_string()))?;
    if state.version != QUEUE_VERSION {
        state.version = QUEUE_VERSION;
    }
    Ok(state)
}

fn save_state(file: &mut File, state: &AgentQueueState) -> LoTaRResult<()> {
    let mut state = state.clone();
    state.updated_at = Utc::now().to_rfc3339();
    let payload = serde_json::to_string_pretty(&state)
        .map_err(|e| LoTaRError::SerializationError(e.to_string()))?;
    file.set_len(0).map_err(LoTaRError::IoError)?;
    file.seek(SeekFrom::Start(0)).map_err(LoTaRError::IoError)?;
    file.write_all(payload.as_bytes())
        .map_err(LoTaRError::IoError)?;
    file.flush().map_err(LoTaRError::IoError)?;
    Ok(())
}

fn resolve_max_parallel(tasks_dir: &Path) -> LoTaRResult<Option<usize>> {
    let cfg_mgr = ConfigManager::new_manager_with_tasks_dir_readonly(tasks_dir)
        .map_err(|e| LoTaRError::ValidationError(e.to_string()))?;
    Ok(cfg_mgr
        .get_resolved_config()
        .agent_worktree
        .max_parallel_jobs)
}

fn spawn_worker(tasks_dir: &Path) -> LoTaRResult<()> {
    let exe = std::env::current_exe().map_err(LoTaRError::IoError)?;
    let mut cmd = Command::new(exe);
    cmd.arg("--tasks-dir")
        .arg(tasks_dir)
        .arg("agent")
        .arg("worker");
    cmd.env("LOTAR_TASKS_DIR", tasks_dir);
    cmd.stdin(Stdio::null());

    if std::env::var("LOTAR_AGENT_WORKER_VERBOSE").is_ok() {
        cmd.stdout(Stdio::inherit());
        cmd.stderr(Stdio::inherit());
    } else {
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());
    }

    cmd.spawn().map_err(LoTaRError::IoError)?;
    Ok(())
}

fn queue_base_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("LOTAR_AGENT_QUEUE_DIR")
        && !dir.trim().is_empty()
    {
        return PathBuf::from(dir);
    }

    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR")
        && !dir.trim().is_empty()
    {
        return PathBuf::from(dir).join("lotar");
    }

    if let Some(cache) = dirs::cache_dir() {
        return cache.join("lotar");
    }

    std::env::temp_dir().join("lotar")
}

fn queue_file_path(tasks_dir: &Path) -> PathBuf {
    let hash = queue_hash(tasks_dir);
    queue_dir().join(format!("queue-{hash}.json"))
}

fn worker_lock_path(tasks_dir: &Path) -> Option<PathBuf> {
    let hash = queue_hash(tasks_dir);
    Some(queue_dir().join(format!("worker-{hash}.lock")))
}

fn queue_dir() -> PathBuf {
    queue_base_dir().join("agent-queue")
}

fn queue_hash(tasks_dir: &Path) -> String {
    let raw = tasks_dir.to_string_lossy();
    let hash = blake3::hash(raw.as_bytes());
    hash.to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use tempfile::tempdir;

    struct EnvGuard {
        key: &'static str,
        prev: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let prev = env::var(key).ok();
            unsafe {
                env::set_var(key, value);
            }
            Self { key, prev }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(prev) = self.prev.take() {
                unsafe {
                    env::set_var(self.key, prev);
                }
            } else {
                unsafe {
                    env::remove_var(self.key);
                }
            }
        }
    }

    #[test]
    fn enqueue_deduplicates() {
        let dir = tempdir().unwrap();
        let _guard = EnvGuard::set("LOTAR_AGENT_QUEUE_DIR", dir.path().to_str().unwrap());
        let _worker_guard = EnvGuard::set("LOTAR_AGENT_QUEUE_DISABLE_WORKER", "1");
        let tasks_dir = dir.path().join("tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();

        let req = AgentJobCreateRequest {
            ticket_id: "TEST-1".to_string(),
            prompt: "hello".to_string(),
            runner: None,
            agent: Some("implement".to_string()),
        };

        assert!(AgentQueueService::enqueue(&tasks_dir, req.clone()).unwrap());
        assert!(!AgentQueueService::enqueue(&tasks_dir, req).unwrap());
        assert_eq!(pending_count(&tasks_dir).unwrap(), 1);
    }

    #[test]
    fn dequeue_removes_entries() {
        let dir = tempdir().unwrap();
        let _guard = EnvGuard::set("LOTAR_AGENT_QUEUE_DIR", dir.path().to_str().unwrap());
        let _worker_guard = EnvGuard::set("LOTAR_AGENT_QUEUE_DISABLE_WORKER", "1");
        let tasks_dir = dir.path().join("tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();

        for idx in 1..=3 {
            let req = AgentJobCreateRequest {
                ticket_id: format!("TEST-{idx}"),
                prompt: "hello".to_string(),
                runner: None,
                agent: Some("implement".to_string()),
            };
            let _ = AgentQueueService::enqueue(&tasks_dir, req).unwrap();
        }

        let drained = dequeue_entries(&tasks_dir, 2).unwrap();
        assert_eq!(drained.len(), 2);
        assert_eq!(pending_count(&tasks_dir).unwrap(), 1);
    }
}
