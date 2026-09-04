use super::*;

pub(super) fn register(api_server: &mut ApiServer) {
    api_server.register_handler("GET", "/api/whoami", |_req: &HttpRequest| {
        let who = crate::utils::identity::resolve_current_user(None).unwrap_or_default();
        ok_json(200, json!({"data": who}))
    });

    // POST /api/jobs -> create agent job
}
