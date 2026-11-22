//! The status feed (DEV-881): one small document with this instance's
//! running jobs, its agent queue and the tickets that wait for someone, for
//! a tool that shows several instances. It knows nothing of other
//! instances. Cheap to ask every minute: the jobs come from memory and the
//! job records, the queue from its state file, and the waiting tickets from
//! a list kept here that the task events update ticket by ticket; the whole
//! backlog is read only to start that list, after a project or config
//! change, and every [`FULL_READ_EVERY`] in case an event was missed.

use crate::api_types::{StatusFeed, StatusInstance, StatusQueue, StatusQueueEntry, StatusRunningJob, StatusWaitingTicket};
use crate::services::agent_queue_service::{AgentQueueService, QueuedEntry};
use crate::storage::TaskFilter;
use crate::storage::manager::Storage;
use crate::storage::task::Task;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, Once};
use std::time::{Duration, Instant};

/// The format's version: it grows when fields are added.
pub const VERSION: u32 = 1;

/// The statuses of a ticket that waits for someone.
const WAITING_STATUSES: [&str; 2] = ["HelpNeeded", "Blocked"];

const FULL_READ_EVERY: Duration = Duration::from_secs(15 * 60);

/// The feed for the tasks directory. `request_base` is the address the
/// request came to (`http://<Host>`), the base of every link unless
/// `server.public_url` is set.
pub fn build(tasks_dir: &Path, request_base: &str) -> StatusFeed {
    let config = crate::config::resolution::load_and_merge_configs(Some(tasks_dir)).ok();
    let base = config
        .as_ref()
        .and_then(|config| config.server_public_url.as_deref())
        .filter(|url| crate::config::types::public_url_problem(url).is_none())
        .unwrap_or(request_base)
        .trim_end_matches('/')
        .to_string();
    let root = crate::services::branding_service::workspace_root(tasks_dir);
    let name = config
        .as_ref()
        .and_then(|config| config.ui.name.clone())
        .filter(|name| !name.trim().is_empty())
        .or_else(|| crate::services::branding_service::repository_name(&root))
        .unwrap_or_else(|| "lotar".to_string());
    let ticket_url = |id: &str| format!("{base}/task/{id}");
    let storage = Storage::open_read_only(tasks_dir);

    let running = running_jobs(tasks_dir)
        .into_iter()
        .map(|job| StatusRunningJob {
            title: title_of(&storage, &job.ticket_id),
            job_url: format!("{base}/agents?job={}", job.job_id),
            ticket_url: ticket_url(&job.ticket_id),
            ..job
        })
        .collect();
    let waiting = waiting_tickets(tasks_dir).into_iter().map(|ticket| StatusWaitingTicket { ticket_url: ticket_url(&ticket.ticket_id), ..ticket }).collect();

    StatusFeed {
        version: VERSION,
        instance: StatusInstance { name, url: base.clone() },
        generated_at: jiff::Timestamp::now().to_string(),
        running,
        queue: queue(tasks_dir, &ticket_url),
        waiting,
    }
}

/// The jobs of this process and those the other processes' records name,
/// oldest first. Links and titles are filled in by [`build`].
fn running_jobs(tasks_dir: &Path) -> Vec<StatusRunningJob> {
    let job = |job_id: String, ticket_id: String, profile: Option<String>, runner: String, started_at: String| StatusRunningJob {
        job_id,
        ticket_id,
        title: String::new(),
        profile: profile.unwrap_or_default(),
        runner,
        started_at,
        job_url: String::new(),
        ticket_url: String::new(),
    };
    let mut jobs: Vec<StatusRunningJob> = crate::services::agent_active_jobs::list(tasks_dir)
        .into_iter()
        .map(|record| job(record.job_id, record.ticket_id, record.agent, record.runner, record.started_at))
        .collect();
    let mut seen: HashSet<String> = jobs.iter().map(|job| job.job_id.clone()).collect();
    for own in crate::services::agent_job_service::AgentJobService::list_jobs() {
        if own.status == "running" && seen.insert(own.id.clone()) {
            let started_at = own.started_at.unwrap_or(own.created_at);
            jobs.push(job(own.id, own.ticket_id, own.agent, own.runner, started_at));
        }
    }
    jobs.sort_by(|a, b| a.started_at.cmp(&b.started_at));
    jobs
}

fn queue(tasks_dir: &Path, ticket_url: &dyn Fn(&str) -> String) -> StatusQueue {
    let Ok(overview) = AgentQueueService::overview(tasks_dir) else {
        return StatusQueue { count: 0, held: false, held_reason: None, entries: Vec::new() };
    };
    let mut reasons: Vec<String> = Vec::new();
    if let Some(hold) = &overview.memory.hold {
        reasons.push(format!("waiting for memory: {}", hold.describe()));
    }
    if let Some(hold) = &overview.disk {
        reasons.push(format!("waiting for disk space: {}", hold.describe()));
    }
    reasons.extend(overview.pauses.iter().map(|pause| format!("the queue is {}", pause.summary())));
    let held_reason = (!reasons.is_empty()).then(|| reasons.join("; "));
    let entries: Vec<StatusQueueEntry> = overview
        .pending
        .into_iter()
        .map(|QueuedEntry { entry, waits_for, .. }| StatusQueueEntry {
            ticket_url: ticket_url(&entry.ticket_id),
            ticket_id: entry.ticket_id,
            profile: entry.agent,
            queued_at: entry.queued_at,
            waits_for,
        })
        .collect();
    StatusQueue { count: entries.len(), held: held_reason.is_some(), held_reason, entries }
}

