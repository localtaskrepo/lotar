//! Renames, settles and stops T3 Code threads through the T3 Code server's
//! internal WebSocket API, since the `t3` CLI can't (DEV-619, DEV-836), and
//! lists a project's unsettled threads there (DEV-941). T3 Code
//! doesn't document this API and can change it in any release: the owner chose
//! it anyway and checks it after each T3 Code update (2026-11-17, "Implement
//! with the internal API").
//!
//! The route is the one T3 Code's own app takes: a short-lived bearer session
//! from `t3 auth session issue`, a WebSocket to `/ws`, and the RPC
//! `orchestration.dispatchCommand` with a `thread.metadata.update`,
//! `thread.settle` or `thread.stop` command. A list subscribes to
//! `orchestration.subscribeShell` with `omitOlderSettled` and reads its first
//! snapshot: every unsettled thread and the recently settled ones, where
//! `t3 thread list` pages through every thread the project ever had and starts
//! T3 Code's Electron binary each time. Its read-only session is issued once
//! and reused for an hour.
//!
//! A thread of a job on an agent host (DEV-901) is on that host's T3 Code:
//! its runtime state is read, and the session issued and revoked, over ssh,
//! and the WebSocket goes through an ssh forward that lasts the command.

use std::io::ErrorKind;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use tungstenite::client::IntoClientRequest;
use tungstenite::{Message, http::HeaderValue};

/// How long connecting and waiting for the server's answer may take.
const TIMEOUT: Duration = Duration::from_secs(10);

/// Rename `thread` to `title`. `program` is the `t3` CLI, `env` its
/// environment; the error says which step failed.
pub(crate) fn rename_thread(program: &Path, env: &[(String, String)], cwd: &Path, thread: &str, title: &str) -> Result<(), String> {
    send(program, env, cwd, &rename_command(thread, title))
}

/// Move `thread` to T3 Code's Settled group. T3 Code never resumes a settled
/// thread at a usage limit's reset: settling turns its auto-resume off.
pub(crate) fn settle_thread(program: &Path, env: &[(String, String)], cwd: &Path, thread: &str) -> Result<(), String> {
    send(program, env, cwd, &command("thread.settle", thread))
}

/// Stop the run `thread` is running, as T3 Code's Stop button does.
pub(crate) fn stop_thread(program: &Path, env: &[(String, String)], cwd: &Path, thread: &str, reason: &str) -> Result<(), String> {
    let mut stop = command("thread.stop", thread);
    stop["reason"] = serde_json::Value::from(reason);
    send(program, env, cwd, &stop)
}

fn send(program: &Path, env: &[(String, String)], cwd: &Path, command: &serde_json::Value) -> Result<(), String> {
    let thread = command.get("threadId").and_then(serde_json::Value::as_str).unwrap_or_default();
    if let Some(ssh) = crate::services::agent_hosts::thread_route(thread) {
        return send_via(&ssh, command);
    }
    let origin = server_origin(env)?;
    let (session_id, token) = issue_session(program, env, cwd)?;
    let sent = dispatch(&origin, &token, command);
    revoke_session(program, env, cwd, &session_id);
    sent
}

