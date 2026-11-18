//! What ends a running job besides its agent (DEV-682): no sign of the agent for the profile's
//! `stall_timeout_seconds` fails it as stalled, and a ticket that went to a done status or to a person while it ran
//! cancels it. Merge jobs only stall: their ticket goes Done as they land, and a cut-off merge leaves half a rebase.

use super::{JOB_REGISTRY, record_and_emit, ticket_state, update_job};
use crate::services::sprint_metrics::determine_done_statuses_from_config;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How often the job looks at its ticket: a read of the ticket and its recorded outcome.
const LOOK_EVERY: Duration = Duration::from_secs(5);

/// How long a job whose ticket moved on may still run: its own agent hands the ticket back or sets it Done as
/// its last step, and then ends its turn by itself.
pub(super) const MOVED_ON_GRACE: Duration = Duration::from_secs(120);

/// The grace, in milliseconds, for a test that can't wait 120s; the ticket is then also looked at that often.
const TEST_GRACE_VAR: &str = "LOTAR_TEST_MOVED_ON_GRACE_MS";

/// Why the watch ends the job.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum WatchEnd {
    /// The job fails with this message.
    Stalled(String),
    /// The job is cancelled; the message is in its log already.
    MovedOn,
}

pub(super) struct JobWatch {
    job_id: String,
    tasks_dir: PathBuf,
    ticket_id: String,
    stall: Option<Duration>,
    last_activity: Instant,
    last_sequence: u64,
    next_look: Instant,
    /// Lowercase; empty for a merge job, which the ticket's moves don't stop.
    done_statuses: HashSet<String>,
    /// Lowercase: a ticket Done already when the job started (a follow-up on a closed ticket) didn't move on.
    status_at_start: Option<String>,
    reconcile: bool,
    grace: Duration,
    look_every: Duration,
    /// Since when the ticket has moved on, and how.
    moved_on: Option<(Instant, String)>,
}

impl JobWatch {
    pub(super) fn new(job_id: &str, tasks_dir: &Path, ticket_id: &str, stall_timeout_seconds: Option<u64>) -> Self {
        let now = Instant::now();
        let is_merge_job = JOB_REGISTRY.lock().ok().and_then(|registry| registry.jobs.get(job_id).map(|state| state.record.is_merge_job)).unwrap_or(true);
        let prefix = ticket_id.rsplit_once('-').map(|(prefix, _)| prefix);
        let done_statuses = if is_merge_job {
            HashSet::new()
        } else {
            crate::config::resolution::config_for_project(tasks_dir, prefix).map(|config| determine_done_statuses_from_config(&config)).unwrap_or_default()
        };
        let status_at_start = ticket_state(tasks_dir, ticket_id).map(|(status, _)| status.to_ascii_lowercase());
        let grace = std::env::var(TEST_GRACE_VAR).ok().and_then(|ms| ms.trim().parse().ok()).map_or(MOVED_ON_GRACE, Duration::from_millis);
        Self {
            job_id: job_id.to_string(),
            tasks_dir: tasks_dir.to_path_buf(),
            ticket_id: ticket_id.to_string(),
            stall: stall_timeout_seconds.filter(|seconds| *seconds > 0).map(Duration::from_secs),
            last_activity: now,
            last_sequence: 0,
            next_look: now,
            done_statuses,
            status_at_start,
            reconcile: !is_merge_job,
            grace,
            look_every: LOOK_EVERY.min(grace),
            moved_on: None,
        }
    }

    /// The agent sent something.
    pub(super) fn activity(&mut self) {
        self.last_activity = Instant::now();
    }

    /// Whether the job ends now, and why. Called every turn of the job's loop.
    pub(super) fn check(&mut self) -> Option<WatchEnd> {
        self.check_at(Instant::now())
    }

    fn check_at(&mut self, now: Instant) -> Option<WatchEnd> {
        // A new entry in the job's log (a `command` runner's output line) or a question waiting for a person
        // counts as a sign of the agent.
        let (sequence, asking) = JOB_REGISTRY
            .lock()
            .ok()
            .and_then(|registry| registry.jobs.get(&self.job_id).map(|state| (state.next_sequence, state.record.question.is_some())))
            .unwrap_or_default();
        if sequence != self.last_sequence || asking {
            self.last_sequence = sequence;
            self.last_activity = now;
        }
        if let Some(stall) = self.stall
            && now.duration_since(self.last_activity) >= stall
        {
            return Some(WatchEnd::Stalled(format!(
                "Stalled: the agent sent nothing for {}s (the profile's stall_timeout_seconds); LoTaR stopped it.",
                stall.as_secs()
            )));
        }
        if !self.reconcile {
            return None;
        }
        if now >= self.next_look {
            self.next_look = now + self.look_every;
            match self.moved_on_now() {
                Some(how) if self.moved_on.as_ref().is_none_or(|(_, seen)| *seen != how) => {
                    let wait = self.grace.as_secs();
                    let note = format!("{} {how} while the job ran; LoTaR cancels the job unless the agent ends its turn within {wait}s.", self.ticket_id);
                    update_job(&self.job_id, |state| {
                        record_and_emit(state, "agent_job_progress", Some(note.clone()));
                    });
                    self.moved_on = Some((now, how));
                }
                Some(_) => {}
                None => self.moved_on = None,
            }
        }
        let (since, how) = self.moved_on.as_ref()?;
        if now.duration_since(*since) < self.grace {
            return None;
        }
        let note = format!("{} {how} while the job ran: LoTaR cancelled the job.", self.ticket_id);
        // Once: the job's loop stops at its next turn.
        self.reconcile = false;
        update_job(&self.job_id, |state| {
            record_and_emit(state, "agent_job_progress", Some(note.clone()));
        });
        Some(WatchEnd::MovedOn)
    }

