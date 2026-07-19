use std::fs;
use std::path::Path;

use super::{DetectedSnippet, ProjectDetector};

/// Detects Python projects by looking for `pyproject.toml`.
pub struct PythonDetector;

impl ProjectDetector for PythonDetector {
    fn name(&self) -> &str {
        "Python"
    }

    fn detect(&self, root: &Path) -> bool {
        root.join("pyproject.toml").exists()
    }

    fn extract(&self, root: &Path) -> Vec<DetectedSnippet> {
        let content = match fs::read_to_string(root.join("pyproject.toml")) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let value: toml::Value = match content.parse() {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };

        let mut snippets = Vec::new();

        // Check [project.scripts] — console_scripts
        if let Some(scripts) = value
            .get("project")
            .and_then(|p| p.get("scripts"))
            .and_then(|s| s.as_table())
        {
            for (name, val) in scripts {
                let cmd = match val.as_str() {
                    Some(s) => s.to_string(),
                    None => continue,
                };
                snippets.push((
                    "python".to_string(),
                    name.clone(),
                    cmd,
                    name.clone(),
                ));
            }
        }

        // Check [tool.pdm.scripts]
        if let Some(scripts) = value
            .get("tool")
            .and_then(|t| t.get("pdm"))
            .and_then(|p| p.get("scripts"))
            .and_then(|s| s.as_table())
        {
            for (name, val) in scripts {
                // PDM scripts can be strings or tables with `cmd` key
                let cmd = match val.as_str() {
                    Some(s) => s.to_string(),
                    None => match val.get("cmd").and_then(|c| c.as_str()) {
                        Some(s) => s.to_string(),
                        None => continue,
                    },
                };
                // Skip if we already have this name
                if snippets.iter().any(|(_, n, _, _)| n == name) {
                    continue;
                }
                let desc = val
                    .get("help")
                    .and_then(|h| h.as_str())
                    .unwrap_or(name)
                    .to_string();
                snippets.push((
                    "python".to_string(),
                    name.clone(),
                    cmd,
                    desc,
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
    fn test_python_detect() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("pyproject.toml"),
            r#"[project]
name = "test"
version = "0.1.0"

[project.scripts]
mycli = "test:main"
"#,
        )
        .unwrap();

        let detector = PythonDetector;
        assert!(detector.detect(tmp.path()));

        let snippets = detector.extract(tmp.path());
        assert_eq!(snippets.len(), 1);
        assert_eq!(snippets[0].1, "mycli");
        assert_eq!(snippets[0].0, "python");
    }

    #[test]
    fn test_python_pdm_scripts() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("pyproject.toml"),
            r#"[tool.pdm.scripts]
lint = { cmd = "ruff check .", help = "Run linter" }
test = "pytest"
"#,
        )
        .unwrap();

        let detector = PythonDetector;
        let snippets = detector.extract(tmp.path());
        assert_eq!(snippets.len(), 2);
        assert_eq!(snippets[0].1, "lint");
        assert_eq!(snippets[0].3, "Run linter");
    }
}