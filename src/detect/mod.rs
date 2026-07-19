pub mod node;
pub mod makefile;
pub mod cargo;
pub mod python;
pub mod docker;

use std::path::Path;

/// A detected snippet: (section, name, command, description)
pub type DetectedSnippet = (String, String, String, String);

/// Trait for project type detectors.
pub trait ProjectDetector {
    /// Human-readable name of the detector (e.g. "Node.js").
    fn name(&self) -> &str;

    /// Check if this project type is detected at the given root.
    fn detect(&self, root: &Path) -> bool;

    /// Extract snippets from the project.
    /// Returns (section, name, cmd, description).
    fn extract(&self, root: &Path) -> Vec<DetectedSnippet>;
}

/// Return all built-in detectors in priority order.
pub fn all_detectors() -> Vec<Box<dyn ProjectDetector>> {
    vec![
        Box::new(node::NodeDetector),
        Box::new(makefile::MakeDetector),
        Box::new(cargo::CargoDetector),
        Box::new(python::PythonDetector),
        Box::new(docker::DockerDetector),
    ]
}

/// Run all detectors on the given root and collect snippets.
/// Returns (source_name, snippet) pairs.
pub fn detect_all(root: &Path) -> Vec<(String, DetectedSnippet)> {
    let mut results = Vec::new();
    for detector in all_detectors() {
        if detector.detect(root) {
            for snippet in detector.extract(root) {
                results.push((detector.name().to_string(), snippet));
            }
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_detectors_return_names() {
        let detectors = all_detectors();
        for d in &detectors {
            assert!(!d.name().is_empty());
        }
    }
}