//! Live events of jobs other processes run (DEV-765). A job streams its events only in the process that runs it
//! (the queue worker, `agent run --wait`, an MCP session); every job also appends them to its log. `lotar serve`
//! follows the logs and sends each new event line as the `agent_job_*` event the job's own process would send.
//! A job this process holds goes out from memory only, so its lines are not sent twice.

use super::{AgentJobEvent, JOB_REGISTRY, job_event_payload, job_log_dirs, saved_job_dto, workspace_root_of};
use crate::api_events::ApiEvent;
use crate::services::agent_log_service::{JobLogHeader, JobLogLine, JobStatusLine, SavedJob, is_job_log, resolve_logs_dir};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Where one log has been read to, and the job as its lines so far tell it.
#[derive(Default)]
struct Followed {
    /// Bytes read up to the end of the last complete line.
    offset: u64,
    /// Lines that end at or before this offset were in the log when the server started: they set the job's fields
    /// but are not sent.
    sent_from: u64,
    header: Option<JobLogHeader>,
    status: Option<JobStatusLine>,
    started_at: Option<String>,
    /// Event lines read, the sequence of events logged before DEV-518.
    events: u64,
}

/// The job logs a server follows: the log folders its projects configure, as they are when it starts.
pub struct LogFollower {
    dirs: Vec<PathBuf>,
    logs: HashMap<PathBuf, Followed>,
}

impl LogFollower {
    /// Follow the log folders of `tasks_dir`'s projects; what their logs hold now is not sent.
    pub fn new(tasks_dir: &Path) -> Self {
        let root = workspace_root_of(tasks_dir);
        let mut dirs: Vec<PathBuf> = Vec::new();
        for dir in job_log_dirs(tasks_dir) {
            let resolved = resolve_logs_dir(&root, &dir);
            if !dirs.contains(&resolved) {
                dirs.push(resolved);
            }
        }
        let mut logs = HashMap::new();
        for dir in &dirs {
            let Ok(entries) = std::fs::read_dir(dir) else { continue };
            for path in entries.filter_map(Result::ok).map(|entry| entry.path()).filter(|path| is_job_log(path)) {
                let size = std::fs::metadata(&path).map_or(0, |metadata| metadata.len());
                logs.insert(path, Followed { sent_from: size, ..Followed::default() });
            }
        }
        Self { dirs, logs }
    }

    /// The folders to watch, as configured.
    pub fn dirs(&self) -> impl Iterator<Item = &Path> {
        self.dirs.iter().map(PathBuf::as_path)
    }

    /// The log `path` names, keyed as configured, when it is a job log in a followed folder.
    fn log_key(&self, path: &Path) -> Option<PathBuf> {
        let parent = path.parent()?;
        let name = path.file_name()?;
        // The watcher may report canonical paths (/private/var for /var on macOS), and a folder made after the
        // server started has no canonical form before.
        let dir = self.dirs.iter().find(|dir| parent == *dir).or_else(|| {
            let parent = parent.canonicalize().ok()?;
            self.dirs.iter().find(|dir| dir.canonicalize().is_ok_and(|dir| dir == parent))
        })?;
        let key = dir.join(name);
        is_job_log(&key).then_some(key)
    }

    /// Whether `path` is a job log this follower reads.
    pub fn follows(&self, path: &Path) -> bool {
        self.log_key(path).is_some()
    }

    /// The events the complete lines appended to `path` since the last call add, in log order. A partial last
    /// line waits for its newline; a log that shrank is read again from its start; a deleted one is forgotten.
    pub fn changed(&mut self, path: &Path) -> Vec<ApiEvent> {
        let Some(key) = self.log_key(path) else { return Vec::new() };
        let Ok(mut file) = File::open(&key) else {
            self.logs.remove(&key);
            return Vec::new();
        };
        let len = file.metadata().map_or(0, |metadata| metadata.len());
        let followed = self.logs.entry(key).or_default();
        if len < followed.offset {
            *followed = Followed::default();
        }
        let mut appended = Vec::new();
        if file.seek(SeekFrom::Start(followed.offset)).is_err() || file.read_to_end(&mut appended).is_err() {
            return Vec::new();
        }
        let Some(complete) = appended.iter().rposition(|byte| *byte == b'\n').map(|last| last + 1) else {
            return Vec::new();
        };
        let mut events = Vec::new();
        let mut line_end = followed.offset;
        for piece in appended[..complete].split_inclusive(|byte| *byte == b'\n') {
            line_end += piece.len() as u64;
            if let Some(event) = followed.read_line(&piece[..piece.len() - 1])
                && line_end > followed.sent_from
                && let Some(header) = &followed.header
                && !holds_job(&header.job_id)
            {
                events.push(followed.api_event(header, &event));
            }
        }
        followed.offset += complete as u64;
        events
    }
}