    /// "went Done" or "was assigned to mallox": the ticket's status and assignee, with what is recorded for the
    /// job's end over them, against the status and assignee it started with.
    fn moved_on_now(&self) -> Option<String> {
        let (status, assignee) = ticket_state(&self.tasks_dir, &self.ticket_id)?;
        let outcome = crate::services::agent_active_jobs::peek_outcome(&self.tasks_dir, &self.job_id);
        let status = outcome.as_ref().and_then(|outcome| outcome.status.clone()).unwrap_or(status);
        let lowercase = status.to_ascii_lowercase();
        if self.done_statuses.contains(&lowercase) && self.status_at_start.as_ref() != Some(&lowercase) {
            return Some(format!("went {status}"));
        }
        let assignee = match outcome.and_then(|outcome| outcome.assignee) {
            Some(recorded) => Some(recorded).filter(|recorded| !recorded.is_empty()),
            None => assignee,
        }?;
        let at_start = JOB_REGISTRY.lock().ok().and_then(|registry| registry.jobs.get(&self.job_id).and_then(|state| state.assignee_at_start.clone()))?;
        (at_start.as_deref() != Some(assignee.as_str()) && is_person(&assignee)).then(|| format!("was assigned to {assignee}"))
    }
}

/// Not an agent profile (`@fix`) and not an agent's name (`claude`).
fn is_person(assignee: &str) -> bool {
    !assignee.starts_with('@') && !crate::config::operations::AGENT_RUNNERS.iter().any(|runner| runner.eq_ignore_ascii_case(assignee))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_types::JobOutcome;
    use crate::services::agent_active_jobs::record_outcome;

    #[test]
    fn people_are_neither_profiles_nor_agents() {
        assert!(is_person("mallox"));
        assert!(!is_person("@fix"));
        assert!(!is_person("Claude"));
    }

    /// A tasks directory with ticket TEST-1 and job `id` on it in the registry, started unassigned.
    fn job_on_a_ticket(dir: &Path, id: &str, merge: bool) -> PathBuf {
        // SAFETY: test-local override; outcomes stay out of the user's cache.
        unsafe { std::env::set_var("LOTAR_AGENT_QUEUE_DIR", dir.join("queue")) };
        let tasks_dir = dir.join(".tasks");
        std::fs::create_dir_all(&tasks_dir).unwrap();
        std::fs::write(tasks_dir.join("config.yml"), "default:\n  project: TEST\n").unwrap();
        let task = crate::api_types::TaskCreate { title: "Watched".into(), project: Some("TEST".into()), ..Default::default() };
        crate::services::task_service::TaskService::create(&mut crate::storage::manager::Storage::new(&tasks_dir), task).unwrap();
        let mut state = super::super::tests::unit_job_state(id);
        state.record.tasks_dir = tasks_dir.clone();
        state.record.is_merge_job = merge;
        state.assignee_at_start = Some(None);
        JOB_REGISTRY.lock().unwrap().jobs.insert(id.to_string(), state);
        tasks_dir
    }

    fn notes(id: &str) -> Vec<String> {
        JOB_REGISTRY.lock().unwrap().jobs[id].events.iter().filter_map(|event| event.message.clone()).collect()
    }

    /// DEV-682: a job whose ticket went to a person or to a done status while it ran is cancelled once its agent
    /// had the grace to end its turn, and its log says why.
    #[test]
    fn a_job_whose_ticket_moved_on_is_cancelled_after_the_grace() {
        let dir = tempfile::tempdir().unwrap();
        for (id, outcome, how) in [
            ("dev682-person", JobOutcome { assignee: Some("mallox".into()), ..Default::default() }, "was assigned to mallox"),
            ("dev682-done", JobOutcome { status: Some("Done".into()), ..Default::default() }, "went Done"),
        ] {
            let tasks_dir = job_on_a_ticket(&dir.path().join(id), id, false);
            let mut watch = JobWatch::new(id, &tasks_dir, "TEST-1", None);
            let start = Instant::now();
            assert_eq!(watch.check_at(start), None, "nothing moved");
            record_outcome(&tasks_dir, &JobOutcome { job_id: id.into(), ..outcome }).unwrap();
            assert_eq!(watch.check_at(start + Duration::from_secs(1)), None, "looks every {LOOK_EVERY:?}");
            let seen = start + LOOK_EVERY;
            assert_eq!(watch.check_at(seen), None, "the agent may still end its turn");
            assert_eq!(notes(id), [format!("TEST-1 {how} while the job ran; LoTaR cancels the job unless the agent ends its turn within 120s.")]);
            assert_eq!(watch.check_at(seen + MOVED_ON_GRACE), Some(WatchEnd::MovedOn));
            assert_eq!(notes(id).last().map(String::as_str), Some(format!("TEST-1 {how} while the job ran: LoTaR cancelled the job.").as_str()));
            assert_eq!(watch.check_at(seen + MOVED_ON_GRACE * 2), None, "once");
        }
    }

    /// A job started on a ticket that was Done already (a follow-up on a closed ticket) is not cancelled for it.
    #[test]
    fn a_job_on_a_ticket_done_before_it_started_runs_on() {
        let dir = tempfile::tempdir().unwrap();
        let id = "dev682-done-at-start";
        let tasks_dir = job_on_a_ticket(dir.path(), id, false);
        crate::services::task_service::TaskService::update(
            &mut crate::storage::manager::Storage::new(&tasks_dir),
            "TEST-1",
            crate::api_types::TaskUpdate { status: Some("Done".into()), ..Default::default() },
        )
        .unwrap();
        // As the job reads it when it starts: the status change may have assigned the ticket.
        let assignee = ticket_state(&tasks_dir, "TEST-1").and_then(|(_, assignee)| assignee);
        JOB_REGISTRY.lock().unwrap().jobs.get_mut(id).unwrap().assignee_at_start = Some(assignee);
        let mut watch = JobWatch::new(id, &tasks_dir, "TEST-1", None);
        let start = Instant::now();
        assert_eq!(watch.check_at(start), None);
        assert_eq!(watch.check_at(start + MOVED_ON_GRACE * 2), None);
        assert!(notes(id).is_empty());
    }

    #[test]
    fn a_merge_job_runs_on_when_its_ticket_goes_done() {
        let dir = tempfile::tempdir().unwrap();
        let id = "dev682-merge";
        let tasks_dir = job_on_a_ticket(dir.path(), id, true);
        let mut watch = JobWatch::new(id, &tasks_dir, "TEST-1", None);
        record_outcome(&tasks_dir, &JobOutcome { job_id: id.into(), status: Some("Done".into()), ..Default::default() }).unwrap();
        let start = Instant::now();
        assert_eq!(watch.check_at(start + MOVED_ON_GRACE * 2), None);
        assert!(notes(id).is_empty());
    }

    /// DEV-682: no sign of the agent for the profile's stall_timeout_seconds fails the job; a question waiting for
    /// an answer is not a stall.
    #[test]
    fn a_silent_agent_stalls_unless_it_waits_for_an_answer() {
        let dir = tempfile::tempdir().unwrap();
        let id = "dev682-stall";
        let tasks_dir = job_on_a_ticket(dir.path(), id, false);
        let mut watch = JobWatch::new(id, &tasks_dir, "TEST-1", Some(30));
        let start = Instant::now();
        assert_eq!(watch.check_at(start + Duration::from_secs(29)), None);
        JOB_REGISTRY.lock().unwrap().jobs.get_mut(id).unwrap().next_sequence += 1;
        assert_eq!(watch.check_at(start + Duration::from_secs(40)), None, "an output line is a sign of the agent");
        let question = crate::api_types::AgentJobQuestion { request: "1".into(), title: "Edit?".into(), options: Vec::new() };
        JOB_REGISTRY.lock().unwrap().jobs.get_mut(id).unwrap().record.question = Some(question);
        assert_eq!(watch.check_at(start + Duration::from_secs(600)), None, "waiting for an answer");
        JOB_REGISTRY.lock().unwrap().jobs.get_mut(id).unwrap().record.question = None;
        let end = watch.check_at(start + Duration::from_secs(630));
        assert_eq!(end, Some(WatchEnd::Stalled("Stalled: the agent sent nothing for 30s (the profile's stall_timeout_seconds); LoTaR stopped it.".into())));
        assert_eq!(JobWatch::new(id, &tasks_dir, "TEST-1", Some(0)).check_at(start + Duration::from_secs(3600)), None, "0: no limit");
    }
}
