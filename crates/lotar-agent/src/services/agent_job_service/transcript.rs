//! A job's conversation, read from its harnessmux event log (DEV-909).
//!
//! The job log names the turns the job sent with `agent_job_transcript`
//! entries; an older job log only says "`harnessmux attach <name>` follows
//! it", and its turns are the ones that started while the job ran. The reader
//! takes a job id, never a path: it opens only
//! `<harnessmux state>/sessions/<name>.events.jsonl`, or one of its kept copies
//! `<name>.<last used>` (DEV-611), for a name harnessmux accepts. Events become
//! entries shaped like the job log's: a tool call's events one entry, a run of
//! thinking one entry, the session's raw agent messages dropped.

use super::{AgentJobService, t3_state, tools};
use crate::api_types::{AgentJobEvent, AgentJobTranscript, AgentJobTranscriptRef};
use crate::utils::time;
use harnessmux::EventBody;
use jiff::{SignedDuration, Timestamp};
use std::collections::BTreeSet;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// The format of the `agent_job_transcript` reference.
pub(super) const REFERENCE_FORMAT: u32 = 1;

/// Entries in a page when the caller names no limit, and at most.
const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 500;

/// How far a turn may start before the job's start or after its end and still count as the job's: the job's times
/// are lotar's, the turn's harnessmux's.
const SLACK: SignedDuration = SignedDuration::from_secs(60);

impl AgentJobService {
    /// The conversation of the job `id`'s turns, the entries after harnessmux `seq` `after`, at most `limit`. `None`
    /// for a job neither this process nor a log knows; `Some(Err)` for a session name harnessmux refuses.
    pub fn transcript_in(tasks_dir: &Path, id: &str, after: u64, limit: Option<usize>) -> Option<Result<AgentJobTranscript, String>> {
        let (job, events) = Self::job_with_events_in(tasks_dir, id)?;
        let from = job.started_at.as_deref().unwrap_or(&job.created_at);
        let window = Window::new(from, job.finished_at.as_deref());
        let sessions_dir = sessions_dir();
        Some(read(&sessions_dir, &events, &window, after, limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)))
    }
}

/// Where harnessmux keeps its sessions' event logs.
fn sessions_dir() -> PathBuf {
    let events = crate::services::agent_runner::registry().events_path("x");
    events.parent().map(Path::to_path_buf).unwrap_or_default()
}

/// The time a job ran, widened by [`SLACK`]; an open end for a job that runs.
pub(super) struct Window {
    from: Option<Timestamp>,
    to: Option<Timestamp>,
}

impl Window {
    pub(super) fn new(from: &str, to: Option<&str>) -> Self {
        let parse = |at: &str| time::parse_rfc3339(at).ok();
        Self { from: parse(from).map(|at| at - SLACK), to: to.and_then(parse).map(|at| at + SLACK) }
    }

    fn holds(&self, at: &str) -> bool {
        let Ok(at) = time::parse_rfc3339(at) else { return false };
        self.from.is_none_or(|from| at >= from) && self.to.is_none_or(|to| at <= to)
    }
}

/// What the job log says about the job's session.
struct Pointer {
    session: String,
    /// The turns the job sent; empty for an older log, whose turns come from the time window.
    turns: BTreeSet<u64>,
    agent: Option<String>,
    link: Option<String>,
}

fn pointer(events: &[AgentJobEvent]) -> Option<Pointer> {
    let references: Vec<&AgentJobTranscriptRef> =
        events.iter().filter_map(|event| event.transcript.as_ref()).filter(|reference| reference.source == "harnessmux").collect();
    let recorded_link = || t3_state::recorded_thread(events).and_then(|(_, link)| link);
    if let Some(last) = references.last() {
        return Some(Pointer {
            session: last.session.clone(),
            turns: references.iter().filter(|reference| reference.session == last.session).map(|reference| reference.turn).collect(),
            agent: Some(last.agent.clone()).filter(|agent| !agent.is_empty()),
            link: references.iter().rev().find_map(|reference| reference.link.clone()).or_else(recorded_link),
        });
    }
    // Before DEV-909: "Session <name> (`harnessmux attach <name>` follows it)."
    let session = events.iter().rev().find_map(|event| {
        let (_, rest) = event.message.as_deref()?.split_once("`harnessmux attach ")?;
        let (name, _) = rest.split_once('`')?;
        Some(name.trim().to_string()).filter(|name| !name.is_empty())
    })?;
    Some(Pointer { session, turns: BTreeSet::new(), agent: None, link: recorded_link() })
}

