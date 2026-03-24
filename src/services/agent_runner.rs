use crate::config::types::AgentProfileDetail;
use crate::errors::{LoTaRError, LoTaRResult};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunnerKind {
    Codex,
    Claude,
    Copilot,
    Gemini,
    Command,
}

impl AgentRunnerKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Copilot => "copilot",
            Self::Gemini => "gemini",
            Self::Command => "command",
        }
    }
}

impl FromStr for AgentRunnerKind {
    type Err = ();

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().to_lowercase().as_str() {
            "codex" => Ok(Self::Codex),
            "claude" => Ok(Self::Claude),
            "copilot" => Ok(Self::Copilot),
            "gemini" => Ok(Self::Gemini),
            "command" => Ok(Self::Command),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RunnerCommand {
    pub program: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerEventKind {
    Init,
    Progress,
    Message,
    Result,
}

#[derive(Debug, Clone)]
pub struct RunnerEvent {
    pub kind: RunnerEventKind,
    pub text: Option<String>,
    pub session_id: Option<String>,
    pub payload: Option<JsonValue>,
}

pub fn build_runner_command(
    kind: AgentRunnerKind,
    profile: &AgentProfileDetail,
    prompt: &str,
) -> LoTaRResult<RunnerCommand> {
    if kind == AgentRunnerKind::Command {
        let program = profile.command.clone().ok_or_else(|| {
            LoTaRError::ValidationError(
                "Command runner requires 'command' in agent profile".to_string(),
            )
        })?;
        let args = profile.args.clone();
        // The prompt is not appended as an argument for command runners;
        // it is available via the LOTAR_AGENT_PROMPT env var instead.
        let mut env = expand_env_map(&profile.env);
        env.insert("LOTAR_AGENT_PROMPT".to_string(), prompt.to_string());
        return Ok(RunnerCommand { program, args, env });
    }

    let program = profile
        .command
        .clone()
        .unwrap_or_else(|| kind.as_str().to_string());

    let mut args: Vec<String> = match kind {
        AgentRunnerKind::Codex => vec!["exec".to_string(), "--json".to_string()],
        AgentRunnerKind::Claude => vec![
            "-p".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--input-format".to_string(),
            "stream-json".to_string(),
            "--verbose".to_string(),
            "--include-partial-messages".to_string(),
        ],
        AgentRunnerKind::Copilot => vec![
            "-p".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--input-format".to_string(),
            "stream-json".to_string(),
        ],
        AgentRunnerKind::Gemini => vec!["--output-format".to_string(), "stream-json".to_string()],
        AgentRunnerKind::Command => unreachable!("handled above"),
    };

    if !profile.args.is_empty() {
        args.extend(profile.args.iter().cloned());
    }

    args.push(prompt.to_string());

    let env = expand_env_map(&profile.env);

    Ok(RunnerCommand { program, args, env })
}

pub fn parse_runner_line(kind: AgentRunnerKind, line: &str) -> Option<RunnerEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    match kind {
        AgentRunnerKind::Codex => parse_codex_line(trimmed),
        AgentRunnerKind::Claude => parse_claude_line(trimmed),
        AgentRunnerKind::Copilot => parse_copilot_line(trimmed),
        AgentRunnerKind::Gemini => parse_gemini_line(trimmed),
        AgentRunnerKind::Command => Some(RunnerEvent {
            kind: RunnerEventKind::Progress,
            text: Some(trimmed.to_string()),
            session_id: None,
            payload: None,
        }),
    }
}

fn parse_codex_line(line: &str) -> Option<RunnerEvent> {
    let payload: JsonValue = serde_json::from_str(line).ok()?;
    let event_type = payload.get("type").and_then(|v| v.as_str())?;
    let session_id = payload
        .get("thread_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    match event_type {
        "thread.started" => Some(RunnerEvent {
            kind: RunnerEventKind::Init,
            text: None,
            session_id,
            payload: Some(payload),
        }),
        "item.completed" => {
            let item = payload.get("item");
            let item_type = item.and_then(|v| v.get("type")).and_then(|v| v.as_str());
            let text = item
                .and_then(|v| v.get("text"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let kind = match item_type {
                Some("agent_message") => RunnerEventKind::Message,
                _ => RunnerEventKind::Progress,
            };

            Some(RunnerEvent {
                kind,
                text,
                session_id,
                payload: Some(payload),
            })
        }
        "turn.completed" => Some(RunnerEvent {
            kind: RunnerEventKind::Result,
            text: None,
            session_id,
            payload: Some(payload),
        }),
        _ => None,
    }
}

fn parse_claude_line(line: &str) -> Option<RunnerEvent> {
    let payload: JsonValue = serde_json::from_str(line).ok()?;
    let event_type = payload.get("type").and_then(|v| v.as_str())?;
    let session_id = payload
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    match event_type {
        "system" => {
            let subtype = payload.get("subtype").and_then(|v| v.as_str());
            if subtype == Some("init") {
                Some(RunnerEvent {
                    kind: RunnerEventKind::Init,
                    text: None,
                    session_id,
                    payload: Some(payload),
                })
            } else {
                None
            }
        }
        "stream_event" => {
            let event = payload.get("event");
            let event_type = event.and_then(|v| v.get("type")).and_then(|v| v.as_str());
            if event_type == Some("content_block_delta") {
                let text = event
                    .and_then(|v| v.get("delta"))
                    .and_then(|v| v.get("text"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                Some(RunnerEvent {
                    kind: RunnerEventKind::Progress,
                    text,
                    session_id,
                    payload: Some(payload),
                })
            } else {
                None
            }
        }
        "assistant" => {
            let text = payload
                .get("message")
                .and_then(|v| v.get("content"))
                .and_then(|v| v.get(0))
                .and_then(|v| v.get("text"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            Some(RunnerEvent {
                kind: RunnerEventKind::Message,
                text,
                session_id,
                payload: Some(payload),
            })
        }
        "result" => Some(RunnerEvent {
            kind: RunnerEventKind::Result,
            text: payload
                .get("result")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            session_id,
            payload: Some(payload),
        }),
        _ => None,
    }
}

fn parse_copilot_line(line: &str) -> Option<RunnerEvent> {
    let payload: JsonValue = serde_json::from_str(line).ok()?;
    let event_type = payload.get("type").and_then(|v| v.as_str())?;
    let session_id = payload
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    match event_type {
        "system" => {
            let subtype = payload.get("subtype").and_then(|v| v.as_str());
            if subtype == Some("init") {
                Some(RunnerEvent {
                    kind: RunnerEventKind::Init,
                    text: None,
                    session_id,
                    payload: Some(payload),
                })
            } else {
                None
            }
        }
        "message" => {
            let text = payload
                .get("content")
                .and_then(|v| v.as_str())
                .or_else(|| payload.get("text").and_then(|v| v.as_str()))
                .map(|s| s.to_string());
            let kind = if payload
                .get("delta")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                RunnerEventKind::Progress
            } else {
                RunnerEventKind::Message
            };
            Some(RunnerEvent {
                kind,
                text,
                session_id,
                payload: Some(payload),
            })
        }
        "result" => Some(RunnerEvent {
            kind: RunnerEventKind::Result,
            text: payload
                .get("result")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            session_id,
            payload: Some(payload),
        }),
        _ => None,
    }
}

fn parse_gemini_line(line: &str) -> Option<RunnerEvent> {
    if !line.trim_start().starts_with('{') {
        return None;
    }
    let payload: JsonValue = serde_json::from_str(line).ok()?;
    let event_type = payload.get("type").and_then(|v| v.as_str())?;
    let session_id = payload
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    match event_type {
        "init" => Some(RunnerEvent {
            kind: RunnerEventKind::Init,
            text: None,
            session_id,
            payload: Some(payload),
        }),
        "message" => {
            let text = payload
                .get("content")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let kind = if payload
                .get("delta")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                RunnerEventKind::Progress
            } else {
                RunnerEventKind::Message
            };
            Some(RunnerEvent {
                kind,
                text,
                session_id,
                payload: Some(payload),
            })
        }
        "result" => Some(RunnerEvent {
            kind: RunnerEventKind::Result,
            text: None,
            session_id,
            payload: Some(payload),
        }),
        _ => None,
    }
}

fn expand_env_map(env: &HashMap<String, String>) -> HashMap<String, String> {
    env.iter()
        .map(|(key, value)| (key.clone(), expand_env_value(value)))
        .collect()
}

fn expand_env_value(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(stripped) = trimmed.strip_prefix("${").and_then(|v| v.strip_suffix('}')) {
        return std::env::var(stripped).unwrap_or_default();
    }
    if let Some(stripped) = trimmed.strip_prefix('$') {
        return std::env::var(stripped).unwrap_or_else(|_| trimmed.to_string());
    }
    raw.to_string()
}

pub fn validate_runner_command(cmd: &RunnerCommand) -> LoTaRResult<()> {
    if cmd.program.trim().is_empty() {
        return Err(LoTaRError::ValidationError(
            "Runner command cannot be empty".to_string(),
        ));
    }
    Ok(())
}

/// Returns whether stdin should be piped for this runner kind.
/// When true, the runner accepts user messages via stdin.
pub fn supports_stdin(kind: AgentRunnerKind) -> bool {
    match kind {
        AgentRunnerKind::Claude | AgentRunnerKind::Copilot => true,
        AgentRunnerKind::Codex | AgentRunnerKind::Gemini | AgentRunnerKind::Command => false,
    }
}

/// Format a user message for the runner's stdin protocol.
/// Returns the bytes to write (including trailing newline).
pub fn format_stdin_message(kind: AgentRunnerKind, message: &str) -> Option<Vec<u8>> {
    match kind {
        AgentRunnerKind::Claude | AgentRunnerKind::Copilot => {
            // stream-json input format:
            // {"type":"user","message":{"role":"user","content":"<text>"}}
            let obj = serde_json::json!({
                "type": "user",
                "message": {
                    "role": "user",
                    "content": message,
                }
            });
            let mut bytes = serde_json::to_vec(&obj).ok()?;
            bytes.push(b'\n');
            Some(bytes)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_claude_stream_event_delta() {
        let line = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}},"session_id":"test-123"}"#;
        let event = parse_runner_line(AgentRunnerKind::Claude, line);
        assert!(event.is_some(), "Expected Some, got None for stream_event");
        let event = event.unwrap();
        assert_eq!(event.kind, RunnerEventKind::Progress);
        assert_eq!(event.text.as_deref(), Some("Hello"));
        assert_eq!(event.session_id.as_deref(), Some("test-123"));
    }

    #[test]
    fn parse_claude_init() {
        let line = r#"{"type":"system","subtype":"init","session_id":"abc-123"}"#;
        let event = parse_runner_line(AgentRunnerKind::Claude, line);
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.kind, RunnerEventKind::Init);
        assert_eq!(event.session_id.as_deref(), Some("abc-123"));
    }

    #[test]
    fn parse_claude_assistant_message() {
        let line = r#"{"type":"assistant","message":{"content":[{"text":"Hello world"}]},"session_id":"abc"}"#;
        let event = parse_runner_line(AgentRunnerKind::Claude, line);
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.kind, RunnerEventKind::Message);
        assert_eq!(event.text.as_deref(), Some("Hello world"));
    }

    #[test]
    fn parse_claude_result() {
        let line = r#"{"type":"result","result":"Done!","session_id":"abc"}"#;
        let event = parse_runner_line(AgentRunnerKind::Claude, line);
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.kind, RunnerEventKind::Result);
        assert_eq!(event.text.as_deref(), Some("Done!"));
    }

    #[test]
    fn parse_copilot_init() {
        let line = r#"{"type":"system","subtype":"init","session_id":"cp-123"}"#;
        let event = parse_runner_line(AgentRunnerKind::Copilot, line);
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.kind, RunnerEventKind::Init);
        assert_eq!(event.session_id.as_deref(), Some("cp-123"));
    }

    #[test]
    fn parse_copilot_message() {
        let line = r#"{"type":"message","content":"Hello from copilot","session_id":"cp-1"}"#;
        let event = parse_runner_line(AgentRunnerKind::Copilot, line);
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.kind, RunnerEventKind::Message);
        assert_eq!(event.text.as_deref(), Some("Hello from copilot"));
    }

