//! Interactive fuzzy picker — shells out to fzf when available, falls back to built-in.

use std::io::{self, Write};
use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use colored::Colorize;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

/// A single item in the picker.
#[derive(Debug, Clone)]
pub struct PickerItem {
    pub key: String,
    pub display: String,
    pub detail: String,
    /// Optional full command for the preview pane.
    #[doc(hidden)]
    pub cmd: String,
    /// Optional tags for the preview pane.
    #[doc(hidden)]
    pub tags: Vec<String>,
    /// Optional variable names for the preview pane.
    #[doc(hidden)]
    pub vars: Vec<String>,
}

impl PickerItem {
    /// Build a picker item from a key, display string, and detail.
    /// Keeps backward compatibility — cmd/tags/vars default to empty.
    pub fn new(
        key: impl Into<String>,
        display: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            key: key.into(),
            display: display.into(),
            detail: detail.into(),
            cmd: String::new(),
            tags: Vec::new(),
            vars: Vec::new(),
        }
    }

    /// Attach the underlying command (shown in the preview pane).
    pub fn with_cmd(mut self, cmd: impl Into<String>) -> Self {
        self.cmd = cmd.into();
        self
    }

    /// Attach tags (shown in the preview pane).
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Attach variable names (shown in the preview pane).
    pub fn with_vars(mut self, vars: Vec<String>) -> Self {
        self.vars = vars;
        self
    }
}

/// Result of the interactive picker.
#[derive(Debug, Clone)]
pub enum PickerResult {
    Selected(String),
    Cancelled,
}

/// Check if fzf is available on PATH.
pub fn fzf_available() -> bool {
    which::which("fzf").is_ok()
}

/// Pick an item using fzf if available, otherwise use the built-in picker.
pub fn pick(items: &[PickerItem]) -> Result<PickerResult> {
    if fzf_available() {
        pick_fzf(items)
    } else {
        pick_builtin(items)
    }
}

/// Pick using fzf via subprocess pipe.
///
/// Format: `description\tkey\n` piped to fzf with `--with-nth=1 --nth=1 --delimiter=$'\t'`.
/// When items carry `cmd`/`tags`/`vars` metadata, a preview pane is enabled
/// that shows the full command + tags + variables for the highlighted entry.
fn pick_fzf(items: &[PickerItem]) -> Result<PickerResult> {
    // Build the input for fzf: "description\tkey\n"
    let input: String = items
        .iter()
        .map(|item| {
            let desc = if item.detail.is_empty() {
                &item.display
            } else {
                &item.detail
            };
            format!("{}\t{}\n", desc, item.key)
        })
        .collect();

    // Build a side-channel preview file: each line is "key\tcmd\ttags\tvars"
    // so the preview command can look up the highlighted item by key.
    let preview_data: String = items
        .iter()
        .filter(|i| !i.cmd.is_empty())
        .map(|i| {
            format!(
                "{}\t{}\t{}\t{}\n",
                i.key,
                i.cmd.replace('\n', " ⏎ ").replace('\t', " "),
                i.tags.join(","),
                i.vars.join(",")
            )
        })
        .collect();

    let mut cmd = Command::new("fzf");
    cmd.arg("--with-nth=1")
        .arg("--nth=1")
        .arg("--delimiter=\t")
        .arg("--ansi")
        .arg("--prompt=snip> ")
        .arg("--header=↑/↓ navigate · enter select · esc cancel · type to filter")
        .arg("--height=~50%")
        .arg("--reverse")
        .arg("--bind=ctrl-e:accept");

    // Enable preview pane if any item has cmd metadata
    if !preview_data.is_empty() {
        // Write preview data to a temp file so the preview command can grep it
        let preview_file =
            std::env::temp_dir().join(format!("snip-fzf-preview-{}.tsv", std::process::id()));
        if std::fs::write(&preview_file, &preview_data).is_ok() {
            let preview_path = preview_file.to_string_lossy().replace('\'', "'\\''");
            // fzf replaces {1} with the first delimiter-separated field (the key)
            let preview_cmd = format!(
                "grep -F -m1 \"^$(printf '%s\\t' {{1}})\" '{}' 2>/dev/null | awk -F'\\t' '{{ \
                    printf \"\\n  \\033[36mcmd:\\033[0m %s\\n\", $2; \
                    if ($3 != \"\") printf \"  \\033[36mtags:\\033[0m %s\\n\", $3; \
                    if ($4 != \"\") printf \"  \\033[36mvars:\\033[0m %s\\n\", $4; \
                }}'",
                preview_path
            );
            cmd.arg(format!("--preview={}", preview_cmd));
            cmd.arg("--preview-window=down:3:wrap");

            // Schedule cleanup of the temp file (best-effort, after fzf exits)
            let preview_file_clone = preview_file.clone();
            // Spawn a thread that waits ~30s then deletes — gives fzf time to finish reading
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(30));
                let _ = std::fs::remove_file(&preview_file_clone);
            });
        }
    }

    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to spawn fzf")?;

    // Write input to fzf's stdin
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(input.as_bytes())
            .context("failed to write to fzf stdin")?;
    }

    let output = child
        .wait_with_output()
        .context("failed to read fzf output")?;

    if !output.status.success() {
        return Ok(PickerResult::Cancelled);
    }

    let selection = String::from_utf8_lossy(&output.stdout);
    let selection = selection.trim();

    if selection.is_empty() {
        return Ok(PickerResult::Cancelled);
    }

    // Extract the key from "description\tkey" format
    // fzf outputs the full line, so we need to extract after the tab
    if let Some(tab_pos) = selection.find('\t') {
        let key = selection[tab_pos + 1..].trim();
        Ok(PickerResult::Selected(key.to_string()))
    } else {
        // Fallback: the whole line is the key
        Ok(PickerResult::Selected(selection.to_string()))
    }
}

