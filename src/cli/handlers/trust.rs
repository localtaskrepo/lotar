//! `lotar trust`: approve this repository's automation commands and agent
//! profiles (DEV-259), like `direnv allow`.

use crate::output::{OutputFormat, OutputRenderer};
use crate::services::trust_service::{TrustService, TrustState};
use crate::workspace::TasksDirectoryResolver;

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

        let commands = TrustService::describe(tasks_dir);
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
