//! `.snips` file I/O — read, write, find, and merge snippet files.
//!
//! Supports:
//! - Single `.snips` file (primary)
//! - `.snips.d/` directory with modular TOML files (merged by priority)
//! - `format` version header for forward compatibility

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::snippet::{SnipFile, Snippet};

const SNIPFILE_NAME: &str = ".snips";
const SNIPS_DIR_NAME: &str = ".snips.d";
const CURRENT_FORMAT_VERSION: &str = "1.0";

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

/// Read a single `.snips` or `.snips.d/*.toml` file, ignoring `format` header.
fn read_single_snipfile(path: &Path) -> Result<SnipFile> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read snippet file: {}", path.display()))?;

    let value: toml::Value = content
        .parse::<toml::Value>()
        .with_context(|| format!("failed to parse snippet file: {}", path.display()))?;

    SnipFile::from_toml_value(&value)
}

/// Read all snippets from the primary `.snips` file merged with `.snips.d/` directory.
///
/// The merge chain (later entries override earlier ones):
/// 1. `.snips.d/*.toml` files (sorted alphabetically, except `_local.toml` which has highest priority)
/// 2. `.snips` (main file — overrides everything in `.snips.d/`)
/// 3. `.snips.d/_local.toml` (local overrides — never committed, highest priority)
pub fn read_all_snippets(root: &Path) -> Result<SnipFile> {
    let mut merged = SnipFile::new();
    let snips_path = root.join(SNIPFILE_NAME);
    let snips_dir = root.join(SNIPS_DIR_NAME);

    // Step 1: Read .snips.d/*.toml (sorted, excluding _local.toml)
    if snips_dir.is_dir() {
        let mut toml_files: Vec<PathBuf> = fs::read_dir(&snips_dir)
            .with_context(|| format!("failed to read directory: {}", snips_dir.display()))?
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "toml") {
                    let name = path.file_name()?.to_string_lossy().to_string();
                    // _local.toml is handled last
                    if !name.starts_with('_') {
                        Some(path)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        toml_files.sort();

        for path in toml_files {
            let file = read_single_snipfile(&path)?;
            merge_snipfile(&mut merged, file);
        }
    }

    // Step 2: Read main .snips (overrides .snips.d/)
    if snips_path.is_file() {
        let file = read_single_snipfile(&snips_path)?;
        merge_snipfile(&mut merged, file);
    }

    // Step 3: Read .snips.d/_local.toml (highest priority, never committed)
    let local_path = snips_dir.join("_local.toml");
    if local_path.is_file() {
        let file = read_single_snipfile(&local_path)?;
        merge_snipfile(&mut merged, file);
    }

    Ok(merged)
}

/// Merge `other` into `base`, with `other` overriding `base` for same keys.
fn merge_snipfile(base: &mut SnipFile, other: SnipFile) {
    for (key, snippet) in other.iter() {
        base.insert(key.clone(), snippet.clone());
    }
}

/// Write a `SnipFile` to disk at the given path.
///
/// The file is written atomically: first to a temporary file, then renamed.
/// Includes the `format` version header.
pub fn write_snippets(path: &Path, data: &SnipFile) -> Result<()> {
    let mut toml_value = data.to_toml_value();

    // Insert format version header
    if let Some(table) = toml_value.as_table_mut() {
        table.insert(
            "format".to_string(),
            toml::Value::String(CURRENT_FORMAT_VERSION.to_string()),
        );
    }

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

/// Read the format version from a .snips file, if present.
pub fn read_format_version(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let value: toml::Value = content.parse().ok()?;
    value
        .get("format")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Check if a .snips file uses the current format version.
pub fn is_current_format(path: &Path) -> bool {
    read_format_version(path)
        .map(|v| v == CURRENT_FORMAT_VERSION)
        .unwrap_or(false)
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

/// Find the `.snips.d/` directory for the given project root.
/// Creates it if it doesn't exist (when `create` is true).
pub fn find_snips_dir(root: &Path, create: bool) -> Result<PathBuf> {
    let dir = root.join(SNIPS_DIR_NAME);
    if create && !dir.exists() {
        fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create {}: ", dir.display()))?;
    }
    Ok(dir)
}

/// List all `.toml` files in `.snips.d/` directory.
pub fn list_snips_d_files(root: &Path) -> Result<Vec<PathBuf>> {
    let dir = root.join(SNIPS_DIR_NAME);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .with_context(|| format!("failed to read {}", dir.display()))?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "toml") {
                Some(path)
            } else {
                None
            }
        })
        .collect();

    files.sort();
    Ok(files)
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

    #[test]
    fn write_includes_format_header() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");

        let mut file = SnipFile::new();
        file.insert("hello", Snippet::new("echo hello"));
        write_snippets(&snipfile, &file).unwrap();

        let content = fs::read_to_string(&snipfile).unwrap();
        assert!(content.contains("format = \"1.0\""));
    }

    #[test]
    fn read_format_version_works() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");
        fs::write(&snipfile, "format = \"1.0\"\n\n[hello]\ncmd = \"echo hello\"\n").unwrap();

        assert_eq!(read_format_version(&snipfile), Some("1.0".to_string()));
        assert!(is_current_format(&snipfile));
    }

    #[test]
    fn read_all_snippets_merges_d_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Create .snips.d/ with a file
        let dir = root.join(".snips.d");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("common.toml"),
            "[build]\ncmd = \"make build\"\n",
        )
        .unwrap();

        // Create main .snips
        fs::write(
            root.join(".snips"),
            "[test]\ncmd = \"make test\"\n",
        )
        .unwrap();

        let merged = read_all_snippets(root).unwrap();
        assert_eq!(merged.len(), 2);
        assert!(merged.get("build").is_some());
        assert!(merged.get("test").is_some());
    }

    #[test]
    fn read_all_snippets_local_overrides() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Main .snips
        fs::write(
            root.join(".snips"),
            "[build]\ncmd = \"make build\"\n",
        )
        .unwrap();

        // _local.toml overrides
        let dir = root.join(".snips.d");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("_local.toml"),
            "[build]\ncmd = \"make build --verbose\"\n",
        )
        .unwrap();

        let merged = read_all_snippets(root).unwrap();
        assert_eq!(merged.len(), 1);
        assert_eq!(merged.get("build").unwrap().cmd, "make build --verbose");
    }
}