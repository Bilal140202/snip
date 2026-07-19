//! `snip doctor` — Validate all snippet commands, with optional auto-fix.

use std::path::Path;

use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;

use crate::core::snipfile::{find_snipfile, read_snippets, write_snippets};
use crate::core::validator;

/// Validate snippets and report issues.
#[derive(Debug, Args)]
pub struct DoctorCmd {
    /// Automatically fix fixable issues.
    #[arg(long)]
    pub fix: bool,
}

impl DoctorCmd {
    pub fn run(&self) -> Result<()> {
        let cwd = std::env::current_dir().context("Failed to determine current directory")?;
        run_at(&cwd, self.fix)
    }
}

/// Internal doctor function that accepts a root path.
pub(crate) fn run_at(root: &Path, auto_fix: bool) -> Result<()> {
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

    let mut file = read_snippets(&snipfile_path)?;

    if file.is_empty() {
        println!("{}", "No snippets to check.".dimmed());
        return Ok(());
    }

    // First run core validation (empty cmds, undefined vars, etc.)
    let issues = validator::validate(&file);

    // Then check if binaries exist
    let mut valid_count = 0;
    let mut broken_count = 0;
    let mut missing_binaries: Vec<(String, String)> = Vec::new(); // (key, binary)

    for (key, snippet) in file.iter() {
        let first_word = snippet.cmd.split_whitespace().next().unwrap_or("");
        let binary = first_word.to_string();

        // Skip special shells and builtins
        if ["sh", "bash", "zsh", "fish", "true", "false", "echo", "cd", "test"].contains(&binary.as_str()) {
            println!("{} {} — {}", "✓".green(), key.cyan(), snippet.cmd.dimmed());
            valid_count += 1;
            continue;
        }

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
            missing_binaries.push((key.clone(), binary));
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

    // Auto-fix mode
    let mut fixed_count = 0;
    if auto_fix {
        let mut fixes_applied = Vec::new();

        // Fix: remove snippets with missing binaries
        for (key, _binary) in &missing_binaries {
            file.remove(key);
            fixes_applied.push(format!("Removed '{}' (missing binary)", key));
            fixed_count += 1;
        }

        // Fix: add empty var definitions for undefined variables
        for issue in &issues {
            if issue.message.contains("undefined variable") {
                // Extract var name from message like "undefined variable: {{env}}"
                if let Some(start) = issue.message.find("{{") {
                    if let Some(end) = issue.message.find("}}") {
                        let var_name = issue.message[start + 2..end].trim();
                        if let Some(snippet) = file.get_mut(&issue.key) {
                            use crate::core::snippet::VarDef;
                            let var_def = VarDef::new(var_name, &format!("{} variable", var_name));
                            snippet.vars.push(var_def);
                            fixes_applied.push(format!(
                                "Added var definition '{}' to '{}'",
                                var_name, issue.key
                            ));
                            fixed_count += 1;
                        }
                    }
                }
            }
        }

        // Fix: remove unused variable definitions
        for issue in &issues.clone() {
            if issue.message.contains("unused variable definition") {
                // Extract var name
                if let Some(snippet) = file.get_mut(&issue.key) {
                    let var_name = issue
                        .message
                        .strip_prefix("unused variable definition: ")
                        .unwrap_or("");
                    snippet.vars.retain(|v| v.name != var_name);
                    fixes_applied.push(format!(
                        "Removed unused var '{}' from '{}'",
                        var_name, issue.key
                    ));
                    fixed_count += 1;
                }
            }
        }

        if !fixes_applied.is_empty() {
            println!();
            println!("{}", "Auto-fixes applied:".green().bold());
            for fix in &fixes_applied {
                println!("  {} {}", "✓".green(), fix);
            }

            // Write the fixed file
            write_snippets(&snipfile_path, &file)?;
            println!();
            println!("{}", "  .snips file updated.".dimmed());
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
        let fix_hint = if auto_fix {
            String::new()
        } else {
            format!(" Run {} to auto-fix.", "snip doctor --fix".cyan())
        };
        println!(
            "{} {} valid, {} broken{}",
            "!".yellow(),
            valid_count,
            broken_count + issues.len(),
            fix_hint
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

        super::run_at(tmp.path(), false).unwrap();
    }

    #[test]
    fn test_doctor_broken() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");

        let mut file = crate::core::snippet::SnipFile::new();
        file.insert("bad", Snippet::new("nonexistent_binary_xyz_123 --flag").with_desc("A broken command"));
        crate::core::snipfile::write_snippets(&snipfile, &file).unwrap();

        super::run_at(tmp.path(), false).unwrap();
    }

    #[test]
    fn test_doctor_no_snips() {
        let tmp = tempfile::tempdir().unwrap();
        super::run_at(tmp.path(), false).unwrap();
    }

    #[test]
    fn test_doctor_fix_removes_missing_binaries() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");

        let mut file = crate::core::snippet::SnipFile::new();
        file.insert("good", Snippet::new("echo hello"));
        file.insert("bad", Snippet::new("nonexistent_xyz_123_command"));
        crate::core::snipfile::write_snippets(&snipfile, &file).unwrap();

        super::run_at(tmp.path(), true).unwrap();

        let file_after = crate::core::snipfile::read_snippets(&snipfile).unwrap();
        assert!(file_after.get("good").is_some());
        assert!(file_after.get("bad").is_none());
    }
}