use clap::{Args, ValueEnum};
use clap_complete::{generate, shells};

use anyhow::Result;

/// Generate shell completions.
#[derive(Debug, Args)]
pub struct CompletionsCmd {
    /// Shell to generate completions for.
    pub shell: Shell,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
    Elvish,
    PowerShell,
}

impl Shell {
    fn to_clap_shell(&self) -> shells::Shell {
        match self {
            Shell::Bash => shells::Shell::Bash,
            Shell::Zsh => shells::Shell::Zsh,
            Shell::Fish => shells::Shell::Fish,
            Shell::Elvish => shells::Shell::Elvish,
            Shell::PowerShell => shells::Shell::PowerShell,
        }
    }
}

impl CompletionsCmd {
    pub fn run(&self, app: &mut clap::Command) -> Result<()> {
        let shell = self.shell.to_clap_shell();
        generate(shell, app, "snip", &mut std::io::stdout());
        Ok(())
    }
}