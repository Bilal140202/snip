use std::path::{Path, PathBuf};

/// Walk up from the current working directory looking for a git repository root
/// (a directory containing `.git`).
pub fn find_repo_root() -> Option<PathBuf> {
    find_repo_root_from(&std::env::current_dir().ok()?)
}

/// Walk up from the given start directory looking for a git repository root.
pub fn find_repo_root_from(start: &Path) -> Option<PathBuf> {
    let mut dir = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };

    loop {
        if dir.join(".git").exists() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Check whether the current working directory is inside a git repository.
pub fn is_git_repo() -> bool {
    find_repo_root().is_some()
}

/// Check whether the given path is inside a git repository.
pub fn is_git_repo_from(start: &Path) -> bool {
    find_repo_root_from(start).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn find_repo_root_finds_git() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join(".git")).unwrap();

        let subdir = tmp.path().join("src/main.rs");
        fs::create_dir_all(subdir.parent().unwrap()).unwrap();
        fs::write(&subdir, "").unwrap();

        let root = find_repo_root_from(&subdir).unwrap();
        assert_eq!(root, tmp.path());
    }

    #[test]
    fn find_repo_root_none_when_no_git() {
        let tmp = tempfile::tempdir().unwrap();
        let result = find_repo_root_from(tmp.path());
        assert!(result.is_none());
    }

    #[test]
    fn is_git_repo_true() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join(".git")).unwrap();
        assert!(is_git_repo_from(tmp.path()));
    }

    #[test]
    fn is_git_repo_false() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(!is_git_repo_from(tmp.path()));
    }
}