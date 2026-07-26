use std::fs;
use std::path::Path;

use super::{DetectedSnippet, ProjectDetector};

/// Detects Ruby Rake projects by looking for a `Rakefile`.
pub struct RakeDetector;

impl ProjectDetector for RakeDetector {
    fn name(&self) -> &str {
        "Rake"
    }

    fn detect(&self, root: &Path) -> bool {
        root.join("Rakefile").exists() || root.join("rakefile").exists()
    }

    fn extract(&self, root: &Path) -> Vec<DetectedSnippet> {
        let path = if root.join("Rakefile").exists() {
            root.join("Rakefile")
        } else {
            root.join("rakefile")
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

            // Rake uses `# ...` comments immediately above a task
            if let Some(rest) = trimmed.strip_prefix("# ") {
                pending_doc.push(rest.to_string());
                continue;
            }
            if trimmed == "#" {
                pending_doc.push(String::new());
                continue;
            }

            // Skip blank or non-task lines
            if trimmed.is_empty() || trimmed.starts_with("require") || trimmed.starts_with("load") {
                if !trimmed.starts_with('#') {
                    pending_doc.clear();
                }
                continue;
            }

            // Patterns:
            //   task :name
            //   task :name => [:dep]
            //   task :name do ... end
            //   desc "..."  (Rake's built-in doc system)
            if trimmed.starts_with("desc ") {
                let d = trimmed.strip_prefix("desc ").unwrap_or("").trim();
                let d = d.trim_matches(|c| c == '"' || c == '\'');
                pending_doc.push(d.to_string());
                continue;
            }

            if trimmed.starts_with("task ") {
                // Extract the task name token after "task "
                let rest = trimmed.strip_prefix("task ").unwrap_or("").trim_start();
                let rest = rest.trim_start_matches(':');
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == ':')
                    .collect();
                if name.is_empty() || name.starts_with('_') {
                    pending_doc.clear();
                    continue;
                }
                let desc = if pending_doc.is_empty() {
                    format!("rake {}", name)
                } else {
                    pending_doc.join(" ")
                };
                snippets.push((
                    "rake".to_string(),
                    name.to_string(),
                    format!("rake {}", name),
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
    fn test_rake_detect() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("Rakefile"),
            r#"require 'rake'

desc "Run the test suite"
task :test do
  ruby "-Itest", "test/*.rb"
end

# Build the gem
task :build do
  sh "gem build foo.gemspec"
end

task :default => :test
"#,
        )
        .unwrap();

        let detector = RakeDetector;
        assert!(detector.detect(tmp.path()));

        let snippets = detector.extract(tmp.path());
        let names: Vec<&str> = snippets.iter().map(|s| s.1.as_str()).collect();
        assert!(names.contains(&"test"));
        assert!(names.contains(&"build"));

        let test_desc = snippets.iter().find(|s| s.1 == "test").unwrap();
        assert_eq!(test_desc.3, "Run the test suite");
    }

    #[test]
    fn test_rake_no_file() {
        let tmp = tempfile::tempdir().unwrap();
        let detector = RakeDetector;
        assert!(!detector.detect(tmp.path()));
    }
}
