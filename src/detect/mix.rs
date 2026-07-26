use std::fs;
use std::path::Path;

use super::{DetectedSnippet, ProjectDetector};

/// Detects Elixir Mix projects by looking for `mix.exs`.
pub struct MixDetector;

impl ProjectDetector for MixDetector {
    fn name(&self) -> &str {
        "Mix"
    }

    fn detect(&self, root: &Path) -> bool {
        root.join("mix.exs").exists()
    }

    fn extract(&self, root: &Path) -> Vec<DetectedSnippet> {
        let _ = fs::read_to_string(root.join("mix.exs"));

        // Common mix commands every Elixir project uses.
        let common = [
            ("compile", "Compile the project", "mix compile"),
            ("test", "Run tests", "mix test"),
            ("fmt", "Format code", "mix format"),
            ("deps.get", "Install dependencies", "mix deps.get"),
            ("deps.clean", "Clean dependencies", "mix deps.clean"),
            ("phx.server", "Start Phoenix server", "mix phx.server"),
            ("ecto.migrate", "Run DB migrations", "mix ecto.migrate"),
            ("release", "Build a release", "mix release"),
        ];
        common
            .iter()
            .map(|(name, desc, cmd)| {
                (
                    "mix".to_string(),
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
    fn test_mix_detect() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("mix.exs"),
            "defmodule Foo.MixProject do\n  use Mix.Project\nend\n",
        )
        .unwrap();

        let detector = MixDetector;
        assert!(detector.detect(tmp.path()));

        let snippets = detector.extract(tmp.path());
        assert!(snippets.len() >= 5);
        let names: Vec<&str> = snippets.iter().map(|s| s.1.as_str()).collect();
        assert!(names.contains(&"compile"));
        assert!(names.contains(&"test"));
    }

    #[test]
    fn test_mix_no_file() {
        let tmp = tempfile::tempdir().unwrap();
        let detector = MixDetector;
        assert!(!detector.detect(tmp.path()));
    }
}
