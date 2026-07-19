use std::io::{self, BufRead, Write};

use anyhow::{Context, Result};
use colored::Colorize;

use crate::core::history::{detect_history_path, parse_history, suggest_from_history, Suggestion};
use crate::core::snippet::{Snippet, SnipFile};
use crate::core::snipfile::{find_snipfile, read_snippets, write_snippets};

/// Default number of suggestions to show.
const DEFAULT_LIMIT: usize = 10;

/// Run `snip suggest` — analyze shell history and suggest snippet candidates.
pub fn run(show_all: bool, add_count: Option<usize>) -> Result<()> {
    let limit = if show_all { usize::MAX } else { DEFAULT_LIMIT };

    // Detect history path
    let history_path = match detect_history_path() {
        Some(p) => p,
        None => {
            eprintln!("{}", "No shell history file found.".dimmed());
            eprintln!(
                "Searched for ~/.bash_history, ~/.zsh_history, and ~/.local/share/fish/fish_history"
            );
            return Ok(());
        }
    };

    // Parse history
    let entries = parse_history(&history_path)
        .with_context(|| format!("failed to parse history at {}", history_path.display()))?;

    if entries.is_empty() {
        println!("{}", "No commands found in shell history.".dimmed());
        return Ok(());
    }

    // Load existing snipfile (if any)
    let cwd = std::env::current_dir().context("failed to get current directory")?;
    let existing = match find_snipfile(Some(&cwd))? {
        Some(path) => read_snippets(&path)?,
        None => SnipFile::new(),
    };

    // Generate suggestions
    let suggestions = suggest_from_history(&entries, &existing, limit);

    if suggestions.is_empty() {
        println!("{}", "No good snippet candidates found in history.".dimmed());
        println!("Try running more commands, or use {} to add snippets manually.", "snip add".cyan());
        return Ok(());
    }

    // Handle --add mode
    if let Some(n) = add_count {
        return add_suggestions(suggestions, n, &cwd);
    }

    // Display suggestions
    println!(
        "{} {} suggestions from {}",
        "📋".dimmed(),
        suggestions.len().to_string().bold(),
        history_path.display().to_string().dimmed()
    );
    println!();

    for s in suggestions.iter() {
        let freq_str = format!("{} run{}", s.frequency, if s.frequency != 1 { "s" } else { "" });
        println!(
            "  [{} {}] {}",
            freq_str.cyan(),
            format!("score {:.1}", s.recency_score).dimmed(),
            s.cmd
        );
    }

    println!();
    println!(
        "  To add suggestions, run: {}",
        "snip suggest --add 3".cyan()
    );

    Ok(())
}

/// Interactively add the top N suggestions to the .snips file.
fn add_suggestions(suggestions: Vec<Suggestion>, count: usize, cwd: &std::path::Path) -> Result<()> {
    let to_add = &suggestions[..count.min(suggestions.len())];

    println!(
        "{} {} suggestion{} to add:",
        "➕".dimmed(),
        to_add.len().to_string().bold(),
        if to_add.len() != 1 { "s" } else { "" }
    );
    println!();

    // Find or create the snipfile path
    let snipfile_path = match find_snipfile(Some(cwd))? {
        Some(p) => p,
        None => cwd.join(".snips"),
    };

    let mut file = if snipfile_path.exists() {
        read_snippets(&snipfile_path)?
    } else {
        SnipFile::new()
    };

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for (i, s) in to_add.iter().enumerate() {
        let freq_str = format!("{} run{}", s.frequency, if s.frequency != 1 { "s" } else { "" });
        println!(
            "  {}. {}",
            (i + 1).to_string().cyan().bold(),
            s.cmd
        );
        println!(
            "     {} (score {:.1})",
            freq_str.dimmed(),
            s.recency_score
        );

        // Suggest a default name from the command
        let default_name = suggest_name(&s.cmd);

        // Prompt for name
        print!("     Name [{}]: ", default_name.cyan());
        stdout.flush()?;
        let mut name_input = String::new();
        stdin.lock().read_line(&mut name_input)?;
        let name = name_input.trim();
        let name = if name.is_empty() { &default_name } else { name };

        // Prompt for description
        let default_desc = suggest_desc(&s.cmd);
        print!("     Desc [{}]: ", default_desc.dimmed());
        stdout.flush()?;
        let mut desc_input = String::new();
        stdin.lock().read_line(&mut desc_input)?;
        let desc = desc_input.trim();
        let desc = if desc.is_empty() { &default_desc } else { desc };

        let snippet = Snippet::new(&s.cmd).with_desc(desc);
        file.insert(name, snippet);

        println!("     {} Added as '{}'\n", "✓".green(), name);
    }

    // Write the snipfile
    write_snippets(&snipfile_path, &file)?;
    println!(
        "  {} {} snippet{} to {}",
        "✓".green().bold(),
        to_add.len(),
        if to_add.len() != 1 { "s" } else { "" },
        snipfile_path.display().to_string().dimmed()
    );

    Ok(())
}

/// Suggest a snippet name from a command string.
///
/// For `cargo build --release` → `cargo-build`
/// For `docker compose up --build` → `docker-compose-up`
fn suggest_name(cmd: &str) -> String {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    if parts.is_empty() {
        return String::new();
    }

    // Take the binary name (last component of path)
    let base = parts[0].rsplit('/').next().unwrap_or(parts[0]);

    // Take up to 2 subcommand words
    let mut name_parts = vec![base.to_string()];
    for part in parts.iter().skip(1) {
        if part.starts_with('-') {
            break;
        }
        name_parts.push(part.to_string());
        if name_parts.len() >= 3 {
            break;
        }
    }

    name_parts.join("-")
}

/// Suggest a description from a command string.
fn suggest_desc(cmd: &str) -> String {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    if parts.len() < 2 {
        return cmd.to_string();
    }

    // Join first few words as a description
    let words: Vec<&str> = parts.iter().take(4).cloned().collect();
    words.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_suggest_name() {
        assert_eq!(suggest_name("cargo build --release"), "cargo-build");
        assert_eq!(suggest_name("docker compose up --build"), "docker-compose-up");
        assert_eq!(suggest_name("npm run dev"), "npm-run-dev");
        assert_eq!(suggest_name("kubectl apply -f deploy.yaml"), "kubectl-apply");
        assert_eq!(suggest_name("git"), "git");
    }

    #[test]
    fn test_suggest_desc() {
        assert_eq!(suggest_desc("cargo build --release"), "cargo build --release");
        assert_eq!(suggest_desc("docker compose up --build"), "docker compose up --build");
        assert_eq!(suggest_desc("ls"), "ls");
    }

    #[test]
    fn test_suggest_name_with_path() {
        assert_eq!(suggest_name("/usr/local/bin/cargo build"), "cargo-build");
    }
}