//! `lotar trust`: approve this repository's automation commands and agent
//! profiles (DEV-259), like `direnv allow`.

use crate::automation::types::{AutomationRunAction, AutomationRunCommand};
use crate::config::trust::{TrustService, TrustState};
use crate::output::{OutputFormat, OutputRenderer};
use crate::services::automation_service::AutomationService;
use crate::workspace::TasksDirectoryResolver;
use std::path::Path;

pub struct TrustHandler;

impl TrustHandler {
    pub fn execute(status: bool, revoke: bool, resolver: &TasksDirectoryResolver, renderer: &OutputRenderer) -> Result<(), String> {
        let tasks_dir = resolver.path.as_path();
        let json = matches!(renderer.format, OutputFormat::Json);

        if revoke {
            let removed = TrustService::revoke(tasks_dir).map_err(|e| e.to_string())?;
            if json {
                renderer.emit_json(&serde_json::json!({ "revoked": removed }));
            } else if removed {
                renderer.emit_success(format!("Stopped trusting {}", tasks_dir.display()));
            } else {
                renderer.emit_notice(format!("{} was not trusted", tasks_dir.display()));
            }
            return Ok(());
        }

        let commands = describe(tasks_dir);
        if status {
            let state = TrustService::state(tasks_dir);
            if json {
                renderer.emit_json(&serde_json::json!({
                    "state": state.as_str(),
                    "allows_commands": state.allows_commands(),
                    "commands": commands,
                }));
                return Ok(());
            }
            let line = match state {
                TrustState::NothingToTrust => "Nothing to trust: no automation rules or agent profiles in this repository".to_string(),
                TrustState::Trusted => format!("Trusted: {}", tasks_dir.display()),
                TrustState::Changed => format!("Changed since you trusted it: {} (run `lotar trust` after reviewing)", tasks_dir.display()),
                TrustState::Untrusted => format!("Not trusted: {} (run `lotar trust` after reviewing)", tasks_dir.display()),
            };
            renderer.emit_info(line);
            for command in &commands {
                renderer.emit_raw_stdout(format_args!("  {command}"));
            }
            return Ok(());
        }

        let trusted = TrustService::trust(tasks_dir).map_err(|e| e.to_string())?;
        let skipped_runs = if trusted { AutomationService::rerun_skipped_runs(tasks_dir) } else { Vec::new() };
        if json {
            renderer.emit_json(&serde_json::json!({
                "trusted": trusted,
                "commands": commands,
                "skipped_runs": skipped_runs,
            }));
            return Ok(());
        }
        if !trusted {
            renderer.emit_notice("Nothing to trust: no automation rules or agent profiles in this repository");
            return Ok(());
        }
        renderer.emit_success(format!("Trusted {}", tasks_dir.display()));
        renderer.emit_info("These can now run commands on this machine:");
        for command in &commands {
            renderer.emit_raw_stdout(format_args!("  {command}"));
        }
        renderer.emit_info("Any change to these files (a pull, a merge) needs `lotar trust` again; edits made through LoTaR keep the trust.");
        if !skipped_runs.is_empty() {
            renderer.emit_info("Rule runs skipped while untrusted:");
        }
        for run in &skipped_runs {
            let what = match (run.result, &run.error) {
                ("ran", _) => "ran now".to_string(),
                ("failed", Some(err)) => format!("failed now: {err}"),
                _ => "not run: the rule's run command changed since".to_string(),
            };
            renderer.emit_raw_stdout(format_args!("  {} rule '{}' (skipped {}): {what}", run.ticket, run.rule, run.skipped_at));
        }
        Ok(())
    }
}

/// Lines describing what trusting would allow: every `run` action and
/// every repository agent profile.
fn describe(tasks_dir: &Path) -> Vec<String> {
    let mut lines = Vec::new();
    for source in TrustService::sources(tasks_dir) {
        match source.kind {
            "automation" => {
                let runs = run_commands(source.content());
                if runs.is_empty() {
                    lines.push(format!("{}: no run commands", source.path));
                }
                for (rule, event, command) in runs {
                    lines.push(format!("{}: rule '{rule}' on {event} runs {command}", source.path));
                }
            }
            "merge" => {
                let detail = source.content().trim().replace('\n', ", ");
                lines.push(format!("{}: lotar's merge runs ({detail})", source.path));
            }
            "waiting_run" => {
                let command =
                    serde_yaml_ng::from_str::<Vec<String>>(source.content()).map_or_else(|_| source.content().trim().to_string(), |command| command.join(" "));
                lines.push(format!("{}: runs `{command}` in a waiting ticket's worktrees (agent.worktree.waiting_run)", source.path));
            }
            _ => {
                let Ok(serde_yaml_ng::Value::Mapping(map)) = serde_yaml_ng::from_str::<serde_yaml_ng::Value>(source.content()) else {
                    continue;
                };
                for (name, profile) in map {
                    let name = name.as_str().unwrap_or_default().to_string();
                    let detail = serde_yaml_ng::to_string(&profile).unwrap_or_default().trim().replace('\n', ", ");
                    lines.push(format!("{}: agent profile '{name}' ({detail})", source.path));
                }
            }
        }
    }
    lines
}

