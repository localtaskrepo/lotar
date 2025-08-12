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

pub struct ApiServer {
    // key is normalized "METHOD path"
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

    pub fn handle_request(&self, req: &HttpRequest) -> HttpResponse {
        let key = Self::normalize_key(&req.method, &req.path);
        if let Some(handler) = self.handlers.get(&key) {
            (handler.callback)(req)
        } else {
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

//unsafe impl Sync for ApiServer {}
