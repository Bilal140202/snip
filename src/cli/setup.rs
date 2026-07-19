//! `snip setup` — Interactive team onboarding wizard.

use std::io::Write as IoWrite;

use anyhow::{Context, Result};
use colored::Colorize;

use crate::core::snipfile::{find_snipfile, find_snips_dir, read_snippets, write_snippets};
use crate::core::validator;

/// Run `snip setup` — interactive team onboarding wizard.
pub fn run() -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to determine current directory")?;

    println!();
    println!("{}", "═══════════════════════════════════════".dimmed());
    println!("{}", "  snip setup — Project Onboarding".bold());
    println!("{}", "═══════════════════════════════════════".dimmed());
    println!();

    // Step 1: Check environment
    step_environment(&cwd)?;

    // Step 2: Ensure .snips exists
    let snipfile_path = ensure_snipfile(&cwd)?;

    // Step 3: Validate snippets
    step_validate(&snipfile_path)?;

    // Step 4: Check binaries
    step_binaries(&snipfile_path)?;

    // Step 5: Shell integration
    step_shell_integration()?;

    // Step 6: Summary
    step_summary(&snipfile_path)?;

    Ok(())
}

fn step_environment(cwd: &std::path::Path) -> Result<()> {
    println!("{}", "Step 1: Environment".bold());
    println!();

    // Check git repo
    let is_git = crate::utils::git::is_git_repo_from(cwd);
    let git_icon = if is_git { "✓".green() } else { "✗".yellow() };
    println!("  {} Git repository: {}", git_icon, if is_git { "yes" } else { "no" });

    // Show shell
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "unknown".to_string());
    println!("  {} Shell: {}", "✓".green(), shell);

    // Show editor
    let editor = crate::utils::shell::default_editor();
    println!("  {} Editor: {}", "✓".green(), editor);

    // Check fzf
    let has_fzf = which::which("fzf").is_ok();
    let fzf_icon = if has_fzf { "✓".green() } else { "⚠".yellow() };
    println!(
        "  {} fzf: {}",
        fzf_icon,
        if has_fzf { "installed" } else { "not found (recommended for interactive picker)" }
    );

    if !has_fzf {
        println!(
            "    {} Install: {}",
            "".dimmed(),
            "https://github.com/junegunn/fzf#installation".cyan()
        );
    }

    println!();
    Ok(())
}

fn ensure_snipfile(cwd: &std::path::Path) -> Result<std::path::PathBuf> {
    println!("{}", "Step 2: Snippets File".bold());
    println!();

    let snipfile_path = cwd.join(".snips");

    if snipfile_path.exists() {
        let file = read_snippets(&snipfile_path)?;
        println!("  {} .snips exists with {} snippet(s)", "✓".green(), file.len());

        // Check for .snips.d/
        let snips_dir = cwd.join(".snips.d");
        if snips_dir.is_dir() {
            let files = crate::core::snipfile::list_snips_d_files(cwd)?;
            println!("  {} .snips.d/ exists with {} modular file(s)", "✓".green(), files.len());
        }
    } else {
        // Auto-detect
        let detected = crate::core::detector::detect_snippets(cwd);
        if detected.is_empty() {
            println!("  {} No project type detected, creating empty .snips", "!".yellow());
            let file = crate::core::snippet::SnipFile::new();
            write_snippets(&snipfile_path, &file)?;
        } else {
            let mut file = crate::core::snippet::SnipFile::new();
            for (section, name, cmd, desc) in &detected {
                let key = if section.is_empty() {
                    name.clone()
                } else {
                    format!("{}.{}", section, name)
                };
                file.insert(
                    key,
                    crate::core::snippet::Snippet::new(cmd.as_str()).with_desc(desc.as_str()),
                );
            }
            write_snippets(&snipfile_path, &file)?;
            println!("  {} Created .snips with {} detected snippet(s)", "✓".green(), file.len());
        }

        // Create .snips.d/ directory
        find_snips_dir(cwd, true)?;
        // Create .gitignore in .snips.d/ to ignore _local.toml
        let gitignore = cwd.join(".snips.d/.gitignore");
        if !gitignore.exists() {
            std::fs::write(&gitignore, "_local.toml\n")?;
        }
        println!("  {} Created .snips.d/ for modular snippets", "✓".green());
    }

    println!();
    Ok(snipfile_path)
}

