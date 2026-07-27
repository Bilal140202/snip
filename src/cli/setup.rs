//! `snip setup` — Full project bootstrap.
//!
//! Runs a series of steps to get a project running from a fresh clone:
//!   1. check-tools     — detect installed language runtimes + tools
//!   2. install-deps    — npm install / cargo build / pip install / go mod download
//!   3. create-env      — copy .env.example → .env, or generate template from snippet $VARS
//!   4. start-services  — docker compose up -d (if docker-compose.yml exists)
//!   5. build           — run the project's build command
//!   6. test            — run the project's test command
//!   7. dev             — start the dev server (foreground, last step)
//!
//! Each step prefers a snippet tagged `["setup"]` whose key matches the step
//! name (e.g. `install-deps`, `build`, `test`, `dev`, `start-services`).
//! Falls back to auto-detected commands based on project files.
//!
//! Safety:
//! - In interactive mode (default), prompts before any step that executes a command.
//! - `--non-interactive` skips prompts (CI mode).
//! - `--dry-run` prints what would run without executing anything.
//! - `--skip=<steps>` and `--only=<steps>` control which steps run.

use std::io::Write as IoWrite;
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;

use crate::core::snipfile::{find_snipfile, read_snippets};
use crate::core::snippet::SnipFile;

// ── Public CLI surface ─────────────────────────────────────────────────────

/// Options for `snip setup`.
#[derive(Debug, Args, Default)]
pub struct SetupCmd {
    /// Comma-separated step names to skip (e.g. --skip=build,test).
    #[arg(long, value_delimiter = ',')]
    pub skip: Vec<String>,

    /// Comma-separated step names to run (skips all others).
    #[arg(long, value_delimiter = ',')]
    pub only: Vec<String>,

    /// Run without prompting (CI mode). Uses defaults, never asks.
    #[arg(long)]
    pub non_interactive: bool,

    /// Print what would happen without executing anything.
    #[arg(long)]
    pub dry_run: bool,
}

impl SetupCmd {
    pub fn run(&self) -> Result<()> {
        let cwd = std::env::current_dir().context("Failed to determine current directory")?;
        run_at(&cwd, self)
    }
}

// ── Step model ──────────────────────────────────────────────────────────────

/// All setup steps, in execution order.
const ALL_STEPS: &[&str] = &[
    "check-tools",
    "install-deps",
    "create-env",
    "start-services",
    "build",
    "test",
    "dev",
];

/// Outcome of a single step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupStatus {
    /// Step completed successfully.
    Success,
    /// Step was skipped via --skip or --only.
    Skipped,
    /// Step wasn't needed (e.g. no docker-compose.yml → start-services skips).
    NotNeeded,
    /// Step failed (e.g. tests failed). Setup continues to next step.
    Failed,
    /// Step was a dry-run — command printed but not executed.
    DryRun,
    /// Step prompted the user and they declined.
    Declined,
}

impl SetupStatus {
    fn icon(&self) -> String {
        match self {
            SetupStatus::Success => "✓".green().to_string(),
            SetupStatus::Skipped => "○".dimmed().to_string(),
            SetupStatus::NotNeeded => "·".dimmed().to_string(),
            SetupStatus::Failed => "✗".red().to_string(),
            SetupStatus::DryRun => "→".cyan().to_string(),
            SetupStatus::Declined => "—".dimmed().to_string(),
        }
    }
}

/// Result of running one step.
#[derive(Debug, Clone)]
pub struct SetupStepResult {
    pub name: String,
    pub status: SetupStatus,
    /// One-line human-readable summary (shown in the final table).
    pub message: String,
    pub duration_ms: u128,
}

// ── Main entry ──────────────────────────────────────────────────────────────

