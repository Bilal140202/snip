//! `snip doctor` — Validate all snippet commands, with optional auto-fix.

use std::path::Path;

use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;

use crate::core::snipfile::{find_snipfile, read_snippets, write_snippets};
use crate::core::validator;

/// Validate snippets and report issues.
#[derive(Debug, Args)]
pub struct DoctorCmd {
    /// Automatically fix fixable issues.
    #[arg(long)]
    pub fix: bool,
}

impl DoctorCmd {
    pub fn run(&self) -> Result<()> {
        let cwd = std::env::current_dir().context("Failed to determine current directory")?;
        run_at(&cwd, self.fix)
    }
}

/// Internal doctor function that accepts a root path.
pub(crate) fn run_at(root: &Path, auto_fix: bool) -> Result<()> {
    let snipfile_path = match find_snipfile(Some(root))? {
        Some(p) => p,
        None => {
            println!(
                "{}",
                "No .snips file found. Run `snip init` first.".dimmed()
            );
            return Ok(());
        }
    };

    let mut file = read_snippets(&snipfile_path)?;

    if file.is_empty() {
        println!("{}", "No snippets to check.".dimmed());
        return Ok(());
    }

    // First run core validation (empty cmds, undefined vars, etc.)
    let issues = validator::validate(&file);

    // Then check if binaries exist
    let mut valid_count = 0;
    let mut broken_count = 0;
    let mut missing_binaries: Vec<(String, String)> = Vec::new(); // (key, binary)

    for (key, snippet) in file.iter() {
        let first_word = snippet.cmd.split_whitespace().next().unwrap_or("");
        let binary = first_word.to_string();

        // Skip special shells and builtins
        if [
            "sh", "bash", "zsh", "fish", "true", "false", "echo", "cd", "test",
        ]
        .contains(&binary.as_str())
        {
            println!("{} {} — {}", "✓".green(), key.cyan(), snippet.cmd.dimmed());
            valid_count += 1;
            continue;
        }

        let exists = which::which(&binary).is_ok();

        if exists {
            println!("{} {} — {}", "✓".green(), key.cyan(), snippet.cmd.dimmed());
            valid_count += 1;
        } else {
            println!(
                "{} {} — {} {}",
                "✗".red(),
                key.cyan(),
                snippet.cmd.dimmed(),
                format!("(binary '{}' not found)", binary).yellow().dimmed()
            );
            broken_count += 1;
            missing_binaries.push((key.clone(), binary));
        }
    }

    // Show core validation issues
    if !issues.is_empty() {
        println!();
        println!("{}", "Additional issues:".dimmed());
        for issue in &issues {
            let icon = match issue.severity {
                validator::Severity::Error => "✗".red().to_string(),
                validator::Severity::Warning => "⚠".yellow().to_string(),
            };
            println!("  {} [{}] {}", icon, issue.key, issue.message);
        }
    }

    // Auto-fix mode
    let mut fixed_count = 0;
    if auto_fix {
        let mut fixes_applied = Vec::new();

        // Fix: remove snippets with missing binaries
        for (key, _binary) in &missing_binaries {
            file.remove(key);
            fixes_applied.push(format!("Removed '{}' (missing binary)", key));
            fixed_count += 1;
        }

        // Fix: add empty var definitions for undefined variables
        for issue in &issues {
            if issue.message.contains("undefined variable") {
                // Extract var name from message like "undefined variable: {{env}}"
                if let Some(start) = issue.message.find("{{") {
                    if let Some(end) = issue.message.find("}}") {
                        let var_name = issue.message[start + 2..end].trim();
                        if let Some(snippet) = file.get_mut(&issue.key) {
                            use crate::core::snippet::VarDef;
                            let var_def = VarDef::new(var_name, format!("{} variable", var_name));
                            snippet.vars.push(var_def);
                            fixes_applied.push(format!(
                                "Added var definition '{}' to '{}'",
                                var_name, issue.key
                            ));
                            fixed_count += 1;
                        }
                    }
                }
            }
        }

        // Fix: remove unused variable definitions
        for issue in &issues.clone() {
            if issue.message.contains("unused variable definition") {
                // Extract var name
                if let Some(snippet) = file.get_mut(&issue.key) {
                    let var_name = issue
                        .message
                        .strip_prefix("unused variable definition: ")
                        .unwrap_or("");
                    snippet.vars.retain(|v| v.name != var_name);
                    fixes_applied.push(format!(
                        "Removed unused var '{}' from '{}'",
                        var_name, issue.key
                    ));
                    fixed_count += 1;
                }
            }
        }

        if !fixes_applied.is_empty() {
            println!();
            println!("{}", "Auto-fixes applied:".green().bold());
            for fix in &fixes_applied {
                println!("  {} {}", "✓".green(), fix);
            }

            // Write the fixed file
            write_snippets(&snipfile_path, &file)?;
            println!();
            println!("{}", "  .snips file updated.".dimmed());
            println!("  {} fix(es) applied.", fixed_count);
        }
    }

    // ── Project environment checks ───────────────────────────────────────
    // Run after snippet validation so env issues appear in their own section.
    let env_issues = run_env_checks(&file, root);
    if !env_issues.is_empty() {
        println!();
        println!("{}", "Project environment:".bold());
        for (severity, msg) in &env_issues {
            let icon = match severity.as_str() {
                "warning" => "⚠".yellow().to_string(),
                "info" => "ℹ".cyan().to_string(),
                _ => "•".dimmed().to_string(),
            };
            println!("  {} {}", icon, msg);
        }
    }

    println!();
    if broken_count == 0 && issues.is_empty() && env_issues.is_empty() {
        println!("{} All {} snippet(s) are valid.", "✓".green(), valid_count);
    } else {
        let fix_hint = if auto_fix {
            String::new()
        } else {
            format!(" Run {} to auto-fix.", "snip doctor --fix".cyan())
        };
        println!(
            "{} {} valid, {} broken, {} env issue(s){}",
            "!".yellow(),
            valid_count,
            broken_count + issues.len(),
            env_issues.len(),
            fix_hint
        );
    }

    Ok(())
}

