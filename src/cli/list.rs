//! `snip list` — List all snippets, with optional JSON output and auto-init.

use std::collections::BTreeSet;
use std::io::Write as IoWrite;

use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;
use serde::Serialize;

use crate::core::snippet::Snippet;
use crate::core::snipfile::{find_snipfile, read_snippets};

/// List snippets.
#[derive(Debug, Args)]
pub struct ListCmd {
    /// Output as JSON.
    #[arg(long)]
    pub json: bool,

    /// Output format template (e.g. "{{key}}: {{cmd}}").
    /// Available fields: {{key}}, {{cmd}}, {{desc}}, {{section}}, {{name}}
    #[arg(long)]
    pub format: Option<String>,

    /// Only show snippets from this section.
    #[arg(long, short)]
    pub section: Option<String>,

    /// Launch interactive picker (uses fzf if available).
    #[arg(long, short)]
    pub interactive: bool,
}

/// JSON-serializable snippet entry for --json output.
#[derive(Debug, Serialize)]
pub struct SnippetEntry {
    pub key: String,
    pub cmd: String,
    pub desc: String,
    pub section: String,
    pub name: String,
    pub tags: Vec<String>,
    pub has_vars: bool,
}

impl ListCmd {
    pub fn run(&self) -> Result<()> {
        let cwd = std::env::current_dir().context("Failed to determine current directory")?;
        run_from(&cwd, self)
    }
}

/// Internal list function that accepts a path for testing.
pub(crate) fn run_from(path: &std::path::Path, opts: &ListCmd) -> Result<()> {
    let snipfile_path = match find_snipfile(Some(path))? {
        Some(p) => p,
        None => {
            // Auto-init: detect project type and offer to create .snips
            return auto_init(path);
        }
    };

    let file = read_snippets(&snipfile_path)?;

    if file.is_empty() {
        if opts.json {
            println!("[]");
        } else {
            println!("{}", "No snippets defined.".dimmed());
            println!("Add snippets with:");
            println!("  {}", "snip add <name> \"<command>\" [description]".cyan());
        }
        return Ok(());
    }

    // Interactive mode: launch picker
    if opts.interactive {
        return run_interactive(&file);
    }

    // JSON output
    if opts.json {
        return output_json(&file, opts.section.as_deref());
    }

    // Format template
    if let Some(ref fmt) = opts.format {
        output_formatted(&file, fmt, opts.section.as_deref());
        return Ok(());
    }

    // Default: human-readable grouped output
    output_human(&file, opts.section.as_deref());
    Ok(())
}

fn output_human(file: &crate::core::snippet::SnipFile, section_filter: Option<&str>) {
    // Collect entries, optionally filtered
    let entries: Vec<_> = if let Some(section) = section_filter {
        file.iter()
            .filter(|(key, _)| {
                if key.contains('.') {
                    key.starts_with(&format!("{}.", section))
                } else {
                    key.as_str() == section
                }
            })
            .collect()
    } else {
        file.iter().collect()
    };

    if entries.is_empty() {
        println!("{}", "No snippets match the filter.".dimmed());
        return;
    }

    // Calculate column widths
    let mut max_name_len = 0;
    for (key, _) in &entries {
        max_name_len = max_name_len.max(key.len());
    }
    let name_width = max_name_len.max(16);

    // Collect section groups
    let mut sections: BTreeSet<String> = BTreeSet::new();
    for (key, _) in &entries {
        if let Some(dot_pos) = key.find('.') {
            sections.insert(key[..dot_pos].to_string());
        }
    }

    // Print top-level (no dot) snippets first
    let mut has_top = false;
    for (key, _) in &entries {
        if !key.contains('.') {
            has_top = true;
        }
    }
    if has_top {
        for &(ref key, ref snippet) in &entries {
            if !key.contains('.') {
                let padded_name = format!("{:width$}", key, width = name_width);
                let desc = if snippet.desc.is_empty() {
                    &snippet.cmd
                } else {
                    &snippet.desc
                };
                println!("  {} {}", padded_name.cyan(), desc.dimmed());
            }
        }
    }

    // Print sectioned snippets
    for section in &sections {
        println!();
        println!("{}", format!("[{}]", section).dimmed().bold());
        for &(ref key, ref snippet) in &entries {
            if let Some(dot_pos) = key.find('.') {
                let sec = &key[..dot_pos];
                if sec == section.as_str() {
                    let short_name = &key[dot_pos + 1..];
                    let display = format!("{}:{}", section, short_name);
                    let padded_name = format!("{:width$}", display, width = name_width);
                    let desc = if snippet.desc.is_empty() {
                        &snippet.cmd
                    } else {
                        &snippet.desc
                    };
                    println!("  {} {}", padded_name.cyan(), desc.dimmed());
                }
            }
        }
    }

    // Summary
    println!();
    println!(
        "{} {} snippet(s)",
        "→".dimmed(),
        entries.len().to_string().cyan()
    );
}

