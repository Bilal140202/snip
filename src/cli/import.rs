use std::path::PathBuf;

use clap::Args;

use anyhow::Result;

/// Import snippets from another project's `.snips` file, a directory
/// containing one, or a GitHub gist URL.
#[derive(Debug, Args)]
pub struct ImportCmd {
    /// Path to the other project's `.snips` file or directory, OR
    /// a GitHub gist URL (https://gist.github.com/<user>/<id> or
    /// https://api.github.com/gists/<id>).
    pub path: String,

    /// Only import snippets from this section prefix.
    #[arg(long)]
    pub prefix: Option<String>,

    /// Overwrite existing snippets with the same key.
    #[arg(long)]
    pub overwrite: bool,

    /// When importing from a gist, the filename inside the gist to import
    /// (defaults to the first .toml or .snips file in the gist).
    #[arg(long)]
    pub file: Option<String>,
}

impl ImportCmd {
    pub fn run(&self) -> Result<()> {
        // Detect URL-style import (gist)
        if self.path.starts_with("https://") || self.path.starts_with("http://") {
            return import_from_url(
                &self.path,
                self.file.as_deref(),
                self.prefix.as_deref(),
                self.overwrite,
            );
        }

        let p = PathBuf::from(&self.path);
        let snip_path = if p.is_file() {
            p
        } else {
            let candidate = p.join(".snips");
            if candidate.exists() {
                candidate
            } else {
                anyhow::bail!("no .snips file found at {}", p.display());
            }
        };

        import_from_file(&snip_path, self.prefix.as_deref(), self.overwrite)
    }
}

fn import_from_file(
    snip_path: &std::path::Path,
    prefix: Option<&str>,
    overwrite: bool,
) -> Result<()> {
    let source = crate::core::read_snippets(snip_path)?;
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
        if let Some(prefix) = prefix {
            if !key.starts_with(prefix) {
                skipped += 1;
                continue;
            }
        }

        if dest.get(key).is_some() && !overwrite {
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

fn import_from_url(
    url: &str,
    file: Option<&str>,
    prefix: Option<&str>,
    overwrite: bool,
) -> Result<()> {
    // Normalize gist URLs into the GitHub API endpoint.
    let api_url = normalize_gist_url(url)?;

    println!("Fetching gist: {}", api_url);

    // Use curl for portability (no extra Rust deps required).
    let raw = std::process::Command::new("curl")
        .args([
            "-fsSL",
            "-H",
            "Accept: application/vnd.github+json",
            &api_url,
        ])
        .output()
        .map_err(|e| anyhow::anyhow!("failed to spawn curl: {}", e))?;

    if !raw.status.success() {
        let stderr = String::from_utf8_lossy(&raw.stderr);
        anyhow::bail!("gist fetch failed ({}): {}", raw.status, stderr.trim());
    }

    let gist: serde_json::Value = serde_json::from_slice(&raw.stdout)
        .map_err(|e| anyhow::anyhow!("failed to parse gist JSON: {}", e))?;

    let files = gist
        .get("files")
        .and_then(|f| f.as_object())
        .ok_or_else(|| anyhow::anyhow!("gist response missing 'files' object"))?;

    // Pick the file: explicit name → first .toml → first .snips → first file
    let chosen: (&String, &serde_json::Value) = if let Some(wanted) = file {
        let entry = files
            .iter()
            .find(|(name, _)| name.as_str() == wanted)
            .ok_or_else(|| anyhow::anyhow!("file '{}' not found in gist", wanted))?;
        entry
    } else {
        // Prefer .toml, then .snips, then any file
        let mut chosen: Option<(&String, &serde_json::Value)> = None;
        for (name, value) in files {
            let lower = name.to_lowercase();
            if lower.ends_with(".toml") {
                chosen = Some((name, value));
                break;
            }
            if chosen.is_none() && lower.ends_with(".snips") {
                chosen = Some((name, value));
            }
            if chosen.is_none() {
                chosen = Some((name, value));
            }
        }
        chosen.ok_or_else(|| anyhow::anyhow!("gist contains no files"))?
    };

    let content = chosen
        .1
        .get("content")
        .and_then(|c| c.as_str())
        .ok_or_else(|| anyhow::anyhow!("gist file '{}' has no 'content' field", chosen.0))?;

    // Parse the content as TOML into a SnipFile
    let value: toml::Value = content
        .parse()
        .map_err(|e| anyhow::anyhow!("failed to parse gist content as TOML: {}", e))?;
    let source = crate::core::snippet::SnipFile::from_toml_value(&value)?;

    if source.is_empty() {
        println!("Gist contains no snippets.");
        return Ok(());
    }

    let dest_path = crate::core::find_snipfile(None)?
        .ok_or_else(|| anyhow::anyhow!("no .snips file found — run `snip init` first"))?;

    let mut dest = crate::core::read_snippets(&dest_path)?;
    let mut imported = 0;
    let mut skipped = 0;

    for (key, snippet) in source.iter() {
        if let Some(prefix) = prefix {
            if !key.starts_with(prefix) {
                skipped += 1;
                continue;
            }
        }
        if dest.get(key).is_some() && !overwrite {
            skipped += 1;
            continue;
        }
        dest.insert(key.clone(), snippet.clone());
        imported += 1;
    }

    crate::core::write_snippets(&dest_path, &dest)?;
    println!(
        "Imported {} snippet(s) from gist (file: '{}'), skipped {}.",
        imported, chosen.0, skipped
    );
    Ok(())
}

/// Convert a gist URL into a GitHub API URL.
fn normalize_gist_url(url: &str) -> Result<String> {
    // Already an API URL
    if url.contains("://api.github.com/gists/") {
        return Ok(url.to_string());
    }
    // https://gist.github.com/<user>/<id>
    if let Some(rest) = url.strip_prefix("https://gist.github.com/") {
        let parts: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
        if parts.len() >= 2 {
            let id = parts[1];
            return Ok(format!("https://api.github.com/gists/{}", id));
        }
        // Sometimes the URL is just https://gist.github.com/<id>
        if parts.len() == 1 && parts[0].len() >= 20 {
            return Ok(format!("https://api.github.com/gists/{}", parts[0]));
        }
    }
    // http:// variant
    if let Some(rest) = url.strip_prefix("http://gist.github.com/") {
        let parts: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
        if parts.len() >= 2 {
            let id = parts[1];
            return Ok(format!("https://api.github.com/gists/{}", id));
        }
    }
    anyhow::bail!(
        "unrecognized gist URL: {} (expected https://gist.github.com/<user>/<id>)",
        url
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_gist_url_with_user() {
        let url = "https://gist.github.com/someuser/abc123def456";
        let api = normalize_gist_url(url).unwrap();
        assert_eq!(api, "https://api.github.com/gists/abc123def456");
    }

    #[test]
    fn test_normalize_gist_url_already_api() {
        let url = "https://api.github.com/gists/abc123def456";
        let api = normalize_gist_url(url).unwrap();
        assert_eq!(api, url);
    }

    #[test]
    fn test_normalize_gist_url_invalid() {
        let url = "https://example.com/foo";
        let result = normalize_gist_url(url);
        assert!(result.is_err());
    }
}
