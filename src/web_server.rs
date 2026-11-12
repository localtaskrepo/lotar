use crate::api_server::{self, HttpRequest};
use crate::output::{LogLevel, OutputFormat, OutputRenderer};
use crate::workspace::TasksDirectoryResolver;
use include_dir::{Dir, include_dir};
use notify::event::EventKind;
use notify::{Config as NotifyConfig, PollWatcher, RecursiveMode, Watcher, recommended_watcher};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex, Weak};
use std::time::Duration;

static STATIC_FILES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/target/web-embed");
static STOP_FLAGS: LazyLock<Mutex<HashMap<u16, bool>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

const MAX_REQUEST_BODY_BYTES: usize = 16 * 1024 * 1024;
const SOCKET_TIMEOUT: Duration = Duration::from_secs(30);

/// Upper bound on simultaneously serviced connections.
///
/// Each accepted connection is handled on its own thread so a slow or idle
/// client (for example a browser's speculative preconnect sockets, which
/// connect and then send nothing) cannot starve other requests. The bound
/// keeps that thread-per-connection model finite: beyond it, additional
/// connections are closed immediately rather than queued or served.
const MAX_CONCURRENT_CONNECTIONS: usize = 64;

/// The peer registry is pruned to live sockets once it grows past this many
/// entries, bounding memory independently of the connection bound.
const MAX_TRACKED_PEERS: usize = 4 * MAX_CONCURRENT_CONNECTIONS;

