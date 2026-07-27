//! `snip run` — Execute a snippet by name, fuzzy match, or interactive picker.

use std::io::Write as IoWrite;

use anyhow::{bail, Context, Result};
use clap::Args;
use colored::Colorize;

use crate::core::executor;
use crate::core::fuzzy;
use crate::core::snipfile::{find_snipfile, read_snippets};
use crate::ui::picker::{pick, PickerItem, PickerResult};

/// Options for `snip run`.
#[derive(Debug, Args, Default)]
pub struct RunOptions {
    /// Print the resolved command but do NOT execute it.
    #[arg(long)]
    pub dry_run: bool,

    /// Alias of --dry-run.
    #[arg(long, alias = "dry-run")]
    pub print: bool,
}

impl RunOptions {
    pub fn should_print(&self) -> bool {
        self.dry_run || self.print
    }
}

/// Run `snip run <NAME_OR_FUZZY>`.
pub fn run(name_or_fuzzy: &str) -> Result<()> {
    run_with_options(name_or_fuzzy, &RunOptions::default())
}

/// Run a snippet by natural-language phrase (e.g. `snip "deploy staging"`).
///
/// Falls back to this from `main.rs` when clap can't parse the args because
/// the first positional looks like an unknown subcommand. We score every
/// snippet against the phrase and execute the best match.
pub fn run_natural_language(phrase: &str) -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to determine current directory")?;
    let snipfile_path = match find_snipfile(Some(&cwd))? {
        Some(p) => p,
        None => bail!("No .snips file found. Run {} first.", "snip init".cyan()),
    };

    let file = read_snippets(&snipfile_path)?;
    if file.is_empty() {
        bail!("No snippets defined. Add one with `snip add <name> \"<cmd>\"`.");
    }

    let matches = crate::core::nl::nl_match(&file, phrase);

    if matches.is_empty() {
        bail!(
            "No snippet matched '{}'. Try `snip search {}` to find one.",
            phrase,
            phrase
        );
    }

    // Clear winner: top score beats runner-up by a meaningful margin.
    // The scoring system uses 10/15/20/25-point increments per token match,
    // plus 200/300-point phrase bonuses, so a 30-point margin is roughly
    // "one extra token matched" — a meaningful signal.
    if matches.len() == 1 {
        let key = &matches[0].key;
        let snippet = file.get(key).unwrap();
        let cmd = resolve_variables(snippet)?;
        println!("{} {}", "→".dimmed(), cmd.dimmed());
        println!();
        return executor::execute(&cmd);
    }

    let top = &matches[0];
    let runner_up = &matches[1];
    let margin = top.score - runner_up.score;
    if margin >= 30 || top.score >= runner_up.score * 2 {
        let key = &top.key;
        let snippet = file.get(key).unwrap();
        let cmd = resolve_variables(snippet)?;
        println!("{} {}", "→".dimmed(), cmd.dimmed());
        println!();
        return executor::execute(&cmd);
    }

    // Multiple close matches — show top 5 and let the user be more specific
    println!(
        "{}  Multiple snippets matched '{}':",
        "→".dimmed(),
        phrase.cyan()
    );
    for m in &matches[..5.min(matches.len())] {
        let desc = file
            .get(&m.key)
            .map(|s| {
                if s.desc.is_empty() {
                    s.cmd.clone()
                } else {
                    s.desc.clone()
                }
            })
            .unwrap_or_default();
        println!("  {} {}", m.key.cyan(), desc.dimmed());
    }
    println!();
    println!("Run {} to execute a specific one.", "snip run <key>".cyan());
    Ok(())
}

/// Run `snip run <NAME_OR_FUZZY> --dry-run` (or `--print`).
pub fn run_with_options(name_or_fuzzy: &str, opts: &RunOptions) -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to determine current directory")?;
    run_from(&cwd, name_or_fuzzy, false, opts)
}