    #[test]
    fn parse_copilot_progress_delta() {
        let line = r#"{"type":"message","content":"chunk","delta":true,"session_id":"cp-1"}"#;
        let event = parse_runner_line(AgentRunnerKind::Copilot, line);
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.kind, RunnerEventKind::Progress);
        assert_eq!(event.text.as_deref(), Some("chunk"));
    }

    #[test]
    fn parse_copilot_result() {
        let line = r#"{"type":"result","result":"All done","session_id":"cp-1"}"#;
        let event = parse_runner_line(AgentRunnerKind::Copilot, line);
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.kind, RunnerEventKind::Result);
        assert_eq!(event.text.as_deref(), Some("All done"));
    }

    #[test]
    fn supports_stdin_for_claude_and_copilot() {
        assert!(supports_stdin(AgentRunnerKind::Claude));
        assert!(supports_stdin(AgentRunnerKind::Copilot));
        assert!(!supports_stdin(AgentRunnerKind::Codex));
        assert!(!supports_stdin(AgentRunnerKind::Gemini));
        assert!(!supports_stdin(AgentRunnerKind::Command));
    }

    #[test]
    fn format_stdin_message_claude() {
        let msg = format_stdin_message(AgentRunnerKind::Claude, "hello world").unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&msg).unwrap();
        assert_eq!(parsed["type"], "user");
        assert_eq!(parsed["message"]["role"], "user");
        assert_eq!(parsed["message"]["content"], "hello world");
    }

    #[test]
    fn format_stdin_message_unsupported() {
        assert!(format_stdin_message(AgentRunnerKind::Codex, "hello").is_none());
        assert!(format_stdin_message(AgentRunnerKind::Command, "hello").is_none());
    }
}
