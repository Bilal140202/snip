use std::fs;
use std::path::Path;

use super::{DetectedSnippet, ProjectDetector};

/// Detects Go projects by looking for `go.mod`.
pub struct GoDetector;

impl ProjectDetector for GoDetector {
    fn name(&self) -> &str {
        "Go"
    }

    fn detect(&self, root: &Path) -> bool {
        root.join("go.mod").exists()
    }

    fn extract(&self, root: &Path) -> Vec<DetectedSnippet> {
        // Even though we don't parse go.mod for scripts, we provide the
        // common `go` commands that every Go project uses.
        let _ = fs::read_to_string(root.join("go.mod")); // touch for error-silencing

        let common = [
            ("build", "Build the project", "go build ./..."),
            ("test", "Run tests", "go test ./..."),
            ("run", "Run main package", "go run ."),
            ("fmt", "Format code (gofmt)", "gofmt -w ."),
            ("vet", "Run go vet", "go vet ./..."),
            ("mod.tidy", "Tidy go.mod", "go mod tidy"),
            ("mod.download", "Download dependencies", "go mod download"),
        ];

        common
            .iter()
            .map(|(name, desc, cmd)| {
                (
                    "go".to_string(),
                    name.to_string(),
                    cmd.to_string(),
                    desc.to_string(),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_go_detect() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("go.mod"),
            "module example.com/foo\n\ngo 1.21\n",
        )
        .unwrap();

        let detector = GoDetector;
        assert!(detector.detect(tmp.path()));

        let snippets = detector.extract(tmp.path());
        assert!(snippets.len() >= 5);
        assert_eq!(snippets[0].0, "go");
        let names: Vec<&str> = snippets.iter().map(|s| s.1.as_str()).collect();
        assert!(names.contains(&"build"));
        assert!(names.contains(&"test"));
    }

    #[test]
    fn test_go_no_mod() {
        let tmp = tempfile::tempdir().unwrap();
        let detector = GoDetector;
        assert!(!detector.detect(tmp.path()));
    }
}
