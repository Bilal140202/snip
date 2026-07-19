use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::snippet::SnipFile;

/// Severity level for a staleness check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StaleSeverity {
    Info,
    Warning,
    Critical,
}

impl StaleSeverity {
    /// Display label for this severity.
    pub fn label(&self) -> &'static str {
        match self {
            StaleSeverity::Critical => "CRITICAL",
            StaleSeverity::Warning => "WARNING",
            StaleSeverity::Info => "INFO",
        }
    }
}

/// A single staleness check result for a snippet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaleCheck {
    /// The fully-qualified key of the affected snippet.
    pub key: String,
    /// Human-readable explanation of why this snippet may be stale.
    pub reason: String,
    /// How severe this issue is.
    pub severity: StaleSeverity,
    /// Suggested fix action (human-readable).
    pub fix_suggestion: String,
}

/// Known deprecated command patterns and their modern alternatives.
struct DeprecatedPattern {
    /// Substring to search for (case-sensitive).
    pattern: &'static str,
    /// Description of the deprecation.
    reason: &'static str,
    /// Suggested modern alternative.
    suggestion: &'static str,
}

const DEPRECATED_PATTERNS: &[DeprecatedPattern] = &[
    DeprecatedPattern {
        pattern: "npm install",
        reason: "uses 'npm install' instead of 'npm ci'",
        suggestion: "Replace 'npm install' with 'npm ci' for reproducible installs",
    },
    DeprecatedPattern {
        pattern: "docker-compose",
        reason: "uses 'docker-compose' (v1) instead of 'docker compose' (v2)",
        suggestion: "Replace 'docker-compose' with 'docker compose'",
    },
    DeprecatedPattern {
        pattern: "git checkout -b",
        reason: "uses 'git checkout -b' instead of 'git switch -c'",
        suggestion: "Replace 'git checkout -b <branch>' with 'git switch -c <branch>'",
    },
];

/// Default shells we consider "standard" (no need for explicit override).
const DEFAULT_SHELLS: &[&str] = &["sh", "bash", "zsh", "fish", "dash"];

