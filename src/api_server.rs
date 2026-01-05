use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct HttpRequest {
    pub method: String,
    pub path: String,
    pub query: HashMap<String, String>,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

type HandlerFn = dyn Fn(&HttpRequest) -> HttpResponse + Send + Sync + 'static;

struct ApiHandler {
    callback: Box<HandlerFn>,
}

struct ApiPrefixHandler {
    method: String,
    prefix: String,
    callback: Box<HandlerFn>,
}

pub struct ApiServer {
    // Key is normalized "METHOD path"
    handlers: HashMap<String, ApiHandler>,
    prefix_handlers: Vec<ApiPrefixHandler>,
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
            prefix_handlers: Vec::new(),
        }
    }

    pub fn register_handler<F>(&mut self, method: &str, path: &str, callback: F)
    where
        F: Fn(&HttpRequest) -> HttpResponse + Send + Sync + 'static,
    {
        let key = Self::normalize_key(method, path);
        self.handlers.insert(
            key,
            ApiHandler {
                callback: Box::new(callback),
            },
        );
    }

    pub fn register_prefix_handler<F>(&mut self, method: &str, prefix: &str, callback: F)
    where
        F: Fn(&HttpRequest) -> HttpResponse + Send + Sync + 'static,
    {
        let normalized_prefix = prefix.trim_end_matches('/').to_lowercase();
        self.prefix_handlers.push(ApiPrefixHandler {
            method: method.to_uppercase(),
            prefix: normalized_prefix,
            callback: Box::new(callback),
        });
    }

    pub fn handle_request(&self, req: &HttpRequest) -> HttpResponse {
        let key = Self::normalize_key(&req.method, &req.path);
        if let Some(handler) = self.handlers.get(&key) {
            (handler.callback)(req)
        } else {
            let path = req.path.trim_end_matches('/').to_lowercase();
            let method = req.method.to_uppercase();
            for handler in &self.prefix_handlers {
                if handler.method != method {
                    continue;
                }
                if path == handler.prefix || path.starts_with(&format!("{}/", handler.prefix)) {
                    return (handler.callback)(req);
                }
            }
            HttpResponse {
                status: 404,
                headers: vec![("Content-Type".into(), "application/json".into())],
                body: b"{\"error\":{\"code\":\"NOT_FOUND\",\"message\":\"Route not found\"}}"
                    .to_vec(),
            }
        }
    }

    fn normalize_key(method: &str, path: &str) -> String {
        let p = path.trim_end_matches('/').to_lowercase();
        format!("{} {}", method.to_uppercase(), p)
    }
}
