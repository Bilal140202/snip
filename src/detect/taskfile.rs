use std::fs;
use std::path::Path;

use super::{DetectedSnippet, ProjectDetector};

/// Detects Taskfile projects by looking for `Taskfile.yml` / `Taskfile.yaml`.
pub struct TaskfileDetector;

impl ProjectDetector for TaskfileDetector {
    fn name(&self) -> &str {
        "Taskfile"
    }

    fn detect(&self, root: &Path) -> bool {
        root.join("Taskfile.yml").exists() || root.join("Taskfile.yaml").exists()
    }

    fn extract(&self, root: &Path) -> Vec<DetectedSnippet> {
        let path = if root.join("Taskfile.yml").exists() {
            root.join("Taskfile.yml")
        } else {
            root.join("Taskfile.yaml")
        };

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        // Lightweight YAML parse: detect top-level `tasks:` block, then
        // indented `name:` keys. This intentionally does NOT support every
        // YAML feature — for complex Taskfiles, fall back to running `task --list`.
        let mut snippets = Vec::new();
        let mut in_tasks = false;
        let mut tasks_indent: Option<usize> = None;

        for line in content.lines() {
            // Skip comments and blank lines for indentation calculation
            let trimmed = line.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let indent = line.len() - trimmed.len();

            // Top-level key?
            if indent == 0 {
                in_tasks = trimmed.starts_with("tasks:") || trimmed.starts_with("tasks :");
                tasks_indent = None;
                continue;
            }

            if !in_tasks {
                continue;
            }

            // First indented line under tasks: defines the task indent level
            if tasks_indent.is_none() {
                tasks_indent = Some(indent);
            }

            // A task name line is indented exactly at tasks_indent and ends with ':'
            if Some(indent) == tasks_indent {
                if let Some(colon) = trimmed.find(':') {
                    let name = trimmed[..colon].trim();
                    // Skip YAML list markers / directives
                    if name.is_empty() || name.starts_with('-') {
                        continue;
                    }
                    let desc = trimmed[colon + 1..].trim().trim_matches('"').to_string();
                    let desc = if desc.is_empty() {
                        format!("task {}", name)
                    } else {
                        desc
                    };
                    snippets.push((
                        "task".to_string(),
                        name.to_string(),
                        format!("task {}", name),
                        desc,
                    ));
                }
            }
        }

        snippets
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_taskfile_detect() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("Taskfile.yml"),
            r#"version: '3'
tasks:
  build:
    cmds:
      - go build ./...
  test:
    cmds:
      - go test ./...
  deploy:
    desc: Deploy to production
    cmds:
      - kubectl apply -f k8s/
"#,
        )
        .unwrap();

        let detector = TaskfileDetector;
        assert!(detector.detect(tmp.path()));

        let snippets = detector.extract(tmp.path());
        assert_eq!(snippets.len(), 3);
        let names: Vec<&str> = snippets.iter().map(|s| s.1.as_str()).collect();
        assert!(names.contains(&"build"));
        assert!(names.contains(&"test"));
        assert!(names.contains(&"deploy"));
        assert_eq!(snippets[0].2, "task build");
    }

    #[test]
    fn test_taskfile_yaml_extension() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("Taskfile.yaml"),
            "version: '3'\ntasks:\n  hello:\n    cmds:\n      - echo hi\n",
        )
        .unwrap();

        let detector = TaskfileDetector;
        assert!(detector.detect(tmp.path()));
        let snippets = detector.extract(tmp.path());
        assert_eq!(snippets.len(), 1);
        assert_eq!(snippets[0].1, "hello");
    }

    #[test]
    fn test_taskfile_no_file() {
        let tmp = tempfile::tempdir().unwrap();
        let detector = TaskfileDetector;
        assert!(!detector.detect(tmp.path()));
    }
}