/// (rule name, event, command) for every `run` in an automation file.
fn run_commands(content: &str) -> Vec<(String, String, String)> {
    let Ok(file) = serde_yaml_ng::from_str::<crate::automation::types::AutomationFile>(content) else {
        return vec![("?".to_string(), "?".to_string(), "(file does not parse)".to_string())];
    };
    let mut out = Vec::new();
    for (index, rule) in file.automation.rules().iter().enumerate() {
        let name = rule.name.clone().unwrap_or_else(|| format!("#{}", index + 1));
        let value = serde_yaml_ng::to_value(&rule.on).unwrap_or_default();
        let Some(map) = value.as_mapping() else {
            continue;
        };
        for (event, action) in map {
            // A serialized action without a run command holds `run: null`.
            if let Some(run) = action.get("run").filter(|run| !run.is_null()) {
                let command = match serde_yaml_ng::from_value::<AutomationRunAction>(run.clone()) {
                    Ok(AutomationRunAction::Shell(raw)) => raw,
                    Ok(AutomationRunAction::Command(command)) => describe_run_command(&command),
                    Err(_) => serde_json::to_string(run).unwrap_or_default(),
                };
                out.push((name.clone(), event.as_str().unwrap_or_default().to_string(), command));
            }
        }
    }
    out
}

/// A structured `run` as its command line, then the settings that differ
/// from their defaults: `echo 'a b' (env A=1, cwd sub)`.
fn describe_run_command(run: &AutomationRunCommand) -> String {
    let line = std::iter::once(&run.command).chain(&run.args).map(|word| quote_word(word)).collect::<Vec<_>>().join(" ");
    let mut settings = Vec::new();
    if !run.env.is_empty() {
        let mut env: Vec<_> = run.env.iter().collect();
        env.sort();
        let env = env.into_iter().map(|(key, value)| format!("{key}={}", quote_word(value))).collect::<Vec<_>>().join(" ");
        settings.push(format!("env {env}"));
    }
    if let Some(cwd) = &run.cwd {
        settings.push(format!("cwd {}", quote_word(cwd)));
    }
    if run.ignore_failure {
        settings.push("ignore_failure".to_string());
    }
    if !run.wait {
        settings.push("no wait".to_string());
    }
    if let Some(timeout) = &run.timeout {
        settings.push(format!("timeout {timeout}"));
    }
    if settings.is_empty() { line } else { format!("{line} ({})", settings.join(", ")) }
}

/// Single-quotes a word a shell would split or unquote.
fn quote_word(word: &str) -> String {
    if !word.is_empty() && !word.chars().any(|c| c.is_whitespace() || c == '\'' || c == '"') {
        return word.to_string();
    }
    format!("'{}'", word.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_run_commands_and_agent_profiles() {
        let dir = tempfile::tempdir().unwrap();
        let tasks = dir.path();
        std::fs::write(tasks.join("config.yml"), "default:\n  project: A\nagents:\n  bot:\n    runner: command\n    command: sh\n").unwrap();
        std::fs::create_dir_all(tasks.join("A")).unwrap();
        std::fs::write(tasks.join("A").join("automation.yml"), "automation:\n  rules:\n    - name: hi\n      on:\n        created:\n          run: echo hi\n    - name: args\n      on:\n        updated:\n          run: {command: echo, args: [x]}\n",
        )
        .unwrap();
        let described = describe(tasks).join("\n");
        assert!(described.contains("rule 'hi' on created runs echo hi"), "{described}");
        assert!(described.lines().any(|line| line.ends_with("rule 'args' on updated runs echo x")), "{described}");
        assert!(described.contains("agent profile 'bot'"), "{described}");
    }

    /// DEV-847: rules without a run command are not listed as `runs null`.
    #[test]
    fn skips_rules_without_a_run_command() {
        let dir = tempfile::tempdir().unwrap();
        let tasks = dir.path();
        std::fs::write(
            tasks.join("automation.yml"),
            "automation:\n  rules:\n    - name: hi\n      on:\n        created:\n          run: echo hi\n    - name: tag\n      on:\n        updated:\n          set:\n            tags: [x]\n",
        )
        .unwrap();
        assert_eq!(describe(tasks), vec!["automation.yml: rule 'hi' on created runs echo hi"]);

        std::fs::write(
            tasks.join("automation.yml"),
            "automation:\n  rules:\n    - name: tag\n      on:\n        updated:\n          set:\n            tags: [x]\n",
        )
        .unwrap();
        assert_eq!(describe(tasks), vec!["automation.yml: no run commands"]);
    }

    #[test]
    fn describes_a_structured_run_like_its_command_line() {
        let parse = |yaml: &str| serde_yaml_ng::from_str::<AutomationRunCommand>(yaml).unwrap();
        assert_eq!(describe_run_command(&parse("{command: echo, args: [x]}")), "echo x");
        assert_eq!(describe_run_command(&parse("{command: echo, args: ['a b', '', \"it's\"]}")), "echo 'a b' '' 'it'\\''s'");
        assert_eq!(
            describe_run_command(&parse("{command: sh, env: {B: '2', A: 'x y'}, cwd: sub, ignore_failure: true, wait: false, timeout: 10s}")),
            "sh (env A='x y' B=2, cwd sub, ignore_failure, no wait, timeout 10s)"
        );
    }
}