/// [`send`] to the T3 Code of the host `ssh` reaches.
fn send_via(ssh: &crate::services::agent_hosts::Ssh, command: &serde_json::Value) -> Result<(), String> {
    let runtime = ssh
        .run(r#"cat "${T3CODE_HOME:-$HOME/.t3}/userdata/server-runtime.json""#)
        .map_err(|err| format!("T3 Code's server on {} isn't running: {err}", ssh.target))?;
    let state: serde_json::Value =
        serde_json::from_str(runtime.trim()).map_err(|err| format!("T3 Code's runtime state on {} isn't JSON: {err}", ssh.target))?;
    let origin = state.get("origin").and_then(serde_json::Value::as_str).ok_or_else(|| format!("T3 Code's runtime state on {} names no origin", ssh.target))?;
    let uri: tungstenite::http::Uri = origin.parse().map_err(|err| format!("bad T3 Code server address {origin} on {}: {err}", ssh.target))?;
    let (host, port) = (uri.host().unwrap_or("127.0.0.1").to_string(), uri.port_u16().unwrap_or(80));
    let issued = ssh.run("t3 auth session issue --scope orchestration:operate --ttl 2m --label 'lotar thread command' --json")?;
    let issued: serde_json::Value =
        serde_json::from_str(issued.trim()).map_err(|err| format!("`t3 auth session issue` on {} printed no JSON: {err}", ssh.target))?;
    let field = |name: &str| issued.get(name).and_then(serde_json::Value::as_str).map(str::to_string);
    let (session_id, token) = field("sessionId").zip(field("token")).ok_or("`t3 auth session issue` printed no sessionId and token")?;
    let sent = crate::services::agent_hosts::Forward::open(ssh, &host, port)
        .and_then(|forward| dispatch(&format!("http://127.0.0.1:{}", forward.port), &token, command));
    let _ = ssh.run(&format!("t3 auth session revoke {}", crate::services::agent_hosts::quote(&session_id)));
    sent
}

/// A command of `kind` on `thread`, with an id no other command has.
fn command(kind: &str, thread: &str) -> serde_json::Value {
    let millis = jiff::Timestamp::now().as_millisecond();
    serde_json::json!({"type": kind, "commandId": format!("lotar:{kind}:{thread}:{millis}"), "threadId": thread})
}

/// The command T3 Code's app sends when someone renames a thread.
fn rename_command(thread: &str, title: &str) -> serde_json::Value {
    let millis = jiff::Timestamp::now().as_millisecond();
    serde_json::json!({
        "type": "thread.metadata.update",
        "commandId": format!("lotar:thread-rename:{thread}:{millis}"),
        "threadId": thread,
        "title": title,
    })
}

/// T3 Code's data directory: `T3CODE_HOME` of `env` or the process, else `~/.t3`.
fn t3_home(env: &[(String, String)]) -> Result<PathBuf, String> {
    env.iter()
        .rev()
        .find(|(key, _)| key == "T3CODE_HOME")
        .map(|(_, value)| PathBuf::from(value))
        .or_else(|| std::env::var_os("T3CODE_HOME").map(PathBuf::from))
        .or_else(|| dirs::home_dir().map(|home| home.join(".t3")))
        .ok_or_else(|| "no home directory to find T3 Code's data in".to_string())
}

/// The running server's origin, from `<T3CODE_HOME>/userdata/server-runtime.json`.
fn server_origin(env: &[(String, String)]) -> Result<String, String> {
    let path = t3_home(env)?.join("userdata").join("server-runtime.json");
    let text = std::fs::read_to_string(&path).map_err(|err| format!("T3 Code's server isn't running ({}: {err})", path.display()))?;
    let state: serde_json::Value = serde_json::from_str(&text).map_err(|err| format!("{} isn't JSON: {err}", path.display()))?;
    state.get("origin").and_then(serde_json::Value::as_str).map(str::to_string).ok_or_else(|| format!("{} names no origin", path.display()))
}

/// A bearer session that may only operate threads, for two minutes.
fn issue_session(program: &Path, env: &[(String, String)], cwd: &Path) -> Result<(String, String), String> {
    let args = ["auth", "session", "issue", "--scope", "orchestration:operate", "--ttl", "2m", "--label", "lotar thread command", "--json"];
    let output = t3(program, env, cwd, &args)?;
    let issued: serde_json::Value = serde_json::from_slice(&output).map_err(|err| format!("`t3 auth session issue` printed no JSON: {err}"))?;
    let field = |name: &str| issued.get(name).and_then(serde_json::Value::as_str).map(str::to_string);
    field("sessionId").zip(field("token")).ok_or_else(|| "`t3 auth session issue` printed no sessionId and token".to_string())
}

/// How long the session a list reuses lasts, and how long before its end a new one is issued.
const LIST_SESSION_TTL: Duration = Duration::from_secs(60 * 60);
const LIST_SESSION_MARGIN: Duration = Duration::from_secs(5 * 60);

/// The read-only session lists reuse, by T3 Code data directory: its token and when it is replaced.
static LIST_SESSIONS: LazyLock<Mutex<std::collections::HashMap<PathBuf, (String, Instant)>>> = LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

/// The token of the reused read-only session, issued when there is none or it ends soon.
fn list_session(program: &Path, env: &[(String, String)], cwd: &Path, home: &Path) -> Result<String, String> {
    if let Some((token, _)) = LIST_SESSIONS.lock().ok().and_then(|sessions| sessions.get(home).filter(|(_, until)| Instant::now() < *until).cloned()) {
        return Ok(token);
    }
    let ttl = format!("{}m", LIST_SESSION_TTL.as_secs() / 60);
    let args = ["auth", "session", "issue", "--scope", "orchestration:read", "--ttl", &ttl, "--label", "lotar thread list", "--json"];
    let output = t3(program, env, cwd, &args)?;
    let issued: serde_json::Value = serde_json::from_slice(&output).map_err(|err| format!("`t3 auth session issue` printed no JSON: {err}"))?;
    let token = issued.get("token").and_then(serde_json::Value::as_str).ok_or("`t3 auth session issue` printed no token")?.to_string();
    if let Ok(mut sessions) = LIST_SESSIONS.lock() {
        sessions.insert(home.to_path_buf(), (token.clone(), Instant::now() + LIST_SESSION_TTL - LIST_SESSION_MARGIN));
    }
    Ok(token)
}

/// The unsettled threads, and the recently settled ones, of the T3 Code project at `project` (the deepest one whose
/// folder holds it), as `t3 thread list --json` prints them: `threadId`, `status`, `latestRunId`, `link`,
/// `settled`, `title` and `runtimeMode`. Subagents are left out, as the CLI leaves them out.
pub(crate) fn list_threads(program: &Path, env: &[(String, String)], cwd: &Path, project: &Path) -> Result<Vec<serde_json::Value>, String> {
    let home = t3_home(env)?;
    let origin = server_origin(env)?;
    let token = list_session(program, env, cwd, &home)?;
    let socket = connect(&origin, &token).inspect_err(|_| {
        // A refused session (revoked, expired early) is issued again at the next list.
        if let Ok(mut sessions) = LIST_SESSIONS.lock() {
            sessions.remove(&home);
        }
    })?;
    let snapshot = shell_snapshot(socket)?;
    let environment = std::fs::read_to_string(home.join("userdata").join("environment-id")).ok().map(|id| id.trim().to_string()).filter(|id| !id.is_empty());
    Ok(project_threads(&snapshot, project, environment.as_deref()))
}

/// The first snapshot `orchestration.subscribeShell` sends, without older settled threads and idle subagents.
fn shell_snapshot(mut socket: Socket) -> Result<serde_json::Value, String> {
    const WHAT: &str = "orchestration.subscribeShell";
    let payload = serde_json::json!({"omitOlderSettled": true, "omitIdleSubagents": true});
    let request = serde_json::json!({"_tag": "Request", "id": "1", "tag": "orchestration.subscribeShell", "payload": payload, "headers": []});
    socket.send(Message::text(request.to_string())).map_err(|err| format!("could not ask for T3 Code's threads: {err}"))?;
    let started = Instant::now();
    let answer = loop {
        if started.elapsed() > TIMEOUT {
            break Err(format!("T3 Code's server didn't answer {WHAT}"));
        }
        let message = match read_message(&mut socket, WHAT) {
            Ok(Some(message)) => message,
            Ok(None) => continue,
            Err(err) => break Err(err),
        };
        match message.get("_tag").and_then(serde_json::Value::as_str) {
            Some("Chunk") => {
                let values = message.get("values").and_then(serde_json::Value::as_array).into_iter().flatten();
                if let Some(snapshot) =
                    values.filter(|value| value.get("kind").and_then(serde_json::Value::as_str) == Some("snapshot")).find_map(|value| value.get("snapshot"))
                {
                    break Ok(snapshot.clone());
                }
            }
            Some("Exit") => break exit_result(WHAT, &message).and(Err(format!("T3 Code's server ended {WHAT} without a snapshot"))),
            _ => {}
        }
    };
    let _ = socket.close(None);
    answer
}

/// The threads of the snapshot's project at `project`, shaped as `t3 thread list --json` prints them.
fn project_threads(snapshot: &serde_json::Value, project: &Path, environment: Option<&str>) -> Vec<serde_json::Value> {
    let text = |value: &serde_json::Value, key: &str| value.get(key).and_then(serde_json::Value::as_str).map(str::to_string);
    let real = |path: &str| std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
    let target = std::fs::canonicalize(project).unwrap_or_else(|_| project.to_path_buf());
    let projects = snapshot.get("projects").and_then(serde_json::Value::as_array).into_iter().flatten();
    let Some(project_id) = projects
        .filter_map(|shown| Some((text(shown, "id")?, real(&text(shown, "workspaceRoot")?))))
        .filter(|(_, root)| target.starts_with(root))
        .max_by_key(|(_, root)| root.as_os_str().len())
        .map(|(id, _)| id)
    else {
        return Vec::new();
    };
    let threads = snapshot.get("threads").and_then(serde_json::Value::as_array).into_iter().flatten();
    threads
        .filter(|thread| text(thread, "projectId").as_deref() == Some(project_id.as_str()))
        .filter(|thread| thread.pointer("/lineage/relationshipToParent").and_then(serde_json::Value::as_str) != Some("subagent"))
        .filter_map(|thread| {
            let id = text(thread, "id")?;
            let link = environment.map(|environment| format!("https://app.t3.codes/threads/{}/{}", hosted_segment(environment), hosted_segment(&id)));
            Some(serde_json::json!({
                "threadId": id,
                "status": listed_status(thread),
                "latestRunId": thread.get("latestRunId").cloned().unwrap_or(serde_json::Value::Null),
                "link": link,
                "settled": text(thread, "settledOverride").as_deref() == Some("settled"),
                "title": text(thread, "title").unwrap_or_default(),
                "runtimeMode": text(thread, "runtimeMode").unwrap_or_default(),
            }))
        })
        .collect()
}

/// A thread's status as T3 Code's thread list derives it: the active run's, else `failed`, else `waiting` while
/// background work that holds completion (a subagent, an awaited or background task) is open, else the shell's.
fn listed_status(thread: &serde_json::Value) -> String {
    let text = |key: &str| thread.get(key).and_then(serde_json::Value::as_str);
    if let Some(active) = text("activityRunStatus") {
        return active.to_string();
    }
    let status = text("status").unwrap_or_default();
    let holds = thread.get("pendingBackgroundTasks").and_then(serde_json::Value::as_array).into_iter().flatten().any(|task| {
        task.get("awaited").and_then(serde_json::Value::as_bool) == Some(true)
            || matches!(task.get("kind").and_then(serde_json::Value::as_str), Some("subagent" | "background_task"))
    });
    if status != "failed" && holds { "waiting".to_string() } else { status.to_string() }
}

/// A path segment of T3 Code's hosted thread link: URI-encoded, also `.!~*'()`.
fn hosted_segment(id: &str) -> String {
    let mut segment = String::new();
    for byte in id.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_') {
            segment.push(char::from(byte));
        } else {
            segment.push_str(&format!("%{byte:02X}"));
        }
    }
    segment
}