/// Extract environment variable references ($VAR or ${VAR}) from a command string.
fn extract_env_vars(cmd: &str) -> Vec<String> {
    let mut vars = Vec::new();
    let bytes = cmd.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            // ${VAR} form
            if i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                if let Some(end) = cmd[i + 2..].find('}') {
                    let name = &cmd[i + 2..i + 2 + end];
                    if !name.is_empty() && !vars.contains(&name.to_string()) {
                        vars.push(name.to_string());
                    }
                    i = i + 2 + end + 1;
                    continue;
                }
            }
            // $VAR form (alphanumeric + underscore)
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                end += 1;
            }
            if end > start {
                let name = &cmd[start..end];
                if !vars.contains(&name.to_string()) {
                    vars.push(name.to_string());
                }
                i = end;
                continue;
            }
        }
        i += 1;
    }
    vars
}

/// Run project-environment checks: env var presence, Docker daemon,
/// `.env` file. Returns a list of (severity, message) issues.
fn run_env_checks(file: &crate::core::snippet::SnipFile, root: &Path) -> Vec<(String, String)> {
    let mut issues: Vec<(String, String)> = Vec::new();

    // ── Check 1: env vars referenced in snippets but not set in the environment
    let mut referenced_vars: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut uses_docker = false;
    for (_, snippet) in file.iter() {
        for v in extract_env_vars(&snippet.cmd) {
            referenced_vars.insert(v);
        }
        if snippet.cmd.contains("docker compose") || snippet.cmd.starts_with("docker ") {
            uses_docker = true;
        }
    }

    // Filter out common shell-interal vars and CI-provided vars that are
    // always set; we don't want to flag those.
    let always_set: &[&str] = &[
        "PATH", "HOME", "USER", "SHELL", "LANG", "LC_ALL", "LC_CTYPE", "PWD", "OLDPWD", "TERM",
        "SHLVL", "_", "TMPDIR",
    ];

    let mut missing_vars: Vec<String> = Vec::new();
    for var in &referenced_vars {
        if always_set.contains(&var.as_str()) {
            continue;
        }
        if std::env::var_os(var).is_none() {
            missing_vars.push(var.clone());
        }
    }

    if !missing_vars.is_empty() {
        issues.push((
            "warning".to_string(),
            format!(
                "Snippets reference env var(s) not set in your shell: {}",
                missing_vars
                    .iter()
                    .map(|v| format!("${}", v))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));

        // Hint about .env file if missing vars exist but no .env present
        let env_path = root.join(".env");
        if !env_path.exists() {
            issues.push((
                "info".to_string(),
                format!(
                    "No {} file found — consider creating one with: {}",
                    ".env".cyan(),
                    missing_vars
                        .iter()
                        .map(|v| format!("{}=...", v))
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
            ));
        }
    }

    // ── Check 2: Docker daemon running (only if any snippet uses docker)
    if uses_docker {
        let docker_running = check_docker_running();
        if let Some(reason) = docker_running.error() {
            issues.push((
                "warning".to_string(),
                format!("Snippets use Docker but {} — {}", "docker".cyan(), reason),
            ));
        }
    }

    issues
}

/// Tiny wrapper around `docker info` to detect if the daemon is running.
struct DockerStatus {
    #[allow(dead_code)]
    ok: bool,
    err: Option<String>,
}

impl DockerStatus {
    fn error(&self) -> Option<&str> {
        self.err.as_deref()
    }
}

fn check_docker_running() -> DockerStatus {
    // First check if `docker` binary exists
    if which::which("docker").is_err() {
        return DockerStatus {
            ok: false,
            err: Some("docker binary not installed".to_string()),
        };
    }
    // Then check if the daemon responds
    let out = std::process::Command::new("docker")
        .args(["info", "--format", "{{.ServerVersion}}"])
        .output();
    match out {
        Ok(output) => {
            if output.status.success() {
                DockerStatus {
                    ok: true,
                    err: None,
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let reason = if stderr.contains("Cannot connect to the Docker daemon") {
                    "daemon not running (start with `dockerd` or open Docker Desktop)".to_string()
                } else if stderr.contains("permission denied") {
                    "permission denied (add your user to the `docker` group and re-login)"
                        .to_string()
                } else {
                    format!("`docker info` failed: {}", stderr.trim())
                };
                DockerStatus {
                    ok: false,
                    err: Some(reason),
                }
            }
        }
        Err(e) => DockerStatus {
            ok: false,
            err: Some(format!("failed to spawn `docker info`: {}", e)),
        },
    }
}

#[cfg(test)]
mod tests {
    use crate::core::snippet::Snippet;

    #[test]
    fn test_doctor_valid() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");

        let mut file = crate::core::snippet::SnipFile::new();
        file.insert("hello", Snippet::new("echo hello").with_desc("Say hello"));
        crate::core::snipfile::write_snippets(&snipfile, &file).unwrap();

        super::run_at(tmp.path(), false).unwrap();
    }

    #[test]
    fn test_doctor_broken() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");

        let mut file = crate::core::snippet::SnipFile::new();
        file.insert(
            "bad",
            Snippet::new("nonexistent_binary_xyz_123 --flag").with_desc("A broken command"),
        );
        crate::core::snipfile::write_snippets(&snipfile, &file).unwrap();

        super::run_at(tmp.path(), false).unwrap();
    }

    #[test]
    fn test_doctor_no_snips() {
        let tmp = tempfile::tempdir().unwrap();
        super::run_at(tmp.path(), false).unwrap();
    }

    #[test]
    fn test_doctor_fix_removes_missing_binaries() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");

        let mut file = crate::core::snippet::SnipFile::new();
        file.insert("good", Snippet::new("echo hello"));
        file.insert("bad", Snippet::new("nonexistent_xyz_123_command"));
        crate::core::snipfile::write_snippets(&snipfile, &file).unwrap();

        super::run_at(tmp.path(), true).unwrap();

        let file_after = crate::core::snipfile::read_snippets(&snipfile).unwrap();
        assert!(file_after.get("good").is_some());
        assert!(file_after.get("bad").is_none());
    }
}