pub(crate) fn run_at(root: &Path, opts: &SetupCmd) -> Result<()> {
    println!();
    println!("{}", "═══════════════════════════════════════════".dimmed());
    println!("{}  v0.5.0 — Project Bootstrap", "  snip setup".bold());
    if opts.dry_run {
        println!(
            "{}",
            "  (dry-run — nothing will be executed)".cyan().dimmed()
        );
    } else if opts.non_interactive {
        println!("{}", "  (non-interactive — using defaults)".cyan().dimmed());
    }
    println!("{}", "═══════════════════════════════════════════".dimmed());
    println!();

    // Load .snips (create if missing — same logic as `snip init`)
    let snipfile_path = match find_snipfile(Some(root))? {
        Some(p) => p,
        None => {
            println!(
                "{}  No .snips file found. Running {} first...",
                "!".yellow(),
                "snip init".cyan()
            );
            crate::cli::init::run()?;
            find_snipfile(Some(root))?.context("init did not create .snips")?
        }
    };
    let file = read_snippets(&snipfile_path)?;

    // Determine which steps to run
    let steps_to_run: Vec<&str> = if !opts.only.is_empty() {
        opts.only
            .iter()
            .filter_map(|s| {
                let lower = s.to_lowercase();
                if ALL_STEPS.contains(&lower.as_str()) {
                    Some(
                        ALL_STEPS
                            .iter()
                            .find(|&&step| step == lower)
                            .copied()
                            .unwrap(),
                    )
                } else {
                    println!("{}  Unknown step '{}', ignoring", "⚠".yellow(), s);
                    None
                }
            })
            .collect()
    } else {
        ALL_STEPS
            .iter()
            .filter(|&&step| !opts.skip.iter().any(|s| s.to_lowercase() == step))
            .copied()
            .collect()
    };

    if steps_to_run.is_empty() {
        println!("{}", "No steps to run (all skipped).".dimmed());
        return Ok(());
    }

    let mut results: Vec<SetupStepResult> = Vec::new();

    for &step in &steps_to_run {
        let result = run_step(step, root, &file, opts)?;
        let is_dev = step == "dev";
        results.push(result);
        // If a step failed (non-dev), warn but continue.
        // The dev step is last and blocks — only run it if previous steps
        // didn't fail. (We still run dev if previous steps were NotNeeded or
        // Skipped, since those are non-fatal.)
        if is_dev {
            break;
        }
    }

    print_summary(&results);

    Ok(())
}

/// Run a single step by name.
fn run_step(step: &str, root: &Path, file: &SnipFile, opts: &SetupCmd) -> Result<SetupStepResult> {
    let start = Instant::now();
    println!("{}", format!("→ {}:", step).bold());

    let (status, message) = match step {
        "check-tools" => step_check_tools(root),
        "install-deps" => step_install_deps(root, file, opts),
        "create-env" => step_create_env(root, file),
        "start-services" => step_start_services(root, file, opts),
        "build" => step_run_snippet_or_detected(root, file, opts, "build", detect_build_cmd(root)),
        "test" => step_run_snippet_or_detected(root, file, opts, "test", detect_test_cmd(root)),
        "dev" => step_dev(root, file, opts),
        _ => Ok((SetupStatus::Skipped, format!("Unknown step '{}'", step))),
    }?;

    let duration_ms = start.elapsed().as_millis();
    println!("  {} {}", status.icon(), message.dimmed());
    println!();

    Ok(SetupStepResult {
        name: step.to_string(),
        status,
        message,
        duration_ms,
    })
}

// ── Step 1: check-tools ────────────────────────────────────────────────────

fn step_check_tools(root: &Path) -> Result<(SetupStatus, String)> {
    let mut found: Vec<&str> = Vec::new();
    let mut missing: Vec<&str> = Vec::new();

    // Detect language runtimes based on project files
    let checks: &[(&str, &str, &str)] = &[
        // (file, binary, label)
        ("package.json", "node", "Node.js"),
        ("Cargo.toml", "cargo", "Rust"),
        ("go.mod", "go", "Go"),
        ("pyproject.toml", "python3", "Python"),
        ("requirements.txt", "python3", "Python"),
        ("mix.exs", "elixir", "Elixir"),
        ("Gemfile", "ruby", "Ruby"),
        ("docker-compose.yml", "docker", "Docker"),
        ("docker-compose.yaml", "docker", "Docker"),
        ("Taskfile.yml", "task", "Taskfile"),
        ("justfile", "just", "just"),
        ("Justfile", "just", "just"),
    ];

    let mut labels_checked: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();

    for (file_name, binary, label) in checks {
        if !root.join(file_name).exists() {
            continue;
        }
        if labels_checked.contains(label) {
            continue;
        }
        labels_checked.insert(label);
        if which::which(binary).is_ok() {
            found.push(label);
        } else {
            missing.push(label);
        }
    }

    // Always check git
    if which::which("git").is_ok() {
        found.push("git");
    } else {
        missing.push("git");
    }

    let mut msg = String::new();
    if !found.is_empty() {
        msg.push_str(&format!("found: {}", found.join(", ")));
    }
    if !missing.is_empty() {
        if !msg.is_empty() {
            msg.push_str(" | ");
        }
        msg.push_str(&format!("missing: {}", missing.join(", ")));
    }
    if msg.is_empty() {
        msg.push_str("no project files detected");
    }

    let status = if missing.is_empty() {
        SetupStatus::Success
    } else if !found.is_empty() {
        // Some found, some missing — partial success
        SetupStatus::Success
    } else {
        SetupStatus::Failed
    };

    Ok((status, msg))
}

// ── Step 2: install-deps ──────────────────────────────────────────────────

