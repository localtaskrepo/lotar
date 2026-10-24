use super::*;
use crate::api_types::{
    RelationshipGraphRequest, RelationshipMutationRequest, RelationshipRepairRequest,
};
use crate::services::relationships;
use crate::storage::manager::Storage;

fn storage() -> Result<Storage, HttpResponse> {
    TasksDirectoryResolver::resolve_read_only(None, None)
        .map(|resolver| Storage {
            root_path: resolver.path,
        })
        .map_err(|error| internal(json!({"error": {"code": "INTERNAL", "message": error}})))
}

fn query_text(req: &HttpRequest, key: &str) -> Result<Option<String>, HttpResponse> {
    req.query
        .get(key)
        .map(|value| {
            let value = value.trim();
            if value.is_empty() {
                Err(bad_request(format!("{key} must not be blank")))
            } else {
                Ok(value.to_string())
            }
        })
        .transpose()
}

fn query_number(req: &HttpRequest, key: &str) -> Result<Option<usize>, HttpResponse> {
    query_text(req, key)?
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|_| bad_request(format!("{key} must be a non-negative integer")))
        })
        .transpose()
}

pub(super) fn register(api_server: &mut ApiServer) {
    api_server.register_handler("GET", "/api/relationships/graph", |req| {
        let request = (|| {
            Ok::<_, HttpResponse>(RelationshipGraphRequest {
                project: query_text(req, "project")?,
                task_id: query_text(req, "task_id")?,
                depth: query_number(req, "depth")?,
                max_nodes: query_number(req, "max_nodes")?,
                relationship_kinds: req
                    .query
                    .get("kinds")
                    .map(|raw| {
                        if raw.is_empty() {
                            return Ok(Vec::new());
                        }
                        raw.split(',')
                            .map(|kind| {
                                serde_json::from_value(serde_json::Value::String(kind.to_string()))
                                    .map_err(|_| {
                                        bad_request(format!("Invalid relationship kind: {kind}"))
                                    })
                            })
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .transpose()?,
                include_isolated: req
                    .query
                    .get("include_isolated")
                    .map(|raw| match raw.as_str() {
                        "true" => Ok(true),
                        "false" => Ok(false),
                        _ => Err(bad_request("include_isolated must be true or false".into())),
                    })
                    .transpose()?,
                filters: req
                    .query
                    .get("filters")
                    .map(|raw| {
                        serde_json::from_str(raw)
                            .map_err(|error| bad_request(format!("Invalid graph filters: {error}")))
                    })
                    .transpose()?
                    .unwrap_or_default(),
            })
        })();
        let request = match request {
            Ok(request) => request,
            Err(response) => return response,
        };
        let storage = match storage() {
            Ok(storage) => storage,
            Err(response) => return response,
        };
        match relationships::graph(&storage, &request) {
            Ok(response) => ok_json(200, json!({"data": response})),
            Err(error) => map_lotar_error(error),
        }
    });

    api_server.register_handler("GET", "/api/relationships/check", |req| {
        let project = match query_text(req, "project") {
            Ok(project) => project,
            Err(response) => return response,
        };
        let storage = match storage() {
            Ok(storage) => storage,
            Err(response) => return response,
        };
        match relationships::check(&storage, project.as_deref()) {
            Ok(response) => ok_json(200, json!({"data": response})),
            Err(error) => map_lotar_error(error),
        }
    });

    api_server.register_handler("POST", "/api/relationships/mutate", |req| {
        let request: RelationshipMutationRequest = match serde_json::from_slice(&req.body) {
            Ok(request) => request,
            Err(error) => return bad_request(format!("Invalid relationship request: {error}")),
        };
        let storage = match storage() {
            Ok(storage) => storage,
            Err(response) => return response,
        };
        match relationships::mutate(&storage, &request) {
            Ok(response) => ok_json(200, json!({"data": response})),
            Err(error) => map_lotar_error(error),
        }
    });

    api_server.register_handler("POST", "/api/relationships/repair", |req| {
        let request: RelationshipRepairRequest = match serde_json::from_slice(&req.body) {
            Ok(request) => request,
            Err(error) => {
                return bad_request(format!("Invalid relationship repair request: {error}"));
            }
        };
        let storage = match storage() {
            Ok(storage) => storage,
            Err(response) => return response,
        };
        match relationships::repair(&storage, &request) {
            Ok(response) => ok_json(200, json!({"data": response})),
            Err(error) => map_lotar_error(error),
        }
    });
}
