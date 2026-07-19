use std::path::PathBuf;

use clap::Args;

use anyhow::Result;

/// Import snippets from another project's `.snips` file.
#[derive(Debug, Args)]
pub struct ImportCmd {
    /// Path to the other project's `.snips` file or directory.
    pub path: PathBuf,

    /// Only import snippets from this section prefix.
    #[arg(long)]
    pub prefix: Option<String>,

    /// Overwrite existing snippets with the same key.
    #[arg(long)]
    pub overwrite: bool,
}

impl ImportCmd {
    pub fn run(&self) -> Result<()> {
        let snip_path = if self.path.is_file() {
            self.path.clone()
        } else {
            let candidate = self.path.join(".snips");
            if candidate.exists() {
                candidate
            } else {
                anyhow::bail!("no .snips file found at {}", self.path.display());
            }
        };

        let source = crate::core::read_snippets(&snip_path)?;
        if source.is_empty() {
            println!("No snippets to import.");
            return Ok(());
        }

        let dest_path = crate::core::find_snipfile(None)?
            .ok_or_else(|| anyhow::anyhow!("no .snips file found — run `snip init` first"))?;

        let mut dest = crate::core::read_snippets(&dest_path)?;
        let mut imported = 0;
        let mut skipped = 0;

        for (key, snippet) in source.iter() {
            // Apply prefix filter
            if let Some(ref prefix) = self.prefix {
                if !key.starts_with(prefix) {
                    skipped += 1;
                    continue;
                }
            }

            if dest.get(key).is_some() && !self.overwrite {
                skipped += 1;
                continue;
            }

            dest.insert(key.clone(), snippet.clone());
            imported += 1;
        }

        crate::core::write_snippets(&dest_path, &dest)?;
        println!("Imported {} snippet(s), skipped {}.", imported, skipped);
        Ok(())
    }
}