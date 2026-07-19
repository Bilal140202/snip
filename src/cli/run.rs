use std::io::Write as IoWrite;

use anyhow::{bail, Context, Result};
use colored::Colorize;

use crate::core::executor;
use crate::core::fuzzy;
use crate::core::snipfile::{find_snipfile, read_snippets};

/// Run `snip run <NAME_OR_FUZZY>`.
pub fn run(name_or_fuzzy: &str) -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to determine current directory")?;
    run_from(&cwd, name_or_fuzzy)
}

/// Internal run function that accepts a path for testing.
fn run_from(cwd: &std::path::Path, name_or_fuzzy: &str) -> Result<()> {
    let snipfile_path = match find_snipfile(Some(cwd))? {
        Some(p) => p,
        None => {
            bail!(
                "No .snips file found. Run {} first.",
                "snip init"
            );
        }
    };

    let file = read_snippets(&snipfile_path)?;

    // Try exact match first
    if let Some(snippet) = file.get(name_or_fuzzy) {
        let cmd = resolve_variables(snippet)?;
        println!("{} {}", "→".dimmed(), cmd.dimmed());
        println!();
        return executor::execute(&cmd);
    }

    // Fuzzy match against all keys
    let all_keys: Vec<String> = file.iter().map(|(k, _)| k.clone()).collect();
    let matches = fuzzy::fuzzy_match(name_or_fuzzy, &all_keys);

    if matches.is_empty() {
        bail!("No snippet matching '{}' found", name_or_fuzzy);
    }

    if matches.len() == 1 || matches[0].score > matches[1].score * 2 {
        // Clear winner
        let key = &matches[0].key;
        let snippet = file.get(key).unwrap();
        let cmd = resolve_variables(snippet)?;
        println!("{} {}", "→".dimmed(), cmd.dimmed());
        println!();
        return executor::execute(&cmd);
    }

    // Multiple matches — show them
    println!("{}", "Multiple matches found:".dimmed());
    for result in &matches[..5.min(matches.len())] {
        println!("  {}", result.key.cyan());
    }
    println!("Be more specific or use the full name.");
    Ok(())
}

/// If a snippet has {{var}} placeholders, prompt the user to fill them in.
fn resolve_variables(snippet: &crate::core::snippet::Snippet) -> Result<String> {
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
    use crate::core::snippet::Snippet;

    #[test]
    fn test_run_exact_match() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");

        let mut file = crate::core::snippet::SnipFile::new();
        file.insert("hello", Snippet::new("echo hello").with_desc("Say hello"));
        crate::core::snipfile::write_snippets(&snipfile, &file).unwrap();

        super::run_from(tmp.path(), "hello").unwrap();
    }

    #[test]
    fn test_run_no_snips_file() {
        let tmp = tempfile::tempdir().unwrap();
        let result = super::run_from(tmp.path(), "hello");
        assert!(result.is_err());
    }

    #[test]
    fn test_run_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");

        let file = crate::core::snippet::SnipFile::new();
        crate::core::snipfile::write_snippets(&snipfile, &file).unwrap();

        let result = super::run_from(tmp.path(), "nonexistent");
        assert!(result.is_err());
    }
}