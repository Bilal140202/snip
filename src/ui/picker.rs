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
/// Format: `description\tkey\n` piped to fzf with `--with-nth=1 --nth=1 --delimiter=$'\t'`
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

    let mut child = Command::new("fzf")
        .arg("--with-nth=1")
        .arg("--nth=1")
        .arg("--delimiter=\t")
        .arg("--ansi")
        .arg("--prompt=snip> ")
        .arg("--header=↑/↓ navigate, enter select, esc cancel")
        .arg("--height=~40%")
        .arg("--reverse")
        .arg("--bind=ctrl-e:accept")
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
                    KeyCode::Down => {
                        if cursor < filtered.len().saturating_sub(1) {
                            cursor += 1;
                        }
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
    use crossterm::terminal::Clear;
    use crossterm::cursor::MoveTo;

    let mut stdout = io::stdout().lock();
    let _ = crossterm::execute!(
        stdout,
        Clear(ClearType::All),
        MoveTo(0, 0)
    );

    let _ = writeln!(stdout, "{}> {}", ">".cyan(), query);

    let visible_count = 10;
    let start = cursor.saturating_sub(3);
    let end = (start + visible_count).min(items.len());

    for (i, idx) in (start..end).enumerate() {
        let item = &items[idx];
        let marker = if idx == cursor { "❯".green().to_string() } else { " ".to_string() };
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
        let _ = writeln!(stdout, "  {} {} {}", marker, key, desc);
    }

    if items.is_empty() {
        let _ = writeln!(stdout, "  {}", "(no matches)".dimmed());
    }

    let _ = writeln!(
        stdout,
        "\n  {} {}/{}",
        "↓/↑ navigate, enter select, q quit".dimmed(),
        items.len(),
        items.len()
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
    println!("\n  {}", "Install fzf for interactive selection: https://github.com/junegunn/fzf".dimmed());
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
    pub use crossterm::event::{KeyCode, read as read_key, Event};
}

#[cfg(feature = "picker")]
use event_shim::{Event, KeyCode, read_key};

use crossterm::terminal::ClearType;