/// Built-in fallback picker when fzf is not available.
///
/// This is a simplified version that still provides a usable interactive experience.
fn pick_builtin(items: &[PickerItem]) -> Result<PickerResult> {
    // If not a TTY, fall back to listing
    if !is_stdout_tty() {
        return pick_fallback(items);
    }

    #[cfg(feature = "picker")]
    {
        let _raw = crossterm::terminal::enable_raw_mode();
        let _cleanup = RawModeGuard;

        let mut query = String::new();
        let matcher = SkimMatcherV2::default();
        let mut cursor: usize = 0;

        loop {
            let filtered: Vec<&PickerItem> = items
                .iter()
                .filter(|item| {
                    let score = matcher.fuzzy_match(&item.display, &query).unwrap_or(0);
                    score > 0 || query.is_empty()
                })
                .collect();

            cursor = cursor.min(filtered.len().saturating_sub(1));

            render_builtin_ui(&query, &filtered, cursor);

            if let Event::Key(key) = read_key()? {
                match key.code {
                    KeyCode::Enter => {
                        if let Some(item) = filtered.get(cursor) {
                            return Ok(PickerResult::Selected(item.key.clone()));
                        }
                        if let Some(first) = filtered.first() {
                            return Ok(PickerResult::Selected(first.key.clone()));
                        }
                    }
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('c') => {
                        return Ok(PickerResult::Cancelled);
                    }
                    KeyCode::Backspace => {
                        query.pop();
                        if cursor > 0 && cursor >= filtered.len() {
                            cursor = filtered.len().saturating_sub(1);
                        }
                    }
                    KeyCode::Char(c) => {
                        query.push(c);
                        cursor = 0;
                    }
                    KeyCode::Up => {
                        cursor = cursor.saturating_sub(1);
                    }
                    KeyCode::Down if cursor < filtered.len().saturating_sub(1) => {
                        cursor += 1;
                    }
                    _ => {}
                }
            }
        }
    }

    #[cfg(not(feature = "picker"))]
    {
        pick_fallback(items)
    }
}

