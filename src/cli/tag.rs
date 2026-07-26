//! `snip tag <TAG>` — List snippets by tag, optionally run one.

use std::io::Write as IoWrite;

use anyhow::{bail, Context, Result};
use clap::Args;
use colored::Colorize;

use crate::core::snipfile::{find_snipfile, read_snippets};

/// Filter / act on snippets by tag.
#[derive(Debug, Args)]
pub struct TagCmd {
    /// The tag to filter by.
    pub tag: String,

    /// Run a snippet from the filtered set by key (fuzzy match).
    #[arg(long)]
    pub run: Option<String>,

    /// Output as JSON.
    #[arg(long)]
    pub json: bool,
}

impl TagCmd {
    pub fn run(&self) -> Result<()> {
        let cwd = std::env::current_dir().context("Failed to determine current directory")?;
        run_from(&cwd, self)
    }
}

pub(crate) fn run_from(cwd: &std::path::Path, opts: &TagCmd) -> Result<()> {
    let snipfile_path = match find_snipfile(Some(cwd))? {
        Some(p) => p,
        None => {
            if opts.json {
                println!("[]");
            } else {
                println!("{}", "No .snips file found.".dimmed());
            }
            return Ok(());
        }
    };

    let file = read_snippets(&snipfile_path)?;

    let tag = opts.tag.to_lowercase();
    let filtered: Vec<(&String, &crate::core::snippet::Snippet)> = file
        .iter()
        .filter(|(_, s)| s.tags.iter().any(|t| t.to_lowercase() == tag))
        .map(|(k, s)| (k, s))
        .collect();

    if filtered.is_empty() {
        if opts.json {
            println!("[]");
        } else {
            println!(
                "{}  No snippets tagged '{}'.",
                "✗".dimmed(),
                opts.tag.cyan()
            );
            // Suggest existing tags
            let all_tags: std::collections::BTreeSet<String> = file
                .iter()
                .flat_map(|(_, s)| s.tags.iter().cloned())
                .collect();
            if !all_tags.is_empty() {
                println!();
                println!("Available tags:");
                for t in &all_tags {
                    println!("  {}", t.cyan());
                }
            }
        }
        return Ok(());
    }

    // Run mode: pick a snippet from the filtered set
    if let Some(ref query) = opts.run {
        return run_filtered(&filtered, query);
    }

    if opts.json {
        let entries: Vec<crate::cli::list::SnippetEntry> = filtered
            .iter()
            .map(|&(key, snippet)| {
                let (section, name) = if let Some(dot) = key.find('.') {
                    (key[..dot].to_string(), key[dot + 1..].to_string())
                } else {
                    (String::new(), key.to_string())
                };
                crate::cli::list::SnippetEntry {
                    key: key.to_string(),
                    cmd: snippet.cmd.clone(),
                    desc: snippet.desc.clone(),
                    section,
                    name,
                    tags: snippet.tags.clone(),
                    has_vars: snippet.has_placeholders(),
                }
            })
            .collect();
        let json = serde_json::to_string_pretty(&entries)
            .context("failed to serialize snippets to JSON")?;
        println!("{}", json);
        return Ok(());
    }

    // Default: human-readable list
    let max_name_len = filtered
        .iter()
        .map(|(k, _)| k.len())
        .max()
        .unwrap_or(16)
        .max(16);

    println!(
        "{} {} snippet{} tagged '{}':",
        "→".dimmed(),
        filtered.len().to_string().cyan(),
        if filtered.len() == 1 { "" } else { "s" },
        opts.tag.cyan()
    );
    println!();
    for (key, snippet) in &filtered {
        let padded = format!("{:width$}", key, width = max_name_len);
        let desc = if snippet.desc.is_empty() {
            &snippet.cmd
        } else {
            &snippet.desc
        };
        println!("  {} {}", padded.cyan(), desc.dimmed());
    }
    let _ = std::io::stdout().flush();
    Ok(())
}

