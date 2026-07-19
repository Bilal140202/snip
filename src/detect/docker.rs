use std::fs;
use std::path::Path;

use super::{DetectedSnippet, ProjectDetector};

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

        let value: serde_yaml::Value = match serde_yaml::from_str(&content) {
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
            snippets.push((
                "docker".to_string(),
                name_str.to_string(),
                format!("docker compose up {}", name_str),
                format!("Start {} service", name_str),
            ));
        }

        snippets
    }
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