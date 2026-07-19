use std::fs;
use std::path::Path;

use super::{DetectedSnippet, ProjectDetector};

/// Detects Rust/Cargo projects by looking for `Cargo.toml`.
pub struct CargoDetector;

impl ProjectDetector for CargoDetector {
    fn name(&self) -> &str {
        "Cargo"
    }

    fn detect(&self, root: &Path) -> bool {
        root.join("Cargo.toml").exists()
    }

    fn extract(&self, root: &Path) -> Vec<DetectedSnippet> {
        let content = match fs::read_to_string(root.join("Cargo.toml")) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let value: toml::Value = match content.parse() {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };

        let mut snippets = Vec::new();

        // Check for [package.metadata.scripts]
        if let Some(scripts) = value
            .get("package")
            .and_then(|p| p.get("metadata"))
            .and_then(|m| m.get("scripts"))
            .and_then(|s| s.as_table())
        {
            for (name, val) in scripts {
                let cmd = match val.as_str() {
                    Some(s) => s.to_string(),
                    None => continue,
                };
                snippets.push((
                    "cargo".to_string(),
                    name.clone(),
                    cmd,
                    name.clone(),
                ));
            }
        }

        // If no custom scripts, provide common cargo commands
        if snippets.is_empty() {
            let common = [
                ("build", "Build the project", "cargo build"),
                ("test", "Run tests", "cargo test"),
                ("run", "Run the project", "cargo run"),
                ("check", "Type-check without building", "cargo check"),
                ("fmt", "Format code", "cargo fmt"),
                ("clippy", "Run linter", "cargo clippy"),
            ];

            for (name, desc, cmd) in common {
                snippets.push((
                    "cargo".to_string(),
                    name.to_string(),
                    cmd.to_string(),
                    desc.to_string(),
                ));
            }
        }

        snippets
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cargo_detect() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("Cargo.toml"),
            r#"[package]
name = "test"
version = "0.1.0"
edition = "2021"
"#,
        )
        .unwrap();

        let detector = CargoDetector;
        assert!(detector.detect(tmp.path()));

        let snippets = detector.extract(tmp.path());
        // Should provide common commands when no custom scripts
        assert!(!snippets.is_empty());
        assert_eq!(snippets[0].0, "cargo");
    }

    #[test]
    fn test_cargo_custom_scripts() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("Cargo.toml"),
            r#"[package]
name = "test"
version = "0.1.0"
edition = "2021"

[package.metadata.scripts]
lint = "cargo clippy -- -D warnings"
ci = "cargo test --all-features"
"#,
        )
        .unwrap();

        let detector = CargoDetector;
        let snippets = detector.extract(tmp.path());
        assert_eq!(snippets.len(), 2);
        let names: Vec<&str> = snippets.iter().map(|s| s.1.as_str()).collect();
        assert!(names.contains(&"lint"));
        assert!(names.contains(&"ci"));
    }
}