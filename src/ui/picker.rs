//! Interactive fuzzy picker for terminal.

use std::io::{self, Write};

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

/// Show an interactive fuzzy picker and return the selected key.
///
/// This is a placeholder implementation. The full TUI picker will track
/// cursor position, support arrow keys, and have better rendering.
pub fn pick(items: &[PickerItem]) -> io::Result<PickerResult> {
    // If not interactive, fall back to listing.
    if !is_stdout_tty() {
        return pick_fallback(items);
    }

    let _raw = crossterm::terminal::enable_raw_mode();
    let _cleanup = RawModeGuard;

    let mut query = String::new();
    let matcher = SkimMatcherV2::default();

    loop {
        let filtered: Vec<&PickerItem> = items
            .iter()
            .filter(|item| {
                let score = matcher.fuzzy_match(&item.display, &query).unwrap_or(0);
                score > 0 || query.is_empty()
            })
            .collect();

        render_picker_ui(&query, &filtered);

        if let Event::Key(key) = read_key()? {
            match key.code {
                KeyCode::Enter => {
                    if let Some(first) = filtered.first() {
                        return Ok(PickerResult::Selected(first.key.clone()));
                    }
                }
                KeyCode::Esc | KeyCode::Char('q') => {
                    return Ok(PickerResult::Cancelled);
                }
                KeyCode::Backspace => {
                    query.pop();
                }
                KeyCode::Char(c) => {
                    query.push(c);
                }
                KeyCode::Up | KeyCode::Down => {
                    // Full implementation would track cursor position
                }
                _ => {}
            }
        }
    }
}

fn render_picker_ui(query: &str, items: &[&PickerItem]) {
    let mut stdout = io::stdout().lock();
    let _ = crossterm::execute!(
        stdout,
        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
        crossterm::cursor::MoveTo(0, 0)
    );

    let _ = writeln!(stdout, "{}> {}", ">".cyan(), query);

    for item in items.iter().take(10) {
        let _ = writeln!(stdout, "  {} — {}", item.key.green().bold(), item.detail);
    }

    if items.is_empty() {
        let _ = writeln!(stdout, "  (no matches)");
    }

    let _ = writeln!(stdout, "");
    let _ = stdout.flush();
}

/// Fallback non-interactive picker (prints list and returns Cancelled).
fn pick_fallback(items: &[PickerItem]) -> io::Result<PickerResult> {
    for item in items {
        println!("  {} — {}", item.key.green().bold(), item.detail);
    }
    Ok(PickerResult::Cancelled)
}

struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

fn is_stdout_tty() -> bool {
    // Use crossterm's IsTty trait
    use crossterm::tty::IsTty;
    std::io::stdout().is_tty()
}

// Thin wrappers to avoid re-exporting crossterm event types directly.
mod event_shim {
    pub use crossterm::event::{KeyCode, KeyEvent, read as read_key, Event};
}

use event_shim::{Event, KeyCode, read_key};