/// Run `snip run --interactive` (or `snip run -i`) — open the picker.
pub fn run_interactive() -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to determine current directory")?;
    let snipfile_path = match find_snipfile(Some(&cwd))? {
        Some(p) => p,
        None => bail!("No .snips file found. Run {} first.", "snip init".cyan()),
    };

    let file = read_snippets(&snipfile_path)?;
    if file.is_empty() {
        println!("{}", "No snippets defined.".dimmed());
        return Ok(());
    }

    let items: Vec<PickerItem> = file
        .iter()
        .map(|(key, snippet)| {
            let desc = if snippet.desc.is_empty() {
                snippet.cmd.clone()
            } else {
                snippet.desc.clone()
            };
            PickerItem {
                key: key.clone(),
                display: format!("{} — {}", key, desc),
                detail: desc,
                cmd: snippet.cmd.clone(),
                tags: snippet.tags.clone(),
                vars: snippet.placeholder_names(),
            }
        })
        .collect();

    match pick(&items)? {
        PickerResult::Selected(key) => {
            if let Some(snippet) = file.get(&key) {
                let cmd = resolve_variables(snippet)?;
                println!("{} {}", "→".dimmed(), cmd.dimmed());
                println!();
                executor::execute(&cmd)?;
            } else {
                bail!(
                    "Selected snippet '{}' not found (file may have changed)",
                    key
                );
            }
        }
        PickerResult::Cancelled => {
            // Silent cancel
        }
    }

    Ok(())
}

/// Internal run function that accepts a path for testing.
pub(crate) fn run_from(
    cwd: &std::path::Path,
    name_or_fuzzy: &str,
    _test_mode: bool,
    opts: &RunOptions,
) -> Result<()> {
    let snipfile_path = match find_snipfile(Some(cwd))? {
        Some(p) => p,
        None => {
            bail!("No .snips file found. Run {} first.", "snip init".cyan());
        }
    };

    let file = read_snippets(&snipfile_path)?;

    // Try exact match first
    if let Some(snippet) = file.get(name_or_fuzzy) {
        let cmd = resolve_variables(snippet)?;
        if opts.should_print() {
            println!("{}", cmd);
            return Ok(());
        }
        println!("{} {}", "→".dimmed(), cmd.dimmed());
        println!();
        return executor::execute(&cmd);
    }

    // Fuzzy match against all keys
    let all_keys: Vec<String> = file.iter().map(|(k, _)| k.clone()).collect();
    let matches = fuzzy::fuzzy_match(name_or_fuzzy, &all_keys);

    if matches.is_empty() {
        // No fuzzy matches — try Levenshtein "did you mean?"
        let suggestion = crate::cli::completions::suggest_similar(name_or_fuzzy, &all_keys);
        if let Some(hint) = suggestion {
            let msg = crate::cli::completions::did_you_mean(hint);
            bail!("No snippet matching '{}' found. {}", name_or_fuzzy, msg);
        } else {
            bail!("No snippet matching '{}' found", name_or_fuzzy);
        }
    }

    if matches.len() == 1 || (matches.len() > 1 && matches[0].score > matches[1].score * 2) {
        // Clear winner
        let key = &matches[0].key;
        let snippet = file.get(key).unwrap();
        let cmd = resolve_variables(snippet)?;
        if opts.should_print() {
            println!("{}", cmd);
            return Ok(());
        }
        println!("{} {}", "→".dimmed(), cmd.dimmed());
        println!();
        return executor::execute(&cmd);
    }

    // Multiple matches — show them
    println!("{}", "Multiple matches found:".dimmed());
    for result in &matches[..5.min(matches.len())] {
        let desc = file
            .get(&result.key)
            .map(|s| {
                if s.desc.is_empty() {
                    s.cmd.clone()
                } else {
                    s.desc.clone()
                }
            })
            .unwrap_or_default();
        println!("  {} {}", result.key.cyan(), desc.dimmed());
    }
    println!("Be more specific or use the full name.");
    Ok(())
}