#[cfg(feature = "picker")]
fn render_builtin_ui(query: &str, items: &[&PickerItem], cursor: usize) {
    use crossterm::cursor::MoveTo;
    use crossterm::terminal::Clear;

    let mut stdout = io::stdout().lock();
    let _ = crossterm::execute!(stdout, Clear(ClearType::All), MoveTo(0, 0));

    // ── Search bar ───────────────────────────────────────────────────────
    let _ = writeln!(stdout, "{} {}", ">".cyan().bold(), query);

    let visible_count = 8;
    let start = cursor.saturating_sub(2);
    let end = (start + visible_count).min(items.len());

    // ── List pane ────────────────────────────────────────────────────────
    for (idx, item) in items.iter().enumerate().skip(start).take(end - start) {
        let marker = if idx == cursor {
            "❯".green().bold().to_string()
        } else {
            " ".to_string()
        };
        let key = if idx == cursor {
            item.key.green().bold().to_string()
        } else {
            item.key.clone()
        };
        let desc = if item.detail.is_empty() {
            item.display.dimmed().to_string()
        } else {
            item.detail.dimmed().to_string()
        };
        let tags_str = if item.tags.is_empty() {
            String::new()
        } else {
            format!(
                " {}",
                item.tags
                    .iter()
                    .map(|t| format!("#{}", t))
                    .collect::<Vec<_>>()
                    .join(" ")
                    .purple()
                    .dimmed()
            )
        };
        let _ = writeln!(stdout, "  {} {} {}{}", marker, key, desc, tags_str);
    }

    if items.is_empty() {
        let _ = writeln!(
            stdout,
            "  {}",
            "(no matches — try a shorter query)".dimmed()
        );
    }

    // ── Preview pane (selected item only) ────────────────────────────────
    if let Some(selected) = items.get(cursor) {
        let _ = writeln!(stdout);
        let _ = writeln!(stdout, "{}", "─".repeat(60).dimmed());
        if !selected.cmd.is_empty() {
            let _ = writeln!(stdout, "  {} {}", "cmd:".cyan().bold(), selected.cmd);
        }
        if !selected.tags.is_empty() {
            let _ = writeln!(
                stdout,
                "  {} {}",
                "tags:".cyan().bold(),
                selected
                    .tags
                    .iter()
                    .map(|t| format!("#{}", t))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
        }
        if !selected.vars.is_empty() {
            let _ = writeln!(
                stdout,
                "  {} {}",
                "vars:".cyan().bold(),
                selected
                    .vars
                    .iter()
                    .map(|v| format!("{{{{{}}}}}", v))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
        }
    }

    // ── Footer ───────────────────────────────────────────────────────────
    let _ = writeln!(
        stdout,
        "\n  {} {}/{} · {}",
        "↓/↑ navigate · enter select · q quit".dimmed(),
        items.len(),
        items.len(),
        if items.is_empty() {
            "no matches"
        } else {
            "type to filter"
        }
        .dimmed()
    );
    let _ = stdout.flush();
}

/// Fallback non-interactive picker (prints list and returns Cancelled).
fn pick_fallback(items: &[PickerItem]) -> Result<PickerResult> {
    for item in items {
        let desc = if item.detail.is_empty() {
            &item.display
        } else {
            &item.detail
        };
        println!("  {} — {}", item.key.green().bold(), desc);
    }
    println!(
        "\n  {}",
        "Install fzf for interactive selection: https://github.com/junegunn/fzf".dimmed()
    );
    Ok(PickerResult::Cancelled)
}

#[cfg(feature = "picker")]
struct RawModeGuard;

#[cfg(feature = "picker")]
impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

fn is_stdout_tty() -> bool {
    use crossterm::tty::IsTty;
    std::io::stdout().is_tty()
}

// Thin wrappers to avoid re-exporting crossterm event types directly.
#[cfg(feature = "picker")]
mod event_shim {
    pub use crossterm::event::{read as read_key, Event, KeyCode};
}

#[cfg(feature = "picker")]
use event_shim::{read_key, Event, KeyCode};

use crossterm::terminal::ClearType;