/// How often the accept loop wakes to re-check the test-stop flag while no
/// connection is pending. Only delays accepts of connections that arrive
/// while the loop is sleeping, never an already-accepted request.
const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Sent with every response: the UI may only be framed by itself (the
/// attachment viewer embeds PDFs), and browsers must not sniff content types.
const SECURITY_HEADERS: &str = "X-Frame-Options: SAMEORIGIN\r\nContent-Security-Policy: frame-ancestors 'self'\r\nX-Content-Type-Options: nosniff\r\n";

fn write_forbidden(mut stream: &TcpStream, body: &str) {
    let response = format!(
        "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
        SECURITY_HEADERS,
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn reject_oversized_body(mut stream: &TcpStream, limit: usize) {
    let body = format!("{{\"error\":{{\"code\":\"PAYLOAD_TOO_LARGE\",\"message\":\"Request body exceeds the {} MiB limit\"}}}}", limit / (1024 * 1024));
    let response =
        format!("HTTP/1.1 413 Content Too Large\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

/// Uploads with `attachments.max_upload_mb: -1` (unlimited) still stop here,
/// since the whole body is held in memory.
const MAX_UPLOAD_BODY_BYTES: usize = 512 * 1024 * 1024;

/// Body size limit for a request. Attachment uploads are base64 inside JSON
/// (4/3 larger), so their limit follows `attachments.max_upload_mb` instead
/// of the general 16 MiB cap (DEV-276: uploads failed above ~12 MiB).
fn request_body_limit(path: &str) -> usize {
    if path != "/api/tasks/attachments/upload" {
        return MAX_REQUEST_BODY_BYTES;
    }
    let configured =
        resolve_server_tasks_dir().and_then(|dir| crate::config::resolution::load_and_merge_configs(Some(&dir)).ok()).map(|cfg| cfg.attachments_max_upload_mb);
    match configured {
        Some(-1) => MAX_UPLOAD_BODY_BYTES,
        Some(mb) if mb > 0 => {
            let file = (mb as usize).saturating_mul(1024 * 1024);
            (file / 3 * 4 + 1024 * 1024).clamp(MAX_REQUEST_BODY_BYTES, MAX_UPLOAD_BODY_BYTES)
        }
        _ => MAX_REQUEST_BODY_BYTES,
    }
}

/// Configuration for the web server's static file serving behavior.
#[derive(Debug, Clone, Default)]
pub struct WebServerConfig {
    /// Optional path to a directory containing custom web UI assets.
    /// When set and the directory exists, files are served from here first,
    /// falling back to embedded assets if not found.
    pub web_ui_path: Option<PathBuf>,

    /// When true, only serve embedded assets (ignore web_ui_path).
    /// Useful for CI testing to ensure the bundled UI works correctly.
    pub embedded_only: bool,

    /// Extra host names accepted in the `Host` header, besides loopback
    /// names and IP literals (DEV-254). `*` accepts any host.
    pub allowed_hosts: Vec<String>,
}

impl WebServerConfig {
    /// Resolve the effective web UI path, if any.
    /// Returns None if embedded_only is set or the path doesn't exist.
    pub fn effective_web_ui_path(&self) -> Option<&Path> {
        if self.embedded_only {
            return None;
        }
        self.web_ui_path.as_ref().filter(|p| p.is_dir()).map(|p| p.as_path())
    }
}

pub fn serve_with_host(api_server: &api_server::ApiServer, host: &str, port: u16) {
    serve_with_config(api_server, host, port, &WebServerConfig::default())
}

pub fn serve_with_config(api_server: &api_server::ApiServer, host: &str, port: u16, config: &WebServerConfig) {
    let listeners = match bind_host(host, port) {
        Ok(listeners) => listeners,
        Err(e) => {
            OutputRenderer::new(OutputFormat::Text, LogLevel::Error).log_error(format_args!("Failed to bind to {}:{}: {}", host, port, e));
            return;
        }
    };
    let refs: Vec<&TcpListener> = listeners.iter().collect();
    serve_listeners(api_server, &refs, config);
}

/// Bind `host:port`. `localhost` binds both loopback addresses (127.0.0.1
/// and ::1) on the same port, so IPv4 and IPv6 clients reach this server and
/// a port taken on either one is reported as busy (DEV-272: binding the
/// first resolved address left 127.0.0.1 to whatever else held it). A
/// missing IPv6 stack only skips ::1. Port 0 picks one free port for both.
pub fn bind_host(host: &str, port: u16) -> std::io::Result<Vec<TcpListener>> {
    if !host.trim().eq_ignore_ascii_case("localhost") {
        return Ok(vec![TcpListener::bind((host, port))?]);
    }
    // With port 0 the free IPv4 port may be taken on ::1; pick again.
    let attempts = if port == 0 { 8 } else { 1 };
    let mut last_err = None;
    for _ in 0..attempts {
        let v4 = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))?;
        let actual = v4.local_addr()?.port();
        match TcpListener::bind((std::net::Ipv6Addr::LOCALHOST, actual)) {
            Ok(v6) => return Ok(vec![v4, v6]),
            Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => last_err = Some(err),
            // No IPv6 loopback on this machine: IPv4 alone is complete.
            Err(_) => return Ok(vec![v4]),
        }
    }
    Err(last_err.unwrap_or_else(|| std::io::Error::from(std::io::ErrorKind::AddrInUse)))
}

/// Serve on an already-bound listener.
///
/// The port used for the test stop registry is taken from the listener's
/// actual local address, so a listener bound to port 0 (OS-assigned) is
/// keyed and torn down correctly. Callers should advertise the actual port
/// from `listener.local_addr()` only after the bind has succeeded.
pub fn serve_listener(api_server: &api_server::ApiServer, listener: &TcpListener, config: &WebServerConfig) {
    serve_listeners(api_server, &[listener], config);
}

/// Serve on several already-bound listeners that share one port (the IPv4
/// and IPv6 loopback sockets of `localhost`), one accept loop each.
pub fn serve_listeners(api_server: &api_server::ApiServer, listeners: &[&TcpListener], config: &WebServerConfig) {
    let Some(first) = listeners.first() else {
        return;
    };
    let port = first.local_addr().map(|addr| addr.port()).unwrap_or_default();

    // Register this server instance in a global stop registry (used only by tests)
    if let Ok(mut map) = STOP_FLAGS.lock() {
        map.insert(port, false);
    }

    // Best-effort: start a filesystem watcher thread to emit SSE events on changes under .tasks
    start_tasks_watcher();

    // Every accepted connection is serviced on its own thread. A browser
    // routinely opens speculative sockets that stay silent until their read
    // timeout; servicing connections serially on the accept thread let a
    // single idle preconnect starve every later request for up to
    // SOCKET_TIMEOUT, which is what stalled `page.reload` and SSE setup in
    // CI. Scoped threads borrow `api_server`/`config` (both Sync) without
    // changing the public serve API.
    let active = Arc::new(AtomicUsize::new(0));
    let peers: Arc<Mutex<Vec<Weak<TcpStream>>>> = Arc::new(Mutex::new(Vec::new()));

    std::thread::scope(|scope| {
        for listener in listeners {
            let active = Arc::clone(&active);
            let peers = Arc::clone(&peers);
            scope.spawn(move || {
                accept_loop(api_server, listener, config, port, &active, &peers, scope);
            });
        }
    });
    // Final sweep: a peer accepted in the last iteration before the stop
    // flag was observed missed the stop handler's own sweep.
    shutdown_peer_connections(&peers, None);

    let _ = STOP_FLAGS.lock().map(|mut m| m.remove(&port));
}

fn accept_loop<'scope, 'env>(
    api_server: &'env api_server::ApiServer,
    listener: &'env TcpListener,
    config: &'env WebServerConfig,
    port: u16,
    active: &Arc<AtomicUsize>,
    peers: &Arc<Mutex<Vec<Weak<TcpStream>>>>,
    scope: &'scope std::thread::Scope<'scope, 'env>,
) {
    // Poll the listener instead of blocking in accept() so the stop flag is
    // observed promptly even when no further connection arrives.
    let _ = listener.set_nonblocking(true);
    loop {
        // Check for test-initiated shutdown before handling the next connection
        if STOP_FLAGS.lock().ok().and_then(|m| m.get(&port).copied()).unwrap_or(false) {
            break;
        }

        match listener.accept() {
            Ok((stream, _addr)) => {
                // Accepted sockets are blocking per POSIX, but reset the
                // flag explicitly so the non-blocking listener state can
                // never leak into request handling.
                let _ = stream.set_nonblocking(false);
                if active.load(Ordering::Acquire) >= MAX_CONCURRENT_CONNECTIONS {
                    // Fail closed: refuse the overload instead of queuing
                    // it, but with a retryable status rather than a reset
                    // (DEV-276).
                    let mut stream = stream;
                    let _ = stream.set_write_timeout(Some(Duration::from_millis(200)));
                    let body = "{\"error\":{\"code\":\"BUSY\",\"message\":\"Too many open connections; retry shortly\"}}";
                    let _ = stream.write_all(
                        format!(
                            "HTTP/1.1 503 Service Unavailable\r\nRetry-After: 1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        )
                        .as_bytes(),
                    );
                    let _ = stream.shutdown(Shutdown::Both);
                    continue;
                }
                let stream = Arc::new(stream);
                if let Ok(mut tracked) = peers.lock() {
                    if tracked.len() >= MAX_TRACKED_PEERS {
                        tracked.retain(|weak| weak.strong_count() > 0);
                    }
                    tracked.push(Arc::downgrade(&stream));
                }
                active.fetch_add(1, Ordering::AcqRel);
                let active = Arc::clone(active);
                let peers = Arc::clone(peers);
                scope.spawn(move || {
                    handle_connection(api_server, &stream, config, port, &peers);
                    active.fetch_sub(1, Ordering::Release);
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(ACCEPT_POLL_INTERVAL);
            }
            Err(e) => {
                OutputRenderer::new(OutputFormat::Text, LogLevel::Warn).log_warn(format_args!("Connection error: {}", e));
            }
        }
    }
    // A peer accepted in the last iteration before the stop flag was
    // observed missed the stop handler's own sweep and would otherwise delay
    // the scoped-thread join until its socket timeout.
    shutdown_peer_connections(peers, None);
}

/// Service exactly one HTTP request on an accepted connection, then return
/// (the socket closes; one-shot responses advertise `Connection: close`).
fn handle_connection(
    api_server: &api_server::ApiServer,
    mut stream: &TcpStream,
    config: &WebServerConfig,
    port: u16,
    peers: &Arc<Mutex<Vec<Weak<TcpStream>>>>,
) {
    let _ = stream.set_read_timeout(Some(SOCKET_TIMEOUT));
    let _ = stream.set_write_timeout(Some(SOCKET_TIMEOUT));
    // Read a single HTTP request (headers + body)
    let (method, path, query, headers, body) = {
        let mut head_buf: Vec<u8> = Vec::new();
        let mut buf = [0u8; 1024];
        loop {
            let n: usize = stream.read(&mut buf).unwrap_or_default();
            if n == 0 {
                break;
            }
            head_buf.extend_from_slice(&buf[..n]);
            if head_buf.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
            if head_buf.len() > 32 * 1024 {
                break;
            }
        }
        let header_end = head_buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4).unwrap_or(head_buf.len());
        let (head_part, body_bytes) = head_buf.split_at(header_end);
        let request_head = String::from_utf8_lossy(head_part);
        let request_line = match request_head.lines().next() {
            Some(line) => line,
            None => {
                return;
            }
        };
        let parts: Vec<&str> = request_line.split(' ').collect();
        let method = parts.first().copied().unwrap_or("GET").to_string();
        let path_full = parts.get(1).copied().unwrap_or("/");
        let (path, query) = parse_path_and_query(path_full);
        // Header names are case-insensitive; normalize once here.
        let mut headers = HashMap::new();
        for line in request_head.lines().skip(1) {
            if line.trim().is_empty() {
                break;
            }
            if let Some((k, v)) = line.split_once(":") {
                headers.insert(k.trim().to_lowercase(), v.trim().to_string());
            }
        }
        // Bodies are read by Content-Length only; a chunked body used to be
        // treated as empty and fail with a misleading error (DEV-276).
        if headers.get("transfer-encoding").is_some_and(|value| !value.trim().eq_ignore_ascii_case("identity")) {
            let body = "{\"error\":{\"code\":\"LENGTH_REQUIRED\",\"message\":\"Chunked request bodies are not supported; send Content-Length\"}}";
            let response = format!(
                "HTTP/1.1 411 Length Required\r\nContent-Type: application/json\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                SECURITY_HEADERS,
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
            return;
        }
        let body_limit = request_body_limit(&path);
        let mut body: Vec<u8> = body_bytes.to_vec();
        let declared_length: Option<usize> = headers.get("content-length").and_then(|v| v.parse::<usize>().ok());
        if declared_length.is_some_and(|cl| cl > body_limit) || body.len() > body_limit {
            reject_oversized_body(stream, body_limit);
            return;
        }
        if let Some(cl) = declared_length {
            while body.len() < cl {
                let n: usize = stream.read(&mut buf).unwrap_or_default();
                if n == 0 {
                    break;
                }
                body.extend_from_slice(&buf[..n]);
                if body.len() > body_limit {
                    break;
                }
            }
            body.truncate(cl.min(body_limit));
        }
        if body.len() > body_limit {
            reject_oversized_body(stream, body_limit);
            return;
        }
        (method, path, query, headers, body)
    };

    // DNS rebinding guard (DEV-254): a page on attacker.example that
    // rebinds its name to this machine sends `Host: attacker.example`, so
    // every request (API, SSE and static) must name an expected host.
    if let Some(host) = header_value(&headers, "Host")
        && !host_is_allowed(host, &config.allowed_hosts)
    {
        write_forbidden(
            stream,
            "{\"error\":{\"code\":\"FORBIDDEN\",\"message\":\"Host not allowed. Open the server through localhost or an IP address, or start it with --allow-host <name>.\"}}",
        );
        return;
    }

    // SSE endpoints
    if path == "/api/events" || path == "/api/tasks/stream" {
        handle_sse_connection(stream, &query);
    } else if path == "/__test/stop" && std::env::var("LOTAR_ALLOW_TEST_STOP").as_deref() == Ok("1") {
        if let Ok(mut map) = STOP_FLAGS.lock() {
            map.insert(port, true);
        }
        // The server owns prompt teardown of its open connections: shutdown
        // every OTHER live peer so blocked readers and SSE forwarders exit
        // instead of delaying the accept loop's scoped-thread join.
        shutdown_peer_connections(peers, Some(stream));
        let response = "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 8\r\nConnection: close\r\n\r\nstopping";
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    } else if path.starts_with("/api") {
        if path == "/api/openapi.json" {
            // Embedded gzip-compressed by build.rs (DEV-221); sent as is to
            // clients that accept gzip, decompressed for the rest.
            static SPEC_GZ: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/openapi.json.gz"));
            let accepts_gzip = headers.get("accept-encoding").is_some_and(|value| value.split(',').any(|part| part.trim().eq_ignore_ascii_case("gzip")));
            let (body, encoding) = if accepts_gzip {
                (SPEC_GZ.to_vec(), "Content-Encoding: gzip\r\nVary: Accept-Encoding\r\n")
            } else {
                use std::io::Read;
                let mut text = Vec::new();
                let _ = flate2::read::GzDecoder::new(SPEC_GZ).read_to_end(&mut text);
                (text, "Vary: Accept-Encoding\r\n")
            };
            let response =
                format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n", encoding, body.len());
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.write_all(&body);
            let _ = stream.flush();
            return;
        }
        if method.eq_ignore_ascii_case("OPTIONS") {
            let preflight = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Methods: GET,POST,OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            let _ = stream.write_all(preflight.as_bytes());
            let _ = stream.flush();
            return;
        }
        if is_mutating_method(&method)
            && let Err(rejection) = validate_request_origin(&headers)
        {
            write_forbidden(stream, &rejection);
            return;
        }

        // HEAD answers like GET without the body (link checkers, download
        // managers and caches probe attachments this way).
        let head = method.eq_ignore_ascii_case("HEAD");
        let req = HttpRequest { method: if head { "GET".to_string() } else { method }, path: path.clone(), query, headers, body };
        let mut resp = api_server.handle_request(&req);
        resp.headers.push(("Access-Control-Allow-Methods".into(), "GET,POST,OPTIONS".into()));
        resp.headers.push(("Access-Control-Allow-Headers".into(), "Content-Type".into()));
        // A 304 has no body, and browsers may copy its headers onto the
        // cached response, so it must not claim a JSON body.
        let not_modified = resp.status == 304;
        if !not_modified && !resp.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("Content-Type")) {
            resp.headers.push(("Content-Type".into(), "application/json".into()));
        }
        // The socket serves exactly one request and then closes; say so.
        if !resp.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("Connection")) {
            resp.headers.push(("Connection".into(), "close".into()));
        }
        let headers_str = resp.headers.iter().map(|(k, v)| format!("{}: {}\r\n", k, v)).collect::<String>();
        let content_length = if not_modified { String::new() } else { format!("Content-Length: {}\r\n", resp.body.len()) };
        // Routes may set some of these themselves (attachments send
        // nosniff); never send a header twice.
        let security_headers: String = SECURITY_HEADERS
            .split_inclusive("\r\n")
            .filter(|line| {
                let name = line.split(':').next().unwrap_or_default();
                !resp.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(name))
            })
            .collect();
        let response = format!("HTTP/1.1 {} {}\r\n{}{}{}\r\n", resp.status, reason_phrase(resp.status), headers_str, security_headers, content_length);
        let _ = stream.write_all(response.as_bytes());
        if !head {
            let _ = stream.write_all(&resp.body);
        }
        let _ = stream.flush();
    } else {
        let request_path = if path == "/" { "/index.html" } else { &path };
        let rel_path = request_path.trim_start_matches('/');
        let external_path = config.effective_web_ui_path();
        let accepts_gzip = headers.get("accept-encoding").map(|value| value.split(',').any(|part| part.trim().eq_ignore_ascii_case("gzip"))).unwrap_or(false);
        let mut served = try_serve_static(rel_path, stream, external_path, accepts_gzip);
        if !served && should_fallback_to_index(&path) {
            served = try_serve_static("index.html", stream, external_path, accepts_gzip);
        }
        if !served {
            let body = "404 - Page not found.";
            let response = format!(
                "HTTP/1.1 404 NOT FOUND\r\nContent-Type: text/plain\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                SECURITY_HEADERS,
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    }
}

/// Reason phrase for an API status line; unknown codes keep their number.
fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        206 => "Partial Content",
        304 => "Not Modified",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        411 => "Length Required",
        409 => "Conflict",
        413 => "Content Too Large",
        416 => "Range Not Satisfiable",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "",
    }
}

/// Shut down every tracked live peer connection, except the excluded socket.
fn shutdown_peer_connections(peers: &Arc<Mutex<Vec<Weak<TcpStream>>>>, except: Option<&TcpStream>) {
    if let Ok(tracked) = peers.lock() {
        for weak in tracked.iter() {
            let Some(peer) = weak.upgrade() else {
                continue;
            };
            if let Some(keep) = except
                && std::ptr::eq(Arc::as_ptr(&peer), keep)
            {
                continue;
            }
            let _ = peer.shutdown(Shutdown::Both);
        }
    }
}

pub fn serve(api_server: &api_server::ApiServer, port: u16) {
    serve_with_host(api_server, "127.0.0.1", port)
}

/// Serve a static file from the configured sources.
///
/// Resolution order:
/// 1. If `external_path` is Some, try to serve from that directory first
/// 2. Try embedded pre-compressed assets (`<path>.gz`, bundled at compile time)
/// 3. Try raw embedded assets (non-text binaries are embedded byte-exact)
/// 4. Fallback to `target/web` on the filesystem (for development)
fn try_serve_static(rel_path: &str, stream: &TcpStream, external_path: Option<&Path>, accepts_gzip: bool) -> bool {
    if !is_safe_rel_path(rel_path) {
        return false;
    }

    // 1. Try external/custom UI path first (if configured); external trees do
    //    not carry pre-compressed siblings, so they are served raw.
    if let Some(ext_dir) = external_path {
        let fs_path = ext_dir.join(rel_path);
        if let Ok(bytes) = fs::read(&fs_path) {
            return serve_bytes(rel_path, &bytes, stream, false);
        }
    }
    // 2. Embedded assets are stored pre-compressed: gzip clients receive the
    //    bytes as-is; others get a transparent runtime decompression (rare —
    //    every mainstream client negotiates gzip).
    if let Some(file) = STATIC_FILES.get_file(format!("{rel_path}.gz")) {
        if accepts_gzip {
            return serve_bytes(rel_path, file.contents(), stream, true);
        }
        let mut decompressed = Vec::with_capacity(file.contents().len() * 4);
        if std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(file.contents()), &mut decompressed).is_ok() {
            return serve_bytes(rel_path, &decompressed, stream, false);
        }
    }
    // 3. Raw embedded assets: non-compressible binaries (ICO/PNG/JPG/WEBP,
    //    fonts, ...) are embedded byte-exact by scripts/compress-web.mjs;
    //    serve the exact bytes with no Content-Encoding.
    if let Some(file) = STATIC_FILES.get_file(rel_path) {
        return serve_bytes(rel_path, file.contents(), stream, false);
    }
    // 4. Fallback to target/web filesystem (development convenience)
    // Skip this if we already tried an external path to avoid confusion
    if external_path.is_none() {
        let fs_path = Path::new("target/web").join(rel_path);
        if let Ok(bytes) = fs::read(&fs_path) {
            return serve_bytes(rel_path, &bytes, stream, false);
        }
    }

    false
}

/// Write HTTP 200 response with the given bytes
fn serve_bytes(rel_path: &str, bytes: &[u8], mut stream: &TcpStream, gzipped: bool) -> bool {
    let content_type = content_type_for(rel_path);
    // Vite emits content-hashed filenames under assets/, so those can be
    // cached immutably; the HTML entry points must always revalidate.
    let cache_control = if rel_path.starts_with("assets/") { "public, max-age=31536000, immutable" } else { "no-cache" };
    // The extra trailing \r\n below terminates the header block; the encoding
    // lines are formatted with their own line endings. The socket closes
    // after this single response, so the connection is advertised as closed.
    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: {}\r\nConnection: close\r\n{}{}\r\n",
        content_type,
        bytes.len(),
        cache_control,
        SECURITY_HEADERS,
        if gzipped { "Content-Encoding: gzip\r\nVary: Accept-Encoding\r\n" } else { "" },
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(bytes);
    let _ = stream.flush();
    true
}

fn should_fallback_to_index(path: &str) -> bool {
    let trimmed = path.trim_matches('/');
    trimmed.is_empty() || !trimmed.contains('.')
}

/// A static asset path must stay inside the asset root on every platform:
/// no `.`/`..` segments, and nothing Windows would treat as a drive or UNC
/// prefix (`C:/...`, `C:foo`, `\\\\server`), which `Path::join` would let
/// replace the base directory (DEV-254).
fn is_safe_rel_path(rel_path: &str) -> bool {
    if rel_path.contains(':') || rel_path.starts_with(['/', '\\']) {
        return false;
    }
    if rel_path.split(['/', '\\']).any(|segment| segment == "." || segment == "..") {
        return false;
    }
    Path::new(rel_path).components().all(|component| matches!(component, std::path::Component::Normal(_)))
}

fn content_type_for(path: &str) -> &'static str {
    match Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html",
        "ico" => "image/x-icon",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "css" => "text/css",
        "js" => "application/javascript",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

fn handle_sse_connection(mut stream: &TcpStream, query: &HashMap<String, String>) {
    use std::time::{Duration, Instant};

    let debounce_ms = query
        .get("debounce_ms")
        .and_then(|s| s.parse::<u64>().ok())
        .or_else(|| std::env::var("LOTAR_SSE_DEBOUNCE_MS").ok().and_then(|s| s.parse::<u64>().ok()))
        .unwrap_or(100);
    let kinds_filter: Option<Vec<String>> =
        query.get("kinds").or_else(|| query.get("topic")).map(|s| s.split(',').filter(|p| !p.is_empty()).map(|p| p.trim().to_string()).collect());
    let project_filter: Option<String> = query.get("project").cloned();

    let rx = crate::api_events::subscribe();
    let fast = std::env::var("LOTAR_TEST_FAST_IO").ok().as_deref() == Some("1");
    let headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\n\r\n";

    // Send headers and initial retry hint
    let mut initial = String::with_capacity(headers.len() + 14);
    initial.push_str(headers);
    initial.push_str("retry: 1000\n\n");
    let _ = stream.write_all(initial.as_bytes());
    let _ = stream.flush();

    // Optional readiness event for tests
    let allow_ready = std::env::var("LOTAR_SSE_READY").ok().as_deref() == Some("1");
    if allow_ready && query.get("ready").map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false) {
        let _ = stream.write_all(b"event: ready\ndata: {}\n\n");
        let _ = stream.flush();

        // If a project filter is active and client is interested in project_changed,
        // emit a synthetic snapshot event to avoid races with watcher startup.
        if let Some(proj_name) = project_filter.as_ref() {
            let wants_project_changed = match &kinds_filter {
                None => true,
                Some(kinds) => kinds.iter().any(|k| k.eq_ignore_ascii_case("project_changed")),
            };
            if wants_project_changed
                && let Some(tasks_dir) = resolve_server_tasks_dir()
                && crate::storage::safety::is_valid_project_prefix(proj_name)
            {
                let proj_dir = tasks_dir.join(proj_name);
                if proj_dir.exists() {
                    // Emit via bus (picked up by forwarder) and also write one immediate event inline.
                    // The payload is JSON-serialized so hostile query values cannot break out of the frame.
                    crate::api_events::emit(&crate::api_events::ApiEvent {
                        kind: "project_changed".to_string(),
                        data: serde_json::json!({ "name": proj_name }),
                    });
                    let inline = format!(
                        "event: project_changed\ndata: {}\n\n",
                        serde_json::to_string(&serde_json::json!({ "name": proj_name })).unwrap_or_else(|_| "{}".to_string())
                    );
                    let _ = stream.write_all(inline.as_bytes());
                    let _ = stream.flush();
                }
            }
        }
    }

    // Spawn a thread to forward events
    let Ok(mut stream_clone) = stream.try_clone() else {
        return;
    };
    std::thread::spawn(move || {
        let mut buffer: Vec<crate::api_events::ApiEvent> = Vec::new();
        let mut deadline: Option<Instant> = None;
        let debounce = Duration::from_millis(if fast { debounce_ms.min(20) } else { debounce_ms });
        let heartbeat_every = if fast { Duration::from_secs(2) } else { Duration::from_secs(15) };
        loop {
            let timeout = match deadline {
                Some(d) => {
                    let now = Instant::now();
                    if d <= now { Duration::from_millis(0) } else { d - now }
                }
                None => heartbeat_every,
            };

            match rx.recv_timeout(timeout) {
                Ok(evt) => {
                    if let Some(ref kinds) = kinds_filter
                        && !kinds.iter().any(|k| k.eq_ignore_ascii_case(&evt.kind))
                    {
                        continue;
                    }
                    if let Some(ref pf) = project_filter {
                        let matches_project = match evt.kind.as_str() {
                            // Filesystem watcher emits { name: <PROJECT> }
                            "project_changed" => evt.data.get("name").and_then(|v| v.as_str()).map(|name| name == pf).unwrap_or(false),
                            // Sync events include { project: <PREFIX> }
                            "sync_started" | "sync_progress" | "sync_completed" | "sync_failed" => {
                                evt.data.get("project").and_then(|v| v.as_str()).map(|name| name == pf).unwrap_or(false)
                            }
                            // Agent jobs carry the ticket they work on.
                            kind if kind.starts_with("agent_job_") => evt
                                .data
                                .get("ticket_id")
                                .and_then(|v| v.as_str())
                                .and_then(|id| crate::storage::TaskId::parse(id).ok())
                                .is_some_and(|parsed| parsed.project == *pf),
                            // Config changes name their project, or none for
                            // workspace-wide config, which concerns every
                            // project. Order and sprint changes span projects.
                            "config_updated" => evt.data.get("project").and_then(|v| v.as_str()).is_none_or(|name| name == pf),
                            "task_order_changed" | "sprint_changed" | "release_changed" => true,
                            // Task events include { id: "PREFIX-N" }; compare
                            // against the canonical ID prefix (final numeric
                            // suffix split) so hyphenated prefixes match.
                            _ => evt
                                .data
                                .get("id")
                                .and_then(|v| v.as_str())
                                .and_then(|id_str| crate::storage::TaskId::parse(id_str).ok())
                                .map(|parsed| parsed.project == *pf)
                                .unwrap_or(false),
                        };
                        if !matches_project {
                            continue;
                        }
                    }
                    // The window opens with the first buffered event and is
                    // not pushed back by later ones, so a steady stream is
                    // still delivered every `debounce` (DEV-273).
                    if buffer.is_empty() {
                        deadline = Some(Instant::now() + debounce);
                    }
                    coalesce_into(&mut buffer, evt);
                    if buffer.len() >= MAX_SSE_BUFFER {
                        deadline = Some(Instant::now());
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if !buffer.is_empty() {
                        for evt in buffer.drain(..) {
                            let line = format!("event: {}\ndata: {}\n\n", evt.kind, serde_json::to_string(&evt.data).unwrap_or("null".to_string()));
                            if stream_clone.write_all(line.as_bytes()).is_err() {
                                return;
                            }
                        }
                        let _ = stream_clone.flush();
                        deadline = None;
                    } else {
                        if stream_clone.write_all(b":heartbeat\n\n").is_err() {
                            return;
                        }
                        let _ = stream_clone.flush();
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    });
}

/// Flush early once a client has this many events waiting.
const MAX_SSE_BUFFER: usize = 500;

/// Add `evt` to an SSE buffer, merging repeated task_updated and
/// project_changed events for the same task or project: an id-only echo
/// never replaces a full DTO already waiting, and a newer full DTO
/// replaces an older one in place.
fn coalesce_into(buffer: &mut Vec<crate::api_events::ApiEvent>, evt: crate::api_events::ApiEvent) {
    let key_field = match evt.kind.as_str() {
        "task_updated" => "id",
        "project_changed" => "name",
        _ => {
            buffer.push(evt);
            return;
        }
    };
    let key = evt.data.get(key_field).and_then(|v| v.as_str());
    let Some(existing) =
        buffer.iter_mut().find(|queued| queued.kind == evt.kind && queued.data.get(key_field).and_then(|v| v.as_str()) == key && key.is_some())
    else {
        buffer.push(evt);
        return;
    };
    let is_full = |data: &serde_json::Value| data.as_object().is_some_and(|map| map.len() > 1);
    if is_full(&evt.data) || !is_full(&existing.data) {
        *existing = evt;
    }
}

fn is_mutating_method(method: &str) -> bool {
    !method.eq_ignore_ascii_case("GET") && !method.eq_ignore_ascii_case("HEAD") && !method.eq_ignore_ascii_case("OPTIONS")
}

fn header_value<'a>(headers: &'a HashMap<String, String>, name: &str) -> Option<&'a str> {
    headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}

fn origin_authority(origin: &str) -> Option<String> {
    let rest = origin.split_once("://").map(|(_, r)| r).unwrap_or(origin);
    let authority = rest.split(['/', '?', '#']).next()?;
    if authority.is_empty() { None } else { Some(authority.to_string()) }
}

/// Whether a `Host` header names this server rather than a rebound domain.
///
/// IP literals cannot be rebound, and `localhost`/`*.localhost` always
/// resolve to loopback, so those pass. Any other name must be listed in
/// `allowed` (the `--host` name and `--allow-host` values; `*` allows all).
fn host_is_allowed(host_header: &str, allowed: &[String]) -> bool {
    let host_header = host_header.trim();
    if allowed.iter().any(|entry| entry == "*") {
        return true;
    }
    let name = if let Some(rest) = host_header.strip_prefix('[') {
        // Bracketed IPv6 literal, with an optional port after `]`.
        return match rest.split_once(']') {
            Some((literal, tail)) => literal.parse::<std::net::Ipv6Addr>().is_ok() && (tail.is_empty() || tail.starts_with(':')),
            None => false,
        };
    } else {
        match host_header.rsplit_once(':') {
            Some((name, port)) if port.chars().all(|c| c.is_ascii_digit()) => name,
            Some(_) => return false,
            None => host_header,
        }
    };
    let name = name.trim_end_matches('.').to_ascii_lowercase();
    if name.is_empty() {
        return false;
    }
    if name.parse::<std::net::Ipv4Addr>().is_ok() || name == "localhost" || name.ends_with(".localhost") {
        return true;
    }
    allowed.iter().any(|entry| entry.trim().trim_end_matches('.').eq_ignore_ascii_case(&name))
}

fn validate_request_origin(headers: &HashMap<String, String>) -> Result<(), String> {
    let Some(origin) = header_value(headers, "Origin").map(str::trim).filter(|o| !o.is_empty()) else {
        return Ok(());
    };
    let Some(host) = header_value(headers, "Host").map(str::trim).filter(|h| !h.is_empty()) else {
        return Err("{\"error\":{\"code\":\"FORBIDDEN\",\"message\":\"Missing Host header\"}}".to_string());
    };
    match origin_authority(origin) {
        Some(authority) if authority.eq_ignore_ascii_case(host) => Ok(()),
        _ => Err("{\"error\":{\"code\":\"FORBIDDEN\",\"message\":\"Cross-origin request rejected (Origin does not match Host)\"}}".to_string()),
    }
}

fn parse_path_and_query(path_full: &str) -> (String, HashMap<String, String>) {
    let mut out = HashMap::new();
    if let Some((p, q)) = path_full.split_once('?') {
        for part in q.split('&') {
            if part.is_empty() {
                continue;
            }
            let (k, v) = part.split_once('=').unwrap_or((part, ""));
            out.insert(url_decode(k), url_decode(v));
        }
        (p.to_string(), out)
    } else {
        (path_full.to_string(), out)
    }
}

fn url_decode(s: &str) -> String {
    // Minimal percent-decoder for application/x-www-form-urlencoded-like
    // strings. Decoded bytes are collected first and read as UTF-8, so
    // `%C3%A9` is "é", not two Latin-1 characters (DEV-256).
    let hex = |c: u8| -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(10 + c - b'a'),
            b'A'..=b'F' => Some(10 + c - b'A'),
            _ => None,
        }
    };
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                    out.push(h << 4 | l);
                    i += 3;
                } else {
                    out.push(b'%');
                    i += 1;
                }
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|err| String::from_utf8_lossy(err.as_bytes()).into_owned())
}

