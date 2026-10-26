use crate::config::types::{AgentPermissions, AgentProfileDetail};
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
    /// Bytes to write to the runner's stdin right after it starts (Claude
    /// reads its first prompt from stdin in stream-json input mode).
    pub initial_input: Option<Vec<u8>>,
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
    /// The runner reported a failure (an error result, a failed turn), even
    /// if the process later exits 0.
    pub failed: bool,
}

impl RunnerEvent {
    fn new(kind: RunnerEventKind, text: Option<String>, session_id: Option<String>) -> Self {
        Self {
            kind,
            text,
            session_id,
            payload: None,
            failed: false,
        }
    }

    fn with_payload(mut self, payload: &JsonValue) -> Self {
        self.payload = Some(payload.clone());
        self
    }

    fn failing(mut self) -> Self {
        self.failed = true;
        self
    }
}

/// Claude Code settings for the `workspace` level: shell commands run inside
/// the OS sandbox without prompting, so writes outside the working directory
/// (and its allowed directories) are blocked.
const CLAUDE_SANDBOX_SETTINGS: &str =
    r#"{"sandbox":{"enabled":true,"autoAllowBashIfSandboxed":true}}"#;

/// True when the profile's own args already choose a permission mode, so
/// LoTaR must not add its defaults (they could conflict).
fn args_set_permissions(kind: AgentRunnerKind, args: &[String]) -> bool {
    let flags: &[&str] = match kind {
        AgentRunnerKind::Claude => &[
            "--permission-mode",
            "--dangerously-skip-permissions",
            "--allow-dangerously-skip-permissions",
        ],
        AgentRunnerKind::Codex => &[
            "-s",
            "--sandbox",
            "--full-auto",
            "--approve-for-me",
            "--dangerously-bypass-approvals-and-sandbox",
        ],
        AgentRunnerKind::Copilot => &[
            "--allow-all-tools",
            "--allow-all",
            "--yolo",
            "--allow-tool",
            "--deny-tool",
        ],
        AgentRunnerKind::Gemini => &["--approval-mode", "-y", "--yolo"],
        AgentRunnerKind::Command => return true,
    };
    args.iter().any(|arg| {
        let name = arg.split('=').next().unwrap_or(arg);
        flags.contains(&name)
    })
}

/// Each CLI's flags for a permission level. `writable_dirs` are extra
/// directories the `workspace` level may write besides the working directory
/// (the tasks directory and git data of a worktree job).
fn permission_args(
    kind: AgentRunnerKind,
    level: AgentPermissions,
    profile_args: &[String],
    writable_dirs: &[std::path::PathBuf],
) -> Vec<String> {
    let mut args = level_args(kind, level, profile_args);
    if level == AgentPermissions::Workspace {
        let flag = match kind {
            AgentRunnerKind::Gemini => "--include-directories",
            AgentRunnerKind::Command => return args,
            _ => "--add-dir",
        };
        for dir in writable_dirs {
            args.extend([flag.to_string(), dir.display().to_string()]);
        }
    }
    args
}

fn level_args(
    kind: AgentRunnerKind,
    level: AgentPermissions,
    profile_args: &[String],
) -> Vec<String> {
    let owned = |items: &[&str]| {
        items
            .iter()
            .map(|item| item.to_string())
            .collect::<Vec<_>>()
    };
    match (kind, level) {
        (AgentRunnerKind::Command, _) => Vec::new(),
        (AgentRunnerKind::Claude, AgentPermissions::ReadOnly) => {
            owned(&["--permission-mode", "plan"])
        }
        (AgentRunnerKind::Claude, AgentPermissions::Workspace) => {
            let mut args = owned(&["--permission-mode", "acceptEdits"]);
            // A profile that brings its own settings keeps them as they are.
            if !profile_args
                .iter()
                .any(|arg| arg == "--settings" || arg.starts_with("--settings="))
            {
                args.extend([
                    "--settings".to_string(),
                    CLAUDE_SANDBOX_SETTINGS.to_string(),
                ]);
            }
            args
        }
        (AgentRunnerKind::Claude, AgentPermissions::Full) => {
            owned(&["--permission-mode", "bypassPermissions"])
        }
        (AgentRunnerKind::Codex, AgentPermissions::ReadOnly) => owned(&["--sandbox", "read-only"]),
        (AgentRunnerKind::Codex, AgentPermissions::Workspace) => {
            owned(&["--sandbox", "workspace-write"])
        }
        (AgentRunnerKind::Codex, AgentPermissions::Full) => {
            owned(&["--dangerously-bypass-approvals-and-sandbox"])
        }
        (AgentRunnerKind::Copilot, AgentPermissions::ReadOnly) => owned(&[
            "--allow-all-tools",
            "--deny-tool=write",
            "--deny-tool=shell",
        ]),
        // Copilot has no sandbox: its file tools stay limited to the working
        // directory, shell commands are not. Pushing stays a human decision.
        (AgentRunnerKind::Copilot, AgentPermissions::Workspace) => {
            owned(&["--allow-all-tools", "--deny-tool=shell(git push)"])
        }
        (AgentRunnerKind::Copilot, AgentPermissions::Full) => owned(&["--allow-all"]),
        (AgentRunnerKind::Gemini, AgentPermissions::ReadOnly) => {
            owned(&["--approval-mode", "plan"])
        }
        (AgentRunnerKind::Gemini, AgentPermissions::Workspace) => {
            owned(&["--approval-mode", "auto_edit"])
        }
        (AgentRunnerKind::Gemini, AgentPermissions::Full) => owned(&["--approval-mode", "yolo"]),
    }
}

