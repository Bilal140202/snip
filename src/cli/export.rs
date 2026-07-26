//! `snip export <NAME>` — Export a snippet to clipboard or stdout.

use std::io::Write as IoWrite;

use anyhow::{bail, Context, Result};
use clap::Args;
use colored::Colorize;

use crate::core::snipfile::{find_snipfile, read_snippets};

/// Export a snippet.
#[derive(Debug, Args)]
pub struct ExportCmd {
    /// Snippet key (fuzzy match supported).
    pub name: String,

    /// Write to stdout instead of clipboard.
    #[arg(long)]
    pub stdout: bool,

    /// Format: "toml" (default) or "cmd" (just the command).
    #[arg(long, default_value = "toml")]
    pub format: String,
}

impl ExportCmd {
    pub fn run(&self) -> Result<()> {
        let cwd = std::env::current_dir().context("Failed to determine current directory")?;
        run_from(&cwd, self)
    }
}

pub(crate) fn run_from(cwd: &std::path::Path, opts: &ExportCmd) -> Result<()> {
    let snipfile_path = match find_snipfile(Some(cwd))? {
        Some(p) => p,
        None => bail!("No .snips file found. Run `snip init` first."),
    };

    let file = read_snippets(&snipfile_path)?;

    // Try exact match first, then fuzzy
    let snippet = if let Some(s) = file.get(&opts.name) {
        s.clone()
    } else {
        let all_keys: Vec<String> = file.iter().map(|(k, _)| k.clone()).collect();
        let matches = crate::core::fuzzy::fuzzy_match(&opts.name, &all_keys);
        if matches.is_empty() {
            let suggestion = crate::cli::completions::suggest_similar(&opts.name, &all_keys);
            if let Some(hint) = suggestion {
                let msg = crate::cli::completions::did_you_mean(hint);
                bail!("Snippet '{}' not found. {}", opts.name, msg);
            } else {
                bail!("Snippet '{}' not found", opts.name);
            }
        }
        if matches.len() == 1 || (matches.len() > 1 && matches[0].score > matches[1].score * 2) {
            file.get(&matches[0].key).unwrap().clone()
        } else {
            println!("{}", "Multiple matches:".dimmed());
            for m in &matches[..5.min(matches.len())] {
                println!("  {}", m.key.cyan());
            }
            bail!("Be more specific");
        }
    };

    let payload = match opts.format.as_str() {
        "toml" => {
            // Build a TOML snippet block: `[name]\ncmd = "..."\ndesc = "..."\n...`
            let mut toml_value = toml::Value::Table(toml::Table::new());
            if let Some(table) = toml_value.as_table_mut() {
                let s_val = toml::Value::try_from(&snippet)
                    .context("failed to serialize snippet to TOML")?;
                table.insert(opts.name.clone(), s_val);
            }
            toml::to_string_pretty(&toml_value)?
        }
        "cmd" => snippet.cmd.clone(),
        other => bail!("Unknown format: '{}' (use 'toml' or 'cmd')", other),
    };

    if opts.stdout {
        print!("{}", payload);
        let _ = std::io::stdout().flush();
        return Ok(());
    }

    // Try to copy to clipboard using one of several clipboard tools.
    let copied = try_copy_to_clipboard(&payload);
    if copied {
        println!(
            "✓ Copied {} snippet '{}' to clipboard ({} bytes)",
            opts.format.cyan(),
            opts.name.cyan(),
            payload.len()
        );
    } else {
        // No clipboard tool available — fall back to stdout with a hint
        println!(
            "{}  No clipboard tool found (install xclip, wl-copy, pbcopy, or clip).",
            "⚠".yellow()
        );
        println!("Falling back to stdout:\n");
        println!("{}", payload);
    }
    Ok(())
}

/// Try to copy text to the system clipboard.
///
/// Attempts (in order): `wl-copy` (Wayland), `xclip` (X11), `xsel` (X11),
/// `pbcopy` (macOS), `clip` (Windows). Returns `true` on success.
fn try_copy_to_clipboard(text: &str) -> bool {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let tools: &[&[&str]] = &[
        &["wl-copy"],
        &["xclip", "-selection", "clipboard"],
        &["xsel", "--clipboard", "--input"],
        &["pbcopy"],
        &["clip"], // Windows
    ];

    for tool in tools {
        let program = tool[0];
        // Quick existence check via `which` crate would be ideal, but spawning
        // and getting NotFound is just as cheap.
        let mut child = match Command::new(program)
            .args(&tool[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(_) => continue,
        };
        if let Some(stdin) = child.stdin.as_mut() {
            // Ignore write errors — some clipboard tools close stdin early
            let _ = stdin.write_all(text.as_bytes());
        }
        match child.wait() {
            Ok(status) => {
                if status.success() {
                    return true;
                }
            }
            Err(_) => continue,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use crate::core::snipfile::write_snippets;
    use crate::core::snippet::{SnipFile, Snippet};

    #[test]
    fn test_export_toml_to_stdout() {
        let tmp = tempfile::tempdir().unwrap();
        let mut file = SnipFile::new();
        file.insert(
            "build",
            Snippet::new("cargo build").with_desc("Build the project"),
        );
        write_snippets(&tmp.path().join(".snips"), &file).unwrap();

        let opts = super::ExportCmd {
            name: "build".to_string(),
            stdout: true,
            format: "toml".to_string(),
        };
        super::run_from(tmp.path(), &opts).unwrap();
    }

    #[test]
    fn test_export_cmd_format_to_stdout() {
        let tmp = tempfile::tempdir().unwrap();
        let mut file = SnipFile::new();
        file.insert("build", Snippet::new("cargo build --release"));
        write_snippets(&tmp.path().join(".snips"), &file).unwrap();

        let opts = super::ExportCmd {
            name: "build".to_string(),
            stdout: true,
            format: "cmd".to_string(),
        };
        super::run_from(tmp.path(), &opts).unwrap();
    }

    #[test]
    fn test_export_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let file = SnipFile::new();
        write_snippets(&tmp.path().join(".snips"), &file).unwrap();

        let opts = super::ExportCmd {
            name: "missing".to_string(),
            stdout: true,
            format: "toml".to_string(),
        };
        let result = super::run_from(tmp.path(), &opts);
        assert!(result.is_err());
    }

    #[test]
    fn test_export_invalid_format() {
        let tmp = tempfile::tempdir().unwrap();
        let mut file = SnipFile::new();
        file.insert("build", Snippet::new("cargo build"));
        write_snippets(&tmp.path().join(".snips"), &file).unwrap();

        let opts = super::ExportCmd {
            name: "build".to_string(),
            stdout: true,
            format: "yaml".to_string(),
        };
        let result = super::run_from(tmp.path(), &opts);
        assert!(result.is_err());
    }
}
