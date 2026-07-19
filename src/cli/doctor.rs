use std::path::Path;

use anyhow::{Context, Result};
use colored::Colorize;

use crate::core::snipfile::{find_snipfile, read_snippets};
use crate::core::validator;

/// Run `snip doctor` — validate all snippet commands.
pub fn run() -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to determine current directory")?;
    run_at(&cwd)
}

/// Internal doctor function that accepts a root path.
pub(crate) fn run_at(root: &Path) -> Result<()> {
    let snipfile_path = match find_snipfile(Some(root))? {
        Some(p) => p,
        None => {
            println!(
                "{}",
                "No .snips file found. Run `snip init` first.".dimmed()
            );
            return Ok(());
        }
    };

    let file = read_snippets(&snipfile_path)?;

    if file.is_empty() {
        println!("{}", "No snippets to check.".dimmed());
        return Ok(());
    }

    // First run core validation (empty cmds, undefined vars, etc.)
    let issues = validator::validate(&file);

    // Then check if binaries exist
    let mut valid_count = 0;
    let mut broken_count = 0;

    for (key, snippet) in file.iter() {
        let first_word = snippet.cmd.split_whitespace().next().unwrap_or("");
        let binary = first_word.to_string();

        let exists = which::which(&binary).is_ok();

        if exists {
            println!("{} {} — {}", "✓".green(), key.cyan(), snippet.cmd.dimmed());
            valid_count += 1;
        } else {
            println!(
                "{} {} — {} {}",
                "✗".red(),
                key.cyan(),
                snippet.cmd.dimmed(),
                format!("(binary '{}' not found)", binary).yellow().dimmed()
            );
            broken_count += 1;
        }
    }

    // Show core validation issues
    if !issues.is_empty() {
        println!();
        println!("{}", "Additional issues:".dimmed());
        for issue in &issues {
            let icon = match issue.severity {
                validator::Severity::Error => "✗".red().to_string(),
                validator::Severity::Warning => "⚠".yellow().to_string(),
            };
            println!("  {} [{}] {}", icon, issue.key, issue.message);
        }
    }

    println!();
    if broken_count == 0 && issues.is_empty() {
        println!(
            "{} All {} snippet(s) are valid.",
            "✓".green(),
            valid_count
        );
    } else {
        println!(
            "{} {} valid, {} broken",
            "!".yellow(),
            valid_count,
            broken_count
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::core::snippet::Snippet;

    #[test]
    fn test_doctor_valid() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");

        let mut file = crate::core::snippet::SnipFile::new();
        file.insert("hello", Snippet::new("echo hello").with_desc("Say hello"));
        crate::core::snipfile::write_snippets(&snipfile, &file).unwrap();

        super::run_at(tmp.path()).unwrap();
    }

    #[test]
    fn test_doctor_broken() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");

        let mut file = crate::core::snippet::SnipFile::new();
        file.insert("bad", Snippet::new("nonexistent_binary_xyz_123 --flag").with_desc("A broken command"));
        crate::core::snipfile::write_snippets(&snipfile, &file).unwrap();

        super::run_at(tmp.path()).unwrap();
    }

    #[test]
    fn test_doctor_no_snips() {
        let tmp = tempfile::tempdir().unwrap();
        super::run_at(tmp.path()).unwrap();
    }
}