fn output_json(file: &crate::core::snippet::SnipFile, section_filter: Option<&str>) -> Result<()> {
    let entries: Vec<_> = if let Some(section) = section_filter {
        file.iter()
            .filter(|(key, _)| {
                if key.contains('.') {
                    key.starts_with(&format!("{}.", section))
                } else {
                    key.as_str() == section
                }
            })
            .collect()
    } else {
        file.iter().collect()
    };

    let json_entries: Vec<SnippetEntry> = entries
        .iter()
        .map(|&(ref key, ref snippet)| {
            let (section, name) = if let Some(dot) = key.find('.') {
                (key[..dot].to_string(), key[dot + 1..].to_string())
            } else {
                (String::new(), key.clone())
            };
            SnippetEntry {
                key: key.clone(),
                cmd: snippet.cmd.clone(),
                desc: snippet.desc.clone(),
                section,
                name,
                tags: snippet.tags.clone(),
                has_vars: snippet.has_placeholders(),
            }
        })
        .collect();

    let json = serde_json::to_string_pretty(&json_entries)
        .context("failed to serialize snippets to JSON")?;
    println!("{}", json);
    Ok(())
}

fn output_formatted(
    file: &crate::core::snippet::SnipFile,
    fmt: &str,
    section_filter: Option<&str>,
) {
    let entries: Vec<_> = if let Some(section) = section_filter {
        file.iter()
            .filter(|(key, _)| {
                if key.contains('.') {
                    key.starts_with(&format!("{}.", section))
                } else {
                    key.as_str() == section
                }
            })
            .collect()
    } else {
        file.iter().collect()
    };

    for &(ref key, ref snippet) in &entries {
        let (section, name) = if let Some(dot) = key.find('.') {
            (&key[..dot], &key[dot + 1..])
        } else {
            ("", key.as_str())
        };
        let line = fmt
            .replace("{{key}}", key)
            .replace("{{cmd}}", &snippet.cmd)
            .replace("{{desc}}", &snippet.desc)
            .replace("{{section}}", section)
            .replace("{{name}}", name);

        println!("{}", line);
    }
}

fn run_interactive(file: &crate::core::snippet::SnipFile) -> Result<()> {
    use crate::ui::picker::{PickerItem, PickerResult, pick};

    let items: Vec<PickerItem> = file
        .iter()
        .map(|(key, snippet)| {
            let desc = if snippet.desc.is_empty() {
                snippet.cmd.clone()
            } else {
                snippet.desc.clone()
            };
            PickerItem {
                key: key.clone(),
                display: format!("{} — {}", key, desc),
                detail: desc,
            }
        })
        .collect();

    if items.is_empty() {
        println!("{}", "No snippets to pick from.".dimmed());
        return Ok(());
    }

    match pick(&items)? {
        PickerResult::Selected(key) => {
            // Execute the selected snippet
            if let Some(snippet) = file.get(&key) {
                let cmd = crate::cli::run::resolve_variables(snippet)?;
                println!("{} {}", "→".dimmed(), cmd.dimmed());
                println!();
                crate::core::executor::execute(&cmd)?;
            }
        }
        PickerResult::Cancelled => {
            println!("{}", "Cancelled.".dimmed());
        }
    }

    Ok(())
}