impl Followed {
    /// Take one log line into the job's fields; an event line is returned to be sent.
    fn read_line(&mut self, line: &[u8]) -> Option<AgentJobEvent> {
        match serde_json::from_slice::<JobLogLine>(line).ok()? {
            JobLogLine::Header(header) => {
                self.header.get_or_insert(header);
                None
            }
            JobLogLine::Status(status) => {
                self.status = Some(status);
                None
            }
            JobLogLine::Event(entry) => {
                self.events += 1;
                if entry.kind == "agent_job_started" && self.started_at.is_none() {
                    self.started_at = Some(entry.at.clone());
                }
                Some(AgentJobEvent {
                    kind: entry.kind,
                    sequence: entry.sequence.unwrap_or(self.events),
                    at: entry.at,
                    message: entry.message,
                    tool: entry.tool,
                    plan: entry.plan,
                })
            }
        }
    }

    fn api_event(&self, header: &JobLogHeader, event: &AgentJobEvent) -> ApiEvent {
        let mut job = saved_job_dto(&SavedJob { header: header.clone(), events: Vec::new(), status: self.status.clone() }, true);
        job.started_at = self.started_at.clone();
        ApiEvent { kind: event.kind.clone(), data: job_event_payload(&job, event) }
    }
}

/// Whether this process holds the job: it sends the job's events from memory.
fn holds_job(job_id: &str) -> bool {
    JOB_REGISTRY.lock().is_ok_and(|registry| registry.jobs.contains_key(job_id))
}

#[cfg(test)]
mod tests {
    use super::LogFollower;
    use std::io::Write;
    use std::path::Path;

    const HEADER: &str = r#"{"type":"header","job_id":"JOB","ticket_id":"FOL-1","runner":"claude","created_at":"2026-11-21T10:00:00Z","agent_profile":"fix"}"#;

    fn event(kind: &str, sequence: u64, message: &str) -> String {
        format!(r#"{{"type":"event","kind":"{kind}","at":"2026-11-21T10:00:0{sequence}Z","message":"{message}","sequence":{sequence}}}"#)
    }

    fn append(path: &Path, text: &str) {
        std::fs::OpenOptions::new().create(true).append(true).open(path).unwrap().write_all(text.as_bytes()).unwrap();
    }

    fn kinds_and_messages(follower: &mut LogFollower, path: &Path) -> Vec<(String, u64, String)> {
        follower
            .changed(path)
            .into_iter()
            .map(|event| (event.kind, event.data["sequence"].as_u64().unwrap(), event.data["message"].as_str().unwrap_or("").to_string()))
            .collect()
    }

    fn tasks_dir() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let tasks = dir.path().join(".tasks");
        std::fs::create_dir_all(tasks.join("@agent-logs")).unwrap();
        (dir, tasks)
    }

