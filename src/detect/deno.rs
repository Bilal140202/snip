use std::fs;
use std::path::Path;

use serde_json::Value;

use super::{DetectedSnippet, ProjectDetector};

/// Detects Deno projects by looking for `deno.json` or `deno.jsonc`.
pub struct DenoDetector;

impl ProjectDetector for DenoDetector {
    fn name(&self) -> &str {
        "Deno"
    }

    fn detect(&self, root: &Path) -> bool {
        root.join("deno.json").exists() || root.join("deno.jsonc").exists()
    }

    fn extract(&self, root: &Path) -> Vec<DetectedSnippet> {
        let path = if root.join("deno.json").exists() {
            root.join("deno.json")
        } else {
            root.join("deno.jsonc")
        };

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        // deno.jsonc allows comments — strip them for JSON parsing.
        let cleaned = strip_jsonc_comments(&content);
        let json: Value = match serde_json::from_str(&cleaned) {
            Ok(j) => j,
            Err(_) => return common_deno_commands(),
        };

        let tasks = match json.get("tasks").and_then(|t| t.as_object()) {
            Some(m) => m,
            None => return common_deno_commands(),
        };

        let mut snippets = Vec::new();
        for (name, value) in tasks {
            let cmd = match value.as_str() {
                Some(s) => s.to_string(),
                None => continue,
            };
            snippets.push((
                "deno".to_string(),
                name.clone(),
                format!("deno task {}", name),
                name.clone(),
            ));
            // Keep the underlying command as a hidden detail in desc
            let _ = cmd;
        }

        if snippets.is_empty() {
            common_deno_commands()
        } else {
            snippets
        }
    }
}

/// Provide common Deno commands when no tasks are defined.
fn common_deno_commands() -> Vec<DetectedSnippet> {
    let common = [
        ("run", "Run main module", "deno run main.ts"),
        ("test", "Run tests", "deno test"),
        ("fmt", "Format code", "deno fmt"),
        ("lint", "Lint code", "deno lint"),
        ("cache", "Cache dependencies", "deno cache main.ts"),
    ];
    common
        .iter()
        .map(|(name, desc, cmd)| {
            (
                "deno".to_string(),
                name.to_string(),
                cmd.to_string(),
                desc.to_string(),
            )
        })
        .collect()
}

/// Strip `//` line comments and `/* */` block comments from JSONC text
/// so it can be parsed as plain JSON. Naive but works for typical deno.jsonc.
fn strip_jsonc_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                out.push(c);
                // copy string literal verbatim, honoring escapes
                while let Some(&next) = chars.peek() {
                    out.push(chars.next().unwrap());
                    if next == '\\' {
                        if let Some(esc) = chars.next() {
                            out.push(esc);
                        }
                    } else if next == '"' {
                        break;
                    }
                }
            }
            '/' if matches!(chars.peek(), Some('/')) => {
                // line comment — skip to end of line
                for ch in chars.by_ref() {
                    if ch == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if matches!(chars.peek(), Some('*')) => {
                chars.next(); // consume '*'
                let mut prev = ' ';
                for ch in chars.by_ref() {
                    if prev == '*' && ch == '/' {
                        break;
                    }
                    prev = ch;
                }
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deno_detect_json() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("deno.json"),
            r#"{"tasks":{"build":"deno bundle main.ts","test":"deno test"}}"#,
        )
        .unwrap();

        let detector = DenoDetector;
        assert!(detector.detect(tmp.path()));

        let snippets = detector.extract(tmp.path());
        assert_eq!(snippets.len(), 2);
        assert_eq!(snippets[0].0, "deno");
        assert_eq!(snippets[0].1, "build");
        assert_eq!(snippets[0].2, "deno task build");
    }

    #[test]
    fn test_deno_detect_jsonc() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("deno.jsonc"),
            r#"{
                // tasks for this project
                "tasks": {
                    "dev": "deno run --watch main.ts"
                }
            }"#,
        )
        .unwrap();

        let detector = DenoDetector;
        assert!(detector.detect(tmp.path()));

        let snippets = detector.extract(tmp.path());
        assert_eq!(snippets.len(), 1);
        assert_eq!(snippets[0].1, "dev");
    }

    #[test]
    fn test_deno_no_tasks_falls_back_to_common() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("deno.json"), r#"{"compilerOptions":{}}"#).unwrap();

        let detector = DenoDetector;
        let snippets = detector.extract(tmp.path());
        assert!(!snippets.is_empty());
        let names: Vec<&str> = snippets.iter().map(|s| s.1.as_str()).collect();
        assert!(names.contains(&"run"));
        assert!(names.contains(&"test"));
    }

    #[test]
    fn test_deno_no_config() {
        let tmp = tempfile::tempdir().unwrap();
        let detector = DenoDetector;
        assert!(!detector.detect(tmp.path()));
    }
}
