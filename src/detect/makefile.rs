use std::fs;
use std::path::Path;

use super::{DetectedSnippet, ProjectDetector};

/// Detects Make projects by looking for a `Makefile`.
pub struct MakeDetector;

impl ProjectDetector for MakeDetector {
    fn name(&self) -> &str {
        "Make"
    }

    fn detect(&self, root: &Path) -> bool {
        root.join("Makefile").exists() || root.join("makefile").exists()
    }

    fn extract(&self, root: &Path) -> Vec<DetectedSnippet> {
        let makefile_path = if root.join("Makefile").exists() {
            root.join("Makefile")
        } else {
            root.join("makefile")
        };

        let content = match fs::read_to_string(&makefile_path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        // Parse .PHONY targets
        let phony_targets = parse_phony_targets(&content);

        // Parse all targets with their ## descriptions
        let target_descriptions = parse_target_descriptions(&content);

        let mut snippets = Vec::new();

        // If we have .PHONY targets, only use those; otherwise use all targets
        let targets: Vec<&str> = if phony_targets.is_empty() {
            target_descriptions.keys().map(|s| s.as_str()).collect()
        } else {
            phony_targets
                .iter()
                .filter(|t| target_descriptions.contains_key(*t))
                .map(|t| t.as_str())
                .collect()
        };

        for target in &targets {
            if let Some(desc) = target_descriptions.get(*target) {
                snippets.push((
                    "make".to_string(),
                    target.to_string(),
                    format!("make {}", target),
                    desc.clone(),
                ));
            }
        }

        snippets
    }
}

/// Parse `.PHONY: target1 target2 ...` lines.
fn parse_phony_targets(content: &str) -> Vec<String> {
    let mut targets = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(".PHONY:") {
            let rest = &trimmed[7..];
            for part in rest.split_whitespace() {
                targets.push(part.to_string());
            }
        }
    }
    targets
}

/// Parse targets with their `## description` comments.
/// Returns a map of target name → description.
fn parse_target_descriptions(content: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let lines: Vec<&str> = content.lines().collect();

    let mut pending_desc = String::new();

    for i in 0..lines.len() {
        let line = lines[i].trim();

        // Collect ## description lines
        if line.starts_with("##") {
            let desc = line[2..].trim().to_string();
            if pending_desc.is_empty() {
                pending_desc = desc;
            } else {
                pending_desc.push(' ');
                pending_desc.push_str(&desc);
            }
            continue;
        }

        // Check if this is a target line: "target:" or "target: deps"
        if let Some(colon_pos) = line.find(':') {
            let potential_target = line[..colon_pos].trim();
            // Make sure it's a valid target name (no spaces, not a variable)
            if !potential_target.is_empty()
                && !potential_target.contains('$')
                && !potential_target.contains(' ')
                && !potential_target.starts_with('.')
            {
                let desc = if pending_desc.is_empty() {
                    potential_target.to_string()
                } else {
                    std::mem::take(&mut pending_desc)
                };
                map.insert(potential_target.to_string(), desc);
            }
        } else {
            // Reset description if we hit a non-target, non-comment line
            if !line.is_empty() && !line.starts_with('#') {
                pending_desc.clear();
            }
        }
    }

    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_detect() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("Makefile"),
            r#".PHONY: build test

## Build the project
build:
        tsc

## Run tests
test:
        jest
"#,
        )
        .unwrap();

        let detector = MakeDetector;
        assert!(detector.detect(tmp.path()));

        let snippets = detector.extract(tmp.path());
        assert_eq!(snippets.len(), 2);
        assert_eq!(snippets[0].1, "build");
        assert_eq!(snippets[0].3, "Build the project");
    }

    #[test]
    fn test_make_no_makefile() {
        let tmp = tempfile::tempdir().unwrap();
        let detector = MakeDetector;
        assert!(!detector.detect(tmp.path()));
    }
}