fn title_of(storage: &Storage, id: &str) -> String {
    load(storage, id).map(|task| task.title).unwrap_or_default()
}

fn load(storage: &Storage, id: &str) -> Option<Task> {
    let project = crate::storage::TaskId::parse(id).ok()?.project;
    storage.get(id, &project).filter(|task| task.deleted_at.is_none())
}

/// The tickets that wait, kept between calls (see the module comment).
struct WaitingList {
    tasks_dir: PathBuf,
    read_at: Instant,
    tickets: BTreeMap<String, StatusWaitingTicket>,
}

/// What the task events changed since the last call.
#[derive(Default)]
struct Changes {
    ids: HashSet<String>,
    read_all: bool,
}

static WAITING: LazyLock<Mutex<Option<WaitingList>>> = LazyLock::new(|| Mutex::new(None));
static CHANGES: LazyLock<Mutex<Changes>> = LazyLock::new(|| Mutex::new(Changes::default()));
static LISTEN: Once = Once::new();

/// Note the tickets the server's events name: its own writes and the file
/// watcher's (other processes' writes).
fn listen_for_changes() {
    LISTEN.call_once(|| {
        let events = crate::api_events::subscribe();
        let spawned = std::thread::Builder::new().name("lotar-status-feed".into()).spawn(move || {
            for event in events {
                let Ok(mut changes) = CHANGES.lock() else { return };
                match event.kind.as_str() {
                    "task_created" | "task_updated" | "task_deleted" => {
                        for key in ["id", "moved_to"] {
                            if let Some(id) = event.data.get(key).and_then(|id| id.as_str()) {
                                changes.ids.insert(id.to_string());
                            }
                        }
                    }
                    "project_changed" | "config_updated" => changes.read_all = true,
                    _ => {}
                }
            }
        });
        if spawned.is_err()
            && let Ok(mut changes) = CHANGES.lock()
        {
            changes.read_all = true;
        }
    });
}

/// The waiting tickets, longest waiting first, without links.
fn waiting_tickets(tasks_dir: &Path) -> Vec<StatusWaitingTicket> {
    listen_for_changes();
    let changes = CHANGES.lock().map(|mut changes| std::mem::take(&mut *changes)).unwrap_or_default();
    let Ok(mut guard) = WAITING.lock() else {
        return Vec::new();
    };
    let storage = Storage::open_read_only(tasks_dir);
    let stale = guard.as_ref().is_none_or(|list| list.tasks_dir != tasks_dir || list.read_at.elapsed() >= FULL_READ_EVERY);
    let list = match guard.as_mut() {
        Some(list) if !stale && !changes.read_all => {
            for id in changes.ids {
                match load(&storage, &id).and_then(|task| waiting_ticket(&id, &task)) {
                    Some(ticket) => list.tickets.insert(id, ticket),
                    None => list.tickets.remove(&id),
                };
            }
            list
        }
        _ => guard.insert(read_all(tasks_dir, &storage)),
    };
    let mut tickets: Vec<StatusWaitingTicket> = list.tickets.values().cloned().collect();
    tickets.sort_by(|a, b| a.since.cmp(&b.since).then_with(|| a.ticket_id.cmp(&b.ticket_id)));
    tickets
}

fn read_all(tasks_dir: &Path, storage: &Storage) -> WaitingList {
    let filter = TaskFilter { status: WAITING_STATUSES.iter().map(|status| crate::types::TaskStatus::from(*status)).collect(), ..TaskFilter::default() };
    let tickets = storage.search(&filter).into_iter().filter_map(|(id, task)| waiting_ticket(&id, &task).map(|ticket| (id, ticket))).collect();
    WaitingList { tasks_dir: tasks_dir.to_path_buf(), read_at: Instant::now(), tickets }
}

/// The ticket as the feed lists it, when its status waits for someone.
fn waiting_ticket(id: &str, task: &Task) -> Option<StatusWaitingTicket> {
    let status = WAITING_STATUSES.iter().find(|status| task.status.eq_ignore_case(status))?;
    // The newest change into this status; a ticket created waiting has none.
    let since = task
        .history
        .iter()
        .rev()
        .find(|entry| entry.changes.iter().any(|change| change.field == "status" && change.new.as_deref().is_some_and(|new| new.eq_ignore_ascii_case(status))))
        .map_or_else(|| task.modified.clone(), |entry| entry.at.clone());
    Some(StatusWaitingTicket {
        ticket_id: id.to_string(),
        title: task.title.clone(),
        status: (*status).to_string(),
        assignee: task.assignee.clone().filter(|assignee| !assignee.trim().is_empty()),
        since,
        ticket_url: String::new(),
    })
}
