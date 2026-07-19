use anyhow::Result;

use super::snippet::SnipFile;

/// Validation issue for a single snippet.
#[derive(Debug, Clone)]
pub struct Issue {
    pub key: String,
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Error,
}

/// Validate all snippets in a `SnipFile`.
///
/// Checks for:
/// - Empty commands
/// - Undefined template variables (used in cmd but not in vars)
/// - Unused variable definitions
pub fn validate(file: &SnipFile) -> Vec<Issue> {
    let mut issues = Vec::new();

    for (key, snippet) in file.iter() {
        // Empty command
        if snippet.cmd.trim().is_empty() {
            issues.push(Issue {
                key: key.clone(),
                severity: Severity::Error,
                message: "empty command".to_string(),
            });
        }

        // Check for undefined / unused variables
        let placeholders = snippet.placeholder_names();
        let defined_vars: Vec<String> = snippet.vars.iter().map(|v| v.name.clone()).collect();

        for placeholder in &placeholders {
            if !defined_vars.contains(placeholder) {
                issues.push(Issue {
                    key: key.clone(),
                    severity: Severity::Warning,
                    message: format!("undefined variable: {{{{{}}}}}", placeholder),
                });
            }
        }

        for var in &snippet.vars {
            if !placeholders.contains(&var.name) {
                issues.push(Issue {
                    key: key.clone(),
                    severity: Severity::Warning,
                    message: format!("unused variable definition: {}", var.name),
                });
            }
        }
    }

    issues
}

/// Run validation and print a human-readable report. Returns `Ok(())` if no
/// errors were found, or `Err` with a summary.
pub fn doctor(file: &SnipFile) -> Result<()> {
    let issues = validate(file);
    if issues.is_empty() {
        println!("✓ All {} snippet(s) look good.", file.len());
        return Ok(());
    }

    let errors = issues.iter().filter(|i| i.severity == Severity::Error).count();
    let warnings = issues.iter().filter(|i| i.severity == Severity::Warning).count();

    for issue in &issues {
        let icon = match issue.severity {
            Severity::Error => "✗",
            Severity::Warning => "⚠",
        };
        eprintln!("  {} [{}] {}", icon, issue.key, issue.message);
    }

    eprintln!(
        "\n{} error(s), {} warning(s)",
        errors, warnings
    );

    if errors > 0 {
        anyhow::bail!("validation failed with {} error(s)", errors);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::snippet::{Snippet, VarDef};

    #[test]
    fn empty_command_is_error() {
        let mut file = SnipFile::new();
        file.insert("bad", Snippet::new(""));
        let issues = validate(&file);
        assert!(issues.iter().any(|i| i.severity == Severity::Error));
    }

    #[test]
    fn clean_snippet_passes() {
        let mut file = SnipFile::new();
        file.insert("good", Snippet::new("cargo build"));
        let issues = validate(&file);
        assert!(issues.is_empty());
    }

    #[test]
    fn undefined_variable_warning() {
        let mut file = SnipFile::new();
        file.insert("tpl", Snippet::new("deploy --env {{env}}"));
        let issues = validate(&file);
        assert!(issues.iter().any(|i| i.message.contains("undefined variable")));
    }

    #[test]
    fn defined_variable_matches_placeholder() {
        let mut file = SnipFile::new();
        let var = VarDef::new("env", "Deployment environment");
        file.insert("tpl", Snippet::new("deploy --env {{env}}").with_vars(vec![var]));
        let issues = validate(&file);
        assert!(issues.is_empty());
    }
}