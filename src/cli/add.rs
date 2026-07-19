use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::core::snippet::Snippet;
use crate::core::snipfile::{find_snipfile, read_snippets, write_snippets};

/// Run `snip add <NAME> "<CMD>" [DESCRIPTION]`.
pub fn run(name: &str, cmd: &str, description: Option<&str>) -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to determine current directory")?;
    run_at(&cwd, name, cmd, description)
}

/// Internal add function that accepts a root path.
fn run_at(root: &Path, name: &str, cmd: &str, description: Option<&str>) -> Result<()> {
    if name.trim().is_empty() {
        bail!("Snippet name cannot be empty");
    }

    if cmd.trim().is_empty() {
        bail!("Command cannot be empty");
    }

    let key = name.to_string();
    let desc = description.unwrap_or(cmd);

    let snippet = Snippet::new(cmd).with_desc(desc);

    let snipfile_path = match find_snipfile(Some(root))? {
        Some(p) => p,
        None => root.join(".snips"),
    };

    let mut file = if snipfile_path.exists() {
        read_snippets(&snipfile_path)?
    } else {
        crate::core::snippet::SnipFile::new()
    };

    file.insert(&key, snippet);

    write_snippets(&snipfile_path, &file)?;

    println!("✓ Added '{}' to .snips", key);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    #[test]
    fn test_add_snippet() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");

        super::run_at(tmp.path(), "hello", "echo hello", None).unwrap();

        assert!(snipfile.exists());
        let content = fs::read_to_string(&snipfile).unwrap();
        assert!(content.contains("echo hello"));
    }

    #[test]
    fn test_add_with_section() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");

        super::run_at(
            tmp.path(),
            "deploy.stg",
            "kubectl apply -f stg.yaml",
            Some("Deploy to staging"),
        )
        .unwrap();

        assert!(snipfile.exists());
        let content = fs::read_to_string(&snipfile).unwrap();
        assert!(content.contains("stg"));
        assert!(content.contains("deploy"));
    }

    #[test]
    fn test_add_empty_name_fails() {
        let tmp = tempfile::tempdir().unwrap();

        let result = super::run_at(tmp.path(), "", "echo hello", None);
        assert!(result.is_err());
    }
}