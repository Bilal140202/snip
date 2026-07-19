//! Dynamic shell completions — reads .snips and returns snippet names.
//!
//! This replaces the old static clap_complete approach with a dynamic system
//! where shell completion scripts call `snip _complete <kind> [partial]` to get
//! candidate snippet names from the current project's .snips file.

use anyhow::{Context, Result};
use colored::Colorize;

use crate::core::snipfile::find_snipfile;
use crate::core::snipfile::read_snippets;

/// Handle the hidden `snip _complete` subcommand.
///
/// Called by shell completion scripts to get dynamic candidate lists.
pub fn run_complete(kind: &str, partial: Option<&str>) -> Result<()> {
    match kind {
        "snippets" => complete_snippets(partial),
        "subcommands" => complete_subcommands(partial),
        _ => Ok(()),
    }
}

/// Print all snippet keys, optionally filtered by a partial prefix.
fn complete_snippets(partial: Option<&str>) -> Result<()> {
    // Try to find .snips, silently do nothing if not found
    let cwd = std::env::current_dir().ok();
    let snipfile_path = match cwd {
        Some(ref dir) => match find_snipfile(Some(dir))? {
            Some(p) => p,
            None => return Ok(()),
        },
        None => return Ok(()),
    };

    let file = read_snippets(&snipfile_path).unwrap_or_default();

    for (key, _) in file.iter() {
        if let Some(p) = partial {
            if key.starts_with(p) {
                println!("{}", key);
            }
        } else {
            println!("{}", key);
        }
    }

    Ok(())
}

/// Print available subcommands, optionally filtered by a partial prefix.
fn complete_subcommands(partial: Option<&str>) -> Result<()> {
    let subcommands = [
        "init", "add", "rm", "edit", "list", "run",
        "import", "doctor", "completions", "hook",
        "suggest", "explain", "stale", "setup",
    ];

    for cmd in &subcommands {
        if let Some(p) = partial {
            if cmd.starts_with(p) {
                println!("{}", cmd);
            }
        } else {
            println!("{}", cmd);
        }
    }

    Ok(())
}

/// Generate static shell completion scripts (for `snip completions`).
///
/// This now outputs the embedded dynamic scripts rather than clap's static ones.
pub fn generate_completions(shell: &str) -> Result<()> {
    let script = match shell {
        "bash" => include_str!("../../completions/snip.bash"),
        "zsh" => include_str!("../../completions/snip.zsh"),
        "fish" => include_str!("../../completions/snip.fish"),
        "nushell" => include_str!("../../completions/snip.nushell"),
        _ => {
            // Fall back to clap_complete for elvish/powershell
            return generate_clap_fallback(shell);
        }
    };

    println!("{}", script);
    Ok(())
}

/// Fallback to clap_complete for shells we don't have custom scripts for.
fn generate_clap_fallback(shell: &str) -> Result<()> {
    use clap::Command;
    use clap_complete::{generate, shells};

    // Build a minimal clap Command for static completion
    let mut app = Command::new("snip")
        .version("0.2.0")
        .subcommand_required(false)
        .subcommand(clap::Command::new("init").about("Create / detect .snips file"))
        .subcommand(clap::Command::new("add").about("Add a new snippet"))
        .subcommand(clap::Command::new("rm").about("Remove a snippet"))
        .subcommand(clap::Command::new("edit").about("Open .snips in $EDITOR"))
        .subcommand(clap::Command::new("list").about("List snippets"))
        .subcommand(clap::Command::new("run").about("Execute a snippet"))
        .subcommand(clap::Command::new("import").about("Import snippets"))
        .subcommand(clap::Command::new("doctor").about("Validate snippets"))
        .subcommand(clap::Command::new("completions").about("Generate shell completions"))
        .subcommand(clap::Command::new("hook").about("Print shell integration code"))
        .subcommand(clap::Command::new("suggest").about("Suggest snippets from history"))
        .subcommand(clap::Command::new("explain").about("Explain a snippet command"))
        .subcommand(clap::Command::new("stale").about("Detect unused snippets"))
        .subcommand(clap::Command::new("setup").about("Team onboarding wizard"));

    match shell {
        "elvish" => {
            generate(shells::Elvish, &mut app, "snip", &mut std::io::stdout());
        }
        "powershell" => {
            generate(shells::PowerShell, &mut app, "snip", &mut std::io::stdout());
        }
        _ => anyhow::bail!("unsupported shell: {}", shell),
    }

    Ok(())
}

/// Levenshtein distance between two strings.
/// Used for "did you mean?" suggestions.
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a_len = a.len();
    let b_len = b.len();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    let mut matrix = vec![vec![0; b_len + 1]; a_len + 1];

    for i in 0..=a_len {
        matrix[i][0] = i;
    }
    for j in 0..=b_len {
        matrix[0][j] = j;
    }

    for (i, ca) in a.chars().enumerate() {
        for (j, cb) in b.chars().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            matrix[i + 1][j + 1] = (matrix[i][j + 1] + 1)
                .min(matrix[i + 1][j] + 1)
                .min(matrix[i][j] + cost);
        }
    }

    matrix[a_len][b_len]
}

/// Find the closest match to `query` from `candidates` using Levenshtein distance.
/// Returns the best match if distance is within a reasonable threshold.
pub fn suggest_similar<'a>(query: &str, candidates: &'a [String]) -> Option<&'a str> {
    if candidates.is_empty() {
        return None;
    }

    // Threshold: allow up to 1/3 of the query length in edits
    let threshold = (query.len() / 3).max(2);

    let mut best: Option<(&str, usize)> = None;

    for candidate in candidates {
        let dist = levenshtein(query, candidate);
        if dist <= threshold {
            if best.is_none() || dist < best.unwrap().1 {
                best = Some((candidate, dist));
            }
        }
    }

    best.map(|(s, _)| s)
}

/// Format a "did you mean?" suggestion.
pub fn did_you_mean(suggestion: &str) -> String {
    format!("{} Did you mean {}?", "hint:".yellow().bold(), suggestion.cyan().bold())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levenshtein_same() {
        assert_eq!(levenshtein("hello", "hello"), 0);
    }

    #[test]
    fn levenshtein_one_edit() {
        assert_eq!(levenshtein("hello", "hallo"), 1);
    }

    #[test]
    fn levenshtein_empty() {
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", ""), 3);
    }

    #[test]
    fn suggest_finds_close_match() {
        let candidates = vec![
            "build".into(),
            "build.release".into(),
            "test".into(),
        ];
        assert_eq!(suggest_similar("buld", &candidates), Some("build"));
    }

    #[test]
    fn suggest_returns_none_for_distant() {
        let candidates = vec!["build".into(), "test".into()];
        assert_eq!(suggest_similar("xyz", &candidates), None);
    }

    #[test]
    fn suggest_returns_none_for_empty() {
        let candidates: Vec<String> = vec![];
        assert_eq!(suggest_similar("hello", &candidates), None);
    }
}