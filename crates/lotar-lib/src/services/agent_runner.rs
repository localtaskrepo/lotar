//! How a job's agent runs (DEV-503). Every runner but `command` is the harnessmux agent of the same name, run as a
//! harnessmux session that this process owns; harnessmux maps model, effort and permissions onto each agent and
//! turns its output into one event stream. `command` runs a program of the profile's own.

use crate::config::types::{AgentPermissionPolicy, AgentPermissions, AgentProfileDetail, AgentRunnerKind};
use crate::errors::{LoTaRError, LoTaRResult};
use std::collections::{BTreeMap, HashMap};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

/// A `command` runner's process.
#[derive(Debug, Clone)]
pub struct RunnerCommand {
    pub program: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
}

/// Runs the harnessmux sessions of this process's jobs. Job threads block on it; its own threads drive the agents.
static RUNTIME: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("lotar-agents")
        .enable_all()
        .build()
        .expect("the agent session runtime needs two threads and a timer")
});

pub fn block_on<F: std::future::Future>(future: F) -> F::Output {
    RUNTIME.block_on(future)
}

/// The sessions live where the harnessmux CLI keeps its own, so `harnessmux list`, `attach` and `send` reach a job's
/// session (`HARNESSMUX_HOME` moves them, for tests).
pub fn registry() -> harnessmux::Registry {
    harnessmux::Registry::default()
}

/// The harnessmux agent table: the crate's built-in one with the user's `agents.toml` over it.
pub fn agent_table() -> LoTaRResult<harnessmux::AgentTable> {
    harnessmux::AgentTable::load().map_err(|err| LoTaRError::ValidationError(format!("harnessmux agent table: {err}")))
}

/// The harnessmux agent a runner runs; `None` for `command`.
pub fn harness_agent(kind: AgentRunnerKind) -> Option<&'static str> {
    (kind != AgentRunnerKind::Command).then(|| kind.as_str())
}

/// The repository a job's session belongs to, as harnessmux records it: the main checkout of the tasks directory's git
/// repository (also from a worktree), else the tasks directory's parent.
pub fn session_project(tasks_dir: &std::path::Path) -> std::path::PathBuf {
    let project = harnessmux::registry::project_of(tasks_dir).unwrap_or_else(|| tasks_dir.parent().unwrap_or(tasks_dir).to_path_buf());
    std::fs::canonicalize(&project).unwrap_or(project)
}

fn clean_name(raw: &str) -> String {
    raw.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') { c } else { '-' }).collect()
}

/// What every session name of a ticket starts with: the repository's folder name and a hash of its path, so two
/// repositories with the same ticket IDs never share a session, then the ticket (`lotar-1a2b3c-DEV-1`).
pub fn session_prefix(project: &std::path::Path, ticket_id: &str) -> String {
    let folder = project.file_name().map(|name| clean_name(&name.to_string_lossy())).unwrap_or_default();
    let folder: String = folder.trim_start_matches('.').chars().take(20).collect();
    let hash = blake3::hash(project.to_string_lossy().as_bytes()).to_hex();
    let repo = if folder.is_empty() { hash[..6].to_string() } else { format!("{folder}-{}", &hash[..6]) };
    format!("{repo}-{}", clean_name(ticket_id))
}

/// A ticket's session for an agent profile: `lotar-1a2b3c-DEV-1-fix`, or `lotar-1a2b3c-DEV-1` for a job without a
/// profile. A job of the same profile with `session: continue` takes the session's next turn.
pub fn session_name(project: &std::path::Path, ticket_id: &str, agent_profile: Option<&str>) -> String {
    let prefix = session_prefix(project, ticket_id);
    let name = match agent_profile.map(str::trim).filter(|profile| !profile.is_empty()) {
        Some(profile) => format!("{prefix}-{}", clean_name(profile)),
        None => prefix,
    };
    name.chars().take(64).collect()
}

