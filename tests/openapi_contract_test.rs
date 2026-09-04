//! Contract test: the REST surface in `docs/openapi.json` must match the
//! routes actually registered by the server.
//!
//! Two directions are checked:
//! 1. Every documented operation must exist in the server (docs must not lie).
//! 2. Every registered API route must be documented, unless it is explicitly
//!    allowlisted below with a reason.

use lotar::api_server::ApiServer;
use lotar::routes;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;

/// Server-only routes that are intentionally absent from the public API doc.
const UNDOCUMENTED_BY_DESIGN: &[&str] = &[];

/// Routes served outside the ApiServer handler map (SSE streams handled in
/// the connection loop, the spec document itself) that are documented and
/// therefore legitimate parts of the surface.
const SERVED_OUTSIDE_HANDLER_MAP: &[&str] = &[
    "GET /api/events",
    "GET /api/tasks/stream",
    "GET /api/openapi.json",
];

fn server_routes() -> BTreeSet<String> {
    let mut server = ApiServer::new();
    routes::initialize(&mut server);
    server
        .registered_routes()
        .into_iter()
        .map(|(method, path)| format!("{} {}", method, path))
        .collect()
}

fn documented_routes() -> BTreeSet<String> {
    let spec_raw = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/docs/openapi.json"))
        .expect("docs/openapi.json must be readable");
    let spec: Value = serde_json::from_str(&spec_raw).expect("openapi.json must be valid JSON");

    let mut documented = BTreeSet::new();
    let Some(paths) = spec.get("paths").and_then(Value::as_object) else {
        panic!("openapi.json has no paths object");
    };
    for (path, ops) in paths {
        let Some(ops) = ops.as_object() else {
            continue;
        };
        for (method, op) in ops {
            let method = method.to_ascii_uppercase();
            if !matches!(method.as_str(), "GET" | "POST" | "PUT" | "PATCH" | "DELETE") {
                continue;
            }
            if op.as_object().is_none() {
                continue;
            }
            documented.insert(format!("{} {}", method, path));
        }
    }
    documented
}

#[test]
fn every_documented_operation_exists_on_the_server() {
    let mut server = server_routes();
    for entry in SERVED_OUTSIDE_HANDLER_MAP {
        server.insert(entry.to_string());
    }
    let documented = documented_routes();

    let missing: Vec<String> = documented.difference(&server).cloned().collect();
    assert!(
        missing.is_empty(),
        "docs/openapi.json documents operations the server does not register (update src/routes.rs or the spec):\n{}",
        missing.join("\n")
    );
}

#[test]
fn every_server_api_route_is_documented() {
    let server = server_routes();
    let documented = documented_routes();
    let allowlist: BTreeSet<&str> = UNDOCUMENTED_BY_DESIGN.iter().copied().collect();

    let undocumented: Vec<String> = server
        .iter()
        .filter(|entry| entry.starts_with("/api") || entry.contains(" /api"))
        .filter(|entry| !documented.contains(*entry))
        .filter(|entry| !allowlist.contains(entry.as_str()))
        .cloned()
        .collect();
    assert!(
        undocumented.is_empty(),
        "server registers API routes missing from docs/openapi.json (document them or add to UNDOCUMENTED_BY_DESIGN with a reason):\n{}",
        undocumented.join("\n")
    );
}
