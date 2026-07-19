use std::collections::BTreeSet;

use anyhow::{Context, Result};
use colored::Colorize;

use crate::core::snipfile::{find_snipfile, read_snippets};

/// Run `snip` (no subcommand) — list all snippets.
pub fn run() -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to determine current directory")?;

    let snipfile_path = match find_snipfile(Some(&cwd))? {
        Some(p) => p,
        None => {
            println!("{}", "No .snips file found in this directory or any parent.".dimmed());
            println!();
            println!("To get started, run:");
            println!("  {}", "snip init".cyan().bold());
            return Ok(());
        }
    };

    let file = read_snippets(&snipfile_path)?;

    if file.is_empty() {
        println!("{}", "No snippets defined.".dimmed());
        println!("Add snippets with:");
        println!("  {}", "snip add <name> \"<command>\" [description]".cyan());
        return Ok(());
    }

    // Calculate column widths
    let mut max_name_len = 0;
    for (key, _) in file.iter() {
        max_name_len = max_name_len.max(key.len());
    }
    let name_width = max_name_len.max(16);

    // Collect and display section groups
    let mut sections: BTreeSet<String> = BTreeSet::new();
    for (key, _) in file.iter() {
        if let Some(dot_pos) = key.find('.') {
            sections.insert(key[..dot_pos].to_string());
        }
    }

    // Print top-level (no dot) snippets first
    let mut has_top = false;
    for (key, _) in file.iter() {
        if !key.contains('.') {
            has_top = true;
        }
    }
    if has_top {
        for (key, snippet) in file.iter() {
            if !key.contains('.') {
                let padded_name = format!("{:width$}", key, width = name_width);
                let desc = if snippet.desc.is_empty() {
                    snippet.cmd.clone()
                } else {
                    snippet.desc.clone()
                };
                println!("  {} {}", padded_name.cyan(), desc.dimmed());
            }
        }
    }

    // Print sectioned snippets
    for section in &sections {
        println!();
        println!("{}", format!("[{}]", section).dimmed().bold());
        for (key, snippet) in file.iter() {
            if let Some(dot_pos) = key.find('.') {
                let sec = &key[..dot_pos];
                if sec == section {
                    let short_name = &key[dot_pos + 1..];
                    let display = format!("{}:{}", section, short_name);
                    let padded_name = format!("{:width$}", display, width = name_width);
                    let desc = if snippet.desc.is_empty() {
                        snippet.cmd.clone()
                    } else {
                        snippet.desc.clone()
                    };
                    println!("  {} {}", padded_name.cyan(), desc.dimmed());
                }
            }
        }
    }

    Ok(())
}

/// Internal list function that accepts a path for testing.
fn run_from(path: &std::path::Path) -> Result<()> {
    let snipfile_path = match find_snipfile(Some(path))? {
        Some(p) => p,
        None => {
            println!("{}", "No .snips file found in this directory or any parent.".dimmed());
            println!();
            println!("To get started, run:");
            println!("  {}", "snip init".cyan().bold());
            return Ok(());
        }
    };

    let file = read_snippets(&snipfile_path)?;
    if file.is_empty() {
        println!("No snippets defined.");
        return Ok(());
    }
    for (key, _) in file.iter() {
        println!("{}", key);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::core::snippet::Snippet;

    #[test]
    fn test_list_no_snips_file() {
        let tmp = tempfile::tempdir().unwrap();
        // Should not error — just print a message
        super::run_from(tmp.path()).unwrap();
    }

    #[test]
    fn test_list_with_snippets() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");

        let mut file = crate::core::snippet::SnipFile::new();
        file.insert("hello", Snippet::new("echo hello").with_desc("Say hello"));
        file.insert("npm.build", Snippet::new("npm run build").with_desc("Build the project"));
        crate::core::snipfile::write_snippets(&snipfile, &file).unwrap();

        // Should not error
        super::run_from(tmp.path()).unwrap();
    }
}