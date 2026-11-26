use super::*;

pub(super) fn register(api_server: &mut ApiServer) {
    // GET /api/status -> this instance's running jobs, queue and waiting tickets (DEV-881)
    api_server.register_handler("GET", "/api/status", |req: &HttpRequest| {
        let resolver = try_response!(resolve_tasks_dir());
        let host = req.headers.get("host").map(|host| host.trim()).filter(|host| !host.is_empty()).unwrap_or("localhost");
        let feed = crate::services::status_feed::build(&resolver.path, &format!("http://{host}"));
        ok_json(200, json!({ "data": feed }))
    });
}
