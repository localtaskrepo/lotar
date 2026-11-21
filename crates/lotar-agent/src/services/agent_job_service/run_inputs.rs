//! What a job's run went in with (DEV-767): the prompt of its first turn,
//! split into its sections as it is built, and the settings its session
//! started with. One `run_inputs` line in the job's log holds them, so the
//! owner can read a run's instructions and tune them. Secrets stay out:
//! environment variables, MCP server env and HTTP headers go by name only.

use super::{JOB_REGISTRY, workspace_root_of};
use crate::api_types::{
    AgentJobInstructionsOrigin, AgentJobInstructionsSource, AgentJobMcpServer, AgentJobPromptSection, AgentJobRunInputs, AgentJobRunRules, AgentJobRunSession,
    AgentJobRunSettings,
};
use crate::config::types::{AgentInstructionsConfig, AgentProfileDetail, AgentRunnerKind, ResolvedConfig};
use crate::services::agent_log_service::AgentLogService;
use crate::utils::time;
use std::collections::BTreeSet;

/// A job's prompt and where its parts start, as `build_prompt` writes them.
#[derive(Debug, Clone, Default)]
pub(super) struct JobPrompt {
    pub text: String,
    /// Each part's heading and byte offset in `text`, in order; a part runs to the next one.
    starts: Vec<(String, usize)>,
    pub instructions_source: AgentJobInstructionsSource,
}

impl JobPrompt {
    /// Starts a part at the end of the text so far.
    pub fn section(&mut self, heading: &str) {
        self.starts.push((heading.to_string(), self.text.len()));
    }

    pub fn push_str(&mut self, text: &str) {
        self.text.push_str(text);
    }

    pub fn push(&mut self, ch: char) {
        self.text.push(ch);
    }

    /// Puts a part of its own in front of the prompt.
    pub fn prepend(&mut self, heading: &str, text: &str) {
        if text.is_empty() {
            return;
        }
        self.text.insert_str(0, text);
        for (_, start) in &mut self.starts {
            *start += text.len();
        }
        self.starts.insert(0, (heading.to_string(), 0));
    }

    /// Adds a part of its own at the end, from its first non-blank line.
    pub fn append(&mut self, heading: &str, text: &str) {
        if text.is_empty() {
            return;
        }
        let lead = text.len() - text.trim_start_matches('\n').len();
        self.text.push_str(text);
        self.starts.push((heading.to_string(), self.text.len() - text.len() + lead));
    }

    /// The parts in Unicode code points, covering the whole text: text before the first part goes to it.
    pub fn sections(&self) -> Vec<AgentJobPromptSection> {
        let chars = |end: usize| self.text[..end].chars().count();
        let total = chars(self.text.len());
        let mut out = Vec::with_capacity(self.starts.len());
        for (index, (heading, start)) in self.starts.iter().enumerate() {
            let start = if index == 0 { 0 } else { chars(*start) };
            let end = self.starts.get(index + 1).map_or(total, |(_, next)| chars(*next));
            out.push(AgentJobPromptSection { heading: heading.clone(), start, length: end.saturating_sub(start) });
        }
        out
    }
}

/// Where a job's agent instructions come from, as `resolve_agent_instructions` reads them.
pub(super) fn instructions_source(tasks_dir: &std::path::Path, config: &ResolvedConfig, profile: &AgentProfileDetail) -> AgentJobInstructionsSource {
    let origin = |configured: &AgentInstructionsConfig| match configured {
        AgentInstructionsConfig::Inline(_) => AgentJobInstructionsOrigin { kind: "inline".to_string(), path: None },
        AgentInstructionsConfig::File { file } => AgentJobInstructionsOrigin {
            kind: "file".to_string(),
            path: Some(super::resolve_agent_instructions_path(tasks_dir, file).map_or_else(|_| file.clone(), |path| path.to_string_lossy().to_string())),
        },
    };
    AgentJobInstructionsSource {
        base: Some(
            config
                .agent_instructions
                .as_ref()
                .map_or_else(|| AgentJobInstructionsOrigin { kind: "builtin".to_string(), path: Some("docs/help/agent-instructions.md".to_string()) }, origin),
        ),
        profile: profile.instructions.as_ref().map(origin),
    }
}

/// How the job's harnessmux session started.
pub(super) struct SessionStart<'a> {
    pub agent: &'a str,
    pub name: &'a str,
    pub resume: bool,
    pub cwd: &'a std::path::Path,
    /// The agent table's own environment for the agent, on top of the session's.
    pub agent_env: Vec<String>,
}