fn read(sessions_dir: &Path, events: &[AgentJobEvent], window: &Window, after: u64, limit: usize) -> Result<AgentJobTranscript, String> {
    let Some(pointer) = pointer(events) else {
        let reason = Some("The job's log names no harnessmux session.".to_string());
        return Ok(AgentJobTranscript { state: "unsupported".to_string(), reason, ..AgentJobTranscript::default() });
    };
    harnessmux::registry::validate_name(&pointer.session).map_err(|err| err.to_string())?;
    let mut transcript = AgentJobTranscript {
        state: "ok".to_string(),
        source: Some(if pointer.turns.is_empty() { "time_window" } else { "reference" }.to_string()),
        session: Some(pointer.session.clone()),
        agent: pointer.agent.clone(),
        link: pointer.link.clone(),
        ..AgentJobTranscript::default()
    };
    let candidates = candidates(sessions_dir, &pointer.session);
    if candidates.is_empty() {
        transcript.state = "expired".to_string();
        transcript.reason = Some(format!("The session {}'s event log is gone.", pointer.session));
        return Ok(transcript);
    }
    let mut found = None;
    for path in &candidates {
        match scan(path, &pointer.turns, window) {
            Scan::Turns(turns) => {
                found = Some(turns);
                break;
            }
            Scan::Version(version) => {
                transcript.state = "unsupported".to_string();
                transcript.reason =
                    Some(format!("The session's event log has format version {version}; this lotar reads version {}.", harnessmux::event::EVENT_VERSION));
                return Ok(transcript);
            }
            Scan::NoTurn => {}
        }
    }
    let Some(turns) = found else {
        transcript.state = "unsupported".to_string();
        transcript.reason = Some("No turn of the session falls in this job.".to_string());
        return Ok(transcript);
    };
    transcript.agent = transcript.agent.or(turns.agent);
    transcript.link = transcript.link.or(turns.link);
    transcript.session_turns = Some(turns.session_turns);
    transcript.turns = turns.turns.iter().copied().collect();
    transcript.tool_events = turns.tool_events;
    let mut page = turns.entries.into_iter().filter(|entry| entry.sequence > after);
    transcript.events = page.by_ref().take(limit).collect();
    if page.next().is_some() {
        transcript.next_after = transcript.events.last().map(|entry| entry.sequence);
    }
    Ok(transcript)
}

/// The event logs the session `name` may be in: its own, then the copies kept when a new session took the name
/// (`<name>.<YYYYMMDDTHHMMSSZ>`, newest first).
fn candidates(sessions_dir: &Path, name: &str) -> Vec<PathBuf> {
    const SUFFIX: &str = ".events.jsonl";
    let mut found = Vec::new();
    let own = sessions_dir.join(format!("{name}{SUFFIX}"));
    if own.is_file() {
        found.push(own);
    }
    let Ok(entries) = std::fs::read_dir(sessions_dir) else { return found };
    let mut kept: Vec<(String, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file = entry.file_name().to_str()?.to_string();
            let kept_name = file.strip_suffix(SUFFIX)?;
            let (base, stamp) = kept_name.rsplit_once('.')?;
            let is_stamp = stamp.len() == 16 && stamp.as_bytes()[8] == b'T' && stamp.ends_with('Z');
            // A kept name is cut to fit 64 characters with its stamp.
            (is_stamp && !base.is_empty() && name.starts_with(base) && harnessmux::registry::validate_name(kept_name).is_ok())
                .then(|| (stamp.to_string(), entry.path()))
        })
        .filter(|(_, path)| path.is_file())
        .collect();
    kept.sort_by(|a, b| b.0.cmp(&a.0));
    found.extend(kept.into_iter().map(|(_, path)| path));
    found
}

