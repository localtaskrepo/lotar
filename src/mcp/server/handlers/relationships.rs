use super::super::{JsonRpcRequest, JsonRpcResponse, domain_service_error, err, ok};
use crate::api_types::{
    RelationshipGraphRequest, RelationshipMutationRequest, RelationshipRepairRequest,
};
use crate::services::relationships;
use crate::storage::manager::Storage;
use crate::workspace::TasksDirectoryResolver;
use serde_json::{Value, json};

fn respond<T: serde::Serialize>(
    id: Option<Value>,
    result: crate::errors::LoTaRResult<T>,
) -> JsonRpcResponse {
    match result {
        Ok(response) => match serde_json::to_string(&response) {
            Ok(text) => ok(id, json!({"content": [{"type": "text", "text": text}]})),
            Err(error) => err(id, -32603, &error.to_string(), None),
        },
        Err(error) => domain_service_error(id, "Relationship operation failed", &error),
    }
}

fn storage() -> Result<Storage, String> {
    TasksDirectoryResolver::resolve_read_only(None, None).map(|resolver| Storage {
        root_path: resolver.path,
    })
}

pub(crate) fn handle_relationship_graph(req: JsonRpcRequest) -> JsonRpcResponse {
    let request: RelationshipGraphRequest = match serde_json::from_value(req.params) {
        Ok(request) => request,
        Err(error) => return err(req.id, -32602, &error.to_string(), None),
    };
    let storage = match storage() {
        Ok(storage) => storage,
        Err(error) => return err(req.id, -32603, &error, None),
    };
    respond(req.id, relationships::graph(&storage, &request))
}

pub(crate) fn handle_relationship_check(req: JsonRpcRequest) -> JsonRpcResponse {
    let project = req.params.get("project").and_then(Value::as_str);
    let storage = match storage() {
        Ok(storage) => storage,
        Err(error) => return err(req.id, -32603, &error, None),
    };
    respond(req.id, relationships::check(&storage, project))
}

pub(crate) fn handle_relationship_mutate(req: JsonRpcRequest) -> JsonRpcResponse {
    let request: RelationshipMutationRequest = match serde_json::from_value(req.params) {
        Ok(request) => request,
        Err(error) => return err(req.id, -32602, &error.to_string(), None),
    };
    let storage = match storage() {
        Ok(storage) => storage,
        Err(error) => return err(req.id, -32603, &error, None),
    };
    respond(req.id, relationships::mutate(&storage, &request))
}

pub(crate) fn handle_relationship_repair(req: JsonRpcRequest) -> JsonRpcResponse {
    let request: RelationshipRepairRequest = match serde_json::from_value(req.params) {
        Ok(request) => request,
        Err(error) => return err(req.id, -32602, &error.to_string(), None),
    };
    let storage = match storage() {
        Ok(storage) => storage,
        Err(error) => return err(req.id, -32603, &error, None),
    };
    respond(req.id, relationships::repair(&storage, &request))
}