/// Revoke the session; it expires within two minutes if this fails.
fn revoke_session(program: &Path, env: &[(String, String)], cwd: &Path, session_id: &str) {
    let _ = t3(program, env, cwd, &["auth", "session", "revoke", session_id]);
}

fn t3(program: &Path, env: &[(String, String)], cwd: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = Command::new(program)
        .args(args)
        .envs(env.iter().map(|(key, value)| (key, value)))
        .current_dir(cwd)
        .stdin(Stdio::null())
        .output()
        .map_err(|err| format!("could not run `t3 {}`: {err}", args.join(" ")))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(format!("`t3 {}` failed: {}", args[..3.min(args.len())].join(" "), String::from_utf8_lossy(&output.stderr).trim()))
    }
}

type Socket = tungstenite::WebSocket<TcpStream>;

/// A WebSocket to the server's `/ws`, signed in with `token`.
fn connect(origin: &str, token: &str) -> Result<Socket, String> {
    let url = format!("{}/ws?orchestrationProtocol=2", origin.trim_end_matches('/').replacen("http", "ws", 1));
    let mut request = url.as_str().into_client_request().map_err(|err| format!("bad T3 Code server address {url}: {err}"))?;
    let bearer = HeaderValue::from_str(&format!("Bearer {token}")).map_err(|err| format!("unusable session token: {err}"))?;
    request.headers_mut().insert("Authorization", bearer);
    let uri = request.uri();
    if uri.scheme_str() != Some("ws") {
        return Err(format!("T3 Code's server at {origin} isn't plain http, which lotar can't reach"));
    }
    let host = uri.host().unwrap_or("127.0.0.1").to_string();
    let port = uri.port_u16().unwrap_or(80);
    let address = (host.as_str(), port).to_socket_addrs().ok().and_then(|mut addresses| addresses.next()).ok_or_else(|| format!("can't resolve {host}"))?;
    let stream = TcpStream::connect_timeout(&address, TIMEOUT).map_err(|err| format!("can't reach T3 Code's server at {origin}: {err}"))?;
    stream.set_read_timeout(Some(TIMEOUT)).and_then(|()| stream.set_write_timeout(Some(TIMEOUT))).map_err(|err| err.to_string())?;
    let (socket, _) = tungstenite::client(request, stream).map_err(|err| format!("T3 Code's server refused the WebSocket: {err}"))?;
    Ok(socket)
}

