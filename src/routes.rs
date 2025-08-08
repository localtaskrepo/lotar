use crate::api_server::ApiServer;

pub fn initialize(api_server: &mut ApiServer) {
    api_server.register_handler(
        "/api/test",
        Box::new(|_path| {
            // Intentionally avoid printing; diagnostics should be handled by caller
            String::from("{\"result\": \"OK\"}")
        }),
    );
}