/// The ticket's other sessions in this repository, oldest first, for the prompt of a job: their agents' replies and
/// tool calls are in their event logs.
pub fn earlier_sessions(project: &std::path::Path, ticket_id: &str, current: &str) -> Vec<(harnessmux::SessionRecord, std::path::PathBuf)> {
    let registry = registry();
    let prefix = session_prefix(project, ticket_id);
    let mut sessions: Vec<_> = registry
        .list(None)
        .unwrap_or_default()
        .into_iter()
        .filter(|record| record.name != current && (record.name == prefix || record.name.starts_with(&format!("{prefix}-"))))
        .map(|record| {
            let events = registry.events_path(&record.name);
            (record, events)
        })
        .filter(|(_, events)| events.is_file())
        .collect();
    sessions.sort_by(|a, b| a.0.last_used.cmp(&b.0.last_used));
    sessions
}

/// Hands `message` to a session another process owns, as its next turn. False when no process owns it.
pub fn send_to_session(name: &str, message: &str) -> bool {
    let registry = registry();
    block_on(async {
        harnessmux::ipc::owner_running(&registry, name).await
            && harnessmux::client::request(&registry, name, &harnessmux::ipc::Request::Send { prompt: message.to_string() }).await.is_ok()
    })
}

/// The sessions this process owns, by name: a job that continues one while it is still open takes it over.
static OWNED: LazyLock<Mutex<HashMap<String, harnessmux::Session>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// The ticket's session a `session: continue` job can take its next turn in, or why it can't (for the job log).
pub fn continuable_session(name: &str, agent: &str, tasks_dir: &std::path::Path) -> Result<harnessmux::SessionRecord, String> {
    let record = registry().load(name).map_err(|_| format!("No earlier session {name}; this job starts a new one."))?;
    // The repository as harnessmux records it for a session: none outside git.
    let canonical = |path: &std::path::Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let expected = harnessmux::registry::project_of(tasks_dir).map(|project| canonical(&project));
    if record.project.as_deref().map(canonical) != expected {
        let show = |project: Option<&std::path::Path>| project.map_or_else(|| "no repository".to_string(), |project| project.display().to_string());
        return Err(format!(
            "The session {name} belongs to {}, not {}; this job starts a new one.",
            show(record.project.as_deref()),
            show(expected.as_deref())
        ));
    }
    if record.state == harnessmux::SessionState::Ended {
        return Err(format!("The session {name} was closed; this job starts a new one."));
    }
    if record.agent != agent {
        return Err(format!("The session {name} runs the {} agent, not {agent}; this job starts a new one.", record.agent));
    }
    if !record.cwd.is_dir() {
        return Err(format!("The working directory {} of the session {name} is gone; this job starts a new one.", record.cwd.display()));
    }
    Ok(record)
}

/// Opens a job's session: the existing one for `resume` (still open in this process, else resumed), otherwise a new
/// one under `name`, which replaces an earlier session of that name nobody runs.
pub fn open_session(name: &str, agent: &str, cwd: &std::path::Path, profile: harnessmux::Profile, resume: bool) -> Result<harnessmux::Session, String> {
    let registry = registry();
    let table = agent_table().map_err(|err| err.to_string())?;
    let owned = OWNED.lock().ok().and_then(|owned| owned.get(name).cloned());
    let session = block_on(async {
        if let Some(session) = owned {
            if resume && session.status().await.is_ok() {
                return Ok(session);
            }
            let _ = session.close().await;
        }
        if resume {
            return harnessmux::Session::resume(registry, &table, name, Some(SESSION_IDLE_TTL)).await.map_err(|err| err.to_string());
        }
        if let Ok(existing) = registry.load(name)
            && existing.state != harnessmux::SessionState::Ended
            && let Some(pid) = existing.owner_pid.filter(|pid| *pid != std::process::id() && crate::services::agent_active_jobs::process_alive(*pid))
        {
            return Err(format!("the session {name} is in use by process {pid}; close it with `harnessmux close {name}`"));
        }
        registry.remove(name).map_err(|err| err.to_string())?;
        let options = harnessmux::StartOptions {
            name: Some(name.to_string()),
            agent: agent.to_string(),
            cwd: cwd.to_path_buf(),
            profile,
            idle_ttl: Some(SESSION_IDLE_TTL),
        };
        harnessmux::Session::start(registry, &table, options).await.map_err(|err| err.to_string())
    })?;
    if let Ok(mut owned) = OWNED.lock() {
        owned.insert(name.to_string(), session.clone());
    }
    Ok(session)
}

