mod cli;
mod core;
mod detect;
mod ui;
mod utils;

use clap::{Parser, Subcommand};

/// Project-scoped command snippets with built-in fuzzy finder.
#[derive(Parser)]
#[command(name = "snip", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Create / detect .snips file
    Init,
    /// Add a new snippet
    Add {
        /// Fully-qualified snippet key (e.g. `build.release`).
        name: String,
        /// The shell command to run.
        cmd: String,
        /// Human-readable description.
        description: Option<String>,
    },
    /// Remove a snippet
    Rm {
        /// Snippet key to remove.
        name: String,
    },
    /// Open .snips in $EDITOR
    Edit,
    /// List snippets (optionally filtered)
    #[command(alias = "ls")]
    List(cli::list::ListCmd),
    /// Execute a snippet
    Run {
        /// Snippet key, fuzzy query, or use -i for interactive picker.
        name: Option<String>,

        /// Launch interactive picker (uses fzf if available).
        #[arg(short, long)]
        interactive: bool,
    },
    /// Import snippets from another project
    Import(cli::import::ImportCmd),
    /// Validate snippets and report issues
    Doctor(cli::doctor::DoctorCmd),
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for.
        shell: String,
    },
    /// Print shell integration code (use: eval "$(snip hook)")
    Hook(cli::hook::HookCmd),
    /// Suggest snippets from shell history
    Suggest {
        /// Show all suggestions, not just top 10.
        #[arg(long)]
        all: bool,

        /// Interactively add top N suggestions to .snips.
        #[arg(long)]
        add: Option<usize>,
    },
    /// Explain what a snippet command does
    Explain {
        /// Snippet name or raw command to explain.
        name: String,
    },
    /// Detect unused or outdated snippets
    Stale {
        /// Automatically fix fixable issues.
        #[arg(long)]
        fix: bool,

        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Interactive team onboarding wizard
    Setup,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Handle `snip completions` before the default path
    match &cli.command {
        Some(Commands::Completions { shell }) => {
            return cli::completions::generate_completions(shell);
        }
        Some(Commands::Hook(cmd)) => {
            return cmd.run();
        }
        _ => {}
    }

    // Handle hidden `snip _complete` for dynamic shell completions
    // This is called by shell completion scripts
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1] == "_complete" {
        let kind = &args[2];
        let partial = args.get(3).map(|s| s.as_str());
        return cli::completions::run_complete(kind, partial);
    }

    match cli.command {
        Some(Commands::Init) => cli::init::run(),
        Some(Commands::Add { name, cmd, description }) => {
            cli::add::run(&name, &cmd, description.as_deref())
        }
        Some(Commands::Rm { name }) => cli::rm::run(&name),
        Some(Commands::Edit) => cli::edit::run(),
        Some(Commands::List(opts)) => opts.run(),
        Some(Commands::Run { name: Some(name), interactive: false }) => cli::run::run(&name),
        Some(Commands::Run { name: None, .. }) | Some(Commands::Run { name: Some(_), interactive: true }) => {
            cli::run::run_interactive()
        }
        Some(Commands::Import(cmd)) => cmd.run(),
        Some(Commands::Doctor(cmd)) => cmd.run(),
        Some(Commands::Completions { .. }) => unreachable!(),
        Some(Commands::Hook(_)) => unreachable!(),
        Some(Commands::Suggest { all, add }) => cli::suggest::run(all, add),
        Some(Commands::Explain { name }) => cli::explain::run(&name),
        Some(Commands::Stale { fix, json }) => cli::stale::run(fix, json),
        Some(Commands::Setup) => cli::setup::run(),
        None => {
            // No subcommand → list snippets (with auto-init)
            let opts = cli::list::ListCmd {
                json: false,
                format: None,
                section: None,
                interactive: false,
            };
            opts.run()
        }
    }
}