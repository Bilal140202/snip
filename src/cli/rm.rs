use anyhow::{bail, Context, Result};

use crate::core::snipfile::{find_snipfile, read_snippets, write_snippets};

/// Run `snip rm <NAME>`.
pub fn run(name: &str) -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to determine current directory")?;
    let snipfile_path = match find_snipfile(Some(&cwd))? {
        Some(p) => p,
        None => bail!("No .snips file found. Run `snip init` first."),
    };

    let mut file = read_snippets(&snipfile_path)?;

    match file.remove(name) {
        Some(_removed) => {
            write_snippets(&snipfile_path, &file)?;
            println!("✓ Removed '{}' from .snips", name);
            Ok(())
        }
        None => bail!("Snippet '{}' not found", name),
    }
}

#[cfg(test)]
mod tests {
    use crate::core::snippet::Snippet;

    #[test]
    fn test_rm_snippet() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");

        let mut file = crate::core::snippet::SnipFile::new();
        file.insert("hello", Snippet::new("echo hello").with_desc("Say hello"));
        crate::core::snipfile::write_snippets(&snipfile, &file).unwrap();

        // Remove using the file API directly
        let mut file = crate::core::snipfile::read_snippets(&snipfile).unwrap();
        let removed = file.remove("hello");
        assert!(removed.is_some());
        assert!(file.is_empty());
    }

    #[test]
    fn test_rm_nonexistent_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");
        let file = crate::core::snippet::SnipFile::new();
        crate::core::snipfile::write_snippets(&snipfile, &file).unwrap();

        let mut file = crate::core::snipfile::read_snippets(&snipfile).unwrap();
        let removed = file.remove("nonexistent");
        assert!(removed.is_none());
    }

    #[test]
    fn test_rm_no_snips_file() {
        let tmp = tempfile::tempdir().unwrap();
        let result = crate::core::snipfile::find_snipfile(Some(tmp.path())).unwrap();
        assert!(result.is_none());
    }
}