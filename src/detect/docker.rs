use std::fs;
use std::path::Path;

use super::{DetectedSnippet, ProjectDetector};

#[cfg(feature = "detect-docker")]
use serde_yaml;

/// Detects Docker projects by looking for `docker-compose.yml` or `docker-compose.yaml`.
pub struct DockerDetector;

impl ProjectDetector for DockerDetector {
    fn name(&self) -> &str {
        "Docker"
    }

    fn detect(&self, root: &Path) -> bool {
        root.join("docker-compose.yml").exists() || root.join("docker-compose.yaml").exists()
    }

    fn extract(&self, root: &Path) -> Vec<DetectedSnippet> {
        let compose_path = if root.join("docker-compose.yml").exists() {
            root.join("docker-compose.yml")
        } else {
            root.join("docker-compose.yaml")
        };

        let content = match fs::read_to_string(&compose_path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        // When the `detect-docker` feature is disabled, fall back to a
        // naive line-based scan for top-level service names.
        #[cfg(feature = "detect-docker")]
        {
            extract_via_serde_yaml(&content)
        }

        #[cfg(not(feature = "detect-docker"))]
        {
            extract_naive(&content)
        }
    }
}

#[cfg(feature = "detect-docker")]
fn extract_via_serde_yaml(content: &str) -> Vec<DetectedSnippet> {
    let value: serde_yaml::Value = match serde_yaml::from_str(content) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let services = match value.get("services").and_then(|s| s.as_mapping()) {
        Some(m) => m,
        None => return Vec::new(),
    };

    let mut snippets = Vec::new();
    for (name, _config) in services {
        let name_str = name.as_str().unwrap_or_default();
        if name_str.is_empty() {
            continue;
        }
        snippets.push((
            "docker".to_string(),
            name_str.to_string(),
            format!("docker compose up {}", name_str),
            format!("Start {} service", name_str),
        ));
    }
    snippets
}

/// Naive line-based parser used when serde_yaml is not available.
/// Recognises a `services:` block with 2-space-indented `name:` keys.
#[cfg(not(feature = "detect-docker"))]
fn extract_naive(content: &str) -> Vec<DetectedSnippet> {
    let mut snippets = Vec::new();
    let mut in_services = false;
    let mut services_indent: Option<usize> = None;

    for raw in content.lines() {
        let trimmed = raw.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = raw.len() - trimmed.len();

        if indent == 0 {
            in_services = trimmed.starts_with("services:");
            services_indent = None;
            continue;
        }

        if !in_services {
            continue;
        }

        if services_indent.is_none() {
            services_indent = Some(indent);
        }

        if Some(indent) == services_indent {
            if let Some(colon) = trimmed.find(':') {
                let name = trimmed[..colon].trim();
                if !name.is_empty() && !name.starts_with('-') {
                    snippets.push((
                        "docker".to_string(),
                        name.to_string(),
                        format!("docker compose up {}", name),
                        format!("Start {} service", name),
                    ));
                }
            }
        }
    }

    snippets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_docker_detect() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("docker-compose.yml"),
            r#"services:
  web:
    build: .
    ports:
      - "3000:3000"
  db:
    image: postgres:15
"#,
        )
        .unwrap();

        let detector = DockerDetector;
        assert!(detector.detect(tmp.path()));

        let snippets = detector.extract(tmp.path());
        assert_eq!(snippets.len(), 2);
        assert_eq!(snippets[0].1, "web");
        assert_eq!(snippets[1].1, "db");
    }
}