fn resolve_server_tasks_dir() -> Option<PathBuf> {
    TasksDirectoryResolver::resolve(None, None).ok().and_then(|resolver| resolver.absolute_path().ok())
}

// Lightweight watcher that monitors the resolved tasks directory and emits project_changed
fn start_tasks_watcher() {
    // Don't crash server if watcher setup fails
    let tasks_dir = match resolve_server_tasks_dir() {
        Some(path) => path,
        None => return,
    };
    if !tasks_dir.exists() {
        return;
    }
    // Spawn a detached thread that owns the watcher
    std::thread::spawn(move || {
        // Channel for notify events forwarded from the watcher callback
        let (tx, rx) = std::sync::mpsc::channel();

        let mut watcher = match recommended_watcher({
            let tx = tx.clone();
            move |res| {
                let _ = tx.send(res);
            }
        }) {
            Ok(w) => w,
            Err(_) => return,
        };

        if watcher.configure(NotifyConfig::default()).is_err() {
            return;
        }

        if watcher.watch(&tasks_dir, RecursiveMode::Recursive).is_err() {
            return;
        }

        let enable_poll =
            std::env::var("LOTAR_ENABLE_POLL_WATCH").map(|v| v != "0").unwrap_or_else(|_| matches!(std::env::var("LOTAR_TEST_FAST_IO"), Ok(ref v) if v == "1"));
        let mut poll_watcher: Option<PollWatcher> = None;
        if enable_poll {
            let interval = std::env::var("LOTAR_POLL_WATCH_INTERVAL_MS").ok().and_then(|s| s.parse::<u64>().ok()).map(|ms| ms.max(50)).unwrap_or(200);
            let cfg = NotifyConfig::default().with_poll_interval(Duration::from_millis(interval));
            if let Ok(mut poller) = PollWatcher::new(
                {
                    let tx = tx.clone();
                    move |res| {
                        let _ = tx.send(res);
                    }
                },
                cfg,
            ) && poller.watch(&tasks_dir, RecursiveMode::Recursive).is_ok()
            {
                poll_watcher = Some(poller);
            }
        }

        drop(tx);

        let order_storage = crate::storage::manager::Storage { root_path: tasks_dir.clone() };
        let mut order_snapshots = HashMap::new();
        if let Ok(order) = crate::services::task_order::get_order(&order_storage, None) {
            order_snapshots.insert(None, order.tasks);
        }
        if let Ok(records) = crate::services::sprint_service::SprintService::list(&order_storage) {
            for record in records {
                if let Ok(order) = crate::services::task_order::get_order(&order_storage, Some(record.id)) {
                    order_snapshots.insert(Some(record.id), order.tasks);
                }
            }
        }

        // Changes arrive in bursts (one write is a temp file, a rename and
        // a lock); handle them in short batches so each task, project and
        // sprint is announced once per burst (DEV-273).
        let mut sprint_members: HashMap<Option<u32>, HashSet<String>> =
            order_snapshots.iter().map(|(id, tasks)| (*id, tasks.iter().cloned().collect())).collect();
        // Content last announced per file: the native and the poll watcher
        // (and repeated notifications) report one change several times.
        let mut announced: HashMap<PathBuf, u64> = HashMap::new();
        while let Ok(first) = rx.recv() {
            let mut paths: Vec<PathBuf> = Vec::new();
            let mut collect = |res: notify::Result<notify::Event>| {
                if let Ok(event) = res
                    && matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_))
                {
                    for path in event.paths {
                        if !paths.contains(&path) {
                            paths.push(path);
                        }
                    }
                }
            };
            collect(first);
            let started = std::time::Instant::now();
            while started.elapsed() < WATCH_BATCH_MAX {
                match rx.recv_timeout(WATCH_BATCH_QUIET) {
                    Ok(res) => collect(res),
                    Err(_) => break,
                }
            }
            process_watch_batch(&tasks_dir, &paths, &mut order_snapshots, &mut sprint_members, &mut announced);
        }

        drop(poll_watcher);
    });
}

