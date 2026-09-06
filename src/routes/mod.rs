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
    attachment_service::AttachmentService, automation_service::AutomationService,
    config_service::ConfigService, project_service::ProjectService,
    reference_service::ReferenceService, scan_service::ScanService, sprint_service::SprintService,
    sync_service::SyncService, task_service::TaskService,
};
use crate::storage::sprint::{Sprint, SprintActual, SprintCapacity, SprintPlan};
use crate::workspace::TasksDirectoryResolver;
use crate::{
    api_types::{
        ScanRequest, SprintAssignmentRequest, SprintAssignmentResponse, SprintBacklogItem,
        SprintBacklogResponse, SprintCleanupMetric, SprintCleanupSummary, SprintCreateRequest,
        SprintCreateResponse, SprintDeleteRequest, SprintDeleteResponse,
        SprintIntegrityDiagnostics, SprintListItem, SprintListResponse, SprintUpdateRequest,
        SprintUpdateResponse, SyncRequest, SyncValidateRequest,
    },
    types::TaskStatus,
};
use chrono::Utc;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

mod activity;
mod agents;
mod attachments;
mod automation;
mod config;
mod jobs;
mod projects;
mod references;
mod scan;
mod sprints;
mod sync;
mod tasks;
mod whoami;

pub(super) fn make_cleanup_summary(
    outcome: &sprint_integrity::SprintCleanupOutcome,
) -> SprintCleanupSummary {
    SprintCleanupSummary {
        removed_references: outcome.removed_references,
        updated_tasks: outcome.updated_tasks,
        removed_by_sprint: outcome
            .removed_by_sprint
            .iter()
            .map(|metric| SprintCleanupMetric {
                sprint_id: metric.sprint_id,
                count: metric.count,
            })
            .collect(),
        remaining_missing: outcome.remaining_missing.clone(),
    }
}

