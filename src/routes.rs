use crate::api_server::ApiServer;

pub fn initialize(api_server: &mut ApiServer) {
    api_server.register_handler("/api/test", Box::new(|path| {
        println!("Executing test handler for path: {}", path);
        "{\"result\": \"OK\"}".to_string()
    }));
}