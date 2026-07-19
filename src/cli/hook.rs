//! `snip hook` — Print shell integration code for eval "$(snip hook)".

use clap::ValueEnum;
use anyhow::Result;

/// Generate shell integration code.
#[derive(Debug, clap::Args)]
pub struct HookCmd {
    /// Shell to generate hook for (auto-detected if not specified).
    pub shell: Option<Shell>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
    Nushell,
}

impl Shell {
    fn as_str(self) -> &'static str {
        match self {
            Shell::Bash => "bash",
            Shell::Zsh => "zsh",
            Shell::Fish => "fish",
            Shell::Nushell => "nushell",
        }
    }
}

impl HookCmd {
    pub fn run(&self) -> Result<()> {
        let shell = match &self.shell {
            Some(s) => *s,
            None => detect_shell(),
        };

        let script = match shell {
            Shell::Bash => include_str!("../../completions/snip.bash"),
            Shell::Zsh => include_str!("../../completions/snip.zsh"),
            Shell::Fish => include_str!("../../completions/snip.fish"),
            Shell::Nushell => NUSHELL_COMPLETIONS,
        };

        print!("{}", script);
        Ok(())
    }
}

fn detect_shell() -> Shell {
    let shell_env = std::env::var("SHELL").unwrap_or_default();
    if shell_env.contains("zsh") {
        return Shell::Zsh;
    }
    if shell_env.contains("fish") {
        return Shell::Fish;
    }
    if shell_env.contains("nu") {
        return Shell::Nushell;
    }
    // Default to bash
    Shell::Bash
}

const NUSHELL_COMPLETIONS: &str = r#"# snip completions for Nushell
extern "snip" [
    _complete
]

# Complete subcommands and snippet names
def "nu-complete snip" [] {
    let commands = ["init" "add" "rm" "edit" "list" "run" "import" "doctor" "completions" "hook" "suggest" "explain" "stale" "setup"]
    let snippets = (try { snip _complete snippets } | split row "\n")
    $commands | append $snippets
}

def "nu-complete snip-snippets" [] {
    try { snip _complete snippets } | split row "\n"
}

def "nu-complete snip-shells" [] {
    ["bash" "zsh" "fish" "nushell" "elvish" "powershell"]
}
"#;