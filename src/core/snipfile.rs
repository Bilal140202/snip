use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::snippet::{SnipFile, Snippet};

const SNIPFILE_NAME: &str = ".snips";

/// Walk up from the given start directory (or cwd) looking for a `.snips` file.
///
/// Returns `Ok(Some(path))` if found, `Ok(None)` if not found.
pub fn find_snipfile(start: Option<&Path>) -> Result<Option<PathBuf>> {
    let start_dir = match start {
        Some(p) => {
            if p.is_file() {
                p.parent()
                    .ok_or_else(|| anyhow::anyhow!("path has no parent"))?
                    .to_path_buf()
            } else {
                p.to_path_buf()
            }
        }
        None => std::env::current_dir().context("failed to get current directory")?,
    };

    let mut dir = start_dir;
    loop {
        let candidate = dir.join(SNIPFILE_NAME);
        if candidate.is_file() {
            return Ok(Some(candidate));
        }
        if !dir.pop() {
            return Ok(None);
        }
    }
}

/// Read and parse a `.snips` file from the given path.
pub fn read_snippets(path: &Path) -> Result<SnipFile> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read .snips file: {}", path.display()))?;

    let value: toml::Value = content
        .parse::<toml::Value>()
        .with_context(|| format!("failed to parse .snips file: {}", path.display()))?;

    SnipFile::from_toml_value(&value)
}

/// Write a `SnipFile` to disk at the given path.
///
/// The file is written atomically: first to a temporary file, then renamed.
pub fn write_snippets(path: &Path, data: &SnipFile) -> Result<()> {
    let toml_value = data.to_toml_value();

    let toml_str = toml::to_string_pretty(&toml_value)
        .context("failed to serialize .snips data to TOML")?;

    // Ensure parent directory exists.
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory: {}", parent.display()))?;
    }

    // Atomic write via temp file.
    let tmp_path = path.with_extension("snips.tmp");
    {
        let mut file = fs::File::create(&tmp_path)
            .with_context(|| format!("failed to create temp file: {}", tmp_path.display()))?;
        file.write_all(toml_str.as_bytes())
            .context("failed to write .snips data")?;
    }
    fs::rename(&tmp_path, path).with_context(|| {
        format!(
            "failed to rename temp file {} -> {}",
            tmp_path.display(),
            path.display()
        )
    })?;

    Ok(())
}

/// Add a snippet to the `.snips` file at the given path.
///
/// If the file doesn't exist, it is created. If the key already exists, the
/// snippet is replaced.
pub fn add_snippet(
    path: &Path,
    section: &str,
    name: &str,
    snippet: Snippet,
) -> Result<()> {
    let mut file = if path.exists() {
        read_snippets(path)?
    } else {
        SnipFile::new()
    };

    let key = if name.is_empty() {
        section.to_string()
    } else {
        format!("{}.{}", section, name)
    };

    file.insert(key, snippet);
    write_snippets(path, &file)?;

    Ok(())
}

/// Remove a snippet from the `.snips` file at the given path.
///
/// Returns the removed snippet, or an error if not found.
pub fn remove_snippet(path: &Path, section: &str, name: &str) -> Result<Snippet> {
    let mut file = read_snippets(path)?;

    let key = if name.is_empty() {
        section.to_string()
    } else {
        format!("{}.{}", section, name)
    };

    let removed = file
        .remove(&key)
        .ok_or_else(|| anyhow::anyhow!("snippet '{}' not found in {}", key, path.display()))?;

    write_snippets(path, &file)?;
    Ok(removed)
}

/// List all snippets in a `SnipFile` as `(section, name, &Snippet)` tuples.
///
/// For a key like `"build.release"`, section = `"build"`, name = `"release"`.
/// For a key like `"test"`, section = `"test"`, name = `""`.
pub fn list_snippets(file: &SnipFile) -> Vec<(&str, &str, &Snippet)> {
    file.iter()
        .map(|(key, snippet)| {
            if let Some(dot) = key.find('.') {
                let section = &key[..dot];
                let name = &key[dot + 1..];
                (section, name, snippet)
            } else {
                (key.as_str(), "", snippet)
            }
        })
        .collect()
}

/// Resolve a short-form key to a fully-qualified key.
pub fn resolve_key<'a>(file: &'a SnipFile, input: &str) -> Option<String> {
    // Exact match first.
    if file.get(input).is_some() {
        return Some(input.to_string());
    }

    // Try as a section prefix.
    let prefix_matches: Vec<&String> = file
        .iter()
        .filter(|(k, _)| k.starts_with(&format!("{}.", input)))
        .map(|(k, _)| k)
        .collect();

    if prefix_matches.len() == 1 {
        return Some(prefix_matches[0].clone());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_snipfile_from_subdir() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile_path = tmp.path().join(SNIPFILE_NAME);
        fs::write(&snipfile_path, "").unwrap();

        let subdir = tmp.path().join("a/b/c");
        fs::create_dir_all(&subdir).unwrap();

        let found = find_snipfile(Some(&subdir)).unwrap().unwrap();
        assert_eq!(found, snipfile_path);
    }

    #[test]
    fn find_snipfile_none() {
        let tmp = tempfile::tempdir().unwrap();
        let found = find_snipfile(Some(tmp.path())).unwrap();
        assert!(found.is_none());
    }
}