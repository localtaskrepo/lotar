use serde_json::{Value, json};

use super::super::{JsonRpcRequest, JsonRpcResponse, ok};
use crate::utils::identity;

pub(crate) fn handle_whoami(req: JsonRpcRequest) -> JsonRpcResponse {
    let explain = req
        .params
        .get("explain")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let resolved = identity::resolve_current_user(None);

    let mut payload = serde_json::Map::new();
    payload.insert("status".to_string(), Value::String("ok".to_string()));
    payload.insert(
        "user".to_string(),
        resolved.clone().map(Value::String).unwrap_or(Value::Null),
    );

    if explain {
        let detection = identity::resolve_current_user_explain(None);
        let explain_payload = match detection {
            None => Value::Null,
            Some(det) => {
                let mut obj = serde_json::Map::new();
                obj.insert("user".to_string(), Value::String(det.user));
                obj.insert("source".to_string(), Value::String(det.source.to_string()));
                obj.insert("confidence".to_string(), Value::from(det.confidence));
                obj.insert(
                    "details".to_string(),
                    det.details.map(Value::String).unwrap_or(Value::Null),
                );
                Value::Object(obj)
            }
        };
        payload.insert("explain".to_string(), explain_payload);
    }

    ok(
        req.id,
        json!({
            "content": [
                {
                    "type": "text",
                    "text": serde_json::to_string_pretty(&Value::Object(payload)).unwrap_or_else(|_| "{}".into())
                }
            ]
        }),
    )
}