/// A watcher batch ends after this much quiet, or after `WATCH_BATCH_MAX`.
const WATCH_BATCH_QUIET: Duration = Duration::from_millis(40);
const WATCH_BATCH_MAX: Duration = Duration::from_millis(250);

/// Turn one batch of changed paths under the tasks directory into events.
fn process_watch_batch(
    tasks_dir: &Path,
    paths: &[PathBuf],
    order_snapshots: &mut HashMap<Option<u32>, Vec<String>>,
    sprint_members: &mut HashMap<Option<u32>, HashSet<String>>,
    announced: &mut HashMap<PathBuf, u64>,
) {
    let mut projects: Vec<String> = Vec::new();
    let mut tasks: Vec<(String, PathBuf)> = Vec::new();
    let mut sprints: Vec<Option<u32>> = Vec::new();
    let mut config_projects: Vec<Option<String>> = Vec::new();

    // FSEvents reports canonical paths (/private/var/... for /var/...), so
    // match the canonical tasks directory as well.
    let canonical = tasks_dir.canonicalize().ok();
    for path in paths {
        let Some(relative) = path.strip_prefix(tasks_dir).ok().or_else(|| canonical.as_deref().and_then(|dir| path.strip_prefix(dir).ok())) else {
            continue;
        };
        let parts: Vec<&str> = relative
            .components()
            .filter_map(|c| match c {
                std::path::Component::Normal(part) => part.to_str(),
                _ => None,
            })
            .collect();
        // Lock files, temp files and transaction journals are dot-files.
        if parts.is_empty() || parts.iter().any(|part| part.starts_with('.')) {
            continue;
        }
        // Already announced with this exact content (or absence)?
        let fingerprint = file_fingerprint(path);
        if announced.get(relative) == Some(&fingerprint) {
            continue;
        }
        announced.insert(relative.to_path_buf(), fingerprint);
        let is_config = |name: &str| name == "config.yml" || name == "automation.yml";
        match parts.as_slice() {
            [file] if is_config(file) => {
                if !config_projects.contains(&None) {
                    config_projects.push(None);
                }
                continue;
            }
            [_] => continue, // other root files are not projects
            ["@releases", file] => {
                // A release file (DEV-393); its tickets change through their own files.
                if let Some(id) = file.strip_suffix(".yml").and_then(|stem| stem.parse::<u32>().ok()).filter(|id| *id > 0) {
                    crate::api_events::emit(&crate::api_events::ApiEvent {
                        kind: "release_changed".into(),
                        data: serde_json::json!({"id": id, "deleted": !path.exists()}),
                    });
                }
                continue;
            }
            ["@sprints", file] => {
                if let Some(stem) = file.strip_suffix(".yml") {
                    let container = if stem == "backlog" { Some(None) } else { stem.parse::<u32>().ok().filter(|id| *id > 0).map(Some) };
                    if let Some(container) = container
                        && !sprints.contains(&container)
                    {
                        sprints.push(container);
                    }
                }
                continue;
            }
            _ => {}
        }
        let project = parts[0];
        if project.starts_with('@') || !crate::storage::safety::is_valid_project_prefix(project) {
            continue;
        }
        if !projects.iter().any(|known| known == project) {
            projects.push(project.to_string());
        }
        if parts.len() == 2 && is_config(parts[1]) {
            let scope = Some(project.to_string());
            if !config_projects.contains(&scope) {
                config_projects.push(scope);
            }
            continue;
        }
        if let Some(task) = crate::storage::layout::parse_relative(relative) {
            let id = task.id();
            if !tasks.iter().any(|(known, _)| *known == id) {
                tasks.push((id, path.clone()));
            }
        }
    }

    for (id, path) in &tasks {
        emit_watched_task(tasks_dir, id, path);
    }
    for container in sprints {
        emit_watched_sprint(tasks_dir, container, order_snapshots, sprint_members);
    }
    if !config_projects.is_empty() && !crate::api_events::is_recent_own_config_write() {
        for project in config_projects {
            crate::api_events::emit(&crate::api_events::ApiEvent {
                kind: "config_updated".to_string(),
                data: match project {
                    Some(project) => serde_json::json!({ "project": project }),
                    None => serde_json::json!({}),
                },
            });
        }
    }
    for project in projects {
        crate::api_events::emit(&crate::api_events::ApiEvent { kind: "project_changed".to_string(), data: serde_json::json!({ "name": project }) });
    }
}