fn step_validate(snipfile_path: &std::path::Path) -> Result<()> {
    println!("{}", "Step 3: Validation".bold());
    println!();

    let file = read_snippets(snipfile_path)?;
    let issues = validator::validate(&file);

    if issues.is_empty() {
        println!("  {} All snippets pass validation", "✓".green());
    } else {
        let errors = issues.iter().filter(|i| i.severity == validator::Severity::Error).count();
        let warnings = issues.iter().filter(|i| i.severity == validator::Severity::Warning).count();

        for issue in &issues {
            let icon = match issue.severity {
                validator::Severity::Error => "✗".red().to_string(),
                validator::Severity::Warning => "⚠".yellow().to_string(),
            };
            println!("  {} [{}] {}", icon, issue.key, issue.message);
        }

        if errors > 0 {
            println!();
            print!("  Fix {} error(s)? [y/N] ", errors);
            std::io::stdout().flush()?;
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            if input.trim().to_lowercase() == "y" {
                // Run doctor --fix
                let root = snipfile_path.parent().unwrap();
                crate::cli::doctor::run_at(root, true)?;
            }
        }
    }

    println!();
    Ok(())
}

fn step_binaries(snipfile_path: &std::path::Path) -> Result<()> {
    println!("{}", "Step 4: Binary Check".bold());
    println!();

    let file = read_snippets(snipfile_path)?;
    let mut missing = Vec::new();

    for (key, snippet) in file.iter() {
        let first_word = snippet.cmd.split_whitespace().next().unwrap_or("");
        if ["sh", "bash", "zsh", "fish", "true", "false", "echo", "cd", "test"].contains(&first_word) {
            continue;
        }
        if which::which(first_word).is_err() {
            missing.push((key.clone(), first_word.to_string()));
        }
    }

    if missing.is_empty() {
        println!("  {} All required binaries are available", "✓".green());
    } else {
        println!("  {} {} binary/binaries not found:", "⚠".yellow(), missing.len());
        for (key, binary) in &missing {
            println!("    {} [{}] {} not found", "✗".red(), key, binary);
        }
    }

    println!();
    Ok(())
}

fn step_shell_integration() -> Result<()> {
    println!("{}", "Step 5: Shell Integration".bold());
    println!();

    // Detect shell
    let shell_env = std::env::var("SHELL").unwrap_or_default();
    let (shell_name, rc_file) = if shell_env.contains("zsh") {
        ("zsh", "~/.zshrc")
    } else if shell_env.contains("fish") {
        ("fish", "~/.config/fish/config.fish")
    } else if shell_env.contains("nu") {
        ("nushell", "env.nu")
    } else {
        ("bash", "~/.bashrc")
    };

    let hook_line = "eval \"$(snip hook)\"";

    // Check if already in rc file
    let rc_path = shellexpand::shellexpand(rc_file).to_string();
    let already_configured = std::path::Path::new(&rc_path)
        .exists()
        .then(|| {
            std::fs::read_to_string(&rc_path)
                .map(|c| c.contains("snip hook"))
                .unwrap_or(false)
        })
        .unwrap_or(false);

    if already_configured {
        println!("  {} Shell integration already configured in {}", "✓".green(), rc_file);
    } else {
        println!("  {} Shell integration not yet configured", "!".yellow());
        println!();
        println!("  Add this line to {}:", rc_file.cyan());
        println!("    {}", hook_line.green().bold());
        println!();
        println!("  This enables:");
        println!("    • Tab-completion for snippet names");
        println!("    • Dynamic candidate list from .snips");
    }

    println!();
    Ok(())
}

fn step_summary(snipfile_path: &std::path::Path) -> Result<()> {
    println!("{}", "═══════════════════════════════════════".dimmed());
    println!("{}", "  Setup Complete!".green().bold());
    println!("{}", "═══════════════════════════════════════".dimmed());
    println!();

    let file = read_snippets(snipfile_path)?;
    println!("  {} snippet(s) configured", file.len().to_string().cyan());
    println!();
    println!("  Next steps:");
    println!("    {} — See all snippets", "snip".cyan());
    println!("    {} — Run a snippet", "snip run <name>".cyan());
    println!("    {} — Interactive picker", "snip run -i".cyan());
    println!("    {} — Add a new snippet", "snip add <name> \"<cmd>\"".cyan());
    println!("    {} — Check snippet health", "snip doctor".cyan());
    println!();

    Ok(())
}

// Minimal shell expansion (only handles ~/)
mod shellexpand {
    pub fn shellexpand(s: &str) -> String {
        if s.starts_with("~/") {
            if let Some(home) = std::env::var_os("HOME") {
                return format!("{}{}", home.to_string_lossy(), &s[1..]);
            }
        }
        s.to_string()
    }
}