/// The `run_inputs` of a job that runs as a harnessmux session.
pub(super) fn session_inputs(
    prompt: &JobPrompt,
    runner_kind: AgentRunnerKind,
    agent_profile: Option<String>,
    profile: &AgentProfileDetail,
    session_profile: &harnessmux::Profile,
    start: &SessionStart<'_>,
) -> AgentJobRunInputs {
    let mcp_servers = session_profile.mcp_servers.iter().map(|(name, server)| mcp_server(name, server)).collect();
    let rules = &session_profile.rules;
    let settings = AgentJobRunSettings {
        runner: runner_kind.as_str().to_string(),
        agent_profile,
        harness_agent: Some(start.agent.to_string()),
        model: session_profile.model.clone(),
        effort: session_profile.effort.map(|effort| effort.as_str().to_string()).or_else(|| profile.effort.clone()),
        provider: session_profile.provider.clone(),
        permissions: Some(session_profile.permissions.as_str().to_string()),
        on_permission: Some(session_profile.policy().as_str().to_string()),
        rules: AgentJobRunRules {
            deny_commands: rules.deny_commands.clone(),
            allow_tools: rules.allow_tools.clone(),
            deny_tools: rules.deny_tools.clone(),
            confine_writes: rules.confine_writes,
        },
        options: session_profile.options.clone(),
        add_dirs: session_profile.add_dirs.iter().map(|dir| dir.to_string_lossy().to_string()).collect(),
        mcp_servers,
        cwd: Some(start.cwd.to_string_lossy().to_string()),
        title: session_profile.title.clone(),
        command: profile.command.clone(),
        args: profile.args.clone(),
        session: Some(AgentJobRunSession {
            name: start.name.to_string(),
            mode: if start.resume { "resumed" } else { "new" }.to_string(),
            configured: Some(session_label(profile).to_string()),
        }),
    };
    let env = session_profile.env.keys().cloned().chain(start.agent_env.iter().cloned());
    inputs(prompt, settings, env)
}

/// The `run_inputs` of a `command` runner's job.
pub(super) fn command_inputs(
    prompt: &JobPrompt,
    agent_profile: Option<String>,
    profile: &AgentProfileDetail,
    cwd: &std::path::Path,
    env: impl Iterator<Item = String>,
) -> AgentJobRunInputs {
    let settings = AgentJobRunSettings {
        runner: AgentRunnerKind::Command.as_str().to_string(),
        agent_profile,
        model: profile.model.clone(),
        effort: profile.effort.clone(),
        permissions: profile.permissions.and_then(|permissions| serde_json::to_value(permissions).ok()?.as_str().map(str::to_string)),
        cwd: Some(cwd.to_string_lossy().to_string()),
        command: profile.command.clone(),
        args: profile.args.clone(),
        ..AgentJobRunSettings::default()
    };
    let env = profile.env.keys().cloned().chain(env);
    inputs(prompt, settings, env)
}

fn inputs(prompt: &JobPrompt, settings: AgentJobRunSettings, env: impl Iterator<Item = String>) -> AgentJobRunInputs {
    AgentJobRunInputs {
        at: time::now_rfc3339(),
        prompt: prompt.text.clone(),
        sections: prompt.sections(),
        instructions_source: prompt.instructions_source.clone(),
        settings,
        env: env.collect::<BTreeSet<_>>().into_iter().collect(),
    }
}

fn session_label(profile: &AgentProfileDetail) -> &'static str {
    match profile.session.unwrap_or_default() {
        crate::config::types::AgentSession::New => "new",
        crate::config::types::AgentSession::Continue => "continue",
        crate::config::types::AgentSession::Fresh => "fresh",
    }
}

fn mcp_server(name: &str, server: &harnessmux::McpServer) -> AgentJobMcpServer {
    let tools = if name == "lotar" { crate::services::agent_runner::LOTAR_MCP_JOB_TOOLS.iter().map(|tool| tool.to_string()).collect() } else { Vec::new() };
    match server {
        harnessmux::McpServer::Stdio { command, args, env } => AgentJobMcpServer {
            name: name.to_string(),
            command: Some(command.clone()),
            args: args.clone(),
            env: env.keys().cloned().collect(),
            tools,
            ..AgentJobMcpServer::default()
        },
        harnessmux::McpServer::Http { url, headers } => AgentJobMcpServer {
            name: name.to_string(),
            url: Some(url.clone()),
            headers: headers.keys().cloned().collect(),
            tools,
            ..AgentJobMcpServer::default()
        },
    }
}

/// The job's profile name, from the registry.
pub(super) fn agent_profile_of(job_id: &str) -> Option<String> {
    JOB_REGISTRY.lock().ok().and_then(|registry| registry.jobs.get(job_id).and_then(|state| state.record.agent_profile.clone()))
}

/// Write the job's `run_inputs` line. A failure leaves the job running: its inputs then read as not recorded.
pub(super) fn record(job_id: &str, inputs: AgentJobRunInputs) {
    let target = JOB_REGISTRY.lock().ok().and_then(|registry| {
        let state = registry.jobs.get(job_id)?;
        Some((state.record.workspace_root.clone(), state.record.agent_logs_dir.clone()))
    });
    if let Some((workspace_root, logs_dir)) = target {
        let _ = AgentLogService::append_run_inputs(&workspace_root, logs_dir.as_deref(), job_id, inputs);
    }
}