/// Whether the agent takes MCP servers with its session (ACP agents; not Amp, T3 Code or a command).
pub fn takes_mcp_servers(kind: AgentRunnerKind) -> bool {
    !matches!(kind, AgentRunnerKind::Command | AgentRunnerKind::Amp | AgentRunnerKind::T3)
}

/// How long a session stays open after its job's last turn; then its agent stops and the session stays resumable.
pub const SESSION_IDLE_TTL: Duration = Duration::from_secs(60);

/// Tools of LoTaR's MCP server an agent job may use: reading and updating
/// tickets, not starting agents, deleting tasks or syncing.
pub const LOTAR_MCP_JOB_TOOLS: &[&str] =
    &["task_get", "task_list", "task_update", "task_comment_add", "task_reference_add", "relationship_graph", "config_show", "whoami"];

/// LoTaR's MCP server for a job, bound to its tasks directory and limited to [`LOTAR_MCP_JOB_TOOLS`].
pub fn lotar_mcp_server(lotar_bin: &str, tasks_dir: &std::path::Path) -> harnessmux::McpServer {
    harnessmux::McpServer::Stdio {
        command: lotar_bin.to_string(),
        args: vec!["mcp".to_string()],
        env: BTreeMap::from([
            ("LOTAR_TASKS_DIR".to_string(), tasks_dir.to_string_lossy().to_string()),
            ("LOTAR_MCP_TOOLS".to_string(), LOTAR_MCP_JOB_TOOLS.join(",")),
            ("LOTAR_MCP_AUTORELOAD".to_string(), "0".to_string()),
        ]),
    }
}

/// Refuses profile settings a harnessmux agent can't take: its program and arguments come from harnessmux's agent
/// table (`agents.toml`), not from the profile.
pub fn check_agent_profile(kind: AgentRunnerKind, profile: &AgentProfileDetail) -> LoTaRResult<()> {
    let Some(agent) = harness_agent(kind) else {
        return Ok(());
    };
    if profile.command.is_some() || !profile.args.is_empty() {
        return Err(LoTaRError::ValidationError(format!(
            "The {agent} runner takes no command or args: lotar runs it as the harnessmux agent {agent}. Set its program in harnessmux's agents.toml, or use runner: command for a program of your own."
        )));
    }
    if let Some(effort) = profile.effort.as_deref() {
        effort.parse::<harnessmux::Effort>().map_err(|err| LoTaRError::ValidationError(format!("Invalid agent effort: {err}")))?;
    }
    Ok(())
}

/// What a harnessmux session starts with, from a profile, plus notes on the settings it doesn't pass.
pub fn session_profile(profile: &AgentProfileDetail, env: HashMap<String, String>) -> LoTaRResult<(harnessmux::Profile, Vec<String>)> {
    let effort = profile
        .effort
        .as_deref()
        .map(str::parse::<harnessmux::Effort>)
        .transpose()
        .map_err(|err| LoTaRError::ValidationError(format!("Invalid agent effort: {err}")))?;
    let permissions = match profile.permissions.unwrap_or_default() {
        AgentPermissions::ReadOnly => harnessmux::Permissions::ReadOnly,
        AgentPermissions::Workspace => harnessmux::Permissions::Workspace,
        AgentPermissions::Full => harnessmux::Permissions::Full,
    };
    let on_permission = profile.on_permission.map(|policy| match policy {
        AgentPermissionPolicy::Deny => harnessmux::PermissionPolicy::Deny,
        AgentPermissionPolicy::AllowReads => harnessmux::PermissionPolicy::AllowReads,
        AgentPermissionPolicy::Allow => harnessmux::PermissionPolicy::Allow,
        AgentPermissionPolicy::Ask => harnessmux::PermissionPolicy::Ask,
    });
    let mut notes = Vec::new();
    if profile.max_budget_usd.is_some() {
        notes.push("The agent profile's max_budget_usd is not passed: harnessmux has no budget limit.".to_string());
    }
    if profile.tools.as_ref().is_some_and(|tools| !tools.allowed.is_empty() || !tools.disallowed.is_empty()) {
        notes.push("The agent profile's tools are not passed: ACP agents take no tool lists; permissions and on_permission limit them.".to_string());
    }
    if profile.mcp.as_ref().is_some_and(|mcp| !mcp.allow.is_empty()) {
        notes.push("The agent profile's mcp.allow is not passed: the agent's own MCP servers follow its own settings.".to_string());
    }
    let mut session_env: BTreeMap<String, String> = expand_env_map(&profile.env).into_iter().collect();
    session_env.extend(env);
    let session = harnessmux::Profile {
        model: profile.model.clone(),
        effort,
        permissions,
        on_permission,
        env: session_env,
        provider: profile.provider.clone(),
        ..harnessmux::Profile::default()
    };
    Ok((session, notes))
}