fn run_filtered(filtered: &[(&String, &crate::core::snippet::Snippet)], query: &str) -> Result<()> {
    // Exact key match within the filtered set
    if let Some((_, snippet)) = filtered.iter().find(|(k, _)| k.as_str() == query) {
        let cmd = crate::cli::run::resolve_variables(snippet)?;
        println!("{} {}", "→".dimmed(), cmd.dimmed());
        println!();
        return crate::core::executor::execute(&cmd);
    }

    // Fuzzy match within the filtered set
    let keys: Vec<String> = filtered.iter().map(|(k, _)| (*k).clone()).collect();
    let matches = crate::core::fuzzy::fuzzy_match(query, &keys);

    if matches.is_empty() {
        let suggestion = crate::cli::completions::suggest_similar(query, &keys);
        if let Some(hint) = suggestion {
            let msg = crate::cli::completions::did_you_mean(hint);
            bail!("No snippet matching '{}' with this tag. {}", query, msg);
        } else {
            bail!("No snippet matching '{}' with this tag", query);
        }
    }

    if matches.len() == 1 || (matches.len() > 1 && matches[0].score > matches[1].score * 2) {
        let key = &matches[0].key;
        let (_, snippet) = filtered
            .iter()
            .find(|(k, _)| *k == key)
            .expect("fuzzy match must come from filtered set");
        let cmd = crate::cli::run::resolve_variables(snippet)?;
        println!("{} {}", "→".dimmed(), cmd.dimmed());
        println!();
        return crate::core::executor::execute(&cmd);
    }

    println!("{}", "Multiple matches:".dimmed());
    for m in &matches[..5.min(matches.len())] {
        let desc = filtered
            .iter()
            .find(|(k, _)| *k == &m.key)
            .map(|(_, s)| {
                if s.desc.is_empty() {
                    s.cmd.clone()
                } else {
                    s.desc.clone()
                }
            })
            .unwrap_or_default();
        println!("  {} {}", m.key.cyan(), desc.dimmed());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::core::snipfile::write_snippets;
    use crate::core::snippet::{SnipFile, Snippet};

    fn make_tmp_with(snippets: &[(&str, &str, &str, &[&str])]) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let mut file = SnipFile::new();
        for (key, cmd, desc, tags) in snippets {
            let s = Snippet::new(*cmd)
                .with_desc(*desc)
                .with_tags(tags.iter().map(|t| t.to_string()).collect());
            file.insert(*key, s);
        }
        write_snippets(&tmp.path().join(".snips"), &file).unwrap();
        tmp
    }

    #[test]
    fn test_tag_filters_by_tag() {
        let tmp = make_tmp_with(&[
            ("build", "cargo build", "Build", &["ci", "qa"]),
            ("test", "cargo test", "Test", &["qa"]),
            ("deploy", "kubectl apply", "Deploy", &["release"]),
        ]);

        let opts = super::TagCmd {
            tag: "qa".to_string(),
            run: None,
            json: false,
        };
        super::run_from(tmp.path(), &opts).unwrap();
    }

    #[test]
    fn test_tag_no_match_lists_available_tags() {
        let tmp = make_tmp_with(&[
            ("build", "cargo build", "Build", &["ci"]),
            ("test", "cargo test", "Test", &["qa"]),
        ]);

        let opts = super::TagCmd {
            tag: "nonexistent".to_string(),
            run: None,
            json: false,
        };
        super::run_from(tmp.path(), &opts).unwrap();
    }

    #[test]
    fn test_tag_case_insensitive() {
        let tmp = make_tmp_with(&[("build", "cargo build", "Build", &["CI"])]);

        let opts = super::TagCmd {
            tag: "ci".to_string(),
            run: None,
            json: false,
        };
        super::run_from(tmp.path(), &opts).unwrap();
    }

    #[test]
    fn test_tag_json_output() {
        let tmp = make_tmp_with(&[("build", "cargo build", "Build", &["qa"])]);

        let opts = super::TagCmd {
            tag: "qa".to_string(),
            run: None,
            json: true,
        };
        super::run_from(tmp.path(), &opts).unwrap();
    }
}
