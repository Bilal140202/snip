mod cli;
mod core;
mod detect;
mod ui;
mod utils;

use clap::{Parser, Subcommand};

use clap::CommandFactory;

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
    List,
    /// Execute a snippet
    Run {
        /// Snippet key or fuzzy query.
        name: String,
    },
    /// Import snippets from another project
    Import(cli::import::ImportCmd),
    /// Validate snippets and report issues
    Doctor,
    /// Generate shell completions
    Completions(cli::completions::CompletionsCmd),
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Handle `snip completions` before the default path so we can pass
    // the `&mut Command` to clap_complete.
    match &cli.command {
        Some(Commands::Completions(cmd)) => {
            return cmd.run(&mut Cli::command());
        }
        _ => {}
    }

    match cli.command {
        Some(Commands::Init) => cli::init::run(),
        Some(Commands::Add { name, cmd, description }) => {
            cli::add::run(&name, &cmd, description.as_deref())
        }
        Some(Commands::Rm { name }) => cli::rm::run(&name),
        Some(Commands::Edit) => cli::edit::run(),
        Some(Commands::List) => cli::list::run(),
        Some(Commands::Run { name }) => cli::run::run(&name),
        Some(Commands::Import(cmd)) => cmd.run(),
        Some(Commands::Doctor) => cli::doctor::run(),
        Some(Commands::Completions(_)) => unreachable!(),
        None => cli::list::run(),
    }
}