pub fn build_runner_command(
    kind: AgentRunnerKind,
    profile: &AgentProfileDetail,
    prompt: &str,
    writable_dirs: &[std::path::PathBuf],
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
        if let Some(model) = profile.model.as_deref() {
            env.insert("LOTAR_AGENT_MODEL".to_string(), model.to_string());
        }
        if let Some(effort) = profile.effort.as_deref() {
            env.insert("LOTAR_AGENT_EFFORT".to_string(), effort.to_string());
        }
        return Ok(RunnerCommand {
            program,
            args,
            env,
            initial_input: None,
        });
    }

    let program = profile
        .command
        .clone()
        .unwrap_or_else(|| kind.as_str().to_string());
    let owned = |items: &[&str]| {
        items
            .iter()
            .map(|item| item.to_string())
            .collect::<Vec<_>>()
    };

    // Base flags verified against claude 2.1.308, codex-cli 0.163, copilot
    // 1.0.69 and the Gemini CLI headless docs (DEV-199).
    let mut args: Vec<String> = match kind {
        AgentRunnerKind::Codex => owned(&["exec", "--json"]),
        // In stream-json input mode Claude ignores a positional prompt and
        // reads user messages from stdin, so the prompt is sent there.
        AgentRunnerKind::Claude => owned(&[
            "-p",
            "--output-format",
            "stream-json",
            "--input-format",
            "stream-json",
            "--verbose",
        ]),
        // Copilot: `-p` takes the prompt as its value; only text/json output.
        AgentRunnerKind::Copilot => vec![
            "-p".to_string(),
            prompt.to_string(),
            "--output-format".to_string(),
            "json".to_string(),
            "--no-ask-user".to_string(),
        ],
        AgentRunnerKind::Gemini => owned(&["--output-format", "stream-json"]),
        AgentRunnerKind::Command => unreachable!("handled above"),
    };

    if !args_set_permissions(kind, &profile.args) {
        args.extend(permission_args(
            kind,
            profile.permissions.unwrap_or_default(),
            &profile.args,
            writable_dirs,
        ));
    }
    args.extend(model_effort_args(kind, profile));
    args.extend(tool_args(kind, profile));
    args.extend(profile.args.iter().cloned());

    let initial_input = match kind {
        AgentRunnerKind::Claude => format_stdin_message(kind, prompt),
        AgentRunnerKind::Codex => {
            args.push(prompt.to_string());
            None
        }
        AgentRunnerKind::Gemini => {
            args.extend(["-p".to_string(), prompt.to_string()]);
            None
        }
        _ => None,
    };

    let env = expand_env_map(&profile.env);

    Ok(RunnerCommand {
        program,
        args,
        env,
        initial_input,
    })
}

/// Model and effort values end up as CLI arguments (and, for Codex, inside a
/// `-c key=value` override), so only plain names are accepted: letters,
/// digits and `. _ - : / @ [ ]`, not starting with `-`.
pub fn validate_run_setting(name: &str, value: &str) -> LoTaRResult<()> {
    let valid = !value.is_empty()
        && value.len() <= 100
        && !value.starts_with('-')
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-:/@[]".contains(c));
    if valid {
        Ok(())
    } else {
        Err(LoTaRError::ValidationError(format!(
            "Invalid agent {name} '{value}': use letters, digits and . _ - : / @ [ ] only"
        )))
    }
}

fn has_flag(args: &[String], names: &[&str]) -> bool {
    args.iter().any(|arg| {
        let name = arg.split('=').next().unwrap_or(arg);
        names.contains(&name)
    })
}

/// The profile's (or the run's) model and effort as each CLI's own flags.
/// A flag the profile's `args` already pass is left to the profile.
fn model_effort_args(kind: AgentRunnerKind, profile: &AgentProfileDetail) -> Vec<String> {
    let mut args = Vec::new();
    let own = &profile.args;
    if let Some(model) = profile.model.as_deref() {
        let flag = match kind {
            AgentRunnerKind::Claude | AgentRunnerKind::Copilot => Some("--model"),
            AgentRunnerKind::Codex | AgentRunnerKind::Gemini => Some("-m"),
            AgentRunnerKind::Command => None,
        };
        if let Some(flag) = flag
            && !has_flag(own, &["--model", "-m"])
        {
            args.extend([flag.to_string(), model.to_string()]);
        }
    }
    if let Some(effort) = profile.effort.as_deref() {
        match kind {
            AgentRunnerKind::Claude if !has_flag(own, &["--effort"]) => {
                args.extend(["--effort".to_string(), effort.to_string()]);
            }
            AgentRunnerKind::Copilot if !has_flag(own, &["--reasoning-effort"]) => {
                args.extend(["--reasoning-effort".to_string(), effort.to_string()]);
            }
            AgentRunnerKind::Codex
                if !own
                    .iter()
                    .any(|arg| arg.starts_with("model_reasoning_effort=")) =>
            {
                args.extend(["-c".to_string(), format!("model_reasoning_effort={effort}")]);
            }
            // Gemini has no effort flag; command runners read LOTAR_AGENT_EFFORT.
            _ => {}
        }
    }
    args
}