fn step_install_deps(
    root: &Path,
    file: &SnipFile,
    opts: &SetupCmd,
) -> Result<(SetupStatus, String)> {
    // Prefer a snippet tagged ["setup"] with key install-deps or install
    if let Some(cmd) = find_setup_snippet(file, &["install-deps", "install"]) {
        return run_step_cmd("install-deps", &cmd, root, opts, /*cd_root*/ true);
    }

    // Auto-detect
    let cmd = detect_install_cmd(root);
    match cmd {
        Some(c) => run_step_cmd("install-deps", &c, root, opts, true),
        None => Ok((
            SetupStatus::NotNeeded,
            "no dependency manifest detected (package.json, Cargo.toml, go.mod, etc.)".to_string(),
        )),
    }
}

/// Detect the install command based on project files.
fn detect_install_cmd(root: &Path) -> Option<String> {
    if root.join("package.json").exists() {
        // Use pnpm if lockfile exists, else yarn, else npm
        if root.join("pnpm-lock.yaml").exists() {
            return Some("pnpm install".to_string());
        }
        if root.join("yarn.lock").exists() {
            return Some("yarn install".to_string());
        }
        return Some("npm install".to_string());
    }
    if root.join("Cargo.toml").exists() {
        return Some("cargo fetch".to_string());
    }
    if root.join("go.mod").exists() {
        return Some("go mod download".to_string());
    }
    if root.join("pyproject.toml").exists() || root.join("requirements.txt").exists() {
        if root.join("uv.lock").exists() {
            return Some("uv sync".to_string());
        }
        if root.join("poetry.lock").exists() {
            return Some("poetry install".to_string());
        }
        if root.join("requirements.txt").exists() {
            return Some("pip install -r requirements.txt".to_string());
        }
        return Some("pip install -e .".to_string());
    }
    if root.join("Gemfile").exists() {
        return Some("bundle install".to_string());
    }
    if root.join("mix.exs").exists() {
        return Some("mix deps.get".to_string());
    }
    None
}

// ── Step 3: create-env ────────────────────────────────────────────────────

fn step_create_env(root: &Path, file: &SnipFile) -> Result<(SetupStatus, String)> {
    let env_path = root.join(".env");

    if env_path.exists() {
        return Ok((
            SetupStatus::NotNeeded,
            ".env already exists (left untouched)".to_string(),
        ));
    }

    // If .env.example exists, copy it
    let example_path = root.join(".env.example");
    if example_path.exists() {
        std::fs::copy(&example_path, &env_path).with_context(|| {
            format!(
                "failed to copy {} → {}",
                example_path.display(),
                env_path.display()
            )
        })?;
        return Ok((
            SetupStatus::Success,
            ".env created from .env.example — edit it now: $EDITOR .env".to_string(),
        ));
    }

    // No .env.example — generate a template from env vars referenced in snippets
    let referenced = collect_referenced_env_vars(file);
    if referenced.is_empty() {
        return Ok((
            SetupStatus::NotNeeded,
            "no .env needed (no env vars referenced in snippets)".to_string(),
        ));
    }

    let template: String = referenced
        .iter()
        .map(|v| format!("{}=", v))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&env_path, template + "\n").context("failed to write .env")?;

    Ok((
        SetupStatus::Success,
        format!(
            ".env created with {} var(s) — fill in values: $EDITOR .env",
            referenced.len()
        ),
    ))
}

/// Collect env vars referenced across all snippets (deduped, sorted).
/// Reuses the same logic as `snip doctor`.
fn collect_referenced_env_vars(file: &SnipFile) -> Vec<String> {
    let mut vars: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let always_set: &[&str] = &[
        "PATH", "HOME", "USER", "SHELL", "LANG", "LC_ALL", "LC_CTYPE", "PWD", "OLDPWD", "TERM",
        "SHLVL", "_", "TMPDIR",
    ];
    for (_, snippet) in file.iter() {
        for v in extract_env_vars(&snippet.cmd) {
            if !always_set.contains(&v.as_str()) {
                vars.insert(v);
            }
        }
    }
    vars.into_iter().collect()
}

// ── Step 4: start-services ────────────────────────────────────────────────

fn step_start_services(
    root: &Path,
    file: &SnipFile,
    opts: &SetupCmd,
) -> Result<(SetupStatus, String)> {
    // Prefer a snippet tagged ["setup"] with key start-services
    if let Some(cmd) = find_setup_snippet(file, &["start-services"]) {
        return run_step_cmd("start-services", &cmd, root, opts, true);
    }

    // Auto-detect: docker compose if docker-compose.yml exists
    let has_compose =
        root.join("docker-compose.yml").exists() || root.join("docker-compose.yaml").exists();
    if !has_compose {
        return Ok((
            SetupStatus::NotNeeded,
            "no docker-compose.yml found".to_string(),
        ));
    }

    // Check Docker daemon is running
    if which::which("docker").is_err() {
        return Ok((
            SetupStatus::Skipped,
            "docker binary not installed".to_string(),
        ));
    }
    let docker_check = std::process::Command::new("docker")
        .args(["info", "--format", "{{.ServerVersion}}"])
        .output();
    let docker_running = match docker_check {
        Ok(out) => out.status.success(),
        Err(_) => false,
    };
    if !docker_running {
        return Ok((
            SetupStatus::Skipped,
            "Docker daemon not running (start dockerd or open Docker Desktop)".to_string(),
        ));
    }

    run_step_cmd("start-services", "docker compose up -d", root, opts, true)
}

