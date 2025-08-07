use std::collections::HashMap;

struct ApiHandler {
    callback: Box<dyn Fn(String) -> String>,
}

pub struct ApiServer {
    handlers: HashMap<String, ApiHandler>,
}

impl Default for ApiServer {
    fn default() -> Self {
        Self::new()
    }
}

impl ApiServer {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    pub fn register_handler(&mut self, path: &str, callback: Box<dyn Fn(String) -> String>) {
        // Normalize the path to remove trailing slashes and make it lowercase
        let normalized_path: String = path.trim_end_matches('/').to_lowercase();

        self.handlers
            .insert(normalized_path.to_string(), ApiHandler { callback });
    }

    pub fn handle_request(&self, request_path: &str) -> String {
        // Normalize the request path to remove trailing slashes and make it lowercase
        let normalized_request_path = request_path.trim_end_matches('/').to_lowercase();

        // Find the best matching handler for the request path
        let matching_handler = self
            .handlers
            .iter()
            .filter(|(path, _)| normalized_request_path.starts_with(path.as_str()))
            .max_by_key(|(path, _)| path.len());

        match matching_handler {
            Some((_, handler)) => (handler.callback)(request_path.to_string()),
            None => "HTTP/1.1 404 NOT FOUND\r\n\r\n404 - Page not found.".to_string(),
        }
    }
}

//unsafe impl Sync for ApiServer {}
