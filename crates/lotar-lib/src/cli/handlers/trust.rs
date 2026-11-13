//! `lotar trust`: approve this repository's automation commands and agent
//! profiles (DEV-259), like `direnv allow`.

use crate::config::trust::{TrustService, TrustState};
use crate::output::{OutputFormat, OutputRenderer};
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
        if json {
            renderer.emit_json(&serde_json::json!({
                "trusted": trusted,
                "commands": commands,
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
            if let Some(run) = action.get("run") {
                let command = match run {
                    serde_yaml_ng::Value::String(raw) => raw.clone(),
                    other => serde_json::to_string(other).unwrap_or_default(),
                };
                out.push((name.clone(), event.as_str().unwrap_or_default().to_string(), command));
            }
        }
    }
    out
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
        std::fs::write(tasks.join("A").join("automation.yml"), "automation:\n  rules:\n    - name: hi\n      on:\n        created:\n          run: echo hi\n")
            .unwrap();
        let described = describe(tasks).join("\n");
        assert!(described.contains("rule 'hi' on created runs echo hi"), "{described}");
        assert!(described.contains("agent profile 'bot'"), "{described}");
    }
}
