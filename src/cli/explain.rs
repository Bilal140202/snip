use anyhow::{Context, Result};
use colored::Colorize;

use crate::core::explainer::{explain_command, explain_pipes, TokenKind};
use crate::core::fuzzy;
use crate::core::snipfile::{find_snipfile, read_snippets};

/// Run `snip explain <NAME_OR_RAW>`.
///
/// If the argument matches a snippet name (exact or fuzzy), that snippet's
/// command is explained. Otherwise the argument is treated as a raw command
/// string and explained directly.
pub fn run(name_or_cmd: &str) -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to determine current directory")?;

    // Try to look up a snippet first
    if let Some(snipfile_path) = find_snipfile(Some(&cwd))? {
        let file = read_snippets(&snipfile_path)?;

        // Exact match
        if let Some(snippet) = file.get(name_or_cmd) {
            return explain_snippet(name_or_cmd, snippet);
        }

        // Fuzzy match
        let all_keys: Vec<String> = file.iter().map(|(k, _)| k.clone()).collect();
        let matches = fuzzy::fuzzy_match(name_or_cmd, &all_keys);

        if !matches.is_empty()
            && (matches.len() == 1 || matches[0].score > matches[1].score * 2)
        {
            let key = &matches[0].key;
            let snippet = file.get(key).unwrap();
            return explain_snippet(key, snippet);
        }
    }

    // No snippet found — treat as a raw command string
    explain_raw(name_or_cmd)
}

fn explain_snippet(name: &str, snippet: &crate::core::snippet::Snippet) -> Result<()> {
    let parts = explain_command(&snippet.cmd);

    // Header
    println!("{}", format!("snip explain {}", name).bold());
    println!();

    // Command line
    println!(
        "{} {}",
        "Command:".bold(),
        snippet.cmd.cyan()
    );
    println!("{}", "─".repeat(50).dimmed());
    println!();

    // Show variable definitions if any
    if !snippet.vars.is_empty() {
        println!("{}", "Variables:".bold().yellow());
        for var in &snippet.vars {
            println!("  {} {}", format!("{{{{{}}}}}", var.name).cyan(), format!("— {}", var.desc).dimmed());
            if let Some(ref default) = var.default {
                println!("    {} {}", "default:".dimmed(), default.green());
            }
            if !var.options.is_empty() {
                println!("    {} {}", "options:".dimmed(), var.options.join(", ").dimmed());
            }
        }
        println!();
    }

    // Description if present
    if !snippet.desc.is_empty() {
        println!("{} {}", "Description:".bold(), snippet.desc);
        println!();
    }

    // Explanation parts
    print_parts(&parts);

    // If there are pipes, also show per-segment breakdown
    let pipe_segments = explain_pipes(&snippet.cmd);
    if pipe_segments.len() > 1 {
        println!();
        println!("{}", "Pipe breakdown:".bold());
        for segment in &pipe_segments {
            let seg_parts = explain_command(&segment.command);
            println!(
                "  {} {}",
                format!("Segment {}:", segment.position + 1).bold().magenta(),
                segment.command.cyan()
            );
            for part in &seg_parts {
                let (colored_token, _) = color_token(part);
                let width = 20;
                let padding = if token_display_width(&part.token) >= width {
                    2
                } else {
                    width - token_display_width(&part.token) + 2
                };
                println!(
                    "    {}{}{}",
                    colored_token,
                    " ".repeat(padding),
                    part.explanation.dimmed()
                );
            }
        }
    }

    Ok(())
}

fn explain_raw(cmd: &str) -> Result<()> {
    let parts = explain_command(cmd);

    println!("{}", "Command:".bold());
    println!("  {}", cmd.cyan());
    println!("{}", "─".repeat(50).dimmed());
    println!();

    print_parts(&parts);

    // If there are pipes, also show per-segment breakdown
    let pipe_segments = explain_pipes(cmd);
    if pipe_segments.len() > 1 {
        println!();
        println!("{}", "Pipe breakdown:".bold());
        for segment in &pipe_segments {
            let seg_parts = explain_command(&segment.command);
            println!(
                "  {} {}",
                format!("Segment {}:", segment.position + 1).bold().magenta(),
                segment.command.cyan()
            );
            for part in &seg_parts {
                let (colored_token, _) = color_token(part);
                let width = 20;
                let padding = if token_display_width(&part.token) >= width {
                    2
                } else {
                    width - token_display_width(&part.token) + 2
                };
                println!(
                    "    {}{}{}",
                    colored_token,
                    " ".repeat(padding),
                    part.explanation.dimmed()
                );
            }
        }
    }

    Ok(())
}

fn print_parts(parts: &[crate::core::explainer::ExplanationPart]) {
    // Determine the maximum token display width for alignment
    let max_width = parts
        .iter()
        .map(|p| token_display_width(&p.token))
        .max()
        .unwrap_or(0)
        .max(14); // minimum column width

    for part in parts {
        let (colored_token, _kind) = color_token(part);
        let padding = if token_display_width(&part.token) >= max_width {
            2 // always at least 2 spaces
        } else {
            max_width - token_display_width(&part.token) + 2
        };
        println!(
            "{}{}{}",
            colored_token,
            " ".repeat(padding),
            part.explanation.dimmed()
        );
    }
}

/// Return the colored token string.
fn color_token(part: &crate::core::explainer::ExplanationPart) -> (String, &TokenKind) {
    let colored = match part.kind {
        TokenKind::Binary => part.token.cyan().to_string(),
        TokenKind::Flag => part.token.yellow().to_string(),
        TokenKind::Pipe => part.token.magenta().bold().to_string(),
        TokenKind::Redirect => part.token.magenta().to_string(),
        TokenKind::Operator => part.token.magenta().bold().to_string(),
        TokenKind::Argument => part.token.to_string(),
        TokenKind::Unknown => part.token.dimmed().to_string(),
    };
    (colored, &part.kind)
}

/// Get the display width of a token (without ANSI codes).
fn token_display_width(token: &str) -> usize {
    // Strip ANSI escape sequences for width calculation
    let stripped: String = token
        .chars()
        .filter(|c| !c.is_ascii_control())
        .collect();
    stripped.len()
}