/// If a snippet has {{var}} placeholders, prompt the user to fill them in.
pub fn resolve_variables(snippet: &crate::core::snippet::Snippet) -> Result<String> {
    if !snippet.has_placeholders() {
        return Ok(snippet.cmd.clone());
    }

    // Check if vars are defined with options — use them
    let mut vars = std::collections::HashMap::new();
    for placeholder in snippet.placeholder_names() {
        if let Some(var_def) = snippet.vars.iter().find(|v| v.name == placeholder) {
            if !var_def.options.is_empty() {
                // Use the default if available, otherwise first option
                let value = var_def
                    .default
                    .clone()
                    .unwrap_or_else(|| var_def.options[0].clone());
                vars.insert(placeholder, value);
            } else {
                let default = var_def.default.clone().unwrap_or_default();
                print!(
                    "{}{}{}: ",
                    "  ".dimmed(),
                    var_def.to_string().cyan().bold(),
                    if default.is_empty() {
                        String::new()
                    } else {
                        format!(" [{}]", default).dimmed().to_string()
                    }
                );
                std::io::stdout().flush()?;

                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                let input = input.trim();
                let value = if input.is_empty() {
                    default
                } else {
                    input.to_string()
                };
                vars.insert(placeholder, value);
            }
        } else {
            // No var def — just prompt
            print!("{}{}: ", "  ".dimmed(), placeholder.cyan().bold());
            std::io::stdout().flush()?;

            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            vars.insert(placeholder, input.trim().to_string());
        }
    }

    Ok(snippet.substitute(&vars))
}

#[cfg(test)]
mod tests {
    use super::RunOptions;
    use crate::core::snippet::Snippet;

    #[test]
    fn test_run_exact_match() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");

        let mut file = crate::core::snippet::SnipFile::new();
        file.insert("hello", Snippet::new("echo hello").with_desc("Say hello"));
        crate::core::snipfile::write_snippets(&snipfile, &file).unwrap();

        super::run_from(tmp.path(), "hello", true, &RunOptions::default()).unwrap();
    }

    #[test]
    fn test_run_no_snips_file() {
        let tmp = tempfile::tempdir().unwrap();
        let result = super::run_from(tmp.path(), "hello", true, &RunOptions::default());
        assert!(result.is_err());
    }

    #[test]
    fn test_run_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");

        let file = crate::core::snippet::SnipFile::new();
        crate::core::snipfile::write_snippets(&snipfile, &file).unwrap();

        let result = super::run_from(tmp.path(), "nonexistent", true, &RunOptions::default());
        assert!(result.is_err());
        // Should contain "did you mean" hint or "not found"
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not found") || err.contains("No snippet"));
    }

    #[test]
    fn test_run_fuzzy_match() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");

        let mut file = crate::core::snippet::SnipFile::new();
        file.insert("build-release", Snippet::new("cargo build --release"));
        crate::core::snipfile::write_snippets(&snipfile, &file).unwrap();

        // "bldrel" should fuzzy match "build-release"
        super::run_from(tmp.path(), "bldrel", true, &RunOptions::default()).unwrap();
    }

    #[test]
    fn test_run_dry_run_prints_without_executing() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");

        let mut file = crate::core::snippet::SnipFile::new();
        // Use a command that would fail if actually executed
        file.insert("boom", Snippet::new("exit 42").with_desc("should not run"));
        crate::core::snipfile::write_snippets(&snipfile, &file).unwrap();

        let opts = RunOptions {
            dry_run: true,
            print: false,
        };
        // Should succeed — nothing was executed
        super::run_from(tmp.path(), "boom", true, &opts).unwrap();
    }

    #[test]
    fn test_run_print_alias_dry_run() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");

        let mut file = crate::core::snippet::SnipFile::new();
        file.insert("boom", Snippet::new("exit 42").with_desc("should not run"));
        crate::core::snipfile::write_snippets(&snipfile, &file).unwrap();

        let opts = RunOptions {
            dry_run: false,
            print: true,
        };
        super::run_from(tmp.path(), "boom", true, &opts).unwrap();
    }
}
