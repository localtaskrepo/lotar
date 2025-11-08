use clap::CommandFactory;
use clap_complete::{generate, generate_to};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::cli::args::completions::{CompletionShell, CompletionsAction, CompletionsArgs};
use crate::cli::handlers::{CommandHandler, emit_subcommand_overview};
use crate::output::OutputRenderer;
use crate::workspace::TasksDirectoryResolver;

pub struct CompletionsHandler;

impl CommandHandler for CompletionsHandler {
    type Args = CompletionsArgs;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        _project: Option<&str>,
        _resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Self::Result {
        match args.action {
            None => {
                emit_subcommand_overview(renderer, &["completions"]);
                Ok(())
            }
            Some(CompletionsAction::Generate {
                shell,
                output,
                print,
            }) => {
                let (target_path, wrote_to_stdout) = match output {
                    Some(path) => {
                        ensure_parent_exists(&path).map_err(|err| {
                            format!(
                                "Failed to prepare output directory {}: {}",
                                path.display(),
                                err
                            )
                        })?;
                        let generated_path = write_completion(shell, Some(&path))
                            .map_err(|err| format!("Failed to write completion: {}", err))?;
                        if let Some(path) = generated_path.clone() {
                            renderer.emit_success(&format!(
                                "Generated {shell} completion at {}",
                                path.display()
                            ));
                        }
                        (generated_path, false)
                    }
                    None => {
                        write_completion(shell, None)
                            .map_err(|err| format!("Failed to write completion: {}", err))?;
                        (None, true)
                    }
                };

                if print {
                    let script = render_completion(shell)
                        .map_err(|err| format!("Failed to render completion script: {}", err))?;
                    renderer.emit_raw_stdout(&script);
                } else if wrote_to_stdout {
                    // Completion script already emitted to stdout
                } else if target_path.is_none() {
                    renderer.emit_warning(
                        "Completion script was written but resulting path is unknown",
                    );
                }

                Ok(())
            }
            Some(CompletionsAction::Install { shell }) => {
                let shells: Vec<CompletionShell> = match shell {
                    Some(shell) => vec![shell],
                    None => CompletionShell::supported_shells().to_vec(),
                };

                let mut installed_any = false;
                let mut warnings = Vec::new();

                for shell in shells {
                    let Some(target) = default_install_path(shell) else {
                        warnings.push(format!(
                            "Skipping {shell}: no default install location is configured for this shell"
                        ));
                        continue;
                    };

                    if let Some(parent) = target.parent() {
                        if let Err(err) = fs::create_dir_all(parent) {
                            warnings.push(format!(
                                "Could not create directory {} for {shell}: {}",
                                parent.display(),
                                err
                            ));
                            continue;
                        }
                    }

                    match write_completion(shell, Some(&target)) {
                        Ok(Some(actual)) => {
                            renderer.emit_success(&format!(
                                "Installed {shell} completion at {}",
                                actual.display()
                            ));
                            installed_any = true;
                        }
                        Ok(None) => {
                            warnings.push(format!(
                                "Installed {shell} completion but could not determine the target path"
                            ));
                            installed_any = true;
                        }
                        Err(err) => {
                            warnings.push(format!("Failed to install {shell} completion: {}", err));
                        }
                    }
                }

                for warning in warnings {
                    renderer.emit_warning(&warning);
                }

                if installed_any {
                    renderer.emit_info(
                        "Restart your shell or source the generated file to enable completions.",
                    );
                    Ok(())
                } else {
                    Err("No completions were installed. See warnings for details.".to_string())
                }
            }
        }
    }
}

fn ensure_parent_exists(path: &Path) -> io::Result<()> {
    if path.exists() && path.is_dir() {
        fs::create_dir_all(path)
    } else if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
        } else {
            Ok(())
        }
    } else {
        Ok(())
    }
}

fn write_completion(shell: CompletionShell, target: Option<&Path>) -> io::Result<Option<PathBuf>> {
    let mut cmd = crate::cli::Cli::command();
    let binary_name = cmd.get_name().to_string();
    let clap_shell = shell.as_clap_shell();

    match target {
        Some(path) if path.exists() && path.is_dir() => {
            let generated = generate_to(clap_shell, &mut cmd, &binary_name, path)?;
            Ok(Some(generated))
        }
        Some(path) => {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent)?;
                }
            }
            let mut file = File::create(path)?;
            generate(clap_shell, &mut cmd, &binary_name, &mut file);
            file.flush()?;
            Ok(Some(path.to_path_buf()))
        }
        None => {
            let mut stdout = io::stdout();
            generate(clap_shell, &mut cmd, &binary_name, &mut stdout);
            stdout.flush()?;
            Ok(None)
        }
    }
}

fn render_completion(shell: CompletionShell) -> Result<String, String> {
    let mut cmd = crate::cli::Cli::command();
    let mut buffer = Vec::new();
    let clap_shell = shell.as_clap_shell();
    let binary_name = cmd.get_name().to_string();
    generate(clap_shell, &mut cmd, &binary_name, &mut buffer);
    String::from_utf8(buffer)
        .map_err(|err| format!("Completion script is not valid UTF-8: {}", err))
}

fn default_install_path(shell: CompletionShell) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    match shell {
        CompletionShell::Bash => {
            let base = std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".local/share"));
            Some(base.join("bash-completion/completions/lotar"))
        }
        CompletionShell::Zsh => {
            if let Some(zdotdir) = std::env::var_os("ZDOTDIR") {
                return Some(PathBuf::from(zdotdir).join("completions/_lotar"));
            }
            if let Some(zsh) = std::env::var_os("ZSH") {
                return Some(PathBuf::from(zsh).join("completions/_lotar"));
            }
            if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
                return Some(PathBuf::from(data_home).join("zsh/site-functions/_lotar"));
            }
            Some(home.join(".zsh/completions/_lotar"))
        }
        CompletionShell::Fish => Some(home.join(".config/fish/completions/lotar.fish")),
        CompletionShell::Powershell => {
            let base = std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".local/share"));
            Some(base.join("powershell/Scripts/lotar.ps1"))
        }
        CompletionShell::Elvish => Some(home.join(".config/elvish/lib/lotar.elv")),
    }
}