/// `tools.allowed` / `tools.disallowed` and `mcp.allow` as each CLI's own
/// tool flags. Entries use the CLI's own tool syntax, for example
/// `Bash(npm test:*)` for Claude or `shell(npm test)` for Copilot. Codex and
/// Gemini have no per-tool flags, so these settings do not apply to them.
fn tool_args(kind: AgentRunnerKind, profile: &AgentProfileDetail) -> Vec<String> {
    let tools = profile.tools.as_ref();
    let allowed = tools
        .map(|tools| tools.allowed.as_slice())
        .unwrap_or_default();
    let denied = tools
        .map(|tools| tools.disallowed.as_slice())
        .unwrap_or_default();
    let servers = profile
        .mcp
        .as_ref()
        .map(|mcp| mcp.allow.as_slice())
        .unwrap_or_default();
    let mut args = Vec::new();
    match kind {
        AgentRunnerKind::Claude => {
            args.extend(allowed.iter().map(|tool| format!("--allowedTools={tool}")));
            args.extend(
                servers
                    .iter()
                    .map(|server| format!("--allowedTools=mcp__{server}")),
            );
            args.extend(
                denied
                    .iter()
                    .map(|tool| format!("--disallowedTools={tool}")),
            );
        }
        AgentRunnerKind::Copilot => {
            args.extend(
                allowed
                    .iter()
                    .chain(servers)
                    .map(|tool| format!("--allow-tool={tool}")),
            );
            args.extend(denied.iter().map(|tool| format!("--deny-tool={tool}")));
        }
        _ => {}
    }
    args
}

/// Parse one line of runner output. `None` means the line is not a runner
/// event at all (kept as raw output for debugging); an empty list means a
/// recognised event with nothing to show (hooks, deltas, bookkeeping).
pub fn parse_runner_line(kind: AgentRunnerKind, line: &str) -> Option<Vec<RunnerEvent>> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Some(Vec::new());
    }
    if kind == AgentRunnerKind::Command {
        return Some(vec![RunnerEvent::new(
            RunnerEventKind::Progress,
            Some(trimmed.to_string()),
            None,
        )]);
    }
    let payload: JsonValue = serde_json::from_str(trimmed).ok()?;
    payload.get("type").and_then(|v| v.as_str())?;
    Some(match kind {
        AgentRunnerKind::Codex => parse_codex_event(&payload),
        AgentRunnerKind::Claude => parse_claude_event(&payload),
        AgentRunnerKind::Copilot => parse_copilot_event(&payload),
        AgentRunnerKind::Gemini => parse_gemini_event(&payload),
        AgentRunnerKind::Command => unreachable!("handled above"),
    })
}