/// Run all staleness checks on a [`SnipFile`].
///
/// `snipfile_path` is used for contextual messages but is not read.
pub fn detect_stale(file: &SnipFile, _snipfile_path: &Path) -> Vec<StaleCheck> {
    let mut checks = Vec::new();

    // (a) Binary missing
    // (b) Duplicate commands
    // (c) Empty description
    // (d) No tags
    // (e) Deprecated patterns
    // (f) Shell override unused
    // (g) Very long command
    // (h) Undefined variables

    // --- Pre-pass: collect cmd→keys map for duplicate detection ---
    let mut cmd_to_keys: HashMap<String, Vec<String>> = HashMap::new();
    for (key, snippet) in file.iter() {
        cmd_to_keys
            .entry(snippet.cmd.clone())
            .or_default()
            .push(key.clone());
    }

    // --- Per-snippet checks ---
    for (key, snippet) in file.iter() {
        // (a) Binary missing (Critical)
        if let Some(first_word) = snippet.cmd.split_whitespace().next() {
            if which::which(first_word).is_err() {
                checks.push(StaleCheck {
                    key: key.clone(),
                    reason: format!("binary '{}' not found in PATH", first_word),
                    severity: StaleSeverity::Critical,
                    fix_suggestion: format!(
                        "Remove this snippet or install '{}'",
                        first_word
                    ),
                });
            }
        }

        // (c) Empty description (Info) — only if cmd is short enough to be self-explanatory
        if snippet.desc.trim().is_empty() && snippet.cmd.len() <= 40 {
            checks.push(StaleCheck {
                key: key.clone(),
                reason: "has no description and the command is not self-explanatory".to_string(),
                severity: StaleSeverity::Info,
                fix_suggestion: format!("Add a description, e.g. `snip edit` and set desc for [{}]", key),
            });
        }

        // (d) No tags (Info)
        if snippet.tags.is_empty() {
            checks.push(StaleCheck {
                key: key.clone(),
                reason: "has no tags for categorisation".to_string(),
                severity: StaleSeverity::Info,
                fix_suggestion: format!("Add tags to [{}], e.g. tags = [\"build\"]", key),
            });
        }

        // (e) Deprecated patterns (Warning)
        for dep in DEPRECATED_PATTERNS {
            if snippet.cmd.contains(dep.pattern) {
                checks.push(StaleCheck {
                    key: key.clone(),
                    reason: dep.reason.to_string(),
                    severity: StaleSeverity::Warning,
                    fix_suggestion: dep.suggestion.to_string(),
                });
            }
        }

        // (f) Shell override unused (Info)
        if let Some(ref shell) = snippet.shell {
            if DEFAULT_SHELLS.contains(&shell.as_str()) {
                checks.push(StaleCheck {
                    key: key.clone(),
                    reason: format!(
                        "has shell override '{}' which is likely the default",
                        shell
                    ),
                    severity: StaleSeverity::Info,
                    fix_suggestion: "Remove the 'shell' field if it matches your default shell".to_string(),
                });
            }
        }

        // (g) Very long command (Warning)
        if snippet.cmd.len() > 200 {
            checks.push(StaleCheck {
                key: key.clone(),
                reason: format!(
                    "command is {} chars long (should probably be a script)",
                    snippet.cmd.len()
                ),
                severity: StaleSeverity::Warning,
                fix_suggestion: "Move this command to a script file and reference it from the snippet".to_string(),
            });
        }

        // (h) Undefined variables (Warning) — reuse logic from validator
        let placeholders = snippet.placeholder_names();
        let defined_vars: Vec<String> = snippet.vars.iter().map(|v| v.name.clone()).collect();
        for placeholder in &placeholders {
            if !defined_vars.contains(placeholder) {
                checks.push(StaleCheck {
                    key: key.clone(),
                    reason: format!("undefined variable: {{{{{}}}}}", placeholder),
                    severity: StaleSeverity::Warning,
                    fix_suggestion: format!(
                        "Add a var definition for '{}' in [{}]",
                        placeholder, key
                    ),
                });
            }
        }
    }

    // --- Cross-snippet checks ---

    // (b) Duplicate commands (Warning)
    for (_cmd, keys) in &cmd_to_keys {
        if keys.len() > 1 {
            let key_list = keys.join(", ");
            for key in keys {
                checks.push(StaleCheck {
                    key: key.clone(),
                    reason: format!(
                        "duplicate command: same 'cmd' is used by [{}]",
                        key_list
                    ),
                    severity: StaleSeverity::Warning,
                    fix_suggestion: "Consolidate duplicate snippets or differentiate their commands".to_string(),
                });
            }
        }
    }

    // Sort: Critical first, then Warning, then Info; within each group, by key.
    checks.sort_by(|a, b| {
        let ord = severity_order(&b.severity).cmp(&severity_order(&a.severity));
        if ord == std::cmp::Ordering::Equal {
            a.key.cmp(&b.key)
        } else {
            ord
        }
    });

    checks
}

/// Numeric priority for sorting (higher = more severe).
fn severity_order(s: &StaleSeverity) -> u8 {
    match s {
        StaleSeverity::Critical => 3,
        StaleSeverity::Warning => 2,
        StaleSeverity::Info => 1,
    }
}

