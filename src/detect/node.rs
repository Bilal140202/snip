use std::fs;
use std::path::Path;

use serde_json::Value;

use super::{DetectedSnippet, ProjectDetector};

/// Detects Node.js projects by looking for `package.json`.
pub struct NodeDetector;

impl ProjectDetector for NodeDetector {
    fn name(&self) -> &str {
        "Node.js"
    }

    fn detect(&self, root: &Path) -> bool {
        root.join("package.json").exists()
    }

    fn extract(&self, root: &Path) -> Vec<DetectedSnippet> {
        let content = match fs::read_to_string(root.join("package.json")) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let json: Value = match serde_json::from_str(&content) {
            Ok(j) => j,
            Err(_) => return Vec::new(),
        };

        let scripts = match json.get("scripts") {
            Some(Value::Object(m)) => m,
            _ => return Vec::new(),
        };

        let mut snippets = Vec::new();

        for (name, value) in scripts {
            if name.starts_with("pre") || name.starts_with("post") {
                continue; // skip lifecycle scripts
            }
            let _cmd = match value.as_str() {
                Some(s) => s.to_string(),
                None => continue,
            };
            // Use the script name itself as description
            let desc = name.clone();
            snippets.push((
                "npm".to_string(),
                name.clone(),
                format!("npm run {}", name),
                desc,
            ));
        }

        snippets
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_detect() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("package.json"), r#"{"scripts":{"build":"tsc","test":"jest"}}"#).unwrap();

        let detector = NodeDetector;
        assert!(detector.detect(tmp.path()));

        let snippets = detector.extract(tmp.path());
        assert_eq!(snippets.len(), 2);
        assert_eq!(snippets[0].1, "build");
        assert_eq!(snippets[0].2, "npm run build");
    }

    #[test]
    fn test_node_no_package_json() {
        let tmp = tempfile::tempdir().unwrap();
        let detector = NodeDetector;
        assert!(!detector.detect(tmp.path()));
    }

    #[test]
    fn test_node_skips_lifecycle_scripts() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("package.json"),
            r#"{"scripts":{"prebuild":"echo pre","build":"tsc","postbuild":"echo post"}}"#,
        )
        .unwrap();

        let detector = NodeDetector;
        let snippets = detector.extract(tmp.path());
        assert_eq!(snippets.len(), 1);
        assert_eq!(snippets[0].1, "build");
    }
}