fn str_field(value: &JsonValue, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn non_empty(text: Option<String>) -> Option<String> {
    text.filter(|text| !text.trim().is_empty())
}

/// One readable line for a tool call: its name and the most telling input.
fn tool_summary(name: &str, input: Option<&JsonValue>) -> String {
    const KEYS: [&str; 7] = [
        "command",
        "file_path",
        "path",
        "pattern",
        "url",
        "query",
        "description",
    ];
    let detail = input.and_then(|input| {
        if let Some(text) = input.as_str() {
            return Some(text.to_string());
        }
        KEYS.iter().find_map(|key| str_field(input, key))
    });
    let mut line = match detail {
        Some(detail) => format!("{name}: {}", detail.lines().next().unwrap_or_default()),
        None => name.to_string(),
    };
    if line.chars().count() > 160 {
        line = line.chars().take(157).collect::<String>() + "...";
    }
    line
}

fn parse_codex_event(payload: &JsonValue) -> Vec<RunnerEvent> {
    let event_type = payload
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let session_id = str_field(payload, "thread_id");
    let item = payload.get("item");
    let item_type = item.and_then(|v| v.get("type")).and_then(|v| v.as_str());
    match (event_type, item_type) {
        ("thread.started", _) => {
            vec![RunnerEvent::new(RunnerEventKind::Init, None, session_id).with_payload(payload)]
        }
        ("item.completed", Some("agent_message")) => {
            let text = non_empty(item.and_then(|item| str_field(item, "text")));
            vec![RunnerEvent::new(RunnerEventKind::Message, text, session_id).with_payload(payload)]
        }
        ("item.started", Some("command_execution")) => {
            let command = item
                .and_then(|item| str_field(item, "command"))
                .unwrap_or_default();
            vec![RunnerEvent::new(
                RunnerEventKind::Progress,
                Some(tool_summary("Run", Some(&JsonValue::String(command)))),
                session_id,
            )]
        }
        ("item.completed", Some("file_change")) => {
            let paths: Vec<String> = item
                .and_then(|item| item.get("changes"))
                .and_then(|v| v.as_array())
                .map(|changes| {
                    changes
                        .iter()
                        .filter_map(|change| str_field(change, "path"))
                        .collect()
                })
                .unwrap_or_default();
            let text = format!(
                "Edited {}",
                if paths.is_empty() {
                    "files".to_string()
                } else {
                    paths.join(", ")
                }
            );
            vec![RunnerEvent::new(
                RunnerEventKind::Progress,
                Some(text),
                session_id,
            )]
        }
        ("item.completed", Some("error")) => {
            let text = non_empty(item.and_then(|item| str_field(item, "message")));
            vec![RunnerEvent::new(
                RunnerEventKind::Progress,
                text,
                session_id,
            )]
        }
        ("turn.completed", _) => {
            vec![RunnerEvent::new(RunnerEventKind::Result, None, session_id).with_payload(payload)]
        }
        ("turn.failed", _) => {
            let text = payload
                .get("error")
                .and_then(|error| str_field(error, "message"));
            vec![
                RunnerEvent::new(RunnerEventKind::Result, text, session_id)
                    .with_payload(payload)
                    .failing(),
            ]
        }
        ("error", _) => {
            let text = str_field(payload, "message");
            vec![RunnerEvent::new(RunnerEventKind::Progress, text, session_id).failing()]
        }
        _ => Vec::new(),
    }
}

fn parse_claude_event(payload: &JsonValue) -> Vec<RunnerEvent> {
    let event_type = payload
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let session_id = str_field(payload, "session_id");
    match event_type {
        "system" => match payload.get("subtype").and_then(|v| v.as_str()) {
            Some("init") => vec![
                RunnerEvent::new(RunnerEventKind::Init, None, session_id).with_payload(payload),
            ],
            Some("api_retry") => {
                let attempt = payload.get("attempt").and_then(|v| v.as_u64()).unwrap_or(0);
                let error =
                    str_field(payload, "error").unwrap_or_else(|| "request failed".to_string());
                vec![RunnerEvent::new(
                    RunnerEventKind::Progress,
                    Some(format!("API {error}; retrying (attempt {attempt})")),
                    session_id,
                )]
            }
            // Hooks, status and other bookkeeping events.
            _ => Vec::new(),
        },
        // Partial-message deltas (only sent with --include-partial-messages,
        // which LoTaR does not request): the full message follows anyway.
        "stream_event" => Vec::new(),
        "assistant" => {
            let blocks = payload
                .get("message")
                .and_then(|v| v.get("content"))
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let mut events = Vec::new();
            // Every text block counts, not only the first (thinking and tool
            // blocks often come before the text).
            let text: Vec<String> = blocks
                .iter()
                .filter(|block| {
                    block.get("type").and_then(|v| v.as_str()).unwrap_or("text") == "text"
                })
                .filter_map(|block| str_field(block, "text"))
                .collect();
            if let Some(text) = non_empty(Some(text.join("\n\n"))) {
                events.push(
                    RunnerEvent::new(RunnerEventKind::Message, Some(text), session_id.clone())
                        .with_payload(payload),
                );
            }
            for block in &blocks {
                if block.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                    let name = str_field(block, "name").unwrap_or_else(|| "tool".to_string());
                    events.push(RunnerEvent::new(
                        RunnerEventKind::Progress,
                        Some(tool_summary(&name, block.get("input"))),
                        session_id.clone(),
                    ));
                }
            }
            events
        }
        "result" => {
            let is_error = payload
                .get("is_error")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
                || payload
                    .get("subtype")
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| s != "success");
            let mut events = Vec::new();
            let denials: Vec<String> = payload
                .get("permission_denials")
                .and_then(|v| v.as_array())
                .map(|denials| {
                    denials
                        .iter()
                        .map(|denial| {
                            let name = str_field(denial, "tool_name")
                                .unwrap_or_else(|| "tool".to_string());
                            tool_summary(&name, denial.get("tool_input"))
                        })
                        .collect()
                })
                .unwrap_or_default();
            if !denials.is_empty() {
                events.push(RunnerEvent::new(
                    RunnerEventKind::Progress,
                    Some(format!(
                        "{} tool call(s) were denied by the permission settings; see the agent profile's permissions:\n- {}",
                        denials.len(),
                        denials.join("\n- ")
                    )),
                    session_id.clone(),
                ));
            }
            let mut result = RunnerEvent::new(
                RunnerEventKind::Result,
                str_field(payload, "result"),
                session_id,
            )
            .with_payload(payload);
            if is_error {
                result = result.failing();
            }
            events.push(result);
            events
        }
        _ => Vec::new(),
    }
}