enum Scan {
    Turns(Turns),
    /// An event of another format version.
    Version(u64),
    /// The log holds none of the job's turns.
    NoTurn,
}

#[derive(Default)]
struct Turns {
    turns: BTreeSet<u64>,
    session_turns: u64,
    agent: Option<String>,
    link: Option<String>,
    tool_events: bool,
    entries: Vec<AgentJobEvent>,
}

/// Read an event log and keep the entries of the job's turns: `wanted` when the job log names them, else the turns
/// that started in `window`. A named turn counts only when it started in the window too, so a session that took the
/// name later doesn't answer for it.
fn scan(path: &Path, wanted: &BTreeSet<u64>, window: &Window) -> Scan {
    let Ok(file) = std::fs::File::open(path) else { return Scan::NoTurn };
    let mut result = Turns::default();
    let mut builder = Builder::default();
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
        let version = value.get("v").and_then(serde_json::Value::as_u64).unwrap_or(0);
        if version != u64::from(harnessmux::event::EVENT_VERSION) {
            return Scan::Version(version);
        }
        let Ok(event) = serde_json::from_value::<harnessmux::Event>(value) else { continue };
        if let EventBody::SessionStarted { agent, link, .. } = &event.body {
            result.agent = Some(agent.clone());
            if link.is_some() {
                result.link.clone_from(link);
            }
        }
        let Some(turn) = event.turn else { continue };
        if let EventBody::TurnStarted { .. } = &event.body {
            result.session_turns = result.session_turns.max(turn);
            let named = wanted.is_empty() || wanted.contains(&turn);
            if named && window.holds(&event.ts) {
                result.turns.insert(turn);
            }
        }
        if result.turns.contains(&turn) {
            if matches!(event.body, EventBody::ToolStarted { .. } | EventBody::ToolFinished { .. }) {
                result.tool_events = true;
            }
            builder.push(&event);
        }
    }
    if result.turns.is_empty() {
        return Scan::NoTurn;
    }
    result.entries = builder.finish();
    Scan::Turns(result)
}

/// Turns harnessmux events into job log entries, in `seq` order.
#[derive(Default)]
struct Builder {
    entries: Vec<AgentJobEvent>,
    tools: tools::ToolCalls,
    /// Each running tool call's entry.
    tool_at: std::collections::HashMap<String, usize>,
    /// The entry of the thinking and of the streamed reply that go on.
    thought: Option<usize>,
    reply: Option<usize>,
    /// The turn's reply streamed: its whole `message` repeats it.
    streamed: bool,
}

impl Builder {
    fn add(&mut self, kind: &str, event: &harnessmux::Event, message: Option<String>) -> usize {
        self.entries.push(AgentJobEvent { kind: kind.to_string(), sequence: event.seq, at: event.ts.clone(), message, ..AgentJobEvent::default() });
        self.entries.len() - 1
    }

    fn append(&mut self, at: Option<usize>, kind: &str, event: &harnessmux::Event, text: &str) -> usize {
        match at {
            Some(index) => {
                if let Some(message) = self.entries[index].message.as_mut() {
                    message.push_str(text);
                }
                index
            }
            None => self.add(kind, event, Some(text.to_string())),
        }
    }

    fn tool(&mut self, id: &str, event: &harnessmux::Event, tool: crate::api_types::AgentJobTool) {
        let message = Some(tools::tool_line(&tool));
        let index = match self.tool_at.get(id) {
            Some(&index) => index,
            None => {
                let index = self.add("agent_job_tool", event, None);
                self.tool_at.insert(id.to_string(), index);
                index
            }
        };
        self.entries[index].message = message;
        self.entries[index].tool = Some(tool);
    }