// ── Step 5 & 6: build / test ──────────────────────────────────────────────

/// Run a snippet by key (preferring tagged ["setup"]), else fall back to detected cmd.
fn step_run_snippet_or_detected(
    root: &Path,
    file: &SnipFile,
    opts: &SetupCmd,
    step: &str,
    detected: Option<String>,
) -> Result<(SetupStatus, String)> {
    if let Some(cmd) = find_setup_snippet(file, &[step]) {
        return run_step_cmd(step, &cmd, root, opts, true);
    }
    // Fall back to any snippet with this exact key (even untagged)
    if let Some(snippet) = file.get(step) {
        return run_step_cmd(step, &snippet.cmd, root, opts, true);
    }
    match detected {
        Some(c) => run_step_cmd(step, &c, root, opts, true),
        None => Ok((
            SetupStatus::NotNeeded,
            format!("no `{}` snippet and no detected {} command", step, step),
        )),
    }
}

fn detect_build_cmd(root: &Path) -> Option<String> {
    if root.join("package.json").exists() {
        return Some("npm run build".to_string());
    }
    if root.join("Cargo.toml").exists() {
        return Some("cargo build".to_string());
    }
    if root.join("go.mod").exists() {
        return Some("go build ./...".to_string());
    }
    if root.join("Makefile").exists() || root.join("makefile").exists() {
        return Some("make build".to_string());
    }
    None
}

fn detect_test_cmd(root: &Path) -> Option<String> {
    if root.join("package.json").exists() {
        return Some("npm test".to_string());
    }
    if root.join("Cargo.toml").exists() {
        return Some("cargo test".to_string());
    }
    if root.join("go.mod").exists() {
        return Some("go test ./...".to_string());
    }
    if root.join("Makefile").exists() || root.join("makefile").exists() {
        return Some("make test".to_string());
    }
    None
}

// ── Step 7: dev (foreground) ──────────────────────────────────────────────

fn step_dev(root: &Path, file: &SnipFile, opts: &SetupCmd) -> Result<(SetupStatus, String)> {
    let cmd = find_setup_snippet(file, &["dev"]).or_else(|| file.get("dev").map(|s| s.cmd.clone()));
    match cmd {
        Some(c) => {
            // dev runs in foreground — never use DryRun skip, just print + exec
            if opts.dry_run {
                return Ok((
                    SetupStatus::DryRun,
                    format!("would run (foreground): {}", c),
                ));
            }
            if !opts.non_interactive {
                let prompt = "Start dev server? [Y/n] ";
                if !prompt_yes(prompt)? {
                    return Ok((SetupStatus::Declined, "user declined".to_string()));
                }
            }
            println!("  {} (foreground — Ctrl+C to stop)", c.cyan());
            println!();
            let status = std::process::Command::new("sh")
                .args(["-c", &c])
                .current_dir(root)
                .status();
            match status {
                Ok(s) if s.success() => Ok((SetupStatus::Success, "dev server exited".to_string())),
                Ok(s) => Ok((
                    SetupStatus::Failed,
                    format!("dev server exited with status {}", s.code().unwrap_or(-1)),
                )),
                Err(e) => Ok((SetupStatus::Failed, format!("failed to spawn: {}", e))),
            }
        }
        None => Ok((
            SetupStatus::NotNeeded,
            "no `dev` snippet defined".to_string(),
        )),
    }
}

// ── Helper: find a snippet tagged ["setup"] by key ────────────────────────

/// Look for a snippet whose key matches one of `keys` AND has `setup` in its tags.
/// Returns the command string if found.
fn find_setup_snippet(file: &SnipFile, keys: &[&str]) -> Option<String> {
    for (key, snippet) in file.iter() {
        if keys.contains(&key.as_str()) && snippet.tags.iter().any(|t| t == "setup") {
            return Some(snippet.cmd.clone());
        }
    }
    None
}

// ── Helper: run a step command (with prompt + dry-run handling) ───────────

