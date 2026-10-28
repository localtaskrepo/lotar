use crate::LoTaRError;
use crate::api_server::{ApiServer, HttpRequest, HttpResponse};
use crate::api_types::{
    ScanRequest, SprintAssignmentRequest, SprintAssignmentResponse, SprintBacklogItem,
    SprintBacklogResponse, SprintCreateRequest, SprintCreateResponse, SprintDeleteRequest,
    SprintDeleteResponse, SprintListItem, SprintListResponse, SprintUpdateRequest,
    SprintUpdateResponse, SyncRequest, SyncValidateRequest,
};
use crate::config::manager::ConfigManager;
use crate::config::resolution;
use crate::services::automation_service::AutomationEvent;
use crate::services::sprint_assignment;
use crate::services::sprint_integrity;
use crate::services::sprint_metrics::SprintBurndownMetric;
use crate::services::sprint_reports::{compute_sprint_burndown, compute_sprint_summary};
pub(super) use crate::services::sprint_service::{
    sprint_from_create_request, sprint_record_to_list_item,
};
use crate::services::sprint_velocity::{
    DEFAULT_VELOCITY_WINDOW, VelocityComputation, VelocityOptions, compute_velocity,
};
use crate::services::{
    attachment_service::AttachmentService,
    automation_service::AutomationService,
    config_service::ConfigService,
    project_service::ProjectService,
    reference_service::ReferenceService,
    scan_service::ScanService,
    sprint_service::SprintService,
    sync_service::{SyncDirection, SyncOptions, SyncService},
    task_service::TaskService,
};
use crate::workspace::TasksDirectoryResolver;
use chrono::Utc;
use serde_json::json;

/// Unwrap a `Result<T, HttpResponse>` inside a route handler, returning the
/// error response early (handlers return `HttpResponse`, so `?` is not
/// available).
macro_rules! try_response {
    ($result:expr) => {
        match $result {
            Ok(value) => value,
            Err(response) => return response,
        }
    };
}

mod activity;
mod agents;
mod attachments;
mod automation;
mod branding;
mod config;
mod jobs;
mod projects;
mod references;
mod relationships;
mod scan;
mod sprints;
mod sync;
mod tasks;
mod whoami;

/// Run the shared sprint integrity check for a request (DEV-173).
pub(super) fn sprint_integrity_check(
    storage: &mut crate::storage::manager::Storage,
    records: &mut Vec<crate::services::sprint_service::SprintRecord>,
    cleanup: bool,
    target: Option<u32>,
) -> Result<sprint_integrity::IntegrityCheck, HttpResponse> {
    sprint_integrity::check_missing_sprints(storage, records, cleanup, target)
        .map_err(|err| internal_error(format!("Failed to clean up sprint references: {err}")))
}

pub(super) fn sprint_patch_from_request(
    payload: &SprintUpdateRequest,
) -> crate::services::sprint_service::SprintMetadataPatch {
    crate::services::sprint_service::SprintMetadataPatch::from_request(payload)
}

pub(super) fn ok_json(status: u16, v: serde_json::Value) -> HttpResponse {
    json_response(status, v).unwrap_or_else(json_serialize_error)
}

/// Resolve the Git repository root and the tasks directory relative to it,
/// for endpoints that read task history from Git. Symlinked paths resolve
/// the same way as in the CLI (see `repo_relative_path`).
pub(super) fn resolve_repo_tasks_paths()
-> Result<(std::path::PathBuf, std::path::PathBuf), HttpResponse> {
    let resolver = TasksDirectoryResolver::resolve(None, None).map_err(internal_error)?;
    let cwd = std::env::current_dir().unwrap_or_else(|_| resolver.path.clone());
    let repo_root = crate::utils::git::find_repo_root(&cwd)
        .ok_or_else(|| bad_request("Not inside a git repository".into()))?;
    let tasks_rel = crate::utils::git::repo_relative_path(&repo_root, &resolver.path)
        .ok_or_else(|| bad_request("Tasks directory not inside repository".into()))?;
    Ok((repo_root, tasks_rel))
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "callers pass owned temporaries (format!/json!/errors), so taking ownership keeps call sites simple"
)]
pub(super) fn bad_request(msg: String) -> HttpResponse {
    ok_json(
        400,
        json!({"error": {"code": "INVALID_ARGUMENT", "message": msg}}),
    )
}

pub(super) fn internal(v: serde_json::Value) -> HttpResponse {
    ok_json(500, v)
}

/// 500 response in the shared `{"error": {"code": "INTERNAL", ...}}` envelope.
pub(super) fn internal_error(message: impl std::fmt::Display) -> HttpResponse {
    internal(json!({"error": {"code": "INTERNAL", "message": message.to_string()}}))
}

