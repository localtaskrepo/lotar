use crate::LoTaRError;
use crate::api_server::{ApiServer, HttpRequest, HttpResponse};
use crate::config::manager::ConfigManager;
use crate::config::resolution;
use crate::services::automation_service::AutomationEvent;
use crate::services::sprint_assignment;
use crate::services::sprint_integrity;
use crate::services::sprint_metrics::SprintBurndownMetric;
use crate::services::sprint_reports::{compute_sprint_burndown, compute_sprint_summary};
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
use crate::storage::sprint::{Sprint, SprintCapacity, SprintPlan};
use crate::workspace::TasksDirectoryResolver;
use crate::{
    api_types::{
        ScanRequest, SprintAssignmentRequest, SprintAssignmentResponse, SprintBacklogItem,
        SprintBacklogResponse, SprintCreateRequest, SprintCreateResponse, SprintDeleteRequest,
        SprintDeleteResponse, SprintListItem, SprintListResponse, SprintUpdateRequest,
        SprintUpdateResponse, SyncRequest, SyncValidateRequest,
    },
    types::TaskStatus,
};
use chrono::Utc;
use serde_json::json;
use std::collections::BTreeMap;

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

pub(super) fn sprint_record_to_list_item(
    record: &crate::services::sprint_service::SprintRecord,
    reference: chrono::DateTime<Utc>,
) -> SprintListItem {
    let lifecycle = crate::services::sprint_status::derive_status(&record.sprint, reference);
    let plan = record.sprint.plan.as_ref();
    let capacity = plan.and_then(|plan| plan.capacity.as_ref());
    SprintListItem {
        id: record.id,
        label: record
            .sprint
            .plan
            .as_ref()
            .and_then(|plan| plan.label.clone()),
        display_name: sprint_assignment::sprint_display_name(record),
        created: record.sprint.created.clone(),
        modified: record.sprint.modified.clone(),
        state: lifecycle.state.as_str().to_string(),
        planned_start: lifecycle.planned_start.map(|dt| dt.to_rfc3339()),
        planned_end: lifecycle.planned_end.map(|dt| dt.to_rfc3339()),
        actual_start: lifecycle.actual_start.map(|dt| dt.to_rfc3339()),
        actual_end: lifecycle.actual_end.map(|dt| dt.to_rfc3339()),
        computed_end: lifecycle.computed_end.map(|dt| dt.to_rfc3339()),
        goal: plan.and_then(|plan| plan.goal.clone()),
        plan_length: plan.and_then(|plan| plan.length.clone()),
        overdue_after: plan.and_then(|plan| plan.overdue_after.clone()),
        notes: plan.and_then(|plan| plan.notes.clone()),
        capacity_points: capacity.and_then(|capacity| capacity.points),
        capacity_hours: capacity.and_then(|capacity| capacity.hours),
        warnings: lifecycle
            .warnings
            .iter()
            .map(|warning| warning.message())
            .collect(),
    }
}

pub(super) fn sprint_from_create_request(payload: &SprintCreateRequest) -> Sprint {
    let mut plan = SprintPlan::default();

    if let Some(label) = clean_opt_string(payload.label.clone()) {
        plan.label = Some(label);
    }
    if let Some(goal) = clean_opt_string(payload.goal.clone()) {
        plan.goal = Some(goal);
    }
    if let Some(length) = clean_opt_string(payload.plan_length.clone()) {
        plan.length = Some(length);
    }
    if let Some(ends_at) = clean_opt_string(payload.ends_at.clone()) {
        plan.ends_at = Some(ends_at);
    }
    if let Some(starts_at) = clean_opt_string(payload.starts_at.clone()) {
        plan.starts_at = Some(starts_at);
    }
    if let Some(points) = payload.capacity_points {
        plan.capacity
            .get_or_insert_with(SprintCapacity::default)
            .points = Some(points);
    }
    if let Some(hours) = payload.capacity_hours {
        plan.capacity
            .get_or_insert_with(SprintCapacity::default)
            .hours = Some(hours);
    }
    if let Some(overdue_after) = clean_opt_string(payload.overdue_after.clone()) {
        plan.overdue_after = Some(overdue_after);
    }
    if let Some(notes) = payload
        .notes
        .clone()
        .filter(|value| !value.trim().is_empty())
    {
        plan.notes = Some(notes);
    }

    let mut sprint = Sprint::default();
    if plan_has_values(&plan) {
        sprint.plan = Some(plan);
    }
    sprint
}

