//! `snip search <QUERY>` — Full-text search across snippets.

use std::io::Write as IoWrite;

use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;

use crate::core::snipfile::{find_snipfile, read_snippets};

/// Search snippets by free-text query.
#[derive(Debug, Args)]
pub struct SearchCmd {
    /// The query string (searches key, cmd, desc, and tags).
    pub query: String,

    /// Output as JSON (same shape as `snip list --json`).
    #[arg(long)]
    pub json: bool,
}

impl SearchCmd {
    pub fn run(&self) -> Result<()> {
        let cwd = std::env::current_dir().context("Failed to determine current directory")?;
        run_from(&cwd, self)
    }
}

pub(crate) fn run_from(cwd: &std::path::Path, opts: &SearchCmd) -> Result<()> {
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
    if file.is_empty() {
        if opts.json {
            println!("[]");
        } else {
            println!("{}", "No snippets defined.".dimmed());
        }
        return Ok(());
    }

    let q = opts.query.to_lowercase();
    let mut hits: Vec<(String, crate::core::snippet::Snippet)> = Vec::new();

    for (key, snippet) in file.iter() {
        let hay = format!(
            "{} {} {} {}",
            key,
            snippet.cmd,
            snippet.desc,
            snippet.tags.join(" ")
        )
        .to_lowercase();

        if hay.contains(&q) {
            hits.push((key.clone(), snippet.clone()));
        }
    }

    if hits.is_empty() {
        if opts.json {
            println!("[]");
        } else {
            println!(
                "{}  No snippets match '{}'.",
                "✗".dimmed(),
                opts.query.cyan()
            );
        }
        return Ok(());
    }

    if opts.json {
        let entries: Vec<crate::cli::list::SnippetEntry> = hits
            .iter()
            .map(|(key, snippet)| {
                let (section, name) = if let Some(dot) = key.find('.') {
                    (key[..dot].to_string(), key[dot + 1..].to_string())
                } else {
                    (String::new(), key.clone())
                };
                crate::cli::list::SnippetEntry {
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
        let json = serde_json::to_string_pretty(&entries)
            .context("failed to serialize snippets to JSON")?;
        println!("{}", json);
        return Ok(());
    }

    let max_name_len = hits
        .iter()
        .map(|(k, _)| k.len())
        .max()
        .unwrap_or(16)
        .max(16);

    println!(
        "{} {} match{} for '{}':",
        "→".dimmed(),
        hits.len().to_string().cyan(),
        if hits.len() == 1 { "" } else { "es" },
        opts.query.cyan()
    );
    println!();
    for (key, snippet) in &hits {
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

#[cfg(test)]
mod tests {
    use crate::core::snipfile::write_snippets;
    use crate::core::snippet::{SnipFile, Snippet};

    fn make_tmp_with(snippets: &[(&str, &str, &str)]) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let mut file = SnipFile::new();
        for (key, cmd, desc) in snippets {
            file.insert(*key, Snippet::new(*cmd).with_desc(*desc));
        }
        write_snippets(&tmp.path().join(".snips"), &file).unwrap();
        tmp
    }

    #[test]
    fn test_search_finds_in_key() {
        let tmp = make_tmp_with(&[
            ("build", "cargo build", "Build"),
            ("test", "cargo test", "Run tests"),
            ("deploy.staging", "kubectl apply", "Deploy to staging"),
        ]);

        let opts = super::SearchCmd {
            query: "deploy".to_string(),
            json: false,
        };
        super::run_from(tmp.path(), &opts).unwrap();
    }

    #[test]
    fn test_search_finds_in_cmd() {
        let tmp = make_tmp_with(&[
            ("build", "cargo build", "Build"),
            ("test", "cargo test", "Run tests"),
        ]);

        let opts = super::SearchCmd {
            query: "cargo".to_string(),
            json: false,
        };
        super::run_from(tmp.path(), &opts).unwrap();
    }

    #[test]
    fn test_search_no_match() {
        let tmp = make_tmp_with(&[("build", "cargo build", "Build")]);

        let opts = super::SearchCmd {
            query: "nonexistent".to_string(),
            json: false,
        };
        super::run_from(tmp.path(), &opts).unwrap();
    }

    #[test]
    fn test_search_json_output() {
        let tmp = make_tmp_with(&[("build", "cargo build", "Build")]);

        let opts = super::SearchCmd {
            query: "build".to_string(),
            json: true,
        };
        super::run_from(tmp.path(), &opts).unwrap();
    }

    #[test]
    fn test_search_case_insensitive() {
        let tmp = make_tmp_with(&[("build", "cargo build", "Build")]);

        let opts = super::SearchCmd {
            query: "BUILD".to_string(),
            json: false,
        };
        super::run_from(tmp.path(), &opts).unwrap();
    }
}
