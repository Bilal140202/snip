use std::path::Path;

use anyhow::{Context, Result};
use colored::Colorize;

use crate::core::snipfile::{find_snipfile, read_snippets, write_snippets};
use crate::core::stale;

/// Run `snip stale` — detect potentially unused or outdated snippets.
pub fn run(fix: bool, json: bool) -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to determine current directory")?;
    run_at(&cwd, fix, json)
}

/// Internal stale function that accepts a root path for testing.
pub(crate) fn run_at(root: &Path, fix: bool, json: bool) -> Result<()> {
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

    let checks = stale::detect_stale(&file, &snipfile_path);

    if json {
        let json_out = serde_json::to_string_pretty(&checks)
            .context("Failed to serialize stale checks to JSON")?;
        println!("{}", json_out);
        return Ok(());
    }

    // Display grouped report
    if checks.is_empty() {
        println!("{}", stale::format_stale_report(&[]));
        return Ok(());
    }

    // Auto-fix pass if requested
    if fix {
        let mut fixed_count = 0;
        let mut unfixable_count = 0;

        // We need to fix in reverse order if we're removing items,
        // but since fix_stale works by key it should be fine either way.
        for check in &checks {
            match stale::fix_stale(check, &mut file) {
                Ok(true) => fixed_count += 1,
                Ok(false) => unfixable_count += 1,
                Err(e) => {
                    eprintln!(
                        "{} Fixing [{}] failed: {}",
                        "!".yellow(),
                        check.key,
                        e
                    );
                }
            }
        }

        if fixed_count > 0 {
            write_snippets(&snipfile_path, &file)?;
            println!(
                "{} Auto-fixed {} issue(s). File updated.",
                "✓".green(),
                fixed_count
            );
        }

        if unfixable_count > 0 {
            println!(
                "{} {} issue(s) could not be auto-fixed.",
                "⚠".yellow(),
                unfixable_count
            );
        }

        // Re-check after fixes
        let new_checks = stale::detect_stale(&file, &snipfile_path);
        if !new_checks.is_empty() {
            println!();
            println!("{}", "Remaining issues:".dimmed());
            println!("{}", stale::format_stale_report(&new_checks));
        } else if fixed_count > 0 {
            println!(
                "{} All issues resolved.",
                "✓".green()
            );
        }

        return Ok(());
    }

    // Normal display (no --fix)
    println!("{}", stale::format_stale_report(&checks));

    println!(
        "\nRun {} to auto-fix what can be fixed.",
        "snip stale --fix".cyan()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::core::snippet::Snippet;

    #[test]
    fn test_stale_no_snips_file() {
        let tmp = tempfile::tempdir().unwrap();
        // Should not error — just print a message
        super::run_at(tmp.path(), false, false).unwrap();
    }

    #[test]
    fn test_stale_empty_file() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");
        let file = crate::core::snippet::SnipFile::new();
        crate::core::snipfile::write_snippets(&snipfile, &file).unwrap();
        super::run_at(tmp.path(), false, false).unwrap();
    }

    #[test]
    fn test_stale_detects_issues() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");
        let mut file = crate::core::snippet::SnipFile::new();
        file.insert("no-desc", Snippet::new("echo hi"));
        crate::core::snipfile::write_snippets(&snipfile, &file).unwrap();
        // Should not error
        super::run_at(tmp.path(), false, false).unwrap();
    }

    #[test]
    fn test_stale_json_output() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");
        let mut file = crate::core::snippet::SnipFile::new();
        file.insert("no-desc", Snippet::new("echo hi"));
        crate::core::snipfile::write_snippets(&snipfile, &file).unwrap();
        // Should not error, outputs JSON
        super::run_at(tmp.path(), false, true).unwrap();
    }

    #[test]
    fn test_stale_fix_removes_missing_binary() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");
        let mut file = crate::core::snippet::SnipFile::new();
        file.insert(
            "bad",
            Snippet::new("nonexistent_xyz_binary_123").with_desc("Broken"),
        );
        crate::core::snipfile::write_snippets(&snipfile, &file).unwrap();

        super::run_at(tmp.path(), true, false).unwrap();

        // After fix, the file should have been updated (bad removed)
        let updated = crate::core::snipfile::read_snippets(&snipfile).unwrap();
        assert!(updated.get("bad").is_none());
    }

    #[test]
    fn test_stale_fix_adds_description() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");
        let mut file = crate::core::snippet::SnipFile::new();
        file.insert("hi", Snippet::new("echo hello world"));
        crate::core::snipfile::write_snippets(&snipfile, &file).unwrap();

        super::run_at(tmp.path(), true, false).unwrap();

        let updated = crate::core::snipfile::read_snippets(&snipfile).unwrap();
        let snippet = updated.get("hi").unwrap();
        assert!(!snippet.desc.is_empty());
        assert_eq!(snippet.desc, "echo hello world");
    }

    #[test]
    fn test_stale_summary_counts() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");
        let mut file = crate::core::snippet::SnipFile::new();
        // Missing binary → Critical
        file.insert(
            "broken",
            Snippet::new("nonexistent_xyz_binary_123").with_desc("Broken cmd"),
        );
        crate::core::snipfile::write_snippets(&snipfile, &file).unwrap();

        // Run and verify it doesn't panic
        super::run_at(tmp.path(), false, false).unwrap();
    }
}