/// Copilot CLI `--output-format json`: `{type, data}` envelopes.
fn parse_copilot_event(payload: &JsonValue) -> Vec<RunnerEvent> {
    let event_type = payload
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let data = payload.get("data");
    let session_id = str_field(payload, "sessionId")
        .or_else(|| data.and_then(|data| str_field(data, "sessionId")));
    match event_type {
        // A real `-p` run has no session-start line; the echoed prompt is
        // its first event, so that marks the start.
        "session.start" | "user.message" => {
            vec![RunnerEvent::new(RunnerEventKind::Init, None, session_id).with_payload(payload)]
        }
        "assistant.message" => {
            let text = non_empty(data.and_then(|data| str_field(data, "content")));
            match text {
                Some(text) => vec![
                    RunnerEvent::new(RunnerEventKind::Message, Some(text), session_id)
                        .with_payload(payload),
                ],
                None => Vec::new(),
            }
        }
        "tool.execution_start" => {
            let name = data
                .and_then(|data| str_field(data, "toolName"))
                .unwrap_or_else(|| "tool".to_string());
            let summary = tool_summary(&name, data.and_then(|data| data.get("arguments")));
            vec![RunnerEvent::new(
                RunnerEventKind::Progress,
                Some(summary),
                session_id,
            )]
        }
        "session.error" | "error" => {
            let text = data
                .and_then(|data| str_field(data, "message"))
                .or_else(|| str_field(payload, "message"));
            vec![RunnerEvent::new(RunnerEventKind::Progress, text, session_id).failing()]
        }
        "result" => {
            let exit_code = payload
                .get("exitCode")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let mut result =
                RunnerEvent::new(RunnerEventKind::Result, None, session_id).with_payload(payload);
            if exit_code != 0 {
                result = result.failing();
            }
            vec![result]
        }
        _ => Vec::new(),
    }
}