/// Resolve the tasks directory for a request, or the 500 response to return.
pub(super) fn resolve_tasks_dir() -> Result<TasksDirectoryResolver, HttpResponse> {
    TasksDirectoryResolver::resolve(None, None).map_err(internal_error)
}

/// Deserialize a JSON request body, or the 400 response to return.
pub(super) fn parse_body<T: serde::de::DeserializeOwned>(
    req: &HttpRequest,
) -> Result<T, HttpResponse> {
    serde_json::from_slice(&req.body).map_err(|e| bad_request(format!("Invalid body: {e}")))
}

/// Like [`parse_body`], but an empty body counts as `{}` for requests whose
/// fields are all optional.
pub(super) fn parse_body_or_empty<T: serde::de::DeserializeOwned>(
    req: &HttpRequest,
) -> Result<T, HttpResponse> {
    if req.body.iter().all(u8::is_ascii_whitespace) {
        return serde_json::from_value(json!({}))
            .map_err(|e| bad_request(format!("Invalid body: {e}")));
    }
    parse_body(req)
}

/// Response for a configuration or automation write: broadcasts
/// `config_updated` only for genuine writes (DEV-102; no-ops keep the stored
/// bytes) and reports the validation outcome.
pub(super) fn config_write_response(
    resolver: &TasksDirectoryResolver,
    updated: bool,
    validation: &crate::config::validation::errors::ValidationResult,
) -> HttpResponse {
    if updated {
        let actor = crate::utils::identity::resolve_current_user(Some(resolver.path.as_path()));
        crate::api_events::emit_config_updated(actor.as_deref());
    }
    let strings = |items: &[crate::config::validation::errors::ValidationError]| {
        items.iter().map(ToString::to_string).collect::<Vec<_>>()
    };
    ok_json(
        200,
        json!({
            "data": {
                "updated": updated,
                "warnings": strings(&validation.warnings),
                "info": strings(&validation.info),
                "errors": strings(&validation.errors),
            }
        }),
    )
}

/// Load the merged configuration for the tasks directory, or the 500
/// response to return (configuration failures are server-side, DEV-58).
pub(super) fn load_merged_config(
    resolver: &TasksDirectoryResolver,
) -> Result<crate::config::types::ResolvedConfig, HttpResponse> {
    resolution::load_and_merge_configs(Some(resolver.path.as_path()))
        .map_err(|e| internal_error(format!("Failed to load config: {e}")))
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "callers pass owned temporaries (format!/json!/errors), so taking ownership keeps call sites simple"
)]
pub(super) fn not_found(msg: String) -> HttpResponse {
    ok_json(404, json!({"error": {"code": "NOT_FOUND", "message": msg}}))
}

/// Map a typed service error onto the canonical REST error envelope
/// (DEV-58). Classification is exhaustive over [`LoTaRError`] and never
/// inspects message text: validation and identifier-syntax faults are
/// 400 INVALID_ARGUMENT; missing task/sprint/project resources are
/// 404 NOT_FOUND; serialization, index, and genuine IO faults are
/// 500 INTERNAL. Two IO-shaped conditions stay client errors because
/// they are coordination states the caller can resolve and retry: lock
/// contention (io `WouldBlock`) and a pending transaction journal
/// (typed marker, see [`crate::storage::safety`]); both keep their
/// exact diagnostics.
pub(super) fn map_lotar_error(err: LoTaRError) -> HttpResponse {
    match err {
        // Validation diagnostics read best without the enum prefix; the
        // InvalidTaskId display keeps its "Invalid task ID: " qualifier.
        LoTaRError::ValidationError(msg) => bad_request(msg),
        err @ LoTaRError::InvalidTaskId(_) => bad_request(err.to_string()),
        err @ (LoTaRError::TaskNotFound(_)
        | LoTaRError::SprintNotFound(_)
        | LoTaRError::ProjectNotFound(_)) => not_found(err.to_string()),
        err @ (LoTaRError::SerializationError(_) | LoTaRError::IndexError(_)) => {
            internal_error(err.to_string())
        }
        LoTaRError::IoError(io_err) => {
            let coordination_refusal = io_err.kind() == std::io::ErrorKind::WouldBlock
                || io_err.get_ref().is_some_and(|inner| {
                    inner
                        .downcast_ref::<crate::storage::safety::PendingJournalRefusal>()
                        .is_some()
                });
            let message = format!("IO error: {io_err}");
            if coordination_refusal {
                bad_request(message)
            } else {
                internal_error(message)
            }
        }
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "callers pass owned temporaries (format!/json!/errors), so taking ownership keeps call sites simple"
)]
pub(super) fn json_response(
    status: u16,
    v: serde_json::Value,
) -> Result<HttpResponse, serde_json::Error> {
    let body = serde_json::to_vec(&v)?;
    Ok(HttpResponse {
        status,
        headers: vec![("Content-Type".into(), "application/json".into())],
        body,
    })
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "callers pass owned temporaries (format!/json!/errors), so taking ownership keeps call sites simple"
)]
pub(super) fn json_serialize_error(e: serde_json::Error) -> HttpResponse {
    let fallback = json!({"error": {"code": "SERIALIZE", "message": e.to_string()}});
    HttpResponse {
        status: 500,
        headers: vec![("Content-Type".into(), "application/json".into())],
        body: serde_json::to_vec(&fallback).unwrap_or_else(|_| b"{}".to_vec()),
    }
}

