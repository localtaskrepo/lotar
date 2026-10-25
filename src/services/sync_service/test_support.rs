//! Routed mock HTTP API for sync tests. Each request is answered by a route
//! closure and recorded so tests can assert on what the client sent.

use super::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub(super) struct RecordedRequest {
    pub(super) method: String,
    /// Path including the query string.
    pub(super) path: String,
    pub(super) body: String,
}

type Route = dyn Fn(&RecordedRequest) -> (u16, JsonValue) + Send + Sync;

pub(super) struct MockApi {
    address: std::net::SocketAddr,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    stopped: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl MockApi {
    pub(super) fn start(
        route: impl Fn(&RecordedRequest) -> (u16, JsonValue) + Send + Sync + 'static,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stopped = Arc::new(AtomicBool::new(false));
        let route: Arc<Route> = Arc::new(route);
        let (log, stop) = (requests.clone(), stopped.clone());
        let thread = std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let request = read_request(&stream);
                log.lock().unwrap().push(request.clone());
                let (status, payload) = route(&request);
                let body = payload.to_string();
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        Self {
            address,
            requests,
            stopped,
            thread: Some(thread),
        }
    }

    pub(super) fn client(&self, provider: SyncProvider) -> SyncClient {
        SyncClient::new(AuthContext {
            provider,
            api_base: format!("http://{}", self.address),
            auth_header: Some("Bearer test".into()),
            user_agent: "sync-test".into(),
        })
    }

    pub(super) fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for MockApi {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.address);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn read_request(stream: &TcpStream) -> RecordedRequest {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut first = String::new();
    reader.read_line(&mut first).unwrap();
    let mut length = 0;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).unwrap();
        if header == "\r\n" || header.is_empty() {
            break;
        }
        if let Some(value) = header.to_ascii_lowercase().strip_prefix("content-length:") {
            length = value.trim().parse::<usize>().unwrap();
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    let mut parts = first.split_whitespace();
    RecordedRequest {
        method: parts.next().unwrap_or_default().to_string(),
        path: parts.next().unwrap_or_default().to_string(),
        body: String::from_utf8_lossy(&body).into_owned(),
    }
}

/// Temporary workspace with a TEST project and the given remote config.
pub(super) fn workspace(
    provider: SyncProvider,
) -> (tempfile::TempDir, TasksDirectoryResolver, SyncRemoteConfig) {
    let temp = tempfile::tempdir().unwrap();
    let resolver = TasksDirectoryResolver {
        path: temp.path().to_path_buf(),
        source: crate::workspace::TasksDirectorySource::CommandLineFlag,
    };
    fs::write(
        crate::utils::paths::global_config_path(temp.path()),
        "default.project: TEST\nauto.set.reporter: false\nauto.assign: false\n",
    )
    .unwrap();
    let remote = SyncRemoteConfig {
        provider,
        project: (provider == SyncProvider::Jira).then(|| "ENG".into()),
        repo: (provider == SyncProvider::Github).then(|| "org/repo".into()),
        filter: None,
        auth_profile: None,
        mapping: HashMap::new(),
    };
    (temp, resolver, remote)
}

pub(super) fn recorder(direction: SyncDirection, dry_run: bool) -> SyncReportRecorder {
    SyncReportRecorder::new(SyncRunContext {
        run_id: "test".into(),
        started_at: Utc::now().to_rfc3339(),
        direction,
        provider: "test".into(),
        remote: "test".into(),
        project: Some("TEST".into()),
        dry_run,
    })
}
