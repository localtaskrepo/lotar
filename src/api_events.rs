use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::sync::{LazyLock, Mutex, mpsc};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiEvent {
    pub kind: String,
    pub data: JsonValue,
}

static SUBSCRIBERS: LazyLock<Mutex<Vec<mpsc::Sender<ApiEvent>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

pub fn subscribe() -> mpsc::Receiver<ApiEvent> {
    let (tx, rx) = mpsc::channel::<ApiEvent>();
    let mut subs = SUBSCRIBERS.lock().unwrap();
    subs.push(tx);
    rx
}

pub fn emit(event: ApiEvent) {
    let mut subs = SUBSCRIBERS.lock().unwrap();
    let mut alive = Vec::with_capacity(subs.len());
    for s in subs.drain(..) {
        if s.send(event.clone()).is_ok() {
            alive.push(s);
        }
    }
    *subs = alive;
}

pub fn emit_task_created(task: &crate::api_types::TaskDTO) {
    let payload = serde_json::to_value(task).unwrap_or(JsonValue::Null);
    emit(ApiEvent {
        kind: "task_created".into(),
        data: payload,
    });
}

pub fn emit_task_updated(task: &crate::api_types::TaskDTO) {
    let payload = serde_json::to_value(task).unwrap_or(JsonValue::Null);
    emit(ApiEvent {
        kind: "task_updated".into(),
        data: payload,
    });
}

pub fn emit_task_deleted(id: &str) {
    emit(ApiEvent {
        kind: "task_deleted".into(),
        data: serde_json::json!({"id": id}),
    });
}

pub fn emit_config_updated() {
    emit(ApiEvent {
        kind: "config_updated".into(),
        data: serde_json::json!({}),
    });
}