    /// DEV-765: complete appended lines go out in order with the job's fields; a partial line waits.
    #[test]
    fn appended_lines_become_events_and_a_partial_line_waits() {
        let (_dir, tasks) = tasks_dir();
        let log = tasks.join("@agent-logs").join("job-follow-a.jsonl");
        let mut follower = LogFollower::new(&tasks);
        append(&log, &format!("{}\n{}\n", HEADER.replace("JOB", "job-follow-a"), event("agent_job_started", 1, "")));
        let started = follower.changed(&log);
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].kind, "agent_job_started");
        assert_eq!(started[0].data["ticket_id"], "FOL-1");
        assert_eq!(started[0].data["agent"], "fix");
        assert_eq!(started[0].data["status"], "running");
        assert_eq!(started[0].data["started_at"], "2026-11-21T10:00:01Z");

        let message = event("agent_job_message", 2, "hello");
        let (first, rest) = message.split_at(20);
        append(&log, first);
        assert!(follower.changed(&log).is_empty(), "a partial line waits for its newline");
        append(&log, &format!("{rest}\n{}\n", event("agent_job_progress", 3, "Bash: ls")));
        assert_eq!(
            kinds_and_messages(&mut follower, &log),
            vec![("agent_job_message".to_string(), 2, "hello".to_string()), ("agent_job_progress".to_string(), 3, "Bash: ls".to_string())]
        );

        append(&log, "not json\n");
        append(&log, &format!("{}\n", r#"{"type":"status","status":"completed","at":"2026-11-21T10:01:00Z","exit_code":0}"#));
        append(&log, &format!("{}\n", event("agent_job_completed", 4, "")));
        let ended = follower.changed(&log);
        assert_eq!(ended.len(), 1, "a malformed line is skipped");
        assert_eq!(ended[0].data["status"], "completed");
        assert_eq!(ended[0].data["finished_at"], "2026-11-21T10:01:00Z");
        assert_eq!(ended[0].data["exit_code"], 0);
        assert!(follower.changed(&log).is_empty(), "nothing is sent twice");
    }

    /// Logs from before the server started are not replayed, but their header still names the job.
    #[test]
    fn logs_from_before_the_start_are_not_replayed() {
        let (_dir, tasks) = tasks_dir();
        let log = tasks.join("@agent-logs").join("job-follow-b.jsonl");
        append(&log, &format!("{}\n{}\n", HEADER.replace("JOB", "job-follow-b"), event("agent_job_started", 1, "")));
        let mut follower = LogFollower::new(&tasks);
        append(&log, &format!("{}\n", event("agent_job_message", 2, "later")));
        let events = follower.changed(&log);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data["message"], "later");
        assert_eq!(events[0].data["ticket_id"], "FOL-1");
        assert_eq!(events[0].data["started_at"], "2026-11-21T10:00:01Z");
    }

    /// DEV-766: a followed `agent_job_tool` line keeps its `tool` object.
    #[test]
    fn a_followed_tool_entry_keeps_its_tool() {
        let (_dir, tasks) = tasks_dir();
        let log = tasks.join("@agent-logs").join("job-follow-tool.jsonl");
        let mut follower = LogFollower::new(&tasks);
        let tool = r#"{"type":"event","kind":"agent_job_tool","at":"2026-11-21T10:00:02Z","message":"Terminal: ls","sequence":2,"tool":{"id":"toolu_1","kind":"execute","title":"Terminal","status":"completed","input":"ls"}}"#;
        append(&log, &format!("{}\n{}\n{tool}\n", HEADER.replace("JOB", "job-follow-tool"), event("agent_job_started", 1, "")));
        let events = follower.changed(&log);
        assert_eq!(events.len(), 2);
        assert_eq!(events[1].kind, "agent_job_tool");
        assert_eq!(events[1].data["tool"]["id"], "toolu_1");
        assert_eq!(events[1].data["tool"]["input"], "ls");
        assert!(events[0].data["tool"].is_null());
    }

    /// A log that shrank is read again; a deleted one is forgotten; files that are no job logs are not followed.
    #[test]
    fn a_shrunk_log_is_read_again_and_other_files_are_ignored() {
        let (_dir, tasks) = tasks_dir();
        let logs = tasks.join("@agent-logs");
        let log = logs.join("job-follow-c.jsonl");
        let mut follower = LogFollower::new(&tasks);
        append(&log, &format!("{}\n{}\n{}\n", HEADER.replace("JOB", "job-follow-c"), event("agent_job_started", 1, ""), event("agent_job_message", 2, "x")));
        assert_eq!(follower.changed(&log).len(), 2);
        std::fs::write(&log, format!("{}\n{}\n", HEADER.replace("JOB", "job-follow-c"), event("agent_job_started", 1, ""))).unwrap();
        assert_eq!(follower.changed(&log).len(), 1, "read from the start again");
        std::fs::remove_file(&log).unwrap();
        assert!(follower.changed(&log).is_empty());
        assert!(!follower.follows(&logs.join(".gitignore")));
        assert!(!follower.follows(&tasks.join("FOL").join("1.yml")));
    }

    /// Old logs have no sequences: their events count by line, as a reload counts them.
    #[test]
    fn events_without_a_sequence_count_by_line() {
        let (_dir, tasks) = tasks_dir();
        let log = tasks.join("@agent-logs").join("job-follow-d.jsonl");
        let mut follower = LogFollower::new(&tasks);
        let line = |message: &str| format!(r#"{{"type":"event","kind":"agent_job_progress","at":"2026-11-21T10:00:00Z","message":"{message}"}}"#);
        append(&log, &format!("{}\n{}\n{}\n", HEADER.replace("JOB", "job-follow-d"), line("a"), line("b")));
        let sequences: Vec<u64> = kinds_and_messages(&mut follower, &log).into_iter().map(|(_, sequence, _)| sequence).collect();
        assert_eq!(sequences, vec![1, 2]);
    }

    /// A job this process holds streams from memory: its log lines are read but not sent again.
    #[test]
    fn a_job_this_process_holds_is_not_sent_from_its_log() {
        let (_dir, tasks) = tasks_dir();
        let log = tasks.join("@agent-logs").join("job-follow-held.jsonl");
        let mut follower = LogFollower::new(&tasks);
        super::super::JOB_REGISTRY.lock().unwrap().jobs.insert("job-follow-held".to_string(), super::super::tests::unit_job_state("job-follow-held"));
        append(&log, &format!("{}\n{}\n", HEADER.replace("JOB", "job-follow-held"), event("agent_job_started", 1, "")));
        let events = follower.changed(&log);
        super::super::JOB_REGISTRY.lock().unwrap().jobs.remove("job-follow-held");
        assert!(events.is_empty());
    }

    /// A moved log folder is followed; `off` follows none.
    #[test]
    fn the_configured_log_folder_is_followed() {
        let (dir, tasks) = tasks_dir();
        std::fs::write(tasks.join("config.yml"), "agent_logs_dir: elsewhere/logs\n").unwrap();
        let moved = dir.path().join("elsewhere").join("logs");
        let follower = LogFollower::new(&tasks);
        assert!(follower.follows(&moved.join("job-follow-e.jsonl")));
        assert!(!follower.follows(&tasks.join("@agent-logs").join("job-follow-e.jsonl")));

        std::fs::write(tasks.join("config.yml"), "agent_logs_dir: off\n").unwrap();
        assert_eq!(LogFollower::new(&tasks).dirs().count(), 0);
    }
}