/// The agents (runner names) installed on this machine: the CLI each harnessmux agent signs in with is on PATH.
pub fn installed_agent_clis() -> Vec<&'static str> {
    let Ok(table) = harnessmux::AgentTable::load() else {
        return Vec::new();
    };
    AgentRunnerKind::ALL
        .into_iter()
        .filter_map(harness_agent)
        .filter(|agent| {
            table.get(agent).is_ok_and(|spec| {
                let program = spec.requires.clone().or_else(|| spec.command.first().cloned()).unwrap_or_default();
                harnessmux::paths::find_program(&program, &spec.paths).is_some()
            })
        })
        .collect()
}

/// A `command` runner's process: the profile's program and args, with the prompt, model and effort in the
/// environment.
pub fn build_command(profile: &AgentProfileDetail, prompt: &str) -> LoTaRResult<RunnerCommand> {
    let program = profile
        .command
        .clone()
        .filter(|program| !program.trim().is_empty())
        .ok_or_else(|| LoTaRError::ValidationError("Command runner requires 'command' in agent profile".to_string()))?;
    let mut env = expand_env_map(&profile.env);
    env.insert("LOTAR_AGENT_PROMPT".to_string(), prompt.to_string());
    if let Some(model) = profile.model.as_deref() {
        env.insert("LOTAR_AGENT_MODEL".to_string(), model.to_string());
    }
    if let Some(effort) = profile.effort.as_deref() {
        env.insert("LOTAR_AGENT_EFFORT".to_string(), effort.to_string());
    }
    Ok(RunnerCommand { program, args: profile.args.clone(), env })
}

pub fn validate_run_setting(name: &str, value: &str) -> LoTaRResult<()> {
    let valid =
        !value.is_empty() && value.len() <= 100 && !value.starts_with('-') && value.chars().all(|c| c.is_ascii_alphanumeric() || "._-:/@[]".contains(c));
    if valid { Ok(()) } else { Err(LoTaRError::ValidationError(format!("Invalid agent {name} '{value}': use letters, digits and . _ - : / @ [ ] only"))) }
}

