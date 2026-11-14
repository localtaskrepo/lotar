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

    // GET /api/jobs -> list agent jobs, with the ones only the job logs hold (DEV-518)

    api_server.register_handler("GET", "/api/jobs", |req: &HttpRequest| {
        let resolver = resolve_tasks_dir().ok();
        let mut jobs = match &resolver {
            Some(resolver) => crate::services::agent_job_service::AgentJobService::list_jobs_in(&resolver.path),
            None => crate::services::agent_job_service::AgentJobService::list_jobs(),
        };
        if let Some(ticket_id) = req.query.get("ticket_id") {
            // Jobs from before a move name the task's old ID (DEV-183).
            match &resolver {
                Some(resolver) => {
                    let moves = crate::storage::moves::MoveResolver::new(&resolver.path);
                    jobs.retain(|job| job.ticket_id == *ticket_id || moves.same(&job.ticket_id, ticket_id));
                }
                None => jobs.retain(|job| job.ticket_id == *ticket_id),
            }
        }
        if let Some(status) = req.query.get("status") {
            jobs.retain(|job| job.status.eq_ignore_ascii_case(status));
        }
        jobs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        let queue_stats = crate::services::agent_job_service::AgentJobService::queue_stats();
        ok_json(200, json!({"data": {"jobs": jobs, "queue_stats": queue_stats}}))
    });

    // GET /api/jobs/get -> fetch agent job status

    api_server.register_handler("GET", "/api/jobs/get", |req: &HttpRequest| {
        let id = match req.query.get("id") {
            Some(v) if !v.trim().is_empty() => v.clone(),
            _ => return bad_request("Missing job id".into()),
        };
        match job_with_events(&id) {
            Some((job, _)) => ok_json(200, json!({"data": {"job": job}})),
            None => not_found(format!("Job '{}' not found", id)),
        }
    });

    // GET /api/jobs/logs -> fetch agent job logs

    api_server.register_handler("GET", "/api/jobs/logs", |req: &HttpRequest| {
        let id = match req.query.get("id") {
            Some(v) if !v.trim().is_empty() => v.clone(),
            _ => return bad_request("Missing job id".into()),
        };
        match job_with_events(&id) {
            Some((job, events)) => ok_json(200, json!({"data": {"job": job, "events": events}})),
            None => not_found(format!("Job '{}' not found", id)),
        }
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

    api_server.register_handler(
        "POST",
        "/api/jobs/cancel-all",
        |_req: &HttpRequest| match crate::services::agent_job_service::AgentJobService::cancel_all_jobs() {
            Ok(jobs) => ok_json(200, json!({"data": {"cancelled": jobs.len(), "jobs": jobs}})),
            Err(err) => bad_request(err.to_string()),
        },
    );

    // POST /api/jobs/message -> send message to agent job

    api_server.register_handler("POST", "/api/jobs/message", |req: &HttpRequest| {
        let payload: crate::api_types::AgentJobMessageRequest = try_response!(parse_body(req));
        match crate::services::agent_job_service::AgentJobService::send_message(&payload.id, &payload.message) {
            Ok(job) => ok_json(200, json!({"data": {"accepted": true, "job": job}})),
            Err(err) => bad_request(err.to_string()),
        }
    });

    // POST /api/jobs/answer -> answer the agent's open question

    api_server.register_handler("POST", "/api/jobs/answer", |req: &HttpRequest| {
        let payload: crate::api_types::AgentJobAnswerRequest = try_response!(parse_body(req));
        match crate::services::agent_job_service::AgentJobService::answer_question(&payload.id, &payload.request, &payload.option) {
            Ok(job) => ok_json(200, json!({"data": {"accepted": true, "job": job}})),
            Err(err) => bad_request(err.to_string()),
        }
    });
}

/// The job from this process, else from its log (DEV-518).
fn job_with_events(id: &str) -> Option<(crate::api_types::AgentJob, Vec<crate::services::agent_job_service::AgentJobEvent>)> {
    match resolve_tasks_dir() {
        Ok(resolver) => crate::services::agent_job_service::AgentJobService::job_with_events_in(&resolver.path, id),
        Err(_) => crate::services::agent_job_service::AgentJobService::get_job(id)
            .map(|job| (job, crate::services::agent_job_service::AgentJobService::events_for(id))),
    }
}