/// Content fingerprint of a watched file; directories and missing files
/// get fixed markers.
fn file_fingerprint(path: &Path) -> u64 {
    use std::hash::{Hash, Hasher};
    match fs::read(path) {
        Ok(bytes) => {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            bytes.hash(&mut hasher);
            hasher.finish()
        }
        Err(_) if path.is_dir() => 1,
        Err(_) => 0,
    }
}

/// Announce a changed task file: deleted, unreadable, or updated. The
/// file is parsed directly (no full task lookup), and a change this server
/// already announced with a full DTO is not repeated.
fn emit_watched_task(tasks_dir: &Path, id: &str, path: &Path) {
    let emit = |kind: &str| {
        crate::api_events::emit(&crate::api_events::ApiEvent { kind: kind.to_string(), data: serde_json::json!({ "id": id }) });
    };
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => {
            // Moved into or out of `@archive` (or another folder): still
            // stored somewhere means updated, not deleted.
            let still_stored = crate::storage::TaskId::parse(id).is_ok_and(|parsed| !parsed.locate(tasks_dir).is_empty());
            emit(if still_stored { "task_updated" } else { "task_deleted" });
            return;
        }
    };
    match serde_yaml_ng::from_str::<crate::storage::task::Task>(&content) {
        Ok(task) => {
            if !crate::api_events::is_own_task_write(id, &task.modified) {
                emit("task_updated");
            }
        }
        Err(err) => {
            // task_error for the notification, task_updated so UIs refresh.
            crate::api_events::emit_task_error(id, &err.to_string());
            emit("task_updated");
        }
    }
}