/// The next JSON message, `None` for a ping (answered) or other frame; a `Defect`, a close or a timeout is an error,
/// which names the request `what` waits for.
fn read_message(socket: &mut Socket, what: &str) -> Result<Option<serde_json::Value>, String> {
    let text = match socket.read() {
        Ok(Message::Text(text)) => text,
        Ok(Message::Close(_)) => return Err("T3 Code's server closed the WebSocket before it answered".to_string()),
        Ok(_) => return Ok(None),
        Err(tungstenite::Error::Io(err)) if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
            return Err(format!("T3 Code's server didn't answer {what}"));
        }
        Err(err) => return Err(format!("the WebSocket to T3 Code's server failed: {err}")),
    };
    let Ok(message) = serde_json::from_str::<serde_json::Value>(text.as_str()) else { return Ok(None) };
    match message.get("_tag").and_then(serde_json::Value::as_str) {
        Some("Ping") => {
            let _ = socket.send(Message::text(r#"{"_tag":"Pong"}"#));
            Ok(None)
        }
        Some("Defect") => Err(format!("T3 Code's server failed: {}", message.get("defect").unwrap_or(&serde_json::Value::Null))),
        _ => Ok(Some(message)),
    }
}

/// Send `command` as `orchestration.dispatchCommand` and wait for its answer.
fn dispatch(origin: &str, token: &str, command: &serde_json::Value) -> Result<(), String> {
    let kind = command.get("type").and_then(serde_json::Value::as_str).unwrap_or("command");
    let mut socket = connect(origin, token)?;
    let request = serde_json::json!({"_tag": "Request", "id": "1", "tag": "orchestration.dispatchCommand", "payload": command, "headers": []});
    socket.send(Message::text(request.to_string())).map_err(|err| format!("could not send {kind}: {err}"))?;
    let started = Instant::now();
    let answer = loop {
        if started.elapsed() > TIMEOUT {
            break Err(format!("T3 Code's server didn't answer {kind}"));
        }
        match read_message(&mut socket, kind) {
            Ok(Some(message))
                if message.get("_tag").and_then(serde_json::Value::as_str) == Some("Exit")
                    && message.get("requestId").and_then(serde_json::Value::as_str) == Some("1") =>
            {
                break exit_result(kind, &message);
            }
            Ok(_) => {}
            Err(err) => break Err(err),
        }
    };
    let _ = socket.close(None);
    answer
}

/// `Ok` for a `Success` exit, else the failure T3 Code gave.
fn exit_result(kind: &str, message: &serde_json::Value) -> Result<(), String> {
    let exit = message.get("exit").unwrap_or(&serde_json::Value::Null);
    if exit.get("_tag").and_then(serde_json::Value::as_str) == Some("Success") {
        return Ok(());
    }
    let cause = exit.get("cause").unwrap_or(exit);
    let message = cause.as_array().and_then(|causes| causes.iter().find_map(|cause| cause.pointer("/error/message")?.as_str()));
    Err(format!("T3 Code refused {kind}: {}", message.map_or_else(|| cause.to_string(), str::to_string)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_success_exit_renamed_the_thread_and_a_failure_says_why() {
        let success = serde_json::json!({"_tag": "Exit", "requestId": "1", "exit": {"_tag": "Success", "value": {"sequence": 7}}});
        assert_eq!(exit_result("thread.settle", &success), Ok(()));
        let failure = serde_json::json!({"_tag": "Exit", "requestId": "1", "exit": {"_tag": "Failure", "cause": [
            {"_tag": "Fail", "error": {"_tag": "OrchestrationV2DispatchCommandError", "message": "No orchestration projection exists for thread t9."}}
        ]}});
        assert_eq!(exit_result("thread.settle", &failure), Err("T3 Code refused thread.settle: No orchestration projection exists for thread t9.".to_string()));
        let unknown = serde_json::json!({"_tag": "Exit", "requestId": "1", "exit": {"_tag": "Failure", "cause": {"_tag": "Die"}}});
        assert!(exit_result("thread.stop", &unknown).is_err_and(|err| err.contains("Die")));
    }

    #[test]
    fn the_rename_command_names_the_thread_and_title() {
        let command = rename_command("mcp:abc", "TEST-1 merge: Title");
        assert_eq!(command["type"], "thread.metadata.update");
        assert_eq!(command["threadId"], "mcp:abc");
        assert_eq!(command["title"], "TEST-1 merge: Title");
        assert!(command["commandId"].as_str().is_some_and(|id| id.starts_with("lotar:thread-rename:mcp:abc:")));
    }

    /// A server that checks the request the way T3 Code's `/ws` does and answers with `exit`.
    fn fake_server(exit: serde_json::Value) -> (String, std::thread::JoinHandle<serde_json::Value>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let check = |request: &tungstenite::handshake::server::Request, response| {
                assert_eq!(request.uri().query(), Some("orchestrationProtocol=2"));
                assert_eq!(request.headers().get("Authorization").and_then(|value| value.to_str().ok()), Some("Bearer secret"));
                Ok(response)
            };
            let mut socket = tungstenite::accept_hdr(stream, check).unwrap();
            socket.send(Message::text(r#"{"_tag":"Ping"}"#)).unwrap();
            let request: serde_json::Value = loop {
                if let Message::Text(text) = socket.read().unwrap()
                    && text.contains("Request")
                {
                    break serde_json::from_str(text.as_str()).unwrap();
                }
            };
            let answer = serde_json::json!({"_tag": "Exit", "requestId": request["id"], "exit": exit});
            socket.send(Message::text(answer.to_string())).unwrap();
            let _ = socket.read();
            request
        });
        (origin, server)
    }

    #[test]
    fn a_rename_is_dispatched_as_thread_metadata_update() {
        let (origin, server) = fake_server(serde_json::json!({"_tag": "Success", "value": {"sequence": 7}}));
        assert_eq!(dispatch(&origin, "secret", &rename_command("mcp:abc", "TEST-1 merge: Title")), Ok(()));
        let request = server.join().unwrap();
        assert_eq!(request["tag"], "orchestration.dispatchCommand");
        assert_eq!(request["payload"]["title"], "TEST-1 merge: Title");
    }

    #[test]
    fn settle_and_stop_are_dispatched_as_the_commands_t3s_app_sends() {
        let settle = command("thread.settle", "mcp:abc");
        assert_eq!(settle["type"], "thread.settle");
        assert_eq!(settle["threadId"], "mcp:abc");
        assert!(settle["commandId"].as_str().is_some_and(|id| id.starts_with("lotar:thread.settle:mcp:abc:")));
        let (origin, server) = fake_server(serde_json::json!({"_tag": "Success", "value": {"sequence": 8}}));
        assert_eq!(dispatch(&origin, "secret", &settle), Ok(()));
        assert_eq!(server.join().unwrap()["payload"]["type"], "thread.settle");
        let (origin, server) =
            fake_server(serde_json::json!({"_tag": "Failure", "cause": [{"_tag": "Fail", "error": {"message": "Thread mcp:abc is archived."}}]}));
        let refused = dispatch(&origin, "secret", &command("thread.stop", "mcp:abc"));
        assert_eq!(refused, Err("T3 Code refused thread.stop: Thread mcp:abc is archived.".to_string()));
        server.join().unwrap();
    }

    #[test]
    fn a_refused_or_unreachable_rename_says_so() {
        let (origin, server) = fake_server(serde_json::json!({"_tag": "Failure", "cause": [{"_tag": "Fail", "error": {"message": "no such thread"}}]}));
        assert!(dispatch(&origin, "secret", &rename_command("mcp:abc", "Title")).is_err_and(|err| err.contains("no such thread")));
        server.join().unwrap();
        let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
        assert!(dispatch(&format!("http://{closed}"), "secret", &rename_command("mcp:abc", "Title")).is_err_and(|err| err.contains("can't reach")));
    }

    #[test]
    fn the_server_origin_comes_from_t3codes_runtime_state() {
        let home = tempfile::tempdir().unwrap();
        let env = vec![("T3CODE_HOME".to_string(), home.path().to_string_lossy().to_string())];
        assert!(server_origin(&env).is_err_and(|err| err.contains("isn't running")));
        std::fs::create_dir_all(home.path().join("userdata")).unwrap();
        std::fs::write(home.path().join("userdata/server-runtime.json"), r#"{"origin":"http://127.0.0.1:3773","port":3773}"#).unwrap();
        assert_eq!(server_origin(&env).as_deref(), Ok("http://127.0.0.1:3773"));
    }

    /// A shell snapshot with a project, one nested in it, and threads of both: settled, running, a subagent, and one
    /// whose completed run still has a subagent open.
    fn snapshot(root: &Path) -> serde_json::Value {
        let nested = root.join("nested");
        std::fs::create_dir_all(nested.join("src")).unwrap();
        let thread = |id: &str, project: &str, extra: serde_json::Value| {
            let mut thread = serde_json::json!({"id": id, "projectId": project, "title": format!("TEST-1 {id}"), "status": "completed",
                "activityRunStatus": null, "latestRunId": "r1", "settledOverride": null, "runtimeMode": "full-access",
                "pendingBackgroundTasks": [], "lineage": {"relationshipToParent": null}});
            for (key, value) in extra.as_object().unwrap() {
                thread[key] = value.clone();
            }
            thread
        };
        serde_json::json!({
            "projects": [{"id": "p1", "workspaceRoot": root}, {"id": "p2", "workspaceRoot": nested}],
            "threads": [
                thread("settled", "p1", serde_json::json!({"settledOverride": "settled"})),
                thread("mcp:running", "p1", serde_json::json!({"status": "running", "activityRunStatus": "waiting", "latestRunId": "r2"})),
                thread("child", "p1", serde_json::json!({"lineage": {"relationshipToParent": "subagent"}})),
                thread("holds", "p1", serde_json::json!({"pendingBackgroundTasks": [{"taskId": "a", "kind": "subagent"}]})),
                thread("failed", "p1", serde_json::json!({"status": "failed", "pendingBackgroundTasks": [{"taskId": "a", "kind": "subagent"}]})),
                thread("monitor", "p1", serde_json::json!({"pendingBackgroundTasks": [{"taskId": "a", "kind": "monitor"}]})),
                thread("elsewhere", "p2", serde_json::json!({})),
            ],
        })
    }

    #[test]
    fn a_projects_threads_come_from_the_shell_snapshot_as_the_cli_lists_them() {
        let root = tempfile::tempdir().unwrap();
        let snapshot = snapshot(root.path());
        let threads = project_threads(&snapshot, root.path(), Some("env-1"));
        let by_id: std::collections::HashMap<&str, &serde_json::Value> = threads.iter().map(|thread| (thread["threadId"].as_str().unwrap(), thread)).collect();
        let mut ids: Vec<&str> = by_id.keys().copied().collect();
        ids.sort_unstable();
        assert_eq!(ids, ["failed", "holds", "mcp:running", "monitor", "settled"], "no subagent, no thread of the nested project");
        assert_eq!(by_id["settled"]["settled"], true);
        assert_eq!(by_id["mcp:running"]["settled"], false);
        assert_eq!(by_id["mcp:running"]["status"], "waiting", "the active run's status wins");
        assert_eq!(by_id["mcp:running"]["latestRunId"], "r2");
        assert_eq!(by_id["mcp:running"]["link"], "https://app.t3.codes/threads/env-1/mcp%3Arunning");
        assert_eq!(by_id["mcp:running"]["runtimeMode"], "full-access");
        assert_eq!(by_id["mcp:running"]["title"], "TEST-1 mcp:running");
        assert_eq!(by_id["holds"]["status"], "waiting", "an open subagent holds completion");
        assert_eq!(by_id["failed"]["status"], "failed");
        assert_eq!(by_id["monitor"]["status"], "completed", "a monitor doesn't hold completion");
        let nested = project_threads(&snapshot, &root.path().join("nested/src"), None);
        assert_eq!(nested.len(), 1, "the deepest project holding the path");
        assert_eq!(nested[0]["threadId"], "elsewhere");
        assert_eq!(nested[0]["link"], serde_json::Value::Null);
        assert!(project_threads(&snapshot, Path::new("/nowhere"), None).is_empty());
    }

    #[test]
    fn a_hosted_link_segment_is_encoded_as_t3_code_encodes_it() {
        assert_eq!(hosted_segment("mcp:launch:ab.c(1)"), "mcp%3Alaunch%3Aab%2Ec%281%29");
        assert_eq!(hosted_segment("d834978d-9e75_x"), "d834978d-9e75_x");
    }

    #[test]
    fn the_shell_snapshot_is_the_first_one_the_subscription_sends() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();
            let request: serde_json::Value = loop {
                if let Message::Text(text) = socket.read().unwrap() {
                    break serde_json::from_str(text.as_str()).unwrap();
                }
            };
            socket.send(Message::text(r#"{"_tag":"Ping"}"#)).unwrap();
            let chunk = |values: serde_json::Value| serde_json::json!({"_tag": "Chunk", "requestId": "1", "values": values}).to_string();
            socket.send(Message::text(chunk(serde_json::json!([{"kind": "snapshot", "snapshot": {"threads": [], "projects": [{"id": "p1"}]}}])))).unwrap();
            socket.send(Message::text(chunk(serde_json::json!([{"kind": "synchronized"}])))).unwrap();
            let _ = socket.read();
            request
        });
        let snapshot = shell_snapshot(connect(&origin, "secret").unwrap()).unwrap();
        assert_eq!(snapshot["projects"][0]["id"], "p1");
        let request = server.join().unwrap();
        assert_eq!(request["tag"], "orchestration.subscribeShell");
        assert_eq!(request["payload"]["omitOlderSettled"], true);
    }

    #[test]
    fn a_subscription_that_ends_without_a_snapshot_says_so() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();
            let _ = socket.read();
            let exit = serde_json::json!({"_tag": "Exit", "requestId": "1", "exit": {"_tag": "Failure", "cause": [{"_tag": "Fail", "error": {"message": "unknown method"}}]}});
            socket.send(Message::text(exit.to_string())).unwrap();
            let _ = socket.read();
        });
        let refused = shell_snapshot(connect(&origin, "secret").unwrap());
        assert!(refused.is_err_and(|err| err.contains("unknown method")));
        server.join().unwrap();
    }
}
