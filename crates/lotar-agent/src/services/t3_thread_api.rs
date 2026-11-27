//! Renames, settles and stops T3 Code threads through the T3 Code server's
//! internal WebSocket API, since the `t3` CLI can't (DEV-619, DEV-836). T3 Code
//! doesn't document this API and can change it in any release: the owner chose
//! it anyway and checks it after each T3 Code update (2026-11-17, "Implement
//! with the internal API").
//!
//! The route is the one T3 Code's own app takes: a short-lived bearer session
//! from `t3 auth session issue`, a WebSocket to `/ws`, and the RPC
//! `orchestration.dispatchCommand` with a `thread.metadata.update`,
//! `thread.settle` or `thread.stop` command.
//!
//! A thread of a job on an agent host (DEV-901) is on that host's T3 Code:
//! its runtime state is read, and the session issued and revoked, over ssh,
//! and the WebSocket goes through an ssh forward that lasts the command.

use std::io::ErrorKind;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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

/// The running server's origin, from `<T3CODE_HOME>/userdata/server-runtime.json`.
fn server_origin(env: &[(String, String)]) -> Result<String, String> {
    let home = env
        .iter()
        .rev()
        .find(|(key, _)| key == "T3CODE_HOME")
        .map(|(_, value)| PathBuf::from(value))
        .or_else(|| std::env::var_os("T3CODE_HOME").map(PathBuf::from))
        .or_else(|| dirs::home_dir().map(|home| home.join(".t3")))
        .ok_or("no home directory to find T3 Code's data in")?;
    let path = home.join("userdata").join("server-runtime.json");
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

/// Send `command` as `orchestration.dispatchCommand` and wait for its answer.
fn dispatch(origin: &str, token: &str, command: &serde_json::Value) -> Result<(), String> {
    let kind = command.get("type").and_then(serde_json::Value::as_str).unwrap_or("command");
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
    let (mut socket, _) = tungstenite::client(request, stream).map_err(|err| format!("T3 Code's server refused the WebSocket: {err}"))?;
    let request = serde_json::json!({"_tag": "Request", "id": "1", "tag": "orchestration.dispatchCommand", "payload": command, "headers": []});
    socket.send(Message::text(request.to_string())).map_err(|err| format!("could not send {kind}: {err}"))?;
    let started = Instant::now();
    let answer = loop {
        if started.elapsed() > TIMEOUT {
            break Err(format!("T3 Code's server didn't answer {kind}"));
        }
        let text = match socket.read() {
            Ok(Message::Text(text)) => text,
            Ok(Message::Close(_)) => break Err("T3 Code's server closed the WebSocket before it answered".to_string()),
            Ok(_) => continue,
            Err(tungstenite::Error::Io(err)) if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                break Err(format!("T3 Code's server didn't answer {kind}"));
            }
            Err(err) => break Err(format!("the WebSocket to T3 Code's server failed: {err}")),
        };
        let Ok(message) = serde_json::from_str::<serde_json::Value>(text.as_str()) else { continue };
        match message.get("_tag").and_then(serde_json::Value::as_str) {
            Some("Ping") => {
                let _ = socket.send(Message::text(r#"{"_tag":"Pong"}"#));
            }
            Some("Exit") if message.get("requestId").and_then(serde_json::Value::as_str) == Some("1") => break exit_result(kind, &message),
            Some("Defect") => break Err(format!("T3 Code's server failed: {}", message.get("defect").unwrap_or(&serde_json::Value::Null))),
            _ => {}
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
}