/// Auto-detect project type and offer to create .snips when none exists.
fn auto_init(path: &std::path::Path) -> Result<()> {
    let detected = crate::core::detector::detect_snippets(path);

    if detected.is_empty() {
        println!(
            "{}",
            "No .snips file found in this directory or any parent.".dimmed()
        );
        println!();
        println!("To get started, run:");
        println!("  {}", "snip init".cyan().bold());
        return Ok(());
    }

    // Project detected — offer auto-init
    println!(
        "{}",
        "No .snips file found, but this looks like a project that could use one!"
            .yellow()
    );
    println!();

    let mut by_source: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (section, _name, _cmd, _desc) in &detected {
        *by_source.entry(section.clone()).or_insert(0) += 1;
    }

    let parts: Vec<String> = by_source
        .iter()
        .map(|(src, count)| format!("{} commands from {}", count, src))
        .collect();

    println!(
        "  Detected {} {}",
        detected.len().to_string().cyan(),
        parts.join(", ")
    );
    println!();
    print!("  {} ", "Create .snips with these commands? [Y/n]".bold());
    std::io::stdout().flush()?;

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();

    if input.is_empty() || input == "y" || input == "yes" {
        let snipfile_path = path.join(".snips");
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

        crate::core::snipfile::write_snippets(&snipfile_path, &file)?;
        println!(
            "  {} Created .snips with {} snippet(s)",
            "✓".green(),
            file.len()
        );
        println!();
        println!("  Next: {} to see your snippets", "snip".cyan());
    } else {
        println!("  {}", "Skipped. Run `snip init` when ready.".dimmed());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::core::snippet::Snippet;

    #[test]
    fn test_list_no_snips_file() {
        let tmp = tempfile::tempdir().unwrap();
        let opts = super::ListCmd {
            json: false,
            format: None,
            section: None,
            interactive: false,
        };
        super::run_from(tmp.path(), &opts).unwrap();
    }

    #[test]
    fn test_list_with_snippets() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");

        let mut file = crate::core::snippet::SnipFile::new();
        file.insert("hello", Snippet::new("echo hello").with_desc("Say hello"));
        file.insert("npm.build", Snippet::new("npm run build").with_desc("Build the project"));
        crate::core::snipfile::write_snippets(&snipfile, &file).unwrap();

        let opts = super::ListCmd {
            json: false,
            format: None,
            section: None,
            interactive: false,
        };
        super::run_from(tmp.path(), &opts).unwrap();
    }

    #[test]
    fn test_list_json_output() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");

        let mut file = crate::core::snippet::SnipFile::new();
        file.insert("hello", Snippet::new("echo hello").with_desc("Say hello"));
        crate::core::snipfile::write_snippets(&snipfile, &file).unwrap();

        let opts = super::ListCmd {
            json: true,
            format: None,
            section: None,
            interactive: false,
        };
        super::run_from(tmp.path(), &opts).unwrap();
    }

    #[test]
    fn test_list_format_output() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");

        let mut file = crate::core::snippet::SnipFile::new();
        file.insert("build", Snippet::new("cargo build").with_desc("Build"));
        crate::core::snipfile::write_snippets(&snipfile, &file).unwrap();

        let opts = super::ListCmd {
            json: false,
            format: Some("{{key}}: {{cmd}}".to_string()),
            section: None,
            interactive: false,
        };
        super::run_from(tmp.path(), &opts).unwrap();
    }

    #[test]
    fn test_list_section_filter() {
        let tmp = tempfile::tempdir().unwrap();
        let snipfile = tmp.path().join(".snips");

        let mut file = crate::core::snippet::SnipFile::new();
        file.insert("npm.build", Snippet::new("npm run build"));
        file.insert("npm.test", Snippet::new("npm test"));
        file.insert("docker.up", Snippet::new("docker compose up"));
        crate::core::snipfile::write_snippets(&snipfile, &file).unwrap();

        let opts = super::ListCmd {
            json: false,
            format: None,
            section: Some("npm".to_string()),
            interactive: false,
        };
        super::run_from(tmp.path(), &opts).unwrap();
    }
}