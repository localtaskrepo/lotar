use clap::{Args, Subcommand, ValueEnum};
use std::fmt;
use std::path::PathBuf;

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum CompletionShell {
    Bash,
    Zsh,
    Fish,
    Powershell,
    Elvish,
}

impl CompletionShell {
    pub fn as_clap_shell(self) -> clap_complete::Shell {
        match self {
            CompletionShell::Bash => clap_complete::Shell::Bash,
            CompletionShell::Zsh => clap_complete::Shell::Zsh,
            CompletionShell::Fish => clap_complete::Shell::Fish,
            CompletionShell::Powershell => clap_complete::Shell::PowerShell,
            CompletionShell::Elvish => clap_complete::Shell::Elvish,
        }
    }

    pub fn supported_shells() -> &'static [CompletionShell] {
        &[
            CompletionShell::Bash,
            CompletionShell::Zsh,
            CompletionShell::Fish,
            CompletionShell::Powershell,
            CompletionShell::Elvish,
        ]
    }
}

impl fmt::Display for CompletionShell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            CompletionShell::Bash => "bash",
            CompletionShell::Zsh => "zsh",
            CompletionShell::Fish => "fish",
            CompletionShell::Powershell => "powershell",
            CompletionShell::Elvish => "elvish",
        })
    }
}

#[derive(Subcommand, Clone, Debug)]
pub enum CompletionsAction {
    /// Generate shell completion script to stdout or a file
    Generate {
        /// Target shell to generate completion for
        #[arg(long, value_enum)]
        shell: CompletionShell,
        /// Optional output path; defaults to stdout if omitted
        #[arg(long, short = 'o')]
        output: Option<PathBuf>,
        /// Also print the generated script to stdout after writing to file
        #[arg(long)]
        print: bool,
    },
    /// Install completion script into the default location for a shell
    Install {
        /// Shell to install for; installs all supported shells when omitted
        #[arg(long, value_enum)]
        shell: Option<CompletionShell>,
    },
}

#[derive(Args, Clone, Debug)]
pub struct CompletionsArgs {
    #[command(subcommand)]
    pub action: Option<CompletionsAction>,
}