/// Announce a changed sprint or backlog file: `task_order_changed` when the
/// order changed, `sprint_changed` for a numeric sprint, and `task_updated`
/// for every task that joined or left it (their DTO `sprints` changed).
fn emit_watched_sprint(
    tasks_dir: &Path,
    container: Option<u32>,
    order_snapshots: &mut HashMap<Option<u32>, Vec<String>>,
    sprint_members: &mut HashMap<Option<u32>, HashSet<String>>,
) {
    let storage = crate::storage::manager::Storage { root_path: tasks_dir.to_path_buf() };
    let order = crate::services::task_order::get_order(&storage, container).ok().map(|order| order.tasks);
    if let Some(tasks) = order.as_ref()
        && order_snapshots.get(&container) != Some(tasks)
    {
        crate::api_events::emit(&crate::api_events::ApiEvent {
            kind: "task_order_changed".into(),
            data: serde_json::json!({"sprint_id": container, "tasks": tasks}),
        });
    }
    let Some(sprint_id) = container else {
        if let Some(tasks) = order {
            order_snapshots.insert(None, tasks);
        }
        return;
    };
    let now: HashSet<String> = order.iter().flatten().cloned().collect();
    let before = sprint_members.remove(&container).unwrap_or_default();
    crate::api_events::emit(&crate::api_events::ApiEvent {
        kind: "sprint_changed".into(),
        data: serde_json::json!({"id": sprint_id, "deleted": order.is_none()}),
    });
    let mut changed: Vec<&String> = before.symmetric_difference(&now).collect();
    changed.sort();
    for id in changed {
        crate::api_events::emit(&crate::api_events::ApiEvent { kind: "task_updated".into(), data: serde_json::json!({ "id": id }) });
    }
    match order {
        Some(tasks) => {
            order_snapshots.insert(container, tasks);
            sprint_members.insert(container, now);
        }
        None => {
            order_snapshots.remove(&container);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{content_type_for, host_is_allowed, is_safe_rel_path, parse_path_and_query, reason_phrase, url_decode};

    #[test]
    fn query_values_decode_as_utf8() {
        // DEV-256: multibyte values used to decode as Latin-1 ("CafÃ©").
        assert_eq!(url_decode("Caf%C3%A9"), "Café");
        assert_eq!(url_decode("%C3%BCber+alles"), "über alles");
        assert_eq!(url_decode("%F0%9F%9A%80"), "🚀");
        assert_eq!(url_decode("Jos\u{e9}"), "José", "raw UTF-8 stays intact");
        assert_eq!(url_decode("100%"), "100%");
        assert_eq!(url_decode("%zz"), "%zz");
        assert_eq!(url_decode("%FF"), "\u{FFFD}", "invalid UTF-8 is replaced");
        let (_, query) = parse_path_and_query("/api/tasks/list?assignee=Jos%C3%A9&q=caf%C3%A9");
        assert_eq!(query.get("assignee").map(String::as_str), Some("José"));
        assert_eq!(query.get("q").map(String::as_str), Some("café"));
    }

    #[test]
    fn sse_buffer_merges_repeated_task_and_project_events() {
        use crate::api_events::ApiEvent;
        let event = |kind: &str, data: serde_json::Value| ApiEvent { kind: kind.into(), data };
        let mut buffer = Vec::new();
        super::coalesce_into(&mut buffer, event("task_updated", serde_json::json!({"id": "A-1", "title": "full"})));
        super::coalesce_into(&mut buffer, event("task_updated", serde_json::json!({"id": "A-1"})));
        super::coalesce_into(&mut buffer, event("project_changed", serde_json::json!({"name": "A"})));
        super::coalesce_into(&mut buffer, event("project_changed", serde_json::json!({"name": "A"})));
        super::coalesce_into(&mut buffer, event("task_deleted", serde_json::json!({"id": "A-1"})));
        super::coalesce_into(&mut buffer, event("task_updated", serde_json::json!({"id": "A-2", "title": "x"})));
        let kinds: Vec<&str> = buffer.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(kinds, ["task_updated", "project_changed", "task_deleted", "task_updated"]);
        assert_eq!(buffer[0].data["title"], "full", "id-only echo keeps the DTO");
    }

    #[test]
    fn localhost_binds_both_loopbacks_and_reports_either_one_busy() {
        // DEV-272: binding only the first resolved address (::1 on macOS)
        // left 127.0.0.1 unreachable and missed a port taken there.
        let listeners = super::bind_host("localhost", 0).expect("bind localhost");
        let port = listeners[0].local_addr().unwrap().port();
        assert!(listeners[0].local_addr().unwrap().is_ipv4());
        if let Some(v6) = listeners.get(1) {
            assert!(v6.local_addr().unwrap().is_ipv6());
            assert_eq!(v6.local_addr().unwrap().port(), port);
        }
        drop(listeners);

        let held_v4 = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let busy = held_v4.local_addr().unwrap().port();
        let err = super::bind_host("localhost", busy).expect_err("v4 port is busy");
        assert_eq!(err.kind(), std::io::ErrorKind::AddrInUse);

        if let Ok(held_v6) = std::net::TcpListener::bind(("::1", 0)) {
            let busy = held_v6.local_addr().unwrap().port();
            let free_on_v4 = std::net::TcpListener::bind(("127.0.0.1", busy)).is_ok();
            if free_on_v4 {
                let err = super::bind_host("localhost", busy).expect_err("v6 port is busy");
                assert_eq!(err.kind(), std::io::ErrorKind::AddrInUse);
            }
        }
    }

    #[test]
    fn rebound_host_names_are_rejected() {
        let none: Vec<String> = Vec::new();
        assert!(!host_is_allowed("attacker.example:8080", &none));
        assert!(!host_is_allowed("attacker.example", &none));
        assert!(!host_is_allowed("localhost.attacker.example:8080", &none));
        assert!(!host_is_allowed("", &none));
        assert!(!host_is_allowed("[::1", &none));
        assert!(!host_is_allowed("localhost:abc", &none));
    }

    #[test]
    fn loopback_names_and_ip_literals_are_accepted() {
        let none: Vec<String> = Vec::new();
        for host in ["localhost:8080", "LOCALHOST", "localhost.:8080", "app.localhost:8080", "127.0.0.1:8080", "192.168.1.20:8080", "[::1]:8080", "[fe80::1]"] {
            assert!(host_is_allowed(host, &none), "{host} should be allowed");
        }
    }

    #[test]
    fn configured_host_names_are_accepted() {
        let allowed = vec!["devbox.lan".to_string()];
        assert!(host_is_allowed("devbox.lan:8080", &allowed));
        assert!(host_is_allowed("DevBox.LAN", &allowed));
        assert!(!host_is_allowed("other.lan:8080", &allowed));
        assert!(host_is_allowed("anything.example", &["*".to_string()]));
    }

    #[test]
    fn static_paths_cannot_escape_the_asset_root() {
        assert!(is_safe_rel_path("index.html"));
        assert!(is_safe_rel_path("assets/index-abc.js"));
        for path in ["../secret", "assets/../../secret", "./index.html", "C:/Windows/win.ini", "C:foo", "/etc/passwd", "\\\\server\\share", "assets\\..\\x"] {
            assert!(!is_safe_rel_path(path), "{path} should be rejected");
        }
    }

    #[test]
    fn status_lines_keep_their_code() {
        // DEV-216: codes without a listed phrase used to go out as 500.
        assert_eq!(reason_phrase(304), "Not Modified");
        assert_eq!(reason_phrase(409), "Conflict");
        assert_eq!(reason_phrase(418), "");
    }

    #[test]
    fn text_assets_keep_their_documented_mime_types() {
        assert_eq!(content_type_for("index.html"), "text/html");
        assert_eq!(content_type_for("assets/index-sGbZQYYd.css"), "text/css");
        assert_eq!(content_type_for("assets/index-BtHy0XNw.js"), "application/javascript");
        assert_eq!(content_type_for("assets/favicon-C8vPI8o9.svg"), "image/svg+xml");
    }

    #[test]
    fn embedded_branding_binaries_have_concrete_mime_types() {
        assert_eq!(content_type_for("assets/favicon-B3xkQ9.ico"), "image/x-icon");
        assert_eq!(content_type_for("assets/lotar-logo.webp"), "image/webp");
        assert_eq!(content_type_for("assets/branding/favicon.ico"), "image/x-icon");
    }

    #[test]
    fn raster_images_map_to_image_mime_types() {
        assert_eq!(content_type_for("assets/lotar-logo-2296x608.png"), "image/png");
        assert_eq!(content_type_for("assets/lotar-logo-white-1148x304.jpg"), "image/jpeg");
        assert_eq!(content_type_for("assets/photo.jpeg"), "image/jpeg");
    }

    #[test]
    fn unknown_extensions_fall_back_to_octet_stream() {
        assert_eq!(content_type_for("assets/inter-variable.woff2"), "application/octet-stream");
        assert_eq!(content_type_for("assets/manifest"), "application/octet-stream");
        // Extension matching is case-sensitive, matching Vite's lowercase
        // output; anything else must not claim a type it cannot prove.
        assert_eq!(content_type_for("assets/UPPER.SVG"), "application/octet-stream");
    }
}
