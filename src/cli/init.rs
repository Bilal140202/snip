use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};

use crate::core::detector;
use crate::core::snippet::{Snippet, SnipFile};
use crate::core::snipfile::write_snippets;

/// Run `snip init` — detect project type and create `.snips` file.
pub fn run() -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to determine current directory")?;
    run_at(&cwd)
}

/// Internal init function that accepts a root path.
fn run_at(root: &Path) -> Result<()> {
    let snipfile_path = root.join(".snips");

    if snipfile_path.exists() {
        println!(
            "A .snips file already exists at {}",
            snipfile_path.display()
        );
        println!("Use `snip add` to add snippets or edit the file directly.");
        return Ok(());
    }

    let detected = detector::detect_snippets(root);

    if detected.is_empty() {
        println!("No supported project type detected.");
        println!("Creating an empty .snips file.");
        let file = SnipFile::new();
        write_snippets(&snipfile_path, &file)?;
        println!("Created empty .snips");
        return Ok(());
    }

    let mut by_source: HashMap<String, usize> = HashMap::new();
    let mut file = SnipFile::new();

    for (section, name, cmd, desc) in &detected {
        *by_source.entry(section.clone()).or_insert(0) += 1;
        let key = if section.is_empty() {
            name.clone()
        } else {
            format!("{}.{}", section, name)
        };
        file.insert(
            key,
            Snippet::new(cmd.as_str()).with_desc(desc.as_str()),
        );
    }

    write_snippets(&snipfile_path, &file)?;

    let parts: Vec<String> = by_source
        .iter()
        .map(|(src, count)| format!("{} commands from {}", count, src))
        .collect();

    println!("Created .snips with {} {}", file.len(), parts.join(", "));
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    #[test]
    fn test_init_creates_snips() {
        let tmp = tempfile::tempdir().unwrap();

        fs::write(
            tmp.path().join("package.json"),
            r#"{"scripts":{"build":"tsc","test":"jest"}}"#,
        )
        .unwrap();

        super::run_at(tmp.path()).unwrap();

        assert!(tmp.path().join(".snips").exists());

        let content = fs::read_to_string(tmp.path().join(".snips")).unwrap();
        assert!(content.contains("build"));
        assert!(content.contains("test"));
    }

    #[test]
    fn test_init_empty_project() {
        let tmp = tempfile::tempdir().unwrap();

        super::run_at(tmp.path()).unwrap();

        assert!(tmp.path().join(".snips").exists());
    }
}