//! `/api/releases/*` (DEV-393), the REST side of `lotar release`.

use super::*;
use crate::api_types::{
    ReleaseActionRequest, ReleaseAssignmentRequest, ReleaseAssignmentResponse, ReleaseBacklogResponse, ReleaseCreateRequest, ReleaseDeleteResponse,
    ReleaseGetResponse, ReleaseListResponse, ReleaseMutationResponse, ReleaseUpdateRequest,
};
use crate::services::release_service::{ReleaseAssignmentAction, ReleasePatch, ReleaseRecord, ReleaseService};
use crate::storage::manager::Storage;
use crate::storage::release::Release;

pub(super) fn register(api_server: &mut ApiServer) {
    api_server.register_handler("GET", "/api/releases/list", |_req: &HttpRequest| {
        let resolver = try_response!(resolve_tasks_dir());
        let storage = Storage { root_path: resolver.path };
        match ReleaseService::list_items(&storage) {
            Ok(releases) => ok_json(200, json!({"data": ReleaseListResponse { status: "ok".to_string(), total: releases.len(), releases }})),
            Err(error) => map_lotar_error(error),
        }
    });

    api_server.register_handler("GET", "/api/releases/get", |req: &HttpRequest| {
        let resolver = try_response!(resolve_tasks_dir());
        let Some(reference) = req.query.get("release").or_else(|| req.query.get("id")) else {
            return bad_request("Missing release parameter".to_string());
        };
        let storage = Storage { root_path: resolver.path };
        let record = match ReleaseService::resolve(&storage.root_path, reference) {
            Ok(record) => record,
            Err(error) => return map_lotar_error(error),
        };
        let (release, tasks) = ReleaseService::detail(&storage, &record);
        let payload = ReleaseGetResponse { status: "ok".to_string(), release, tasks, history: record.release.history.clone() };
        ok_json(200, json!({"data": payload}))
    });

    api_server.register_handler("GET", "/api/releases/backlog", |req: &HttpRequest| {
        let resolver = try_response!(resolve_tasks_dir());
        let page = match crate::utils::pagination::parse_page(&req.query, 50, 500) {
            Ok(page) => page,
            Err(message) => return bad_request(message),
        };
        // The /api/tasks/list parameters (including `query`) narrow it.
        let mut params = req.query.clone();
        for transport in ["limit", "offset"] {
            params.remove(transport);
        }
        let input = match crate::services::task_list::TaskQueryInput::from_params(&params) {
            Ok(input) => input,
            Err(error) => return bad_request(error.to_string()),
        };
        let config = try_response!(load_merged_config(&resolver));
        let storage = Storage { root_path: resolver.path.clone() };
        let tasks = match ReleaseService::backlog(&storage, input, &config) {
            Ok(tasks) => tasks,
            Err(error) => return bad_request(error.to_string()),
        };
        let total = tasks.len();
        let (start, end) = crate::utils::pagination::slice_bounds(total, page.offset, page.limit);
        let tasks: Vec<_> = tasks[start..end].iter().map(|(_, task)| task.clone()).collect();
        let payload = ReleaseBacklogResponse { status: "ok".to_string(), total, count: tasks.len(), limit: page.limit, offset: page.offset, tasks };
        ok_json(200, json!({"data": payload}))
    });

    api_server.register_handler("POST", "/api/releases/create", |req: &HttpRequest| {
        let resolver = try_response!(resolve_tasks_dir());
        let body: ReleaseCreateRequest = try_response!(parse_body(req));
        let release = Release {
            version: body.version,
            label: body.label,
            notes: body.notes,
            planned_date: body.planned_date,
            released_at: body.released_at,
            ..Release::default()
        };
        match ReleaseService::create(&resolver.path, release) {
            Ok(record) => mutation_response(&resolver.path, &record, true),
            Err(error) => map_lotar_error(error),
        }
    });

    api_server.register_handler("POST", "/api/releases/update", |req: &HttpRequest| {
        let resolver = try_response!(resolve_tasks_dir());
        let body: ReleaseUpdateRequest = try_response!(parse_body(req));
        let patch = ReleasePatch::from_text(body.version, body.label, body.notes, body.planned_date, body.released_at);
        if patch.is_empty() {
            return bad_request("Nothing to update: give version, label, notes, planned_date or released_at".to_string());
        }
        let result =
            ReleaseService::resolve(&resolver.path, &body.release.as_reference()).and_then(|record| ReleaseService::update(&resolver.path, record.id, &patch));
        match result {
            Ok((record, changed)) => mutation_response(&resolver.path, &record, changed),
            Err(error) => map_lotar_error(error),
        }
    });

    api_server.register_handler("POST", "/api/releases/release", |req: &HttpRequest| {
        let resolver = try_response!(resolve_tasks_dir());
        let body: ReleaseActionRequest = try_response!(parse_body(req));
        let result = ReleaseService::resolve(&resolver.path, &body.release.as_reference())
            .and_then(|record| ReleaseService::mark_released(&resolver.path, record.id, body.at.clone()));
        match result {
            Ok((record, changed)) => mutation_response(&resolver.path, &record, changed),
            Err(error) => map_lotar_error(error),
        }
    });

    api_server.register_handler("POST", "/api/releases/reopen", |req: &HttpRequest| {
        let resolver = try_response!(resolve_tasks_dir());
        let body: ReleaseActionRequest = try_response!(parse_body(req));
        let result = ReleaseService::resolve(&resolver.path, &body.release.as_reference()).and_then(|record| ReleaseService::reopen(&resolver.path, record.id));
        match result {
            Ok((record, changed)) => mutation_response(&resolver.path, &record, changed),
            Err(error) => map_lotar_error(error),
        }
    });

    api_server.register_handler("POST", "/api/releases/delete", |req: &HttpRequest| {
        let resolver = try_response!(resolve_tasks_dir());
        let body: ReleaseActionRequest = try_response!(parse_body(req));
        let mut storage = Storage::new(&resolver.path);
        let result = ReleaseService::resolve(&resolver.path, &body.release.as_reference())
            .and_then(|record| ReleaseService::delete(&mut storage, record.id).map(|removed| (record.id, removed)));
        match result {
            Ok((release_id, removed_from)) => {
                ok_json(200, json!({"data": ReleaseDeleteResponse { status: "ok".to_string(), deleted: true, release_id, removed_from }}))
            }
            Err(error) => map_lotar_error(error),
        }
    });

    for (path, action) in [
        ("/api/releases/add", ReleaseAssignmentAction::Add),
        ("/api/releases/move", ReleaseAssignmentAction::Move),
        ("/api/releases/remove", ReleaseAssignmentAction::Remove),
    ] {
        api_server.register_handler("POST", path, move |req: &HttpRequest| assign(req, action));
    }
}