    fn push(&mut self, event: &harnessmux::Event) {
        let quiet = matches!(event.body, EventBody::Usage(_) | EventBody::ConfigChanged { .. });
        if !quiet && !matches!(event.body, EventBody::ThoughtDelta { .. }) {
            self.thought = None;
        }
        if !quiet && !matches!(event.body, EventBody::MessageDelta { .. } | EventBody::Message { .. }) {
            self.reply = None;
        }
        let at = event.ts.clone();
        match &event.body {
            EventBody::TurnStarted { prompt } => {
                self.streamed = false;
                self.add("agent_job_input", event, Some(prompt.clone()));
            }
            EventBody::ThoughtDelta { text } => self.thought = Some(self.append(self.thought, "agent_job_thought", event, text)),
            EventBody::MessageDelta { text } => {
                self.streamed = true;
                self.reply = Some(self.append(self.reply, "agent_job_message", event, text));
            }
            // The turn's whole reply, as the job log takes it: only when nothing streamed.
            EventBody::Message { text } if !self.streamed => {
                self.add("agent_job_message", event, Some(text.clone()));
            }
            EventBody::ToolStarted { id, title, kind, input, locations } => {
                let fields = tools::ToolFields { title: Some(title.clone()), kind: kind.clone(), input: input.clone(), locations: locations.clone() };
                let tool = self.tools.started(id.clone(), fields, at, &[]);
                self.tool(id, event, tool);
            }
            EventBody::ToolUpdated { id, title, kind, input, locations } => {
                let fields = tools::ToolFields { title: title.clone(), kind: kind.clone(), input: input.clone(), locations: locations.clone() };
                if let Some(tool) = self.tools.updated(id.clone(), fields, &[]) {
                    self.tool(id, event, tool);
                }
            }
            EventBody::ToolFinished { id, status, title, output } => {
                let tool = self.tools.finished(id.clone(), status, title.clone(), output.as_deref(), at, &[]);
                self.tool(id, event, tool);
                self.tool_at.remove(id);
            }
            EventBody::Plan { entries } => {
                let plan: Vec<_> = entries
                    .iter()
                    .map(|entry| crate::api_types::AgentJobPlanEntry {
                        content: entry.content.clone(),
                        status: entry.status.clone(),
                        priority: entry.priority.clone(),
                    })
                    .collect();
                let index = self.add("agent_job_plan", event, Some(tools::plan_text(&plan)));
                self.entries[index].plan = Some(plan);
            }
            EventBody::PermissionRequested { title, options, .. } => {
                let names = options.iter().map(|option| option.name.as_str()).collect::<Vec<_>>().join(", ");
                self.add("agent_job_question", event, Some(format!("{title} ({names})")));
            }
            EventBody::Notice { text } => {
                self.add("agent_job_progress", event, Some(text.clone()));
            }
            EventBody::TurnFailed { error, .. } => {
                self.add("agent_job_failed", event, Some(error.clone()));
            }
            _ => {}
        }
    }

