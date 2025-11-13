use crate::cli::handlers::{CommandHandler, emit_subcommand_overview};
use crate::cli::{GitAction, GitHooksAction, GitHooksInstallArgs};
use crate::output::{OutputFormat, OutputRenderer};
use crate::utils::git::find_repo_root;
use crate::workspace::TasksDirectoryResolver;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Handler for `lotar git` subcommands.
pub struct GitHandler;

impl CommandHandler for GitHandler {
    type Args = Option<GitAction>;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        _project: Option<&str>,
        _resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Self::Result {
        let Some(action) = args else {
            emit_subcommand_overview(renderer, &["git"]);
            return Ok(());
        };

        match action {
            GitAction::Hooks { action } => {
                let Some(hook_action) = action else {
                    emit_subcommand_overview(renderer, &["git", "hooks"]);
                    return Ok(());
                };

                match hook_action {
                    GitHooksAction::Install(install_args) => {
                        Self::handle_hooks_install(install_args, renderer)
                    }
                }
            }
        }
    }
}

impl GitHandler {
    fn handle_hooks_install(
        args: GitHooksInstallArgs,
        renderer: &OutputRenderer,
    ) -> Result<(), String> {
        let cwd = std::env::current_dir()
            .map_err(|err| format!("Failed to determine current directory: {err}"))?;
        let repo_root = find_repo_root(&cwd).ok_or_else(|| {
            "Git repository not found. Run this command inside the repository you want to configure."
                .to_string()
        })?;

        let hooks_dir = repo_root.join(".githooks");
        if !hooks_dir.is_dir() {
            return Err(format!(
                "Hooks directory '{}' not found. Ensure the repository includes a .githooks folder.",
                hooks_dir.display()
            ));
        }

        let scripts = Self::collect_hook_scripts(&hooks_dir)?;
        if scripts.is_empty() {
            return Err(format!(
                "No hook scripts found inside '{}'. Add scripts before installing.",
                hooks_dir.display()
            ));
        }

        let current_path = Self::read_hooks_path(&repo_root)?;
        let desired_path = ".githooks";
        let script_count = scripts.len();

        if args.dry_run {
            Self::emit_summary(
                renderer,
                desired_path,
                script_count,
                current_path.as_deref(),
                InstallState::DryRun,
            );
            return Ok(());
        }

        if let Some(ref current) = current_path {
            if current == desired_path {
                // Ensure permissions for scripts even if already configured.
                Self::ensure_executable(&scripts)?;
                Self::emit_summary(
                    renderer,
                    desired_path,
                    script_count,
                    current_path.as_deref(),
                    InstallState::AlreadyConfigured,
                );
                return Ok(());
            }

            if !args.force {
                return Err(format!(
                    "core.hooksPath is currently set to '{}'. Re-run with --force to overwrite.",
                    current
                ));
            }
        }

        Self::ensure_executable(&scripts)?;
        Self::write_hooks_path(&repo_root, desired_path)?;
        Self::emit_summary(
            renderer,
            desired_path,
            script_count,
            current_path.as_deref(),
            InstallState::Updated,
        );
        Ok(())
    }

    fn collect_hook_scripts(dir: &Path) -> Result<Vec<PathBuf>, String> {
        let mut scripts = Vec::new();
        let entries = fs::read_dir(dir)
            .map_err(|err| format!("Failed to read hooks directory '{}': {err}", dir.display()))?;

        for entry in entries {
            let entry = entry
                .map_err(|err| format!("Failed to read entry in '{}': {err}", dir.display()))?;
            let path = entry.path();
            if path.is_file()
                && !path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.starts_with('.'))
                    .unwrap_or(false)
                && !matches!(
                    path.extension().and_then(|ext| ext.to_str()),
                    Some(ext) if ext.eq_ignore_ascii_case("md")
                )
            {
                scripts.push(path);
            }
        }

        scripts.sort();
        Ok(scripts)
    }

    fn read_hooks_path(repo_root: &Path) -> Result<Option<String>, String> {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .args(["config", "--local", "--get", "core.hooksPath"])
            .output()
            .map_err(|err| format!("Failed to execute git: {err}"))?;

        if output.status.success() {
            let value = String::from_utf8(output.stdout)
                .map_err(|err| format!("Invalid UTF-8 from git config: {err}"))?;
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_string()))
            }
        } else if output.status.code() == Some(1) && output.stdout.is_empty() {
            // No value set.
            Ok(None)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(if stderr.trim().is_empty() {
                "Failed to read git configuration".to_string()
            } else {
                format!("Failed to read git configuration: {}", stderr.trim())
            })
        }
    }

    fn write_hooks_path(repo_root: &Path, value: &str) -> Result<(), String> {
        let status = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .args(["config", "--local", "core.hooksPath", value])
            .status()
            .map_err(|err| format!("Failed to execute git: {err}"))?;

        if status.success() {
            Ok(())
        } else {
            Err("git config returned a non-zero status".to_string())
        }
    }

    fn emit_summary(
        renderer: &OutputRenderer,
        path: &str,
        script_count: usize,
        previous: Option<&str>,
        state: InstallState,
    ) {
        match renderer.format {
            OutputFormat::Json => {
                let payload = json!({
                    "status": "success",
                    "action": "git_hooks_install",
                    "path": path,
                    "scripts": script_count,
                    "previous": previous,
                    "state": state.label(),
                });
                renderer.emit_json(&payload);
            }
            _ => {
                let base_message = match state {
                    InstallState::DryRun => format!(
                        "Would set git core.hooksPath to '{}' ({} hook script{} detected).",
                        path,
                        script_count,
                        if script_count == 1 { "" } else { "s" }
                    ),
                    InstallState::AlreadyConfigured => format!(
                        "Git hooks already configured for '{}' ({} hook script{} ensured executable).",
                        path,
                        script_count,
                        if script_count == 1 { "" } else { "s" }
                    ),
                    InstallState::Updated => {
                        let previous_hint = previous
                            .map(|prev| format!(" (previously '{}')", prev))
                            .unwrap_or_default();
                        format!(
                            "Configured git core.hooksPath to '{}'{} ({} hook script{} ready).",
                            path,
                            previous_hint,
                            script_count,
                            if script_count == 1 { "" } else { "s" }
                        )
                    }
                };

                match state {
                    InstallState::DryRun => renderer.emit_notice(&base_message),
                    InstallState::AlreadyConfigured => renderer.emit_notice(&base_message),
                    InstallState::Updated => renderer.emit_success(&base_message),
                }
            }
        }
    }

    fn ensure_executable(scripts: &[PathBuf]) -> Result<(), String> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            for script in scripts {
                let metadata = fs::metadata(script).map_err(|err| {
                    format!(
                        "Failed to read permissions for '{}': {err}",
                        script.display()
                    )
                })?;
                let mut permissions = metadata.permissions();
                let mode = permissions.mode();
                let desired = mode | 0o755;
                if mode != desired {
                    permissions.set_mode(desired);
                    fs::set_permissions(script, permissions).map_err(|err| {
                        format!(
                            "Failed to set executable permissions for '{}': {err}",
                            script.display()
                        )
                    })?;
                }
            }
        }

        #[cfg(not(unix))]
        {
            let _ = scripts;
        }

        Ok(())
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum InstallState {
    DryRun,
    AlreadyConfigured,
    Updated,
}

impl InstallState {
    fn label(self) -> &'static str {
        match self {
            InstallState::DryRun => "dry_run",
            InstallState::AlreadyConfigured => "unchanged",
            InstallState::Updated => "updated",
        }
    }
}
