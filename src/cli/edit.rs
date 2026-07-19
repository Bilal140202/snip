use anyhow::{Context, Result};

use crate::core::snippet::SnipFile;
use crate::core::snipfile::{find_snipfile, write_snippets};

/// Run `snip edit` — open `.snips` in `$EDITOR`.
pub fn run() -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to determine current directory")?;

    let snipfile_path = match find_snipfile(Some(&cwd))? {
        Some(p) => p,
        None => {
            // Create empty .snips first
            let path = cwd.join(".snips");
            let file = SnipFile::new();
            write_snippets(&path, &file)?;
            path
        }
    };

    crate::utils::shell::open_in_editor(&snipfile_path)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_default_editor_fallback() {
        // Should not panic
        let editor = crate::utils::shell::default_editor();
        assert!(!editor.is_empty());
    }
}