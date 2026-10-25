use super::*;

pub(super) fn register(api_server: &mut ApiServer) {
    api_server.register_handler("POST", "/api/jobs", |req: &HttpRequest| {
        let resolver = try_response!(resolve_tasks_dir());
        let payload: crate::api_types::AgentJobCreateRequest = try_response!(parse_body(req));

        match crate::services::agent_job_service::AgentJobService::start_job(payload, &resolver) {
            Ok(job) => ok_json(201, json!({"data": {"job": job}})),
            Err(err) => bad_request(err.to_string()),
        }
    });

    // GET /api/jobs -> list agent jobs

    api_server.register_handler("GET", "/api/jobs", |req: &HttpRequest| {
        let mut jobs = crate::services::agent_job_service::AgentJobService::list_jobs();
        if let Some(ticket_id) = req.query.get("ticket_id") {
            jobs.retain(|job| job.ticket_id == *ticket_id);
        }
        if let Some(status) = req.query.get("status") {
            jobs.retain(|job| job.status.eq_ignore_ascii_case(status));
        }
        let queue_stats = crate::services::agent_job_service::AgentJobService::queue_stats();
        ok_json(
            200,
            json!({"data": {"jobs": jobs, "queue_stats": queue_stats}}),
        )
    });

    // GET /api/jobs/get -> fetch agent job status

    api_server.register_handler("GET", "/api/jobs/get", |req: &HttpRequest| {
        let id = match req.query.get("id") {
            Some(v) if !v.trim().is_empty() => v.clone(),
            _ => return bad_request("Missing job id".into()),
        };
        match crate::services::agent_job_service::AgentJobService::get_job(&id) {
            Some(job) => ok_json(200, json!({"data": {"job": job}})),
            None => not_found(format!("Job '{}' not found", id)),
        }
    });

    // GET /api/jobs/logs -> fetch agent job logs

    api_server.register_handler("GET", "/api/jobs/logs", |req: &HttpRequest| {
        let id = match req.query.get("id") {
            Some(v) if !v.trim().is_empty() => v.clone(),
            _ => return bad_request("Missing job id".into()),
        };
        let Some(job) = crate::services::agent_job_service::AgentJobService::get_job(&id) else {
            return not_found(format!("Job '{}' not found", id));
        };
        let events = crate::services::agent_job_service::AgentJobService::events_for(&id);
        ok_json(200, json!({"data": {"job": job, "events": events}}))
    });

    // POST /api/jobs/cancel -> cancel agent job

    api_server.register_handler("POST", "/api/jobs/cancel", |req: &HttpRequest| {
        let payload: crate::api_types::AgentJobCancelRequest = try_response!(parse_body(req));
        match crate::services::agent_job_service::AgentJobService::cancel_job(&payload.id) {
            Ok(Some(job)) => ok_json(200, json!({"data": {"cancelled": true, "job": job}})),
            Ok(None) => not_found(format!("Job '{}' not found", payload.id)),
            Err(err) => bad_request(err.to_string()),
        }
    });

    // POST /api/jobs/cancel-all -> cancel all queued/running agent jobs

    api_server.register_handler("POST", "/api/jobs/cancel-all", |_req: &HttpRequest| {
        match crate::services::agent_job_service::AgentJobService::cancel_all_jobs() {
            Ok(jobs) => ok_json(
                200,
                json!({"data": {"cancelled": jobs.len(), "jobs": jobs}}),
            ),
            Err(err) => bad_request(err.to_string()),
        }
    });

    // POST /api/jobs/message -> send message to agent job

    api_server.register_handler("POST", "/api/jobs/message", |req: &HttpRequest| {
        let payload: crate::api_types::AgentJobMessageRequest = try_response!(parse_body(req));
        match crate::services::agent_job_service::AgentJobService::send_message(
            &payload.id,
            &payload.message,
        ) {
            Ok(job) => ok_json(200, json!({"data": {"accepted": true, "job": job}})),
            Err(err) => bad_request(err.to_string()),
        }
    });
    // POST /api/tasks/add
}
