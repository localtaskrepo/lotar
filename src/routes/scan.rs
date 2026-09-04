use super::*;

pub(super) fn register(api_server: &mut ApiServer) {
    api_server.register_handler("POST", "/api/scan/run", |req: &HttpRequest| {
        let resolver = match TasksDirectoryResolver::resolve(None, None) {
            Ok(r) => r,
            Err(e) => return internal(json!({"error": {"code": "INTERNAL", "message": e}})),
        };

        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or(json!({}));
        let payload: ScanRequest = match serde_json::from_value(body) {
            Ok(v) => v,
            Err(e) => return bad_request(format!("Invalid body: {}", e)),
        };

        match ScanService::run(&resolver, payload) {
            Ok(result) => ok_json(200, json!({"data": result})),
            Err(err) => match err {
                LoTaRError::ValidationError(_) => bad_request(err.to_string()),
                _ => internal(json!({"error": {"code": "INTERNAL", "message": err.to_string()}})),
            },
        }
    });

    // POST /api/sync/pull
}