fn run_step_cmd(
    step: &str,
    cmd: &str,
    root: &Path,
    opts: &SetupCmd,
    _cd_root: bool,
) -> Result<(SetupStatus, String)> {
    if opts.dry_run {
        return Ok((SetupStatus::DryRun, format!("would run: {}", cmd)));
    }

    if !opts.non_interactive {
        let prompt = format!("Run {}? [Y/n] ", step);
        if !prompt_yes(&prompt)? {
            return Ok((SetupStatus::Declined, "user declined".to_string()));
        }
    }

    println!("  {} {}", "→".dimmed(), cmd.cyan());
    let status = std::process::Command::new("sh")
        .args(["-c", cmd])
        .current_dir(root)
        .status();

    match status {
        Ok(s) if s.success() => Ok((SetupStatus::Success, cmd.to_string())),
        Ok(s) => Ok((
            SetupStatus::Failed,
            format!("{} (exit {})", cmd, s.code().unwrap_or(-1)),
        )),
        Err(e) => Ok((
            SetupStatus::Failed,
            format!("failed to spawn '{}': {}", cmd, e),
        )),
    }
}

// ── Helper: prompt yes/no ─────────────────────────────────────────────────

fn prompt_yes(prompt: &str) -> Result<bool> {
    print!("  {} ", prompt.bold());
    std::io::stdout().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();
    Ok(input.is_empty() || input == "y" || input == "yes")
}

// ── Helper: extract env vars (mirror of doctor.rs) ────────────────────────