    fn finish(self) -> Vec<AgentJobEvent> {
        // Thinking and replies that streamed in pieces, as one.
        self.entries
            .into_iter()
            .filter(|entry| entry.tool.is_some() || entry.plan.is_some() || entry.message.as_deref().is_some_and(|text| !text.trim().is_empty()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn line(seq: u64, turn: Option<u64>, ts: &str, body: &serde_json::Value) -> String {
        let mut value = json!({"v": 1, "ts": ts, "session": "s", "seq": seq});
        if let Some(turn) = turn {
            value["turn"] = json!(turn);
        }
        for (key, field) in body.as_object().unwrap() {
            value[key] = field.clone();
        }
        value.to_string()
    }

    /// A Claude session with two turns: the review job's (turn 1, 10:00) and the merge job's (turn 2, 11:00).
    fn two_turns(dir: &Path, name: &str) {
        let lines = [
            line(1, None, "2026-11-26T10:00:00Z", &json!({"type": "session.started", "agent": "claude", "cwd": "/w", "how": "new", "agent_session": "a1"})),
            line(2, Some(1), "2026-11-26T10:00:01Z", &json!({"type": "turn.started", "prompt": "Review DEV-1"})),
            line(3, Some(1), "2026-11-26T10:00:02Z", &json!({"type": "thought.delta", "text": "Reading "})),
            line(4, Some(1), "2026-11-26T10:00:02Z", &json!({"type": "thought.delta", "text": "the diff."})),
            line(5, Some(1), "2026-11-26T10:00:03Z", &json!({"type": "tool.started", "id": "t1", "title": "Read", "kind": "read", "locations": ["/w/a.rs"]})),
            line(6, Some(1), "2026-11-26T10:00:04Z", &json!({"type": "tool.finished", "id": "t1", "status": "completed", "output": "fn main() {}"})),
            line(7, Some(1), "2026-11-26T10:00:05Z", &json!({"type": "message.delta", "text": "Looks "})),
            line(8, Some(1), "2026-11-26T10:00:05Z", &json!({"type": "message.delta", "text": "good."})),
            line(9, Some(1), "2026-11-26T10:00:05Z", &json!({"type": "message", "text": "Looks good."})),
            line(10, Some(1), "2026-11-26T10:00:06Z", &json!({"type": "turn.finished", "stop_reason": "end_turn"})),
            line(
                11,
                None,
                "2026-11-26T11:00:00Z",
                &json!({"type": "session.started", "agent": "claude", "cwd": "/w", "how": "resumed", "agent_session": "a1"}),
            ),
            line(12, Some(2), "2026-11-26T11:00:01Z", &json!({"type": "turn.started", "prompt": "Merge DEV-1"})),
            line(13, Some(2), "2026-11-26T11:00:02Z", &json!({"type": "message", "text": "Merged.", "raw": {"secret": "x"}})),
            line(14, Some(2), "2026-11-26T11:00:03Z", &json!({"type": "turn.finished", "stop_reason": "end_turn"})),
        ];
        std::fs::write(dir.join(format!("{name}.events.jsonl")), lines.join("\n") + "\n").unwrap();
    }

    fn reference(session: &str, turn: u64) -> AgentJobEvent {
        AgentJobEvent {
            kind: "agent_job_transcript".into(),
            transcript: Some(AgentJobTranscriptRef {
                source: "harnessmux".into(),
                session: session.into(),
                agent: "claude".into(),
                turn,
                format: 1,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn texts(transcript: &AgentJobTranscript) -> Vec<(String, String)> {
        transcript.events.iter().map(|event| (event.kind.clone(), event.message.clone().unwrap_or_default())).collect()
    }

    #[test]
    fn a_reference_reads_only_its_turn() {
        let dir = tempfile::tempdir().unwrap();
        two_turns(dir.path(), "lotar-1-DEV-1-review");
        let window = Window::new("2026-11-26T10:59:00Z", Some("2026-11-26T11:01:00Z"));
        let second = read(dir.path(), &[reference("lotar-1-DEV-1-review", 2)], &window, 0, 100).unwrap();
        assert_eq!((second.state.as_str(), second.source.as_deref(), second.turns.clone(), second.session_turns), ("ok", Some("reference"), vec![2], Some(2)));
        assert_eq!(texts(&second), vec![("agent_job_input".into(), "Merge DEV-1".into()), ("agent_job_message".into(), "Merged.".into())]);
        assert!(!second.tool_events);
        assert!(!serde_json::to_string(&second).unwrap().contains("secret"), "raw agent messages are not sent");

        let window = Window::new("2026-11-26T10:00:00Z", Some("2026-11-26T10:00:10Z"));
        let first = read(dir.path(), &[reference("lotar-1-DEV-1-review", 1)], &window, 0, 100).unwrap();
        assert!(first.tool_events);
        let kinds: Vec<_> = first.events.iter().map(|event| event.kind.as_str()).collect();
        assert_eq!(kinds, ["agent_job_input", "agent_job_thought", "agent_job_tool", "agent_job_message"]);
        assert_eq!(first.events[1].message.as_deref(), Some("Reading the diff."));
        let tool = first.events[2].tool.as_ref().unwrap();
        assert_eq!((tool.status.as_str(), tool.output.as_deref(), tool.paths.clone()), ("completed", Some("fn main() {}"), vec!["/w/a.rs".to_string()]));
        assert_eq!(first.events[3].message.as_deref(), Some("Looks good."));
    }

    #[test]
    fn pages_follow_seq() {
        let dir = tempfile::tempdir().unwrap();
        two_turns(dir.path(), "s1");
        let window = Window::new("2026-11-26T10:00:00Z", Some("2026-11-26T10:00:10Z"));
        let refs = [reference("s1", 1)];
        let page = read(dir.path(), &refs, &window, 0, 2).unwrap();
        assert_eq!((page.events.len(), page.next_after), (2, Some(3)));
        let rest = read(dir.path(), &refs, &window, 3, 2).unwrap();
        assert_eq!((rest.events.iter().map(|event| event.sequence).collect::<Vec<_>>(), rest.next_after), (vec![5, 7], None));
    }

    #[test]
    fn an_old_job_log_reads_the_turns_in_its_time() {
        let dir = tempfile::tempdir().unwrap();
        two_turns(dir.path(), "s2");
        let old = [AgentJobEvent {
            kind: "agent_job_progress".into(),
            message: Some("Continuing the session s2 (`harnessmux attach s2` follows it).".into()),
            ..Default::default()
        }];
        let window = Window::new("2026-11-26T11:00:00Z", Some("2026-11-26T11:00:05Z"));
        let transcript = read(dir.path(), &old, &window, 0, 100).unwrap();
        assert_eq!((transcript.source.as_deref(), transcript.turns.clone()), (Some("time_window"), vec![2]));

        let elsewhere = Window::new("2026-11-27T11:00:00Z", Some("2026-11-27T11:00:05Z"));
        let none = read(dir.path(), &old, &elsewhere, 0, 100).unwrap();
        assert_eq!((none.state.as_str(), none.session.as_deref()), ("unsupported", Some("s2")));
        let unnamed = read(dir.path(), &[], &window, 0, 100).unwrap();
        assert_eq!(unnamed.state, "unsupported");
    }

    #[test]
    fn a_kept_copy_answers_for_a_reused_name() {
        let dir = tempfile::tempdir().unwrap();
        // The job's session was kept when a new one took its name; the new one's turn 1 is later.
        two_turns(dir.path(), "s3.20261126T110003Z");
        std::fs::write(
            dir.path().join("s3.events.jsonl"),
            line(1, Some(1), "2026-11-26T12:00:00Z", &json!({"type": "turn.started", "prompt": "Fix again"})) + "\n",
        )
        .unwrap();
        let window = Window::new("2026-11-26T10:00:00Z", Some("2026-11-26T10:00:10Z"));
        let transcript = read(dir.path(), &[reference("s3", 1)], &window, 0, 100).unwrap();
        assert_eq!(transcript.events[0].message.as_deref(), Some("Review DEV-1"));
    }

    #[test]
    fn expired_unsupported_and_refused_names() {
        let dir = tempfile::tempdir().unwrap();
        let window = Window::new("2026-11-26T10:00:00Z", None);
        assert_eq!(read(dir.path(), &[reference("gone", 1)], &window, 0, 100).unwrap().state, "expired");

        std::fs::write(dir.path().join("v2.events.jsonl"), json!({"v": 2, "ts": "2026-11-26T10:00:01Z", "seq": 1}).to_string() + "\n").unwrap();
        assert_eq!(read(dir.path(), &[reference("v2", 1)], &window, 0, 100).unwrap().state, "unsupported");

        for bad in ["../etc/passwd", "a/b", "..", ".hidden"] {
            assert!(read(dir.path(), &[reference(bad, 1)], &window, 0, 100).is_err(), "{bad}");
        }
    }
}