pub fn initialize(api_server: &mut ApiServer) {
    whoami::register(api_server);
    jobs::register(api_server);
    tasks::register(api_server);
    sprints::register(api_server);
    references::register(api_server);
    relationships::register(api_server);
    attachments::register(api_server);
    branding::register(api_server);
    config::register(api_server);
    automation::register(api_server);
    agents::register(api_server);
    scan::register(api_server);
    sync::register(api_server);
    projects::register(api_server);
    activity::register(api_server);
}

#[cfg(test)]
mod dev58_error_mapping_cases {
    use super::*;

    fn assert_envelope(resp: &HttpResponse, status: u16, code: &str, message: &str) {
        assert_eq!(resp.status, status, "status for {message:?}");
        let body: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
        assert_eq!(body["error"]["code"].as_str(), Some(code), "{body}");
        let actual = body["error"]["message"].as_str().unwrap_or_default();
        assert!(
            actual.contains(message),
            "message {actual:?} must contain {message:?}"
        );
    }

    /// Exhaustive mapper coverage: every `LoTaRError` variant plus the two
    /// IO-shaped coordination refusals pins its HTTP status, error code,
    /// and message contract.
    #[test]
    fn map_lotar_error_is_exhaustive_and_typed() {
        assert_envelope(
            &map_lotar_error(LoTaRError::ValidationError("bad enum".into())),
            400,
            "INVALID_ARGUMENT",
            "bad enum",
        );
        assert_envelope(
            &map_lotar_error(LoTaRError::InvalidTaskId("TP-x: nope".into())),
            400,
            "INVALID_ARGUMENT",
            "Invalid task ID: TP-x: nope",
        );
        assert_envelope(
            &map_lotar_error(LoTaRError::TaskNotFound("TP-9".into())),
            404,
            "NOT_FOUND",
            "Task not found: TP-9",
        );
        assert_envelope(
            &map_lotar_error(LoTaRError::SprintNotFound(42)),
            404,
            "NOT_FOUND",
            "Sprint not found: 42",
        );
        assert_envelope(
            &map_lotar_error(LoTaRError::ProjectNotFound("ORPH".into())),
            404,
            "NOT_FOUND",
            "Project not found: ORPH",
        );
        assert_envelope(
            &map_lotar_error(LoTaRError::SerializationError("bad yaml".into())),
            500,
            "INTERNAL",
            "Serialization error: bad yaml",
        );
        assert_envelope(
            &map_lotar_error(LoTaRError::IndexError("stale".into())),
            500,
            "INTERNAL",
            "Index error: stale",
        );
        assert_envelope(
            &map_lotar_error(LoTaRError::IoError(std::io::Error::other("disk on fire"))),
            500,
            "INTERNAL",
            "IO error: disk on fire",
        );
        // Lock contention (io WouldBlock) stays a client-side retryable
        // refusal with its production diagnostics preserved.
        assert_envelope(
            &map_lotar_error(LoTaRError::IoError(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "Storage lock /w/TP/.task.lock is still busy after 2 seconds; retry after the other writer finishes; mutation was not run",
            ))),
            400,
            "INVALID_ARGUMENT",
            "still busy after 2 seconds",
        );
        // Pending-journal refusal carries the typed marker and stays 400.
        assert_envelope(
            &map_lotar_error(LoTaRError::IoError(std::io::Error::other(
                crate::storage::safety::PendingJournalRefusal(
                    "A pending task/sprint transaction journal exists at /w/.txn-pending.json"
                        .into(),
                ),
            ))),
            400,
            "INVALID_ARGUMENT",
            "pending task/sprint transaction journal",
        );
    }
}
