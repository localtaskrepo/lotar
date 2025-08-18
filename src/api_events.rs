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
    if let Ok(mut subs) = SUBSCRIBERS.lock() {
        subs.push(tx);
    } // else: if poisoned, drop silently; receiver will see no events
    rx
}

pub fn emit(event: ApiEvent) {
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

pub fn emit_task_created(task: &crate::api_types::TaskDTO, triggered_by: Option<&str>) {
    let mut payload = match serde_json::to_value(task) {
        Ok(v) => v,
        Err(_) => JsonValue::Null,
    };
    if let (JsonValue::Object(map), Some(actor)) = (&mut payload, triggered_by) {
        map.insert("triggered_by".into(), JsonValue::String(actor.to_string()));
    }
    emit(ApiEvent {
        kind: "task_created".into(),
        data: payload,
    });
}

pub fn emit_task_updated(task: &crate::api_types::TaskDTO, triggered_by: Option<&str>) {
    let mut payload = match serde_json::to_value(task) {
        Ok(v) => v,
        Err(_) => JsonValue::Null,
    };
    if let (JsonValue::Object(map), Some(actor)) = (&mut payload, triggered_by) {
        map.insert("triggered_by".into(), JsonValue::String(actor.to_string()));
    }
    emit(ApiEvent {
        kind: "task_updated".into(),
        data: payload,
    });
}

pub fn emit_task_deleted(id: &str, triggered_by: Option<&str>) {
    let mut payload = serde_json::json!({"id": id});
    if let (JsonValue::Object(map), Some(actor)) = (&mut payload, triggered_by) {
        map.insert("triggered_by".into(), JsonValue::String(actor.to_string()));
    }
    emit(ApiEvent {
        kind: "task_deleted".into(),
        data: payload,
    });
}

pub fn emit_config_updated(triggered_by: Option<&str>) {
    let mut payload = serde_json::json!({});
    if let (JsonValue::Object(map), Some(actor)) = (&mut payload, triggered_by) {
        map.insert("triggered_by".into(), JsonValue::String(actor.to_string()));
    }
    emit(ApiEvent {
        kind: "config_updated".into(),
        data: payload,
    });
}