/// Gemini CLI `--output-format stream-json` (from the headless docs; not
/// verified against a live run).
fn parse_gemini_event(payload: &JsonValue) -> Vec<RunnerEvent> {
    let event_type = payload
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let session_id = str_field(payload, "session_id");
    match event_type {
        "init" => {
            vec![RunnerEvent::new(RunnerEventKind::Init, None, session_id).with_payload(payload)]
        }
        "message" => {
            if payload
                .get("role")
                .and_then(|v| v.as_str())
                .is_some_and(|role| role != "assistant")
            {
                return Vec::new();
            }
            let text = non_empty(str_field(payload, "content"));
            let delta = payload
                .get("delta")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let kind = if delta {
                RunnerEventKind::Progress
            } else {
                RunnerEventKind::Message
            };
            match text {
                Some(text) => {
                    vec![RunnerEvent::new(kind, Some(text), session_id).with_payload(payload)]
                }
                None => Vec::new(),
            }
        }
        "tool_use" => {
            let name = str_field(payload, "tool_name").unwrap_or_else(|| "tool".to_string());
            vec![RunnerEvent::new(
                RunnerEventKind::Progress,
                Some(tool_summary(&name, payload.get("parameters"))),
                session_id,
            )]
        }
        "error" => vec![RunnerEvent::new(
            RunnerEventKind::Progress,
            str_field(payload, "message"),
            session_id,
        )],
        "result" => {
            let failed = str_field(payload, "status").is_some_and(|status| status != "success");
            let mut result =
                RunnerEvent::new(RunnerEventKind::Result, None, session_id).with_payload(payload);
            if failed {
                result = result.failing();
            }
            vec![result]
        }
        _ => Vec::new(),
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
    // Copilot has no stdin message protocol (DEV-199); Claude does.
    matches!(kind, AgentRunnerKind::Claude)
}

/// Format a user message for the runner's stdin protocol.
/// Returns the bytes to write (including trailing newline).
pub fn format_stdin_message(kind: AgentRunnerKind, message: &str) -> Option<Vec<u8>> {
    match kind {
        AgentRunnerKind::Claude => {
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

    fn profile(runner: &str) -> AgentProfileDetail {
        AgentProfileDetail {
            runner: runner.to_string(),
            ..Default::default()
        }
    }

    fn replay(kind: AgentRunnerKind, fixture: &str) -> Vec<RunnerEvent> {
        let path = format!(
            "{}/tests/fixtures/agent-streams/{fixture}",
            env!("CARGO_MANIFEST_DIR")
        );
        let text = std::fs::read_to_string(path).expect("fixture");
        text.lines()
            .flat_map(|line| {
                parse_runner_line(kind, line).expect("every captured line is a runner event")
            })
            .collect()
    }

    fn texts(events: &[RunnerEvent], kind: RunnerEventKind) -> Vec<String> {
        events
            .iter()
            .filter(|event| event.kind == kind)
            .filter_map(|event| event.text.clone())
            .collect()
    }

    #[test]
    fn claude_gets_its_prompt_on_stdin_with_workspace_permissions() {
        let cmd = build_runner_command(
            AgentRunnerKind::Claude,
            &profile("claude"),
            "Fix the bug",
            &[],
        )
        .unwrap();
        assert!(
            !cmd.args.iter().any(|arg| arg == "Fix the bug"),
            "{:?}",
            cmd.args
        );
        assert!(
            cmd.args
                .windows(2)
                .any(|pair| pair == ["--input-format", "stream-json"])
        );
        assert!(
            cmd.args
                .windows(2)
                .any(|pair| pair == ["--permission-mode", "acceptEdits"])
        );
        assert!(cmd.args.windows(2).any(|pair| pair[0] == "--settings" && pair[1].contains("autoAllowBashIfSandboxed")));
        let input: serde_json::Value =
            serde_json::from_slice(cmd.initial_input.as_ref().unwrap()).unwrap();
        assert_eq!(input["message"]["content"], "Fix the bug");
    }

    #[test]
    fn permission_levels_map_onto_each_cli() {
        let level = |runner: &str, permissions: AgentPermissions| {
            let mut detail = profile(runner);
            detail.permissions = Some(permissions);
            let kind: AgentRunnerKind = runner.parse().unwrap();
            build_runner_command(kind, &detail, "p", &[])
                .unwrap()
                .args
                .join(" ")
        };
        assert!(level("claude", AgentPermissions::ReadOnly).contains("--permission-mode plan"));
        assert!(
            level("claude", AgentPermissions::Full).contains("--permission-mode bypassPermissions")
        );
        assert!(level("codex", AgentPermissions::ReadOnly).contains("--sandbox read-only"));
        assert!(level("codex", AgentPermissions::Workspace).contains("--sandbox workspace-write"));
        assert!(
            level("codex", AgentPermissions::Full)
                .contains("--dangerously-bypass-approvals-and-sandbox")
        );
        assert!(level("copilot", AgentPermissions::ReadOnly).contains("--deny-tool=write"));
        assert!(
            level("copilot", AgentPermissions::Workspace).contains("--deny-tool=shell(git push)")
        );
        assert!(level("copilot", AgentPermissions::Full).contains("--allow-all"));
        assert!(level("gemini", AgentPermissions::Workspace).contains("--approval-mode auto_edit"));
        assert!(level("gemini", AgentPermissions::Full).contains("--approval-mode yolo"));
    }

    #[test]
    fn profile_permission_flags_replace_the_defaults() {
        let mut detail = profile("claude");
        detail.args = vec!["--permission-mode".into(), "plan".into()];
        let args = build_runner_command(AgentRunnerKind::Claude, &detail, "p", &[])
            .unwrap()
            .args;
        assert_eq!(
            args.iter()
                .filter(|arg| *arg == "--permission-mode")
                .count(),
            1
        );
        assert!(!args.iter().any(|arg| arg == "--settings"));

        let mut detail = profile("codex");
        detail.args = vec!["-s".into(), "danger-full-access".into()];
        let args = build_runner_command(AgentRunnerKind::Codex, &detail, "p", &[])
            .unwrap()
            .args;
        assert!(!args.iter().any(|arg| arg == "workspace-write"));
    }

    #[test]
    fn model_and_effort_become_each_clis_flags() {
        let args = |runner: &str| {
            let mut profile = profile(runner);
            profile.model = Some("m1".to_string());
            profile.effort = Some("high".to_string());
            let kind: AgentRunnerKind = runner.parse().unwrap();
            build_runner_command(kind, &profile, "p", &[])
                .unwrap()
                .args
                .join(" ")
        };
        assert!(args("claude").contains("--model m1 --effort high"));
        assert!(args("copilot").contains("--model m1 --reasoning-effort high"));
        assert!(args("codex").contains("-m m1 -c model_reasoning_effort=high"));
        assert!(args("codex").ends_with(" p"), "the prompt stays last");
        let gemini = args("gemini");
        assert!(gemini.contains("-m m1") && !gemini.contains("high"));

        let mut command = profile("command");
        command.command = Some("run-agent".to_string());
        command.model = Some("m1".to_string());
        command.effort = Some("low".to_string());
        let built = build_runner_command(AgentRunnerKind::Command, &command, "p", &[]).unwrap();
        assert!(built.args.is_empty());
        assert_eq!(
            built.env.get("LOTAR_AGENT_MODEL").map(String::as_str),
            Some("m1")
        );
        assert_eq!(
            built.env.get("LOTAR_AGENT_EFFORT").map(String::as_str),
            Some("low")
        );
    }

    #[test]
    fn profile_args_keep_their_own_model_and_effort() {
        let mut claude = profile("claude");
        claude.model = Some("m1".to_string());
        claude.effort = Some("high".to_string());
        claude.args = vec![
            "--model=opus".to_string(),
            "--effort".to_string(),
            "max".to_string(),
        ];
        let args = build_runner_command(AgentRunnerKind::Claude, &claude, "p", &[])
            .unwrap()
            .args;
        assert!(
            !args.iter().any(|arg| arg == "m1" || arg == "high"),
            "{args:?}"
        );

        let mut codex = profile("codex");
        codex.effort = Some("high".to_string());
        codex.args = vec!["-c".to_string(), "model_reasoning_effort=low".to_string()];
        let args = build_runner_command(AgentRunnerKind::Codex, &codex, "p", &[])
            .unwrap()
            .args;
        assert!(!args.contains(&"model_reasoning_effort=high".to_string()));
    }

    #[test]
    fn run_settings_must_be_plain_names() {
        for good in [
            "sonnet",
            "gpt-5.5",
            "claude-opus-5-5[1m]",
            "us.anthropic/x:1",
            "xhigh",
        ] {
            assert!(validate_run_setting("model", good).is_ok(), "{good}");
        }
        for bad in [
            "",
            "--yolo",
            "-m",
            "a b",
            "x\nlow",
            "low;rm",
            "a\"b",
            &"m".repeat(101),
        ] {
            assert!(validate_run_setting("model", bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn tools_and_mcp_servers_become_tool_flags_for_claude_and_copilot() {
        let with_tools = |runner: &str| {
            let mut profile = profile(runner);
            profile.tools = Some(crate::config::types::AgentToolsConfig {
                allowed: vec!["WebFetch".to_string()],
                disallowed: vec!["Bash(git push:*)".to_string()],
            });
            profile.mcp = Some(crate::config::types::AgentMcpConfig {
                allow: vec!["docs".to_string()],
            });
            let kind: AgentRunnerKind = runner.parse().unwrap();
            build_runner_command(kind, &profile, "p", &[]).unwrap().args
        };
        let claude = with_tools("claude");
        for flag in [
            "--allowedTools=WebFetch",
            "--allowedTools=mcp__docs",
            "--disallowedTools=Bash(git push:*)",
        ] {
            assert!(claude.contains(&flag.to_string()), "{claude:?}");
        }
        let copilot = with_tools("copilot");
        for flag in [
            "--allow-tool=WebFetch",
            "--allow-tool=docs",
            "--deny-tool=Bash(git push:*)",
        ] {
            assert!(copilot.contains(&flag.to_string()), "{copilot:?}");
        }
        // The workspace defaults still apply next to the profile's tools.
        assert!(copilot.contains(&"--allow-all-tools".to_string()));
        assert!(
            !with_tools("codex")
                .iter()
                .any(|arg| arg.contains("WebFetch"))
        );
    }

    #[test]
    fn workspace_level_can_write_the_extra_directories() {
        let dirs = vec![
            std::path::PathBuf::from("/repo/.tasks"),
            std::path::PathBuf::from("/repo/.git"),
        ];
        let args = |runner: &str| {
            let kind: AgentRunnerKind = runner.parse().unwrap();
            build_runner_command(kind, &profile(runner), "p", &dirs)
                .unwrap()
                .args
                .join(" ")
        };
        for runner in ["claude", "codex", "copilot"] {
            assert!(
                args(runner).contains("--add-dir /repo/.tasks --add-dir /repo/.git"),
                "{runner}"
            );
        }
        assert!(args("gemini").contains("--include-directories /repo/.tasks"));
        let mut read_only = profile("claude");
        read_only.permissions = Some(AgentPermissions::ReadOnly);
        let args = build_runner_command(AgentRunnerKind::Claude, &read_only, "p", &dirs)
            .unwrap()
            .args;
        assert!(!args.iter().any(|arg| arg == "--add-dir"));
    }

    #[test]
    fn codex_copilot_and_gemini_take_the_prompt_as_an_argument() {
        let codex =
            build_runner_command(AgentRunnerKind::Codex, &profile("codex"), "Do it", &[]).unwrap();
        assert_eq!(codex.args.first().map(String::as_str), Some("exec"));
        assert_eq!(codex.args.last().map(String::as_str), Some("Do it"));
        assert!(codex.initial_input.is_none());

        let copilot =
            build_runner_command(AgentRunnerKind::Copilot, &profile("copilot"), "Do it", &[])
                .unwrap();
        assert_eq!(&copilot.args[..2], ["-p", "Do it"]);
        assert!(
            copilot
                .args
                .windows(2)
                .any(|pair| pair == ["--output-format", "json"])
        );
        assert!(copilot.args.iter().any(|arg| arg == "--no-ask-user"));
        assert!(
            !copilot
                .args
                .iter()
                .any(|arg| arg == "stream-json" || arg == "--input-format")
        );

        let gemini =
            build_runner_command(AgentRunnerKind::Gemini, &profile("gemini"), "Do it", &[])
                .unwrap();
        assert!(
            gemini
                .args
                .ends_with(&["-p".to_string(), "Do it".to_string()])
        );
    }

    #[test]
    fn command_runner_gets_the_prompt_in_the_environment() {
        let mut detail = profile("command");
        detail.command = Some("my-agent".into());
        let cmd = build_runner_command(AgentRunnerKind::Command, &detail, "Do it", &[]).unwrap();
        assert_eq!(
            cmd.env.get("LOTAR_AGENT_PROMPT").map(String::as_str),
            Some("Do it")
        );
        assert!(cmd.args.is_empty());
    }

    #[test]
    fn replays_a_real_claude_edit_run() {
        let events = replay(AgentRunnerKind::Claude, "claude-edit.jsonl");
        assert_eq!(
            events.first().map(|event| event.kind),
            Some(RunnerEventKind::Init)
        );
        let progress = texts(&events, RunnerEventKind::Progress);
        assert!(
            progress
                .iter()
                .any(|text| text.starts_with("Edit: ") && text.ends_with("calc.py")),
            "{progress:?}"
        );
        assert!(
            progress
                .iter()
                .any(|text| text.starts_with("Bash: python3")),
            "{progress:?}"
        );
        assert!(!texts(&events, RunnerEventKind::Message).is_empty());
        let result = events.last().unwrap();
        assert_eq!(result.kind, RunnerEventKind::Result);
        assert!(!events.iter().any(|event| event.failed));
        assert!(result.session_id.is_some());
    }

    #[test]
    fn replays_a_real_codex_edit_run() {
        let events = replay(AgentRunnerKind::Codex, "codex-edit.jsonl");
        assert_eq!(
            events.first().map(|event| event.kind),
            Some(RunnerEventKind::Init)
        );
        let progress = texts(&events, RunnerEventKind::Progress);
        assert!(
            progress
                .iter()
                .any(|text| text.starts_with("Run: ") && text.contains("cat calc.py")),
            "{progress:?}"
        );
        assert!(
            progress
                .iter()
                .any(|text| text.starts_with("Edited ") && text.ends_with("calc.py")),
            "{progress:?}"
        );
        assert!(!texts(&events, RunnerEventKind::Message).is_empty());
        assert_eq!(
            events.last().map(|event| event.kind),
            Some(RunnerEventKind::Result)
        );
        assert!(!events.iter().any(|event| event.failed));
    }

    #[test]
    fn replays_a_real_copilot_edit_run() {
        let events = replay(AgentRunnerKind::Copilot, "copilot-edit.jsonl");
        assert_eq!(events.first().unwrap().kind, RunnerEventKind::Init);
        let progress = texts(&events, RunnerEventKind::Progress);
        assert!(
            progress.iter().any(|text| text.starts_with("apply_patch")),
            "{progress:?}"
        );
        assert!(
            progress
                .iter()
                .any(|text| text.starts_with("bash: python3")),
            "{progress:?}"
        );
        assert!(!texts(&events, RunnerEventKind::Message).is_empty());
        let result = events.last().unwrap();
        assert_eq!(result.kind, RunnerEventKind::Result);
        assert!(result.session_id.is_some());
        assert!(!events.iter().any(|event| event.failed));
    }

    #[test]
    fn runner_reported_failures_are_flagged() {
        let failed = |kind: AgentRunnerKind, line: &str| {
            parse_runner_line(kind, line)
                .unwrap()
                .iter()
                .any(|event| event.failed)
        };
        assert!(failed(
            AgentRunnerKind::Claude,
            r#"{"type":"result","subtype":"error_max_turns","is_error":true,"session_id":"s"}"#
        ));
        assert!(failed(
            AgentRunnerKind::Codex,
            r#"{"type":"turn.failed","error":{"message":"quota"}}"#
        ));
        assert!(failed(
            AgentRunnerKind::Codex,
            r#"{"type":"error","message":"stream disconnected"}"#
        ));
        assert!(failed(
            AgentRunnerKind::Copilot,
            r#"{"type":"result","sessionId":"s","exitCode":1}"#
        ));
        assert!(failed(
            AgentRunnerKind::Gemini,
            r#"{"type":"result","status":"error"}"#
        ));
        assert!(!failed(
            AgentRunnerKind::Claude,
            r#"{"type":"result","subtype":"success","is_error":false}"#
        ));
    }

    #[test]
    fn claude_reports_denied_tools_and_joins_text_blocks() {
        let events = parse_runner_line(
            AgentRunnerKind::Claude,
            r#"{"type":"result","subtype":"success","is_error":false,"permission_denials":[{"tool_name":"Bash","tool_input":{"command":"git push"}}]}"#,
        )
        .unwrap();
        let note = events[0].text.as_deref().unwrap();
        assert!(note.contains("1 tool call(s) were denied"), "{note}");
        assert!(note.contains("- Bash: git push"), "{note}");

        let events = parse_runner_line(
            AgentRunnerKind::Claude,
            r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"hmm"},{"type":"text","text":"First"},{"type":"tool_use","name":"Read","input":{"file_path":"a.rs"}},{"type":"text","text":"Second"}]}}"#,
        )
        .unwrap();
        assert_eq!(events[0].text.as_deref(), Some("First\n\nSecond"));
        assert_eq!(events[1].text.as_deref(), Some("Read: a.rs"));
    }

    #[test]
    fn bookkeeping_lines_are_recognised_but_not_shown_and_noise_is_raw() {
        let hook = r#"{"type":"system","subtype":"hook_started","session_id":"s"}"#;
        assert_eq!(
            parse_runner_line(AgentRunnerKind::Claude, hook).map(|events| events.len()),
            Some(0)
        );
        let delta = r#"{"type":"assistant.reasoning_delta","data":{"deltaContent":"x"}}"#;
        assert_eq!(
            parse_runner_line(AgentRunnerKind::Copilot, delta).map(|events| events.len()),
            Some(0)
        );
        assert!(parse_runner_line(AgentRunnerKind::Claude, "Warning: something odd").is_none());
    }

    #[test]
    fn only_claude_accepts_stdin_messages() {
        assert!(supports_stdin(AgentRunnerKind::Claude));
        assert!(!supports_stdin(AgentRunnerKind::Copilot));
        assert!(!supports_stdin(AgentRunnerKind::Codex));
        assert!(!supports_stdin(AgentRunnerKind::Gemini));
        assert!(!supports_stdin(AgentRunnerKind::Command));
        let msg = format_stdin_message(AgentRunnerKind::Claude, "hello world").unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&msg).unwrap();
        assert_eq!(parsed["type"], "user");
        assert_eq!(parsed["message"]["content"], "hello world");
        assert!(format_stdin_message(AgentRunnerKind::Copilot, "hello").is_none());
    }
}
