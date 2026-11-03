use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, mpsc};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiEvent {
    pub kind: String,
    pub data: JsonValue,
}

static SUBSCRIBERS: LazyLock<Mutex<Vec<mpsc::Sender<ApiEvent>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

pub fn subscribe() -> mpsc::Receiver<ApiEvent> {
    let (tx, rx) = mpsc::channel::<ApiEvent>();
    if let Ok(mut subs) = SUBSCRIBERS.lock() {
        subs.push(tx);
    } // else: if poisoned, drop silently; receiver will see no events
    rx
}

pub fn emit(event: &ApiEvent) {
    if let Ok(mut subs) = SUBSCRIBERS.lock() {
        let mut alive = Vec::with_capacity(subs.len());
        for s in subs.drain(..) {
            if s.send(event.clone()).is_ok() {
                alive.push(s);
            }
        }
        *subs = alive;
    }
}

/// Writes this process announced itself, so the file watcher can drop its
/// own echo of them (DEV-273): task id -> (modified stamp, when).
static OWN_TASK_WRITES: LazyLock<Mutex<HashMap<String, (String, Instant)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static OWN_CONFIG_WRITE: LazyLock<Mutex<Option<Instant>>> = LazyLock::new(|| Mutex::new(None));
const ECHO_WINDOW: Duration = Duration::from_secs(10);
const CONFIG_ECHO_WINDOW: Duration = Duration::from_secs(2);

fn remember_task_write(task: &crate::api_types::TaskDTO) {
    if let Ok(mut writes) = OWN_TASK_WRITES.lock() {
        if writes.len() > 1024 {
            writes.retain(|_, (_, at)| at.elapsed() < ECHO_WINDOW);
        }
        writes.insert(task.id.clone(), (task.modified.clone(), Instant::now()));
    }
}

/// Whether a watcher change to task `id` with stamp `modified` is the echo
/// of a write this process already announced with a full DTO.
pub fn is_own_task_write(id: &str, modified: &str) -> bool {
    OWN_TASK_WRITES.lock().is_ok_and(|writes| {
        writes
            .get(id)
            .is_some_and(|(stamp, at)| stamp == modified && at.elapsed() < ECHO_WINDOW)
    })
}

/// Whether this process announced a config change in the last moments.
pub fn is_recent_own_config_write() -> bool {
    OWN_CONFIG_WRITE
        .lock()
        .is_ok_and(|at| at.is_some_and(|at| at.elapsed() < CONFIG_ECHO_WINDOW))
}

pub fn emit_task_created(task: &crate::api_types::TaskDTO, triggered_by: Option<&str>) {
    remember_task_write(task);
    let mut payload = match serde_json::to_value(task) {
        Ok(v) => v,
        Err(_) => JsonValue::Null,
    };
    if let (JsonValue::Object(map), Some(actor)) = (&mut payload, triggered_by) {
        map.insert("triggered_by".into(), JsonValue::String(actor.to_string()));
    }
    emit(&ApiEvent {
        kind: "task_created".into(),
        data: payload,
    });
}

pub fn emit_task_updated(task: &crate::api_types::TaskDTO, triggered_by: Option<&str>) {
    remember_task_write(task);
    let mut payload = match serde_json::to_value(task) {
        Ok(v) => v,
        Err(_) => JsonValue::Null,
    };
    if let (JsonValue::Object(map), Some(actor)) = (&mut payload, triggered_by) {
        map.insert("triggered_by".into(), JsonValue::String(actor.to_string()));
    }
    emit(&ApiEvent {
        kind: "task_updated".into(),
        data: payload,
    });
}

pub fn emit_task_deleted(id: &str, triggered_by: Option<&str>) {
    let mut payload = serde_json::json!({"id": id});
    if let (JsonValue::Object(map), Some(actor)) = (&mut payload, triggered_by) {
        map.insert("triggered_by".into(), JsonValue::String(actor.to_string()));
    }
    emit(&ApiEvent {
        kind: "task_deleted".into(),
        data: payload,
    });
}

/// A task moved to another project (DEV-183): its old ID is gone from lists
/// (`task_deleted` naming the new ID in `moved_to`, so open views can follow
/// it) and the task appears under its new ID (`task_created`).
pub fn emit_task_moved(from: &str, task: &crate::api_types::TaskDTO, triggered_by: Option<&str>) {
    let mut payload = serde_json::json!({"id": from, "moved_to": task.id});
    if let (JsonValue::Object(map), Some(actor)) = (&mut payload, triggered_by) {
        map.insert("triggered_by".into(), JsonValue::String(actor.to_string()));
    }
    emit(&ApiEvent {
        kind: "task_deleted".into(),
        data: payload,
    });
    emit_task_created(task, triggered_by);
}

pub fn emit_config_updated(triggered_by: Option<&str>) {
    if let Ok(mut at) = OWN_CONFIG_WRITE.lock() {
        *at = Some(Instant::now());
    }
    let mut payload = serde_json::json!({});
    if let (JsonValue::Object(map), Some(actor)) = (&mut payload, triggered_by) {
        map.insert("triggered_by".into(), JsonValue::String(actor.to_string()));
    }
    emit(&ApiEvent {
        kind: "config_updated".into(),
        data: payload,
    });
}

/// Emit a task_error event when a file change is detected but the task can't be parsed.
/// The UI can show a user-friendly notification with the error details.
pub fn emit_task_error(id: &str, message: &str) {
    emit(&ApiEvent {
        kind: "task_error".into(),
        data: serde_json::json!({
            "id": id,
            "message": message,
        }),
    });
}