fn assign(req: &HttpRequest, action: ReleaseAssignmentAction) -> HttpResponse {
    let resolver = try_response!(resolve_tasks_dir());
    let body: ReleaseAssignmentRequest = try_response!(parse_body(req));
    let config = try_response!(load_merged_config(&resolver));
    let mut storage = Storage::new(&resolver.path);
    let tasks = match ReleaseService::selected_tasks(&storage, &config, &body.tasks, body.query.as_deref()) {
        Ok(tasks) => tasks,
        Err(error) => return bad_request(error.to_string()),
    };
    match ReleaseService::assign(&mut storage, &body.release.as_reference(), &tasks, action) {
        Ok(outcome) => {
            let payload = ReleaseAssignmentResponse {
                status: "ok".to_string(),
                action: outcome.action.as_str().to_string(),
                release_id: outcome.record.id,
                release_version: outcome.record.release.version.clone(),
                modified: outcome.modified,
                unchanged: outcome.unchanged,
            };
            ok_json(200, json!({"data": payload}))
        }
        Err(message) if message.starts_with("Release not found") => not_found(message),
        Err(message) => bad_request(message),
    }
}

fn mutation_response(tasks_root: &std::path::Path, record: &ReleaseRecord, changed: bool) -> HttpResponse {
    let storage = Storage { root_path: tasks_root.to_path_buf() };
    let (release, _) = ReleaseService::detail(&storage, record);
    ok_json(200, json!({"data": ReleaseMutationResponse { status: "ok".to_string(), release, changed }}))
}
