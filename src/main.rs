// Bin crate root. The lib (src/lib.rs) exposes a public API; some pub fns
// are not exercised by the CLI but form the library surface. Allow that
// here so `cargo clippy --all-targets` stays clean.
#![allow(dead_code)]

mod cli;
mod core;
mod detect;
mod ui;
mod utils;

use clap::{Parser, Subcommand};

use crate::cli::run::RunOptions;

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
    /// Rename a snippet (preserves all metadata)
    Rename {
        /// Old snippet key.
        old: String,
        /// New snippet key.
        new: String,
    },
    /// Move a snippet to a different section (preserves leaf name)
    Mv {
        /// Snippet key to move (e.g. `build.release`).
        name: String,
        /// Target section (use `_` or `-` to move to top-level).
        section: String,
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

        #[command(flatten)]
        opts: RunOptions,
    },
    /// Search snippets by free-text query
    Search(cli::search::SearchCmd),
    /// List snippets by tag (optionally run one)
    Tag(cli::tag::TagCmd),
    /// Import snippets from another project or a GitHub gist URL
    Import(cli::import::ImportCmd),
    /// Export a snippet to clipboard or stdout
    Export(cli::export::ExportCmd),
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
    // Handle hidden `snip _complete` for dynamic shell completions FIRST,
    // before any clap parsing — clap would reject `_complete` as unknown.
    let raw_args: Vec<String> = std::env::args().collect();
    if raw_args.len() >= 3 && raw_args[1] == "_complete" {
        let kind = &raw_args[2];
        let partial = raw_args.get(3).map(|s| s.as_str());
        return cli::completions::run_complete(kind, partial);
    }

    // Parse args. If clap rejects them because the first positional looks
    // like an unknown subcommand, fall back to natural-language execution:
    //   snip "deploy staging"   →   snip run-by-phrase "deploy staging"
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(_) => {
            // Only attempt NL fallback if there's exactly one positional arg
            // (the phrase). Otherwise re-emit the clap error so users see
            // real usage errors.
            let positional: Vec<&String> = raw_args
                .iter()
                .skip(1)
                .filter(|a| !a.starts_with('-'))
                .collect();
            if positional.len() == 1 {
                let phrase = positional[0].as_str();
                // Don't intercept if it's actually a known subcommand typo
                // (clap would have suggested a fix in its error). Just try NL.
                return cli::run::run_natural_language(phrase);
            }
            // Re-parse to surface clap's error message + exit code
            Cli::parse();
            unreachable!();
        }
    };

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

    match cli.command {
        Some(Commands::Init) => cli::init::run(),
        Some(Commands::Add {
            name,
            cmd,
            description,
        }) => cli::add::run(&name, &cmd, description.as_deref()),
        Some(Commands::Rm { name }) => cli::rm::run(&name),
        Some(Commands::Rename { old, new }) => cli::rename::run(&old, &new),
        Some(Commands::Mv { name, section }) => cli::mv::run(&name, &section),
        Some(Commands::Edit) => cli::edit::run(),
        Some(Commands::List(opts)) => opts.run(),
        Some(Commands::Run {
            name: Some(name),
            interactive: false,
            opts,
        }) => cli::run::run_with_options(&name, &opts),
        Some(Commands::Run { name: None, .. })
        | Some(Commands::Run {
            name: Some(_),
            interactive: true,
            ..
        }) => cli::run::run_interactive(),
        Some(Commands::Search(cmd)) => cmd.run(),
        Some(Commands::Tag(cmd)) => cmd.run(),
        Some(Commands::Import(cmd)) => cmd.run(),
        Some(Commands::Export(cmd)) => cmd.run(),
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
