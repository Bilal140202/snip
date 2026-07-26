use std::fs;
use std::path::Path;

use super::{DetectedSnippet, ProjectDetector};

/// Detects `just` projects by looking for a `justfile` or `Justfile`.
pub struct JustDetector;

impl ProjectDetector for JustDetector {
    fn name(&self) -> &str {
        "just"
    }

    fn detect(&self, root: &Path) -> bool {
        ["justfile", "Justfile", "JUSTFILE"]
            .iter()
            .any(|n| root.join(n).exists())
    }

    fn extract(&self, root: &Path) -> Vec<DetectedSnippet> {
        let path = ["justfile", "Justfile", "JUSTFILE"]
            .iter()
            .map(|n| root.join(n))
            .find(|p| p.exists());

        let path = match path {
            Some(p) => p,
            None => return Vec::new(),
        };

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let mut snippets = Vec::new();
        let mut pending_doc: Vec<String> = Vec::new();

        for raw in content.lines() {
            let line = raw.trim_end();
            let trimmed = line.trim_start();

            // Just supports `# comment` doc lines immediately above a recipe
            if let Some(rest) = trimmed.strip_prefix("# ") {
                pending_doc.push(rest.to_string());
                continue;
            }
            if trimmed == "#" {
                pending_doc.push(String::new());
                continue;
            }

            // Skip non-recipe lines
            if trimmed.is_empty()
                || trimmed.starts_with("import")
                || trimmed.starts_with("export")
                || trimmed.contains('=')
                || trimmed.starts_with("set ")
            {
                if !trimmed.starts_with('#') {
                    pending_doc.clear();
                }
                continue;
            }

            // Recipe line: `name:` or `name arg1 arg2:` or `name: deps`
            if let Some(colon) = trimmed.find(':') {
                let head = &trimmed[..colon];
                // The recipe name is the first whitespace-separated token
                let name = head.split_whitespace().next().unwrap_or("");
                if name.is_empty()
                    || name.starts_with('@')
                    || name.starts_with('_')
                    || name == "default"
                {
                    pending_doc.clear();
                    continue;
                }
                let desc = if pending_doc.is_empty() {
                    format!("just {}", name)
                } else {
                    pending_doc.join(" ")
                };
                snippets.push((
                    "just".to_string(),
                    name.to_string(),
                    format!("just {}", name),
                    desc,
                ));
                pending_doc.clear();
            }
        }

        snippets
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_just_detect() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("justfile"),
            r#"# Build the project
build:
    cargo build

# Run tests
test:
    cargo test

_default:
    @just --list
"#,
        )
        .unwrap();

        let detector = JustDetector;
        assert!(detector.detect(tmp.path()));

        let snippets = detector.extract(tmp.path());
        let names: Vec<&str> = snippets.iter().map(|s| s.1.as_str()).collect();
        assert!(names.contains(&"build"));
        assert!(names.contains(&"test"));
        // Underscore-prefixed and `default` should be skipped
        assert!(!names.contains(&"_default"));
        assert!(!names.contains(&"default"));

        // Check descriptions
        let build = snippets.iter().find(|s| s.1 == "build").unwrap();
        assert_eq!(build.3, "Build the project");
    }

    #[test]
    fn test_just_uppercase_filename() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("Justfile"), "hello:\n    echo hi\n").unwrap();

        let detector = JustDetector;
        assert!(detector.detect(tmp.path()));
        let snippets = detector.extract(tmp.path());
        assert_eq!(snippets.len(), 1);
        assert_eq!(snippets[0].1, "hello");
    }

    #[test]
    fn test_just_no_file() {
        let tmp = tempfile::tempdir().unwrap();
        let detector = JustDetector;
        assert!(!detector.detect(tmp.path()));
    }
}
