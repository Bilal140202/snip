//! `snip rename <OLD> <NEW>` — Rename a snippet key.
//! `snip mv <NAME> <SECTION>` — Move a snippet to a different section.

use anyhow::{bail, Context, Result};
use colored::Colorize;

use crate::core::snipfile::{find_snipfile, read_snippets, write_snippets};

/// Run `snip rename <OLD> <NEW>`.
pub fn run(old: &str, new: &str) -> Result<()> {
    if old.trim().is_empty() {
        bail!("Old name cannot be empty");
    }
    if new.trim().is_empty() {
        bail!("New name cannot be empty");
    }
    if old == new {
        bail!("Old and new names are the same");
    }

    let cwd = std::env::current_dir().context("Failed to determine current directory")?;
    let snipfile_path = match find_snipfile(Some(&cwd))? {
        Some(p) => p,
        None => bail!("No .snips file found. Run `snip init` first."),
    };

    let mut file = read_snippets(&snipfile_path)?;

    let snippet = match file.remove(old) {
        Some(s) => s,
        None => {
            let all_keys: Vec<String> = file.iter().map(|(k, _)| k.clone()).collect();
            let suggestion = crate::cli::completions::suggest_similar(old, &all_keys);
            if let Some(hint) = suggestion {
                let msg = crate::cli::completions::did_you_mean(hint);
                bail!("Snippet '{}' not found. {}", old, msg);
            } else {
                bail!("Snippet '{}' not found", old);
            }
        }
    };

    if file.get(new).is_some() {
        // Re-insert the original to leave the file untouched on failure
        file.insert(old.to_string(), snippet);
        bail!("A snippet named '{}' already exists", new);
    }

    file.insert(new.to_string(), snippet);
    write_snippets(&snipfile_path, &file)?;
    println!("✓ Renamed {} → {}", old.cyan(), new.cyan());
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::core::snippet::{SnipFile, Snippet};

    #[test]
    fn test_rename_snippet() {
        // Use the file API directly — run() uses cwd which we can't easily set.
        use crate::core::snipfile::{read_snippets, write_snippets};

        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");
        let mut file = SnipFile::new();
        file.insert("hello", Snippet::new("echo hello"));
        write_snippets(&snipfile, &file).unwrap();

        let mut file = read_snippets(&snipfile).unwrap();
        let s = file.remove("hello").unwrap();
        file.insert("greet", s);
        write_snippets(&snipfile, &file).unwrap();

        let file = read_snippets(&snipfile).unwrap();
        assert!(file.get("hello").is_none());
        assert!(file.get("greet").is_some());
    }

    // The `run` function uses cwd, which we can't easily set in unit tests.
    // We test argument validation directly here instead.
    #[test]
    fn test_rename_empty_old_fails() {
        let result = super::run("", "new");
        assert!(result.is_err());
    }

    #[test]
    fn test_rename_empty_new_fails() {
        let result = super::run("old", "");
        assert!(result.is_err());
    }

    #[test]
    fn test_rename_same_name_fails() {
        let result = super::run("same", "same");
        assert!(result.is_err());
    }
}