fn expand_env_map(env: &BTreeMap<String, String>) -> HashMap<String, String> {
    env.iter().map(|(key, value)| (key.clone(), expand_env_value(value))).collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(runner: &str) -> AgentProfileDetail {
        AgentProfileDetail { runner: runner.to_string(), ..Default::default() }
    }

    #[test]
    fn run_settings_must_be_plain_names() {
        for good in ["sonnet", "gpt-5.5", "claude-opus-5-5[1m]", "us.anthropic/x:1", "xhigh"] {
            assert!(validate_run_setting("model", good).is_ok(), "{good}");
        }
        for bad in ["", "--yolo", "-m", "a b", "x\nlow", "low;rm", "a\"b", &"m".repeat(101)] {
            assert!(validate_run_setting("model", bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn command_runner_gets_the_prompt_in_the_environment() {
        let mut detail = profile("command");
        detail.command = Some("my-agent".into());
        let cmd = build_command(&detail, "Do it").unwrap();
        assert_eq!(cmd.env.get("LOTAR_AGENT_PROMPT").map(String::as_str), Some("Do it"));
        assert!(cmd.args.is_empty());
        assert!(build_command(&profile("command"), "Do it").is_err());
    }

    #[test]
    fn every_runner_but_command_is_the_harnessmux_agent_of_its_name() {
        let table = harnessmux::AgentTable::parse(harnessmux::agents::BUILTIN, None).unwrap();
        for kind in AgentRunnerKind::ALL {
            match harness_agent(kind) {
                Some(agent) => assert!(table.get(agent).is_ok(), "{agent} is not a harnessmux agent"),
                None => assert_eq!(kind, AgentRunnerKind::Command),
            }
        }
    }

    #[test]
    fn agent_profiles_take_no_command_and_a_known_effort() {
        let mut claude = profile("claude");
        claude.command = Some("/opt/claude".into());
        let err = check_agent_profile(AgentRunnerKind::Claude, &claude).unwrap_err().to_string();
        assert!(err.contains("agents.toml"), "{err}");
        let mut codex = profile("codex");
        codex.effort = Some("ultra".into());
        let err = check_agent_profile(AgentRunnerKind::Codex, &codex).unwrap_err().to_string();
        assert!(err.contains("low, medium, high, xhigh, max"), "{err}");
        let mut command = profile("command");
        command.command = Some("sh".into());
        command.effort = Some("whatever".into());
        assert!(check_agent_profile(AgentRunnerKind::Command, &command).is_ok());
    }

    #[test]
    fn a_profile_becomes_a_session_profile_with_notes_on_what_it_drops() {
        let mut detail = profile("claude");
        detail.model = Some("sonnet".into());
        detail.effort = Some("high".into());
        detail.permissions = Some(AgentPermissions::ReadOnly);
        detail.on_permission = Some(AgentPermissionPolicy::Ask);
        detail.max_budget_usd = Some(2.0);
        detail.env = BTreeMap::from([("A".to_string(), "1".to_string())]);
        let (session, notes) = session_profile(&detail, HashMap::from([("LOTAR_TICKET_ID".to_string(), "DEV-1".to_string())])).unwrap();
        assert_eq!(session.model.as_deref(), Some("sonnet"));
        assert_eq!(session.effort, Some(harnessmux::Effort::High));
        assert_eq!(session.permissions, harnessmux::Permissions::ReadOnly);
        assert_eq!(session.policy(), harnessmux::PermissionPolicy::Ask);
        assert_eq!(session.env["A"], "1");
        assert_eq!(session.env["LOTAR_TICKET_ID"], "DEV-1");
        assert_eq!(notes.len(), 1, "{notes:?}");
    }

    #[test]
    fn sessions_are_named_after_repository_ticket_and_profile() {
        let repo = std::path::Path::new("/work/lotar");
        let prefix = session_prefix(repo, "DEV-12");
        assert!(prefix.starts_with("lotar-") && prefix.ends_with("-DEV-12"), "{prefix}");
        assert_eq!(session_name(repo, "DEV-12", Some("fix")), format!("{prefix}-fix"));
        assert_eq!(session_name(repo, "DEV-12", None), prefix);
        assert_eq!(session_name(repo, "DEV-12", Some("a b/c")), format!("{prefix}-a-b-c"));
        let other = session_name(std::path::Path::new("/elsewhere/lotar"), "DEV-12", Some("fix"));
        assert_ne!(other, session_name(repo, "DEV-12", Some("fix")), "another checkout with the same name and ticket");
        let long = session_name(std::path::Path::new(&format!("/w/{}", "r".repeat(90))), "DEV-1", Some(&"x".repeat(80)));
        assert!(harnessmux::registry::validate_name(&long).is_ok(), "{long}");
    }

    #[test]
    fn lotar_mcp_is_limited_to_the_job_tools() {
        let harnessmux::McpServer::Stdio { command, args, env } = lotar_mcp_server("/bin/lotar", std::path::Path::new("/repo/.tasks")) else {
            panic!("stdio server expected");
        };
        assert_eq!((command.as_str(), args.as_slice()), ("/bin/lotar", ["mcp".to_string()].as_slice()));
        assert_eq!(env["LOTAR_TASKS_DIR"], "/repo/.tasks");
        assert!(env["LOTAR_MCP_TOOLS"].contains("task_update") && !env["LOTAR_MCP_TOOLS"].contains("agent_"));
    }
}