/// The job's `run_inputs` from its log; `None` when it has none.
pub(super) fn load(tasks_dir: &std::path::Path, job_id: &str) -> Option<AgentJobRunInputs> {
    let root = workspace_root_of(tasks_dir);
    super::job_log_dirs(tasks_dir).iter().find_map(|dir| AgentLogService::load_run_inputs(&root, dir, job_id).ok().flatten())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn prompt() -> JobPrompt {
        let mut prompt = JobPrompt::default();
        prompt.section("Ticket context");
        prompt.push_str("Ticket context:\n- ID: T-1 é\n");
        prompt.push('\n');
        prompt.section("User request");
        prompt.push_str("User request:\ngo\n");
        prompt
    }

    fn slice(text: &str, section: &AgentJobPromptSection) -> String {
        text.chars().skip(section.start).take(section.length).collect()
    }

    #[test]
    fn sections_cover_the_prompt_in_code_points_also_after_a_hand_off_in_front_and_a_handoff_after() {
        let mut prompt = prompt();
        prompt.prepend("Merge hand-off", "Merge hand-off: ü\n\n");
        prompt.append("Handoff", "\nHandoff:\nidle\n");
        let sections = prompt.sections();
        let headings: Vec<&str> = sections.iter().map(|section| section.heading.as_str()).collect();
        assert_eq!(headings, ["Merge hand-off", "Ticket context", "User request", "Handoff"]);
        assert_eq!(sections.iter().map(|section| slice(&prompt.text, section)).collect::<String>(), prompt.text, "no gaps, no overlap");
        assert_eq!(slice(&prompt.text, &sections[1]), "Ticket context:\n- ID: T-1 é\n\n");
        assert_eq!(slice(&prompt.text, &sections[2]), "User request:\ngo\n\n", "the handoff's leading blank line stays before it");
        assert_eq!(slice(&prompt.text, &sections[3]), "Handoff:\nidle\n");
    }

    #[test]
    fn env_values_and_mcp_secrets_never_appear() {
        let secret = "s3cret-value-123";
        let session_profile = harnessmux::Profile {
            model: Some("sonnet".to_string()),
            env: BTreeMap::from([("API_TOKEN".to_string(), secret.to_string())]),
            mcp_servers: BTreeMap::from([
                (
                    "lotar".to_string(),
                    harnessmux::McpServer::Stdio {
                        command: "lotar".to_string(),
                        args: vec!["mcp".to_string()],
                        env: BTreeMap::from([("MCP_KEY".to_string(), secret.to_string())]),
                    },
                ),
                (
                    "web".to_string(),
                    harnessmux::McpServer::Http {
                        url: "http://localhost:1/mcp".to_string(),
                        headers: BTreeMap::from([("Authorization".to_string(), format!("Bearer {secret}"))]),
                    },
                ),
            ]),
            rules: harnessmux::Rules { deny_commands: vec!["git push".to_string()], confine_writes: true, ..harnessmux::Rules::default() },
            ..harnessmux::Profile::default()
        };
        let profile = AgentProfileDetail {
            runner: "claude".to_string(),
            env: BTreeMap::from([("PROFILE_SECRET".to_string(), secret.to_string())]),
            ..AgentProfileDetail::default()
        };
        let start = SessionStart { agent: "claude", name: "T-1-fix", resume: false, cwd: std::path::Path::new("/w"), agent_env: vec!["AGENT_VAR".to_string()] };
        let inputs = session_inputs(&prompt(), AgentRunnerKind::Claude, Some("fix".to_string()), &profile, &session_profile, &start);
        let line = serde_json::to_string(&inputs).unwrap();
        assert!(!line.contains(secret), "{line}");
        assert_eq!(inputs.env, ["AGENT_VAR", "API_TOKEN"]);
        assert_eq!(inputs.settings.mcp_servers[0].env, ["MCP_KEY"]);
        assert_eq!(inputs.settings.mcp_servers[0].tools, crate::services::agent_runner::LOTAR_MCP_JOB_TOOLS);
        assert_eq!(inputs.settings.mcp_servers[1].headers, ["Authorization"]);
        assert_eq!(inputs.settings.rules.deny_commands, ["git push"]);
        assert_eq!(inputs.settings.on_permission.as_deref(), Some("allow"), "the workspace default policy");
        assert_eq!(inputs.settings.session.as_ref().map(|session| session.mode.as_str()), Some("new"));

        let command = command_inputs(&prompt(), None, &profile, std::path::Path::new("/w"), ["LOTAR_BIN".to_string()].into_iter());
        let line = serde_json::to_string(&command).unwrap();
        assert!(!line.contains(secret), "{line}");
        assert_eq!(command.env, ["LOTAR_BIN", "PROFILE_SECRET"]);
    }
}
