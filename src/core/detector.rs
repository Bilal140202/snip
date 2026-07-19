use std::path::Path;

use crate::detect;

/// Run all project detectors on the given root and return detected snippets.
///
/// This is a convenience wrapper around `detect::detect_all`.
pub fn detect_snippets(root: &Path) -> Vec<detect::DetectedSnippet> {
    detect::detect_all(root)
        .into_iter()
        .map(|(_source, snippet)| snippet)
        .collect()
}