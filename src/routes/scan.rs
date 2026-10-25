use super::*;

pub(super) fn register(api_server: &mut ApiServer) {
    api_server.register_handler("POST", "/api/scan/run", |req: &HttpRequest| {
        let resolver = try_response!(resolve_tasks_dir());

        let payload: ScanRequest = try_response!(parse_body_or_empty(req));

        match ScanService::run(&resolver, payload) {
            Ok(result) => ok_json(200, json!({"data": result})),
            Err(err) => map_lotar_error(err),
        }
    });

    // POST /api/sync/pull
}
