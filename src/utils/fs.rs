use std::path::{Path, PathBuf};

/// Find the project root by looking for `.git` or `.snips` while walking up
/// from `start` (or cwd).
pub fn find_project_root(start: Option<&Path>) -> Option<PathBuf> {
    let cwd = start
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    let mut dir = cwd.as_path();
    loop {
        if dir.join(".git").exists() || dir.join(".snips").exists() {
            return Some(dir.to_path_buf());
        }
        match dir.parent() {
            Some(parent) if parent != dir => dir = parent,
            _ => return None,
        }
    }
}