/// Format a list of staleness checks into a human-readable report string.
pub fn format_stale_report(checks: &[StaleCheck]) -> String {
    use colored::Colorize;

    if checks.is_empty() {
        return "✓ No staleness issues found.".green().to_string();
    }

    let critical: Vec<_> = checks.iter().filter(|c| c.severity == StaleSeverity::Critical).collect();
    let warnings: Vec<_> = checks.iter().filter(|c| c.severity == StaleSeverity::Warning).collect();
    let infos: Vec<_> = checks.iter().filter(|c| c.severity == StaleSeverity::Info).collect();

    let mut out = String::new();

    if !critical.is_empty() {
        out.push_str(&format!("\n{}\n", "CRITICAL".red().bold()));
        for check in &critical {
            out.push_str(&format!(
                "  {} [{}] {}\n",
                "✗".red(),
                check.key.cyan(),
                check.reason
            ));
            out.push_str(&format!(
                "    {} {}\n",
                "fix:".dimmed(),
                check.fix_suggestion.dimmed()
            ));
        }
    }

    if !warnings.is_empty() {
        out.push_str(&format!("\n{}\n", "WARNINGS".yellow().bold()));
        for check in &warnings {
            out.push_str(&format!(
                "  {} [{}] {}\n",
                "⚠".yellow(),
                check.key.cyan(),
                check.reason
            ));
            out.push_str(&format!(
                "    {} {}\n",
                "fix:".dimmed(),
                check.fix_suggestion.dimmed()
            ));
        }
    }

    if !infos.is_empty() {
        out.push_str(&format!("\n{}\n", "INFO".blue().bold()));
        for check in &infos {
            out.push_str(&format!(
                "  {} [{}] {}\n",
                "ℹ".blue(),
                check.key.cyan(),
                check.reason
            ));
            out.push_str(&format!(
                "    {} {}\n",
                "fix:".dimmed(),
                check.fix_suggestion.dimmed()
            ));
        }
    }

    // Summary
    out.push_str(&format!(
        "\n{} critical, {} warning(s), {} info\n",
        critical.len().to_string().red().bold(),
        warnings.len().to_string().yellow().bold(),
        infos.len().to_string().blue().bold(),
    ));

    out
}

