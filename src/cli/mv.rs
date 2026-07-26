//! `snip mv <NAME> <SECTION>` — Move a snippet to a different section.

use anyhow::{bail, Context, Result};
use colored::Colorize;

use crate::core::snipfile::{find_snipfile, read_snippets, write_snippets};

/// Run `snip mv <NAME> <SECTION>`.
///
/// `NAME` is the fully-qualified snippet key (e.g. `build.release`).
/// `SECTION` is the target section (e.g. `ci`). The leaf name is preserved.
/// If the snippet is top-level (no dot), the leaf is the entire key.
pub fn run(name: &str, section: &str) -> Result<()> {
    if name.trim().is_empty() {
        bail!("Snippet name cannot be empty");
    }
    if section.trim().is_empty() {
        bail!("Section cannot be empty");
    }

    let cwd = std::env::current_dir().context("Failed to determine current directory")?;
    let snipfile_path = match find_snipfile(Some(&cwd))? {
        Some(p) => p,
        None => bail!("No .snips file found. Run `snip init` first."),
    };

    let mut file = read_snippets(&snipfile_path)?;

    let leaf = match name.rsplit('.').next() {
        Some(l) => l,
        None => bail!("Invalid snippet name: '{}'", name),
    };

    let new_key = if section == "_" || section == "-" || section.is_empty() {
        // Sentinel values for "top-level" — move out of any section
        leaf.to_string()
    } else {
        format!("{}.{}", section, leaf)
    };

    if name == new_key {
        bail!("Snippet is already in section '{}'", section);
    }

    let snippet = match file.remove(name) {
        Some(s) => s,
        None => {
            let all_keys: Vec<String> = file.iter().map(|(k, _)| k.clone()).collect();
            let suggestion = crate::cli::completions::suggest_similar(name, &all_keys);
            if let Some(hint) = suggestion {
                let msg = crate::cli::completions::did_you_mean(hint);
                bail!("Snippet '{}' not found. {}", name, msg);
            } else {
                bail!("Snippet '{}' not found", name);
            }
        }
    };

    if file.get(&new_key).is_some() {
        file.insert(name.to_string(), snippet);
        bail!("A snippet named '{}' already exists", new_key);
    }

    file.insert(new_key.clone(), snippet);
    write_snippets(&snipfile_path, &file)?;
    println!("✓ Moved {} → {}", name.cyan(), new_key.cyan());
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_mv_empty_name_fails() {
        let result = super::run("", "section");
        assert!(result.is_err());
    }

    #[test]
    fn test_mv_empty_section_fails() {
        let result = super::run("name", "");
        assert!(result.is_err());
    }

    // End-to-end move via the file API.
    #[test]
    fn test_mv_via_api() {
        use crate::core::snipfile::{read_snippets, write_snippets};
        use crate::core::snippet::{SnipFile, Snippet};

        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");

        let mut file = SnipFile::new();
        file.insert("build.release", Snippet::new("cargo build --release"));
        write_snippets(&snipfile, &file).unwrap();

        let mut file = read_snippets(&snipfile).unwrap();
        let s = file.remove("build.release").unwrap();
        file.insert("ci.release", s);
        write_snippets(&snipfile, &file).unwrap();

        let file = read_snippets(&snipfile).unwrap();
        assert!(file.get("build.release").is_none());
        assert!(file.get("ci.release").is_some());
    }
}