pub(super) fn make_integrity_payload(
    baseline: &sprint_integrity::MissingSprintReport,
    current: &sprint_integrity::MissingSprintReport,
    cleanup: Option<&sprint_integrity::SprintCleanupOutcome>,
) -> Option<SprintIntegrityDiagnostics> {
    if baseline.missing_sprints.is_empty() && cleanup.is_none() {
        return None;
    }

    Some(SprintIntegrityDiagnostics {
        missing_sprints: current.missing_sprints.clone(),
        tasks_with_missing: if baseline.tasks_with_missing > 0 {
            Some(baseline.tasks_with_missing)
        } else {
            None
        },
        auto_cleanup: cleanup.map(make_cleanup_summary),
    })
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

pub(super) fn apply_update_to_sprint(target: &mut Sprint, payload: &SprintUpdateRequest) {
    if let Some(label) = payload.label.clone() {
        let plan = target.plan.get_or_insert_with(SprintPlan::default);
        plan.label = clean_opt_string(Some(label));
    }
    if let Some(goal) = payload.goal.clone() {
        let plan = target.plan.get_or_insert_with(SprintPlan::default);
        plan.goal = clean_opt_string(Some(goal));
    }
    if let Some(length) = payload.plan_length.clone() {
        let plan = target.plan.get_or_insert_with(SprintPlan::default);
        plan.length = clean_opt_string(Some(length));
    }
    if let Some(ends_at) = payload.ends_at.clone() {
        let plan = target.plan.get_or_insert_with(SprintPlan::default);
        plan.ends_at = clean_opt_string(Some(ends_at));
    }
    if let Some(starts_at) = payload.starts_at.clone() {
        let plan = target.plan.get_or_insert_with(SprintPlan::default);
        plan.starts_at = clean_opt_string(Some(starts_at));
    }
    if let Some(overdue_after) = payload.overdue_after.clone() {
        let plan = target.plan.get_or_insert_with(SprintPlan::default);
        plan.overdue_after = clean_opt_string(Some(overdue_after));
    }
    if let Some(notes) = payload.notes.clone() {
        let plan = target.plan.get_or_insert_with(SprintPlan::default);
        plan.notes = clean_opt_string(Some(notes));
    }
    if let Some(capacity_points) = payload.capacity_points {
        match capacity_points {
            Some(value) => {
                let plan = target.plan.get_or_insert_with(SprintPlan::default);
                plan.capacity
                    .get_or_insert_with(SprintCapacity::default)
                    .points = Some(value);
            }
            None => {
                if let Some(plan) = target.plan.as_mut()
                    && let Some(capacity) = plan.capacity.as_mut()
                {
                    capacity.points = None;
                    if capacity.points.is_none() && capacity.hours.is_none() {
                        plan.capacity = None;
                    }
                }
            }
        }
    }
    if let Some(capacity_hours) = payload.capacity_hours {
        match capacity_hours {
            Some(value) => {
                let plan = target.plan.get_or_insert_with(SprintPlan::default);
                plan.capacity
                    .get_or_insert_with(SprintCapacity::default)
                    .hours = Some(value);
            }
            None => {
                if let Some(plan) = target.plan.as_mut()
                    && let Some(capacity) = plan.capacity.as_mut()
                {
                    capacity.hours = None;
                    if capacity.points.is_none() && capacity.hours.is_none() {
                        plan.capacity = None;
                    }
                }
            }
        }
    }

    if let Some(actual_started_at) = payload.actual_started_at.clone() {
        match actual_started_at {
            Some(value) => {
                let actual = target.actual.get_or_insert_with(SprintActual::default);
                actual.started_at = clean_opt_string(Some(value));
            }
            None => {
                if let Some(actual) = target.actual.as_mut() {
                    actual.started_at = None;
                }
            }
        }
    }

    if let Some(actual_closed_at) = payload.actual_closed_at.clone() {
        match actual_closed_at {
            Some(value) => {
                let actual = target.actual.get_or_insert_with(SprintActual::default);
                actual.closed_at = clean_opt_string(Some(value));
            }
            None => {
                if let Some(actual) = target.actual.as_mut() {
                    actual.closed_at = None;
                }
            }
        }
    }

    let should_clear_actual = matches!(
        target.actual.as_ref(),
        Some(actual) if actual.started_at.is_none() && actual.closed_at.is_none()
    );
    if should_clear_actual {
        target.actual = None;
    }

    if let Some(plan) = target.plan.as_ref()
        && !plan_has_values(plan)
    {
        target.plan = None;
    }
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
    "limit",
    "offset",
    "page_size",
    "per_page",
    "due",
    "recent",
    "needs",
];

/// Parse a REST task query into a `TaskListFilter` plus a map of leftover
/// unknown keys (custom fields not declared in config) for in-memory matching.
///
/// Shared by the list and export endpoints so both support the full grammar:
/// CSV enum lists, `q`, `tags`, `sprints`, custom fields (declared or
/// `field:`-prefixed), and `assignee` including `@me` and `__none__`.
pub(super) fn parse_task_query(
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

    let mut statuses = Vec::new();
    for s in parse_list("status") {
        statuses.push(crate::types::TaskStatus::parse_with_config(&s, cfg)?);
    }
    let mut priorities = Vec::new();
    for s in parse_list("priority") {
        priorities.push(crate::types::Priority::parse_with_config(&s, cfg)?);
    }
    let mut types_vec = Vec::new();
    for s in parse_list("type") {
        types_vec.push(crate::types::TaskType::parse_with_config(&s, cfg)?);
    }

    let mut filter = crate::api_types::TaskListFilter {
        status: statuses,
        priority: priorities,
        task_type: types_vec,
        project: query.get("project").cloned(),
        tags: parse_list("tags"),
        text_query: query.get("q").cloned(),
        sprints: query
            .get("sprints")
            .map(|s| {
                s.split(',')
                    .filter_map(|p| p.trim().parse::<u32>().ok())
                    .collect()
            })
            .unwrap_or_default(),
        custom_fields: BTreeMap::new(),
        assignee: Vec::new(),
        assignee_none: false,
    };

    // Assignee: @me resolves to the current identity, __none__ means unassigned.
    // An unresolvable @me is an explicit error (fail closed) rather than a
    // silently broadened or empty result.
    if let Some(a) = query.get("assignee") {
        if a == "__none__" {
            filter.assignee_none = true;
        } else if !a.trim().is_empty() {
            let resolved = if a.trim().eq_ignore_ascii_case("@me") {
                crate::utils::identity::resolve_current_user(Some(tasks_root)).ok_or_else(|| {
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

/// Apply leftover unknown-key filters (fuzzy set matching) in memory.
pub(super) fn apply_unknown_key_filters(
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

#[allow(clippy::needless_pass_by_value)]
pub(super) fn bad_request(msg: String) -> HttpResponse {
    ok_json(
        400,
        json!({"error": {"code": "INVALID_ARGUMENT", "message": msg}}),
    )
}

pub(super) fn internal(v: serde_json::Value) -> HttpResponse {
    ok_json(500, v)
}

#[allow(clippy::needless_pass_by_value)]
pub(super) fn not_found(msg: String) -> HttpResponse {
    ok_json(404, json!({"error": {"code": "NOT_FOUND", "message": msg}}))
}

#[allow(clippy::needless_pass_by_value)]
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

#[allow(clippy::needless_pass_by_value)]
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
    attachments::register(api_server);
    config::register(api_server);
    automation::register(api_server);
    agents::register(api_server);
    scan::register(api_server);
    sync::register(api_server);
    projects::register(api_server);
    activity::register(api_server);
}