/// Attempt to auto-fix a single staleness check in-place.
///
/// Returns `Ok(true)` if a fix was applied, `Ok(false)` if the check
/// could not be auto-fixed (no-op).
pub fn fix_stale(check: &StaleCheck, file: &mut SnipFile) -> anyhow::Result<bool> {
    let snippet = match file.get(&check.key) {
        Some(s) => s.clone(),
        None => return Ok(false),
    };

    // Fixable cases:

    // (a) Binary missing → remove the snippet
    if check.severity == StaleSeverity::Critical && check.reason.contains("not found in PATH") {
        file.remove(&check.key);
        return Ok(true);
    }

    // (c) Empty description → generate a default one from cmd
    if check.reason.contains("no description") {
        let mut fixed = snippet;
        // Use the first ~60 chars of the command as a description
        let default_desc = if fixed.cmd.len() > 60 {
            format!("{}...", &fixed.cmd[..57])
        } else {
            fixed.cmd.clone()
        };
        fixed.desc = default_desc;
        file.insert(check.key.clone(), fixed);
        return Ok(true);
    }

    // (f) Shell override unused → remove the shell field
    if check.reason.contains("shell override") {
        let mut fixed = snippet;
        fixed.shell = None;
        file.insert(check.key.clone(), fixed);
        return Ok(true);
    }

    // Non-fixable: deprecated patterns, duplicate commands, no tags, long cmd, undefined vars
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::snippet::Snippet;

    #[test]
    fn detect_stale_empty_file() {
        let file = SnipFile::new();
        let path = Path::new("/tmp/.snips");
        let checks = detect_stale(&file, path);
        assert!(checks.is_empty());
    }

    #[test]
    fn detect_stale_no_tags() {
        let mut file = SnipFile::new();
        file.insert(
            "build",
            Snippet::new("cargo build").with_desc("Build the project"),
        );
        let path = Path::new("/tmp/.snips");
        let checks = detect_stale(&file, path);
        // Should have "no tags" info check (cargo binary exists, so no critical)
        assert!(checks.iter().any(|c| c.reason.contains("no tags")));
    }

    #[test]
    fn detect_stale_missing_binary() {
        let mut file = SnipFile::new();
        file.insert(
            "deploy",
            Snippet::new("nonexistent_xyz_binary_123 --flag").with_desc("Deploy"),
        );
        let path = Path::new("/tmp/.snips");
        let checks = detect_stale(&file, path);
        assert!(checks.iter().any(|c| {
            c.severity == StaleSeverity::Critical && c.reason.contains("not found in PATH")
        }));
    }

    #[test]
    fn detect_stale_duplicate_commands() {
        let mut file = SnipFile::new();
        file.insert(
            "build",
            Snippet::new("cargo build").with_desc("Build the project"),
        );
        file.insert(
            "compile",
            Snippet::new("cargo build").with_desc("Compile project"),
        );
        let path = Path::new("/tmp/.snips");
        let checks = detect_stale(&file, path);
        assert!(checks.iter().any(|c| c.reason.contains("duplicate command")));
    }

    #[test]
    fn detect_stale_empty_description() {
        let mut file = SnipFile::new();
        file.insert("short", Snippet::new("echo hi").with_tags(vec!["test".to_string()]));
        let path = Path::new("/tmp/.snips");
        let checks = detect_stale(&file, path);
        assert!(checks
            .iter()
            .any(|c| c.reason.contains("no description")));
    }

    #[test]
    fn detect_stale_no_empty_desc_for_long_cmd() {
        let mut file = SnipFile::new();
        // 41+ char command should not trigger empty-desc check
        let long_cmd = "echo this is a very long command that is clearly self-explanatory enough";
        file.insert("verbose", Snippet::new(long_cmd).with_tags(vec!["test".to_string()]));
        let path = Path::new("/tmp/.snips");
        let checks = detect_stale(&file, path);
        assert!(!checks.iter().any(|c| c.reason.contains("no description")));
    }

    #[test]
    fn detect_stale_deprecated_npm_install() {
        let mut file = SnipFile::new();
        file.insert(
            "install",
            Snippet::new("npm install").with_desc("Install deps").with_tags(vec!["npm".to_string()]),
        );
        let path = Path::new("/tmp/.snips");
        let checks = detect_stale(&file, path);
        assert!(checks
            .iter()
            .any(|c| c.reason.contains("npm install")));
    }

    #[test]
    fn detect_stale_deprecated_docker_compose() {
        let mut file = SnipFile::new();
        file.insert(
            "up",
            Snippet::new("docker-compose up -d")
                .with_desc("Start containers")
                .with_tags(vec!["docker".to_string()]),
        );
        let path = Path::new("/tmp/.snips");
        let checks = detect_stale(&file, path);
        assert!(checks
            .iter()
            .any(|c| c.reason.contains("docker-compose")));
    }

    #[test]
    fn detect_stale_deprecated_git_checkout() {
        let mut file = SnipFile::new();
        file.insert(
            "branch",
            Snippet::new("git checkout -b feature/foo")
                .with_desc("New branch")
                .with_tags(vec!["git".to_string()]),
        );
        let path = Path::new("/tmp/.snips");
        let checks = detect_stale(&file, path);
        assert!(checks
            .iter()
            .any(|c| c.reason.contains("git checkout -b")));
    }

    #[test]
    fn detect_stale_shell_override_unused() {
        let mut file = SnipFile::new();
        file.insert(
            "hello",
            Snippet::new("echo hello")
                .with_desc("Say hello")
                .with_shell("bash")
                .with_tags(vec!["test".to_string()]),
        );
        let path = Path::new("/tmp/.snips");
        let checks = detect_stale(&file, path);
        assert!(checks
            .iter()
            .any(|c| c.reason.contains("shell override")));
    }

    #[test]
    fn detect_stale_very_long_command() {
        let mut file = SnipFile::new();
        let long_cmd = "echo ".to_string() + &"a".repeat(200);
        file.insert(
            "long",
            Snippet::new(&long_cmd)
                .with_desc("A long one")
                .with_tags(vec!["test".to_string()]),
        );
        let path = Path::new("/tmp/.snips");
        let checks = detect_stale(&file, path);
        assert!(checks
            .iter()
            .any(|c| c.reason.contains("chars long")));
    }

    #[test]
    fn detect_stale_undefined_variables() {
        let mut file = SnipFile::new();
        file.insert(
            "deploy",
            Snippet::new("deploy --env {{env}}")
                .with_desc("Deploy")
                .with_tags(vec!["deploy".to_string()]),
        );
        let path = Path::new("/tmp/.snips");
        let checks = detect_stale(&file, path);
        assert!(checks
            .iter()
            .any(|c| c.reason.contains("undefined variable")));
    }

    #[test]
    fn fix_stale_removes_missing_binary() {
        let mut file = SnipFile::new();
        file.insert(
            "bad",
            Snippet::new("nonexistent_xyz_binary_123 --flag").with_desc("Broken"),
        );
        let check = StaleCheck {
            key: "bad".to_string(),
            reason: "binary 'nonexistent_xyz_binary_123' not found in PATH".to_string(),
            severity: StaleSeverity::Critical,
            fix_suggestion: "Remove this snippet or install 'nonexistent_xyz_binary_123'".to_string(),
        };
        let fixed = fix_stale(&check, &mut file).unwrap();
        assert!(fixed);
        assert!(file.get("bad").is_none());
    }

    #[test]
    fn fix_stale_adds_description() {
        let mut file = SnipFile::new();
        file.insert("hi", Snippet::new("echo hello world"));
        let check = StaleCheck {
            key: "hi".to_string(),
            reason: "has no description and the command is not self-explanatory".to_string(),
            severity: StaleSeverity::Info,
            fix_suggestion: "Add a description".to_string(),
        };
        let fixed = fix_stale(&check, &mut file).unwrap();
        assert!(fixed);
        let s = file.get("hi").unwrap();
        assert_eq!(s.desc, "echo hello world");
    }

    #[test]
    fn fix_stale_removes_shell_override() {
        let mut file = SnipFile::new();
        file.insert(
            "hello",
            Snippet::new("echo hello")
                .with_shell("bash"),
        );
        let check = StaleCheck {
            key: "hello".to_string(),
            reason: "has shell override 'bash' which is likely the default".to_string(),
            severity: StaleSeverity::Info,
            fix_suggestion: "Remove the 'shell' field".to_string(),
        };
        let fixed = fix_stale(&check, &mut file).unwrap();
        assert!(fixed);
        assert!(file.get("hello").unwrap().shell.is_none());
    }

    #[test]
    fn fix_stale_noop_for_deprecated() {
        let mut file = SnipFile::new();
        file.insert(
            "install",
            Snippet::new("npm install").with_desc("Install"),
        );
        let check = StaleCheck {
            key: "install".to_string(),
            reason: "uses 'npm install' instead of 'npm ci'".to_string(),
            severity: StaleSeverity::Warning,
            fix_suggestion: "Replace 'npm install' with 'npm ci'".to_string(),
        };
        let fixed = fix_stale(&check, &mut file).unwrap();
        assert!(!fixed);
    }

    #[test]
    fn format_stale_report_empty() {
        let report = format_stale_report(&[]);
        assert!(report.contains("No staleness issues found"));
    }

    #[test]
    fn format_stale_report_has_summary() {
        let checks = vec![
            StaleCheck {
                key: "a".to_string(),
                reason: "test critical".to_string(),
                severity: StaleSeverity::Critical,
                fix_suggestion: "fix it".to_string(),
            },
            StaleCheck {
                key: "b".to_string(),
                reason: "test warning".to_string(),
                severity: StaleSeverity::Warning,
                fix_suggestion: "fix it".to_string(),
            },
            StaleCheck {
                key: "c".to_string(),
                reason: "test info".to_string(),
                severity: StaleSeverity::Info,
                fix_suggestion: "fix it".to_string(),
            },
        ];
        let report = format_stale_report(&checks);
        assert!(report.contains("1 critical"));
        assert!(report.contains("1 warning"));
        assert!(report.contains("1 info"));
    }

    #[test]
    fn detect_stale_sort_order() {
        let mut file = SnipFile::new();
        file.insert(
            "info_snip",
            Snippet::new("echo hi").with_desc("Say hi").with_tags(vec!["test".to_string()]),
        );
        file.insert(
            "critical_snip",
            Snippet::new("nonexistent_xyz_binary_123").with_desc("Broken"),
        );
        let path = Path::new("/tmp/.snips");
        let checks = detect_stale(&file, path);
        // Critical should come before Info
        let first_severity = &checks[0].severity;
        let last_severity = &checks[checks.len() - 1].severity;
        assert!(severity_order(first_severity) >= severity_order(last_severity));
    }
}