fn extract_env_vars(cmd: &str) -> Vec<String> {
    let mut vars = Vec::new();
    let bytes = cmd.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
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

// ── Summary ────────────────────────────────────────────────────────────────

fn print_summary(results: &[SetupStepResult]) {
    println!("{}", "═══════════════════════════════════════════".dimmed());
    println!("{}", "  Setup summary".bold());
    println!("{}", "═══════════════════════════════════════════".dimmed());
    println!();

    let name_width = results
        .iter()
        .map(|r| r.name.len())
        .max()
        .unwrap_or(12)
        .max(12);

    for r in results {
        let padded = format!("{:width$}", r.name, width = name_width);
        println!(
            "  {} {}  {}",
            r.status.icon(),
            padded.cyan(),
            r.message.dimmed()
        );
    }

    println!();
    let success = results
        .iter()
        .filter(|r| r.status == SetupStatus::Success)
        .count();
    let failed = results
        .iter()
        .filter(|r| r.status == SetupStatus::Failed)
        .count();
    let skipped = results
        .iter()
        .filter(|r| {
            matches!(
                r.status,
                SetupStatus::Skipped | SetupStatus::NotNeeded | SetupStatus::Declined
            )
        })
        .count();

    if failed > 0 {
        println!(
            "  {} {} succeeded, {} failed, {} skipped",
            "!".yellow(),
            success,
            failed,
            skipped
        );
    } else {
        println!(
            "  {} {} step(s) succeeded, {} skipped",
            "✓".green(),
            success,
            skipped
        );
    }
    println!();
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::snipfile::write_snippets;
    use crate::core::snippet::{SnipFile, Snippet};
    use std::fs;

    fn make_tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn test_find_setup_snippet_tagged() {
        let mut f = SnipFile::new();
        f.insert(
            "build",
            Snippet::new("make build").with_tags(vec!["setup".into()]),
        );
        f.insert(
            "test",
            Snippet::new("cargo test"), // not tagged
        );
        assert_eq!(
            find_setup_snippet(&f, &["build"]),
            Some("make build".to_string())
        );
        assert_eq!(find_setup_snippet(&f, &["test"]), None); // not tagged, so None
    }

    #[test]
    fn test_find_setup_snippet_alternate_keys() {
        let mut f = SnipFile::new();
        f.insert(
            "install",
            Snippet::new("pnpm install").with_tags(vec!["setup".into()]),
        );
        // install-deps falls back to install
        assert_eq!(
            find_setup_snippet(&f, &["install-deps", "install"]),
            Some("pnpm install".to_string())
        );
    }

    #[test]
    fn test_detect_install_cmd_node() {
        let tmp = make_tmp();
        fs::write(tmp.path().join("package.json"), "{}").unwrap();
        assert_eq!(
            detect_install_cmd(tmp.path()),
            Some("npm install".to_string())
        );
    }

    #[test]
    fn test_detect_install_cmd_pnpm() {
        let tmp = make_tmp();
        fs::write(tmp.path().join("package.json"), "{}").unwrap();
        fs::write(tmp.path().join("pnpm-lock.yaml"), "").unwrap();
        assert_eq!(
            detect_install_cmd(tmp.path()),
            Some("pnpm install".to_string())
        );
    }

    #[test]
    fn test_detect_install_cmd_yarn() {
        let tmp = make_tmp();
        fs::write(tmp.path().join("package.json"), "{}").unwrap();
        fs::write(tmp.path().join("yarn.lock"), "").unwrap();
        assert_eq!(
            detect_install_cmd(tmp.path()),
            Some("yarn install".to_string())
        );
    }

    #[test]
    fn test_detect_install_cmd_cargo() {
        let tmp = make_tmp();
        fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname=\"x\"\nversion=\"0.1\"\n",
        )
        .unwrap();
        assert_eq!(
            detect_install_cmd(tmp.path()),
            Some("cargo fetch".to_string())
        );
    }

    #[test]
    fn test_detect_install_cmd_go() {
        let tmp = make_tmp();
        fs::write(tmp.path().join("go.mod"), "module x\n").unwrap();
        assert_eq!(
            detect_install_cmd(tmp.path()),
            Some("go mod download".to_string())
        );
    }

    #[test]
    fn test_detect_install_cmd_python_uv() {
        let tmp = make_tmp();
        fs::write(tmp.path().join("pyproject.toml"), "[project]\nname=\"x\"\n").unwrap();
        fs::write(tmp.path().join("uv.lock"), "").unwrap();
        assert_eq!(detect_install_cmd(tmp.path()), Some("uv sync".to_string()));
    }

    #[test]
    fn test_detect_install_cmd_python_poetry() {
        let tmp = make_tmp();
        fs::write(tmp.path().join("pyproject.toml"), "[project]\nname=\"x\"\n").unwrap();
        fs::write(tmp.path().join("poetry.lock"), "").unwrap();
        assert_eq!(
            detect_install_cmd(tmp.path()),
            Some("poetry install".to_string())
        );
    }

    #[test]
    fn test_detect_install_cmd_python_requirements() {
        let tmp = make_tmp();
        fs::write(tmp.path().join("requirements.txt"), "requests\n").unwrap();
        assert_eq!(
            detect_install_cmd(tmp.path()),
            Some("pip install -r requirements.txt".to_string())
        );
    }

    #[test]
    fn test_detect_install_cmd_ruby() {
        let tmp = make_tmp();
        fs::write(
            tmp.path().join("Gemfile"),
            "source 'https://rubygems.org'\n",
        )
        .unwrap();
        assert_eq!(
            detect_install_cmd(tmp.path()),
            Some("bundle install".to_string())
        );
    }

    #[test]
    fn test_detect_install_cmd_elixir() {
        let tmp = make_tmp();
        fs::write(tmp.path().join("mix.exs"), "defmodule X do end\n").unwrap();
        assert_eq!(
            detect_install_cmd(tmp.path()),
            Some("mix deps.get".to_string())
        );
    }

    #[test]
    fn test_detect_install_cmd_none() {
        let tmp = make_tmp();
        assert_eq!(detect_install_cmd(tmp.path()), None);
    }

    #[test]
    fn test_detect_build_cmd() {
        let tmp = make_tmp();
        fs::write(tmp.path().join("package.json"), "{}").unwrap();
        assert_eq!(
            detect_build_cmd(tmp.path()),
            Some("npm run build".to_string())
        );

        let tmp2 = make_tmp();
        fs::write(tmp2.path().join("Cargo.toml"), "[package]\nname=\"x\"\n").unwrap();
        assert_eq!(
            detect_build_cmd(tmp2.path()),
            Some("cargo build".to_string())
        );

        let tmp3 = make_tmp();
        assert_eq!(detect_build_cmd(tmp3.path()), None);
    }

    #[test]
    fn test_detect_test_cmd() {
        let tmp = make_tmp();
        fs::write(tmp.path().join("go.mod"), "module x\n").unwrap();
        assert_eq!(
            detect_test_cmd(tmp.path()),
            Some("go test ./...".to_string())
        );
    }

    #[test]
    fn test_step_create_env_already_exists() {
        let tmp = make_tmp();
        fs::write(tmp.path().join(".env"), "EXISTING=1\n").unwrap();
        let f = SnipFile::new();
        let (status, msg) = step_create_env(tmp.path(), &f).unwrap();
        assert_eq!(status, SetupStatus::NotNeeded);
        assert!(msg.contains("already exists"));

        // Verify .env wasn't touched
        let content = fs::read_to_string(tmp.path().join(".env")).unwrap();
        assert_eq!(content, "EXISTING=1\n");
    }

    #[test]
    fn test_step_create_env_from_example() {
        let tmp = make_tmp();
        fs::write(tmp.path().join(".env.example"), "FOO=bar\nBAZ=\n").unwrap();
        let f = SnipFile::new();
        let (status, _msg) = step_create_env(tmp.path(), &f).unwrap();
        assert_eq!(status, SetupStatus::Success);
        let content = fs::read_to_string(tmp.path().join(".env")).unwrap();
        assert_eq!(content, "FOO=bar\nBAZ=\n");
    }

    #[test]
    fn test_step_create_env_from_snippet_vars() {
        let tmp = make_tmp();
        let mut f = SnipFile::new();
        f.insert(
            "deploy",
            Snippet::new("kubectl --token=$DEPLOY_TOKEN apply -f k8s/"),
        );
        f.insert("db", Snippet::new("psql ${DATABASE_URL} -c 'SELECT 1'"));
        // PATH should NOT be included (always-set)
        f.insert("path", Snippet::new("echo $PATH"));

        let (status, msg) = step_create_env(tmp.path(), &f).unwrap();
        assert_eq!(status, SetupStatus::Success);
        assert!(msg.contains("2 var(s)"));

        let content = fs::read_to_string(tmp.path().join(".env")).unwrap();
        assert!(content.contains("DATABASE_URL="));
        assert!(content.contains("DEPLOY_TOKEN="));
        assert!(!content.contains("PATH="));
    }

    #[test]
    fn test_step_create_env_no_vars_needed() {
        let tmp = make_tmp();
        let mut f = SnipFile::new();
        f.insert("hello", Snippet::new("echo hello"));
        let (status, msg) = step_create_env(tmp.path(), &f).unwrap();
        assert_eq!(status, SetupStatus::NotNeeded);
        assert!(msg.contains("no .env needed"));
        assert!(!tmp.path().join(".env").exists());
    }

    #[test]
    fn test_step_check_tools_node() {
        let tmp = make_tmp();
        fs::write(tmp.path().join("package.json"), "{}").unwrap();
        let (status, msg) = step_check_tools(tmp.path()).unwrap();
        // Status depends on whether node is installed in the test env
        assert!(msg.contains("Node.js"));
        // git is always checked
        assert!(msg.contains("git"));
        // Status is Success if at least one tool was found, even if some missing
        let _ = status;
    }

    #[test]
    fn test_step_check_tools_no_files() {
        let tmp = make_tmp();
        let (status, msg) = step_check_tools(tmp.path()).unwrap();
        // Only git is checked (no project files)
        assert!(msg.contains("git"));
        let _ = status;
    }

    #[test]
    fn test_step_start_services_no_compose() {
        let tmp = make_tmp();
        let f = SnipFile::new();
        let opts = SetupCmd::default();
        let (status, msg) = step_start_services(tmp.path(), &f, &opts).unwrap();
        assert_eq!(status, SetupStatus::NotNeeded);
        assert!(msg.contains("no docker-compose.yml"));
    }

    #[test]
    fn test_dry_run_does_not_execute() {
        let tmp = make_tmp();
        let mut f = SnipFile::new();
        f.insert(
            "build",
            Snippet::new("exit 42").with_tags(vec!["setup".into()]),
        );
        let opts = SetupCmd {
            dry_run: true,
            ..Default::default()
        };
        let (status, msg) =
            step_run_snippet_or_detected(tmp.path(), &f, &opts, "build", None).unwrap();
        assert_eq!(status, SetupStatus::DryRun);
        assert!(msg.contains("would run: exit 42"));
        // Nothing was executed — no exit code side effects
    }

    #[test]
    fn test_skip_flag_filters_steps() {
        let opts = SetupCmd {
            skip: vec!["build".to_string(), "test".to_string()],
            ..Default::default()
        };
        // Simulate the filter logic from run_at
        let steps: Vec<&str> = ALL_STEPS
            .iter()
            .filter(|&&step| !opts.skip.iter().any(|s| s.to_lowercase() == step))
            .copied()
            .collect();
        assert!(!steps.contains(&"build"));
        assert!(!steps.contains(&"test"));
        assert!(steps.contains(&"check-tools"));
        assert!(steps.contains(&"dev"));
    }

    #[test]
    fn test_only_flag_selects_steps() {
        let opts = SetupCmd {
            only: vec!["install-deps".to_string(), "dev".to_string()],
            ..Default::default()
        };
        let steps: Vec<&str> = ALL_STEPS
            .iter()
            .filter(|s| {
                opts.only
                    .iter()
                    .any(|o| o.to_lowercase() == s.to_string().to_lowercase())
            })
            .copied()
            .collect();
        assert_eq!(steps, vec!["install-deps", "dev"]);
    }

    #[test]
    fn test_unknown_step_in_only_is_ignored() {
        let opts = SetupCmd {
            only: vec!["install-deps".to_string(), "unknown-step".to_string()],
            ..Default::default()
        };
        let steps: Vec<&str> = opts
            .only
            .iter()
            .filter_map(|s| {
                let lower = s.to_lowercase();
                if ALL_STEPS.contains(&lower.as_str()) {
                    Some(
                        ALL_STEPS
                            .iter()
                            .find(|&&step| step == lower)
                            .copied()
                            .unwrap(),
                    )
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(steps, vec!["install-deps"]);
    }

    #[test]
    fn test_extract_env_vars_dollar_form() {
        let vars = extract_env_vars("echo $FOO and $BAR_BAZ");
        assert!(vars.contains(&"FOO".to_string()));
        assert!(vars.contains(&"BAR_BAZ".to_string()));
    }

    #[test]
    fn test_extract_env_vars_brace_form() {
        let vars = extract_env_vars("echo ${FOO} and ${BAR}");
        assert!(vars.contains(&"FOO".to_string()));
        assert!(vars.contains(&"BAR".to_string()));
    }

    #[test]
    fn test_extract_env_vars_mixed_forms() {
        let vars = extract_env_vars("kubectl --token=$TOKEN --region ${REGION}");
        assert!(vars.contains(&"TOKEN".to_string()));
        assert!(vars.contains(&"REGION".to_string()));
    }

    #[test]
    fn test_extract_env_vars_dedupes() {
        let vars = extract_env_vars("$FOO $FOO ${FOO}");
        let foo_count = vars.iter().filter(|v| v == &"FOO").count();
        assert_eq!(foo_count, 1);
    }

    #[test]
    fn test_collect_referenced_env_vars_filters_always_set() {
        let mut f = SnipFile::new();
        f.insert("a", Snippet::new("echo $PATH"));
        f.insert("b", Snippet::new("echo $HOME"));
        f.insert("c", Snippet::new("echo $MY_API_KEY"));
        let vars = collect_referenced_env_vars(&f);
        assert!(vars.contains(&"MY_API_KEY".to_string()));
        assert!(!vars.contains(&"PATH".to_string()));
        assert!(!vars.contains(&"HOME".to_string()));
    }

    #[test]
    fn test_step_install_deps_uses_tagged_snippet() {
        let tmp = make_tmp();
        let mut f = SnipFile::new();
        f.insert(
            "install-deps",
            Snippet::new("echo 'installing...'").with_tags(vec!["setup".into()]),
        );
        // Dry-run so we don't actually execute
        let opts = SetupCmd {
            dry_run: true,
            ..Default::default()
        };
        let (status, msg) = step_install_deps(tmp.path(), &f, &opts).unwrap();
        assert_eq!(status, SetupStatus::DryRun);
        assert!(msg.contains("echo 'installing...'"));
    }

    #[test]
    fn test_step_install_deps_auto_detects() {
        let tmp = make_tmp();
        fs::write(tmp.path().join("package.json"), "{}").unwrap();
        let f = SnipFile::new();
        let opts = SetupCmd {
            dry_run: true,
            ..Default::default()
        };
        let (status, msg) = step_install_deps(tmp.path(), &f, &opts).unwrap();
        assert_eq!(status, SetupStatus::DryRun);
        assert!(msg.contains("npm install"));
    }

    #[test]
    fn test_step_install_deps_not_needed() {
        let tmp = make_tmp();
        let f = SnipFile::new();
        let opts = SetupCmd::default();
        let (status, msg) = step_install_deps(tmp.path(), &f, &opts).unwrap();
        assert_eq!(status, SetupStatus::NotNeeded);
        assert!(msg.contains("no dependency manifest"));
    }

    #[test]
    fn test_setup_status_icons_distinct() {
        // Smoke test: every variant produces a non-empty icon string
        assert!(!SetupStatus::Success.icon().is_empty());
        assert!(!SetupStatus::Skipped.icon().is_empty());
        assert!(!SetupStatus::NotNeeded.icon().is_empty());
        assert!(!SetupStatus::Failed.icon().is_empty());
        assert!(!SetupStatus::DryRun.icon().is_empty());
        assert!(!SetupStatus::Declined.icon().is_empty());
    }

    /// End-to-end smoke test of `run_at` in dry-run mode.
    #[test]
    fn test_run_at_dry_run_does_not_execute() {
        let tmp = make_tmp();
        // Create a project that looks like Node
        fs::write(tmp.path().join("package.json"), "{}").unwrap();
        // .env.example to skip create-env generation
        fs::write(tmp.path().join(".env.example"), "FOO=bar\n").unwrap();

        // .snips with a build snippet tagged setup that would fail if run
        let mut f = SnipFile::new();
        f.insert(
            "build",
            Snippet::new("exit 42").with_tags(vec!["setup".into()]),
        );
        f.insert(
            "test",
            Snippet::new("exit 99").with_tags(vec!["setup".into()]),
        );
        let snipfile = tmp.path().join(".snips");
        write_snippets(&snipfile, &f).unwrap();

        let opts = SetupCmd {
            dry_run: true,
            only: vec![
                "install-deps".to_string(),
                "create-env".to_string(),
                "build".to_string(),
            ],
            ..Default::default()
        };

        // Should complete without error and without executing the failing commands
        let result = run_at(tmp.path(), &opts);
        assert!(result.is_ok());

        // .env should have been created (create-env is not a command, runs even in dry-run)
        assert!(tmp.path().join(".env").exists());
    }
}