pub(super) fn sprint_patch_from_request(
    payload: &SprintUpdateRequest,
) -> crate::services::sprint_service::SprintMetadataPatch {
    crate::services::sprint_service::SprintMetadataPatch::from_request(payload)
}

pub(super) fn plan_has_values(plan: &SprintPlan) -> bool {
    plan.label.is_some()
        || plan.goal.is_some()
        || plan.length.is_some()
        || plan.ends_at.is_some()
        || plan.starts_at.is_some()
        || plan.capacity.is_some()
        || plan.overdue_after.is_some()
        || plan.notes.is_some()
}

pub(super) fn clean_opt_string(input: Option<String>) -> Option<String> {
    input.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
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

/// Query keys with dedicated handling in the task list/export endpoints.
pub(super) const TASK_LIST_KNOWN_KEYS: &[&str] = &[
    "project",
    "status",
    "priority",
    "type",
    "tags",
    "sprints",
    "q",
    "assignee",
    "order",
    "sort_by",
    "limit",
    "offset",
    "page_size",
    "per_page",
    "due",
    "recent",
    "needs",
    // RESERVED lifecycle property key (DEV-92): strict active|deleted|all
    // grammar, never interpreted as a custom-field filter.
    "deletion",
];

/// Parse a REST task query into a `TaskListFilter` plus a map of leftover
/// unknown keys (custom fields not declared in config) for in-memory matching.
///
/// Shared by the list and export endpoints so both support the full grammar:
/// CSV enum lists, `q`, `tags`, `sprints`, custom fields (declared or
/// `field:`-prefixed), and `assignee` including `@me` and `__none__`.
pub(crate) fn parse_task_query(
    query: &std::collections::HashMap<String, String>,
    cfg: &crate::config::types::ResolvedConfig,
    tasks_root: &std::path::Path,
) -> Result<
    (
        crate::api_types::TaskListFilter,
        std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    ),
    String,
> {
    use std::collections::BTreeSet;

    let parse_list = |key: &str| -> Vec<String> {
        query
            .get(key)
            .map(|s| {
                s.split(',')
                    .map(|p| p.trim().to_string())
                    .filter(|p| !p.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    };

    // Enum validation is scoped to the explicit project's resolved config
    // when one is requested, so project-only enum values validate instead of
    // being rejected against the base config (DEV-57). Cross-project queries
    // (no `project` param) keep the base config.
    let project_cfg_owner;
    let enum_cfg = if let Some(project) = query
        .get("project")
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
    {
        project_cfg_owner =
            crate::config::resolution::config_for_project(tasks_root, Some(project))
                .unwrap_or_else(|_| cfg.clone());
        &project_cfg_owner
    } else {
        cfg
    };

    let mut statuses = Vec::new();
    for s in parse_list("status") {
        statuses.push(crate::types::TaskStatus::parse_with_config(&s, enum_cfg)?);
    }
    let mut priorities = Vec::new();
    for s in parse_list("priority") {
        priorities.push(crate::types::Priority::parse_with_config(&s, enum_cfg)?);
    }
    let mut types_vec = Vec::new();
    for s in parse_list("type") {
        types_vec.push(crate::types::TaskType::parse_with_config(&s, enum_cfg)?);
    }

    let mut filter = crate::api_types::TaskListFilter {
        status: statuses,
        priority: priorities,
        task_type: types_vec,
        project: query.get("project").cloned(),
        tags: parse_list("tags"),
        text_query: query.get("q").cloned(),
        sprints: parse_strict_sprints_csv(query.get("sprints"))?,
        custom_fields: BTreeMap::new(),
        assignee: Vec::new(),
        assignee_none: false,
        deletion: parse_deletion_query(query)?,
    };

    // Assignee: @me resolves to the current identity, __none__ means unassigned.
    // An unresolvable @me is an explicit error (fail closed) rather than a
    // silently broadened or empty result.
    if let Some(a) = query.get("assignee") {
        if a == "__none__" {
            filter.assignee_none = true;
        } else if !a.trim().is_empty() {
            let resolved = if a.trim().eq_ignore_ascii_case("@me") {
                crate::utils::identity::resolve_current_user_for_project(Some(tasks_root), filter.project.as_deref()).ok_or_else(|| {
                    "Could not resolve @me: no identity configured (default_reporter, git user.name, or USER env)"
                        .to_string()
                })?
            } else {
                a.trim().to_string()
            };
            // Normalize: strip @ prefix from regular names for consistent matching.
            let v = crate::utils::member::normalize_member_value(&resolved, |name| {
                cfg.agent_profiles.contains_key(name)
            });
            filter.assignee.push(v);
        }
    }

    // Unknown keys become either declared custom-field filters or in-memory keys
    let mut uf: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (k, v) in query.iter() {
        if TASK_LIST_KNOWN_KEYS.contains(&k.as_str()) {
            continue;
        }
        for part in v.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
            if let Some(name) = crate::utils::custom_fields::resolve_filter_name(k, cfg) {
                filter
                    .custom_fields
                    .entry(name)
                    .or_default()
                    .push(part.to_string());
            } else {
                uf.entry(k.clone()).or_default().insert(part.to_string());
            }
        }
    }

    Ok((filter, uf))
}

/// Strict `deletion` query parsing shared by list and export (DEV-92):
/// blank/absent means the default (`active`); anything other than exactly
/// `active`, `deleted`, or `all` is rejected. `deletion` is a RESERVED
/// property key, so it can never be repurposed as a custom-field filter.
pub(super) fn parse_deletion_query(
    query: &std::collections::HashMap<String, String>,
) -> Result<crate::api_types::DeletionFilter, String> {
    use crate::api_types::DeletionFilter;
    match query.get("deletion") {
        None => Ok(DeletionFilter::default()),
        // Blank means "no lifecycle filter" (default active), mirroring the
        // sprints grammar; every other value must match EXACTLY.
        Some(value) if value.trim().is_empty() => Ok(DeletionFilter::default()),
        Some(value) => match value.as_str() {
            "active" => Ok(DeletionFilter::Active),
            "deleted" => Ok(DeletionFilter::Deleted),
            "all" => Ok(DeletionFilter::All),
            other => Err(format!(
                "Invalid deletion filter: '{other}' (expected active, deleted, or all)"
            )),
        },
    }
}

/// Strict `sprints` CSV parsing shared by list and export: every entry of a
/// non-blank value must be a positive integer. Blank values mean "no sprint
/// filter"; invalid or empty entries are rejected (DEV-57).
pub(super) fn parse_strict_sprints_csv(raw: Option<&String>) -> Result<Vec<u32>, String> {
    let Some(value) = raw else {
        return Ok(Vec::new());
    };
    if value.trim().is_empty() {
        return Ok(Vec::new());
    }
    let mut ids = Vec::new();
    for part in value.split(',') {
        let part = part.trim();
        let id = part.parse::<u32>().map_err(|_| {
            format!("Invalid sprints entry: '{part}' (expected CSV of positive sprint ids)")
        })?;
        if id == 0 {
            return Err(
                "Invalid sprints entry: '0' (expected CSV of positive sprint ids)".to_string(),
            );
        }
        ids.push(id);
    }
    Ok(ids)
}

/// Apply leftover unknown-key filters (fuzzy set matching) in memory.
pub(crate) fn apply_unknown_key_filters(
    tasks: &mut Vec<(String, crate::api_types::TaskDTO)>,
    uf: &std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    cfg: &crate::config::types::ResolvedConfig,
) {
    if uf.is_empty() {
        return;
    }

    tasks.retain(|(id, t)| {
        for (fk, allowed) in uf {
            let vals = match crate::utils::custom_fields::resolve_task_filter_values(id, t, fk, cfg)
            {
                Some(vs) => vs.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>(),
                None => return false,
            };
            if vals.is_empty() {
                return false;
            }
            let allowed_vec: Vec<String> = allowed.iter().cloned().collect();
            if !crate::utils::fuzzy_match::fuzzy_set_match(&vals, &allowed_vec) {
                return false;
            }
        }
        true
    });
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
