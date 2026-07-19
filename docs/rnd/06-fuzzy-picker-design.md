# Fuzzy Picker Design Document

> Date: 2025-01
> Status: **Approved** — Ready for implementation
> Depends on: `04-fzf-analysis.md`, `02-pet-analysis.md`

---

## Table of Contents

1. [Decision: fzf Shell-Out vs Built-In Picker](#1-decision-fzf-shell-out-vs-built-in-picker)
2. [Fzf Shell-Out Integration (Primary Path)](#2-fzf-shell-out-integration-primary-path)
3. [Built-In Picker Architecture (Fallback Path)](#3-built-in-picker-architecture-fallback-path)
4. [Scoring Algorithm Improvements](#4-scoring-algorithm-improvements)
5. [Performance Targets](#5-performance-targets)
6. [Implementation Phases](#6-implementation-phases)

---

## 1. Decision: fzf Shell-Out vs Built-In Picker

### 1.1 The Verdict: Hybrid — fzf First, Built-In Fallback

**Primary strategy**: Shell out to `fzf` when available. Fall back to a built-in picker when fzf is not found.

### 1.2 Decision Matrix

| Criterion | fzf Shell-Out | Built-In | Hybrid |
|-----------|:-------------:|:--------:|:------:|
| Matching quality (out of box) | ★★★★★ | ★★★☆☆ | ★★★★★ |
| Zero external deps | ★★☆☆☆ | ★★★★★ | ★★★★☆ |
| Implementation effort | ★★★★★ (days) | ★★☆☆☆ (weeks) | ★★★★☆ |
| Users already know keybindings | ★★★★★ | ★★☆☆☆ | ★★★★★ |
| Works on CI/headless | ★★★★★ | ★★★★★ | ★★★★★ |
| Works without fzf installed | ★☆☆☆☆ | ★★★★★ | ★★★★★ |
| Preview window, multi-select | ★★★★★ | ★☆☆☆☆ | ★★★★★ |
| Total UX quality | ★★★★★ | ★★★☆☆ | ★★★★★ |

### 1.3 Why Not Pure Built-In?

1. **fzf is pre-installed on ~90% of dev machines** (Homebrew, apt, pacman, nix, conda). The `which = "6"` crate is already in `Cargo.toml`.
2. **fzf has 10+ years of algorithm refinement** — the Smith-Waterman V2 algorithm with word-boundary bonuses, consecutive-match bonuses, gap penalties, and camelCase detection. Reimplementing this to fzf-quality takes weeks and still won't match edge-case polish.
3. **Users already know fzf's keybindings** (`Ctrl+J/K`, `Tab` multi-select, `Ctrl+A` select-all, `?` help). A custom picker breaks muscle memory.
4. **fzf handles the hard problems for free**: SIGWINCH resize, alt-screen buffer, mouse support, tmux detection, ANSI passthrough, preview windows.

### 1.4 Why Not Pure fzf?

1. **Breaks the "single binary, zero deps" story**. Users on minimal servers, Docker scratch containers, or Windows without WSL would have no picker.
2. **Non-interactive contexts** (piped output, CI, `--filter` mode) should work without fzf.
3. **Pet's approach** (`exec.Command("sh", "-c", command)`) silently fails on missing fzf — terrible UX. We need a graceful fallback.

### 1.5 The Hybrid Protocol

```
┌──────────────┐     which fzf?     ┌──────────────┐
│   snip run   │ ──── found? ────▶ │  fzf picker  │
│  (interactive)│                    │  (primary)   │
└──────────────┘                     └──────┬───────┘
       │                                     │
       │  not found?                         │ exit 130?
       ▼                                     ▼
┌──────────────┐                     ┌──────────────┐
│ built-in     │                     │  Cancelled   │
│ picker       │                     │  (None)      │
│ (fallback)   │                     └──────────────┘
└──────────────┘
```

**Detection logic**:

```rust
fn find_fzf() -> Option<PathBuf> {
    // 1. Check SNIP_FZF env var (user override)
    if let Ok(path) = std::env::var("SNIP_FZF") {
        let p = PathBuf::from(&path);
        if p.is_file() {
            return Some(p);
        }
    }

    // 2. Use `which` crate to find fzf on PATH
    which::which("fzf").ok()
}
```

---

## 2. Fzf Shell-Out Integration (Primary Path)

### 2.1 Item Format

Items are formatted as **tab-separated lines** with the key (machine-readable identifier) in the last column:

```
{description}\t{tags}\t{key}
```

Example stdin:

```
Deploy to staging	deploy, stag	deploy-staging
Deploy to production	deploy, prod	deploy-prod
Run database migrations	db, migrate	run-db-migrate
Build release binary	build, release	build-release
```

**Why tab-separated?**
- Tabs are the standard fzf delimiter — they don't appear in snippet names or descriptions
- `--with-nth=1` restricts the search to column 1 (description) while the full line (including key) is returned on selection
- The key column is invisible to the user but recoverable via `split('\t').last()`

### 2.2 The Core Pipe Function

```rust
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Result of the fzf picker, with the action taken.
#[derive(Debug, Clone)]
pub enum FzfResult {
    /// User selected an item. Contains the raw fzf output line.
    Selected(String),
    /// User cancelled (Esc, Ctrl+C).
    Cancelled,
    /// fzf not found — caller should fall back to built-in picker.
    NotFound,
}

/// Launch fzf as an interactive picker.
///
/// - `items`: tab-separated lines to pipe to stdin
/// - `query`: pre-filled search query (e.g., "d" from `snip run d`)
/// - `fzf_path`: path to the fzf binary
/// - `prompt`: the prompt string shown to the left of the query
pub fn fzf_pick(
    items: &[String],
    query: &str,
    fzf_path: &PathBuf,
    prompt: &str,
) -> std::io::Result<FzfResult> {
    let mut child = Command::new(fzf_path)
        // --- Layout ---
        .arg("--height=50%")
        .arg("--reverse")
        .arg("--border=rounded")
        .arg(format!("--prompt={}> ", prompt))
        .arg("--pointer=▶")
        .arg("--marker=✓")
        // --- Column control ---
        .arg("--delimiter=\t")
        .arg("--with-nth=1,2")          // Search & display columns 1-2 (desc + tags)
        .arg("--nth=1")                  // Only fuzzy-search column 1 (description)
        // --- Preview ---
        .arg("--preview=echo {3}")       // Show the key in preview (could be command)
        .arg("--preview-window=down:3:wrap:border-rounded")
        // --- Behavior ---
        .arg("--cycle")                  // Wrap around at top/bottom
        .arg("--info=inline")            // Match count inline
        .arg("--layout=reverse")
        // --- Pre-fill query ---
        .arg(format!("--query={}", query))
        // --- I/O ---
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())        // Let fzf render to our stderr/tty
        .spawn()?;

    // Write items to stdin
    {
        let stdin = child.stdin.as_mut().expect("stdin available after spawn");
        for item in items {
            writeln!(stdin, "{}", item).map_err(|e| {
                std::io::Error::new(e.kind(), format!("failed to write to fzf stdin: {e}"))
            })?;
        }
    }
    // Drop stdin to signal EOF — fzf will process all input
    drop(child.stdin.take());

    // Wait for fzf to exit
    let output = child.wait_with_output()?;

    match output.status.code() {
        Some(0) => {
            // User selected something
            let line = String::from_utf8_lossy(&output.stdout);
            let trimmed = line.trim().to_string();
            if trimmed.is_empty() {
                Ok(FzfResult::Cancelled)
            } else {
                Ok(FzfResult::Selected(trimmed))
            }
        }
        Some(130) => {
            // Ctrl+C — user cancelled
            Ok(FzfResult::Cancelled)
        }
        Some(1) => {
            // No match found (can happen with --select-1/--exit-0)
            Ok(FzfResult::Cancelled)
        }
        Some(2) => {
            // fzf error
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "fzf reported an error (exit code 2)",
            ))
        }
        _ => Ok(FzfResult::Cancelled),
    }
}
```

### 2.3 Non-Interactive Filter Mode

For `snip run <partial>` (where the user provides a query on the CLI without an interactive picker), use fzf's `--filter` mode:

```rust
/// Non-interactive fuzzy filter. Returns the best match without launching a TUI.
pub fn fzf_filter(
    items: &[String],
    query: &str,
    fzf_path: &PathBuf,
) -> std::io::Result<Option<String>> {
    if query.is_empty() {
        return Ok(items.first().cloned());
    }

    let mut child = Command::new(fzf_path)
        .arg("--filter")
        .arg(query)
        .arg("--delimiter=\t")
        .arg("--nth=1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    {
        let stdin = child.stdin.as_mut().expect("stdin available");
        for item in items {
            writeln!(stdin, "{}", item)?;
        }
    }
    drop(child.stdin.take());

    let output = child.wait_with_output()?;

    if output.status.success() {
        let result = String::from_utf8_lossy(&output.stdout);
        let first_line = result.lines().next();
        Ok(first_line.map(|s| s.to_string()))
    } else {
        Ok(None)
    }
}
```

### 2.4 Multi-Action Picker with `--expect`

For commands that need multiple actions (run, edit, copy, delete), use `--expect`:

```rust
/// Result from a multi-action picker.
#[derive(Debug, Clone)]
pub enum PickerAction {
    /// Enter — run the snippet. Contains the selected line.
    Run(String),
    /// Ctrl+E — edit the snippet. Contains the key.
    Edit(String),
    /// Ctrl+Y — copy to clipboard. Contains the selected line.
    Copy(String),
    /// Cancelled.
    Cancel,
}

pub fn fzf_pick_with_actions(
    items: &[String],
    query: &str,
    fzf_path: &PathBuf,
    prompt: &str,
) -> std::io::Result<PickerAction> {
    let output = Command::new(fzf_path)
        .arg("--height=50%")
        .arg("--reverse")
        .arg("--border=rounded")
        .arg(format!("--prompt={}> ", prompt))
        .arg("--delimiter=\t")
        .arg("--with-nth=1,2")
        .arg("--nth=1")
        .arg("--cycle")
        .arg("--expect=ctrl-e,ctrl-y")   // These keys print the key name before the line
        .arg(format!("--query={}", query))
        .arg("--bind=enter:accept")       // Enter prints nothing (default action)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?
        .stdin
        .take()
        .and_then(|mut stdin| {
            for item in items {
                writeln!(stdin, "{}", item)?;
            }
            Ok(())
        })
        .and_then(|()| {
            // Re-join to get output — we need to restructure this
            // See the full implementation below for the proper pattern
            Ok(())
        });

    // Full implementation:
    let mut child = Command::new(fzf_path)
        .arg("--height=50%")
        .arg("--reverse")
        .arg("--border=rounded")
        .arg(format!("--prompt={}> ", prompt))
        .arg("--delimiter=\t")
        .arg("--with-nth=1,2")
        .arg("--nth=1")
        .arg("--cycle")
        .arg("--expect=ctrl-e,ctrl-y")
        .arg(format!("--query={}", query))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;

    {
        let stdin = child.stdin.as_mut().expect("stdin available");
        for item in items {
            writeln!(stdin, "{}", item)?;
        }
    }
    drop(child.stdin.take());

    let output = child.wait_with_output()?;
    let result = String::from_utf8_lossy(&output.stdout).to_string();

    // With --expect, output is: "key\nline" or just "line" for Enter
    let mut lines = result.lines();
    match lines.next() {
        Some("ctrl-e") => {
            let line = lines.next().unwrap_or("");
            let key = line.split('\t').last().unwrap_or("").to_string();
            Ok(PickerAction::Edit(key))
        }
        Some("ctrl-y") => {
            let line = lines.next().unwrap_or("");
            Ok(PickerAction::Copy(line.to_string()))
        }
        Some(line) if !line.is_empty() => {
            Ok(PickerAction::Run(line.to_string()))
        }
        _ => Ok(PickerAction::Cancel),
    }
}
```

### 2.5 Pre-Fill Query from CLI Argument

When the user types `snip run d`, we pre-fill the fzf query with `"d"`:

```rust
// In the CLI handler:
let query = match args.command.as_deref() {
    Some(name) => name.to_string(),
    None => String::new(),
};

// The query is passed via --query to fzf, so the user sees it
// pre-typed and can continue typing or just press Enter.
```

### 2.6 FZF_DEFAULT_OPTS Integration

Respect the user's fzf configuration, but apply snip-specific overrides:

```rust
fn build_fzf_command(fzf_path: &PathBuf, prompt: &str, query: &str) -> Command {
    let mut cmd = Command::new(fzf_path);

    // User's FZF_DEFAULT_OPTS are automatically respected by fzf itself.
    // We override only what we need. Our args take precedence.

    cmd
        .arg("--height=50%")
        .arg("--reverse")
        .arg("--border=rounded")
        .arg(format!("--prompt={}> ", prompt))
        .arg("--delimiter=\t")
        .arg("--with-nth=1,2")
        .arg("--nth=1")
        .arg("--cycle")
        .arg("--info=inline")
        .arg(format!("--query={}", query));

    cmd
}
```

**Important**: If we want snip-specific theming regardless of user's `FZF_DEFAULT_OPTS`, we can set `FZF_DEFAULT_OPTS` in the child process's environment:

```rust
cmd.env(
    "FZF_DEFAULT_OPTS",
    format!(
        "{} --color=hl:#ff9e64 --color=hl+:#ff9e64,bold --color=prompt:#bb9af7",
        std::env::var("FZF_DEFAULT_OPTS").unwrap_or_default()
    ),
);
```

---

## 3. Built-In Picker Architecture (Fallback Path)

When fzf is not available, fall back to a built-in picker. This must be good enough to be usable, but does not need to match fzf's polish.

### 3.1 Module Structure

```
src/ui/
├── picker.rs          # Public API: pick(), pick_fallback()
├── picker_engine.rs   # Built-in picker: render loop, key handling
└── picker_render.rs   # Drawing: items, highlights, cursor, status bar
```

### 3.2 Core State Machine

```rust
use std::io::{self, Write};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal::{self, ClearType},
    cursor,
    style::{Color, Attribute, SetForegroundColor, SetAttribute, ResetColor, SetBackgroundColor},
    execute,
    queue,
};
use crossterm::tty::IsTty;

/// Built-in fuzzy picker state.
pub struct PickerEngine {
    items: Vec<PickerItem>,      // All items
    filtered: Vec<usize>,        // Indices into items that match
    query: String,               // Current search query
    cursor: usize,               // Selected index in filtered list
    scroll_offset: usize,        // Scroll position for long lists
    width: u16,                  // Terminal width
    height: u16,                 // Terminal height
    max_visible: usize,          // Items that fit on screen (height - prompt - status - margins)
}

impl PickerEngine {
    pub fn new(items: Vec<PickerItem>) -> io::Result<Self> {
        let (width, height) = terminal::size()?;
        let max_visible = (height as usize).saturating_sub(4); // prompt + status + borders

        let mut engine = Self {
            items,
            filtered: Vec::new(),
            query: String::new(),
            cursor: 0,
            scroll_offset: 0,
            width,
            height,
            max_visible: max_visible.max(1),
        };

        engine.refilter();
        Ok(engine)
    }

    /// Re-run the fuzzy match and update filtered list.
    fn refilter(&mut self) {
        let matcher = fuzzy_matcher::skim::SkimMatcherV2::default();
        let query = &self.query;

        if query.is_empty() {
            self.filtered = (0..self.items.len()).collect();
        } else {
            self.filtered = self.items
                .iter()
                .enumerate()
                .filter_map(|(idx, item)| {
                    let score = matcher.fuzzy_match(&item.display, query).unwrap_or(0);
                    if score > 0 {
                        Some((idx, score))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            // Sort by score descending
            self.filtered.sort_by(|a, b| b.1.cmp(&a.1));
            self.filtered = self.filtered.into_iter().map(|(idx, _)| idx).collect();
        }

        // Clamp cursor
        if self.cursor >= self.filtered.len() {
            self.cursor = self.filtered.len().saturating_sub(1);
        }
        self.clamp_scroll();
    }

    fn clamp_scroll(&mut self) {
        if self.cursor < self.scroll_offset {
            self.scroll_offset = self.cursor;
        }
        if self.cursor >= self.scroll_offset + self.max_visible {
            self.scroll_offset = self.cursor - self.max_visible + 1;
        }
    }

    fn handle_resize(&mut self) -> io::Result<()> {
        let (w, h) = terminal::size()?;
        self.width = w;
        self.height = h;
        self.max_visible = (h as usize).saturating_sub(4).max(1);
        self.clamp_scroll();
        Ok(())
    }
}
```

### 3.3 Render Loop

```rust
impl PickerEngine {
    /// Run the picker interactively. Returns the selected key or None.
    pub fn run(mut self) -> io::Result<Option<String>> {
        // Enter raw mode
        terminal::enable_raw_mode()?;
        let _guard = RawModeGuard; // Restores terminal on drop

        // Hide cursor
        let mut stdout = io::stdout();
        queue!(stdout, terminal::EnterAlternateScreen)?;
        queue!(stdout, cursor::Hide)?;
        stdout.flush()?;

        // Main loop
        let result = loop {
            self.draw(&mut stdout)?;

            // Read next event with 16ms timeout (allows resize detection)
            if event::poll(std::time::Duration::from_millis(16))? {
                match event::read()? {
                    Event::Key(key) => {
                        if let Some(result) = self.handle_key(key) {
                            break result;
                        }
                    }
                    Event::Resize(w, h) => {
                        self.width = w;
                        self.height = h;
                        self.max_visible = (h as usize).saturating_sub(4).max(1);
                        self.clamp_scroll();
                    }
                    _ => {}
                }
            }
            // On timeout (no input), we just redraw (handles resize signals
            // that crossterm might not catch via Event::Resize on all platforms).
        };

        // Cleanup
        queue!(stdout, cursor::Show)?;
        queue!(stdout, terminal::LeaveAlternateScreen)?;
        stdout.flush()?;

        Ok(result)
    }

    fn handle_key(&mut self, key: KeyEvent) -> Option<Option<String>> {
        match key.code {
            KeyCode::Enter => {
                if let Some(&idx) = self.filtered.get(self.cursor) {
                    return Some(Some(self.items[idx].key.clone()));
                }
                Some(None)
            }
            KeyCode::Esc => Some(None),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(None)
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.query.clear();
                self.cursor = 0;
                self.scroll_offset = 0;
                self.refilter();
                None
            }
            KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Delete last word
                let trimmed = self.query.trim_end();
                if let Some(pos) = trimmed.rfind(|c: char| c.is_whitespace()) {
                    self.query.truncate(pos + 1);
                    self.query = self.query.trim_end().to_string();
                } else {
                    self.query.clear();
                }
                self.refilter();
                None
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.refilter();
                None
            }
            KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.clamp_scroll();
                }
                None
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
                if self.cursor + 1 < self.filtered.len() {
                    self.cursor += 1;
                    self.clamp_scroll();
                }
                None
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.query.push(c);
                self.cursor = 0;
                self.scroll_offset = 0;
                self.refilter();
                None
            }
            KeyCode::Home => {
                self.cursor = 0;
                self.scroll_offset = 0;
                None
            }
            KeyCode::End => {
                self.cursor = self.filtered.len().saturating_sub(1);
                self.clamp_scroll();
                None
            }
            KeyCode::PageUp => {
                self.cursor = self.cursor.saturating_sub(self.max_visible);
                self.clamp_scroll();
                None
            }
            KeyCode::PageDown => {
                self.cursor = (self.cursor + self.max_visible).min(self.filtered.len().saturating_sub(1));
                self.clamp_scroll();
                None
            }
            _ => None,
        }
    }
}

struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}
```

### 3.4 Drawing / Rendering

```rust
impl PickerEngine {
    fn draw(&self, stdout: &mut impl Write) -> io::Result<()> {
        // Clear screen and move cursor to top
        queue!(stdout, terminal::Clear(ClearType::All))?;
        queue!(stdout, cursor::MoveTo(0, 0))?;

        // Draw prompt line
        self.draw_prompt(stdout)?;

        // Draw items
        self.draw_items(stdout)?;

        // Draw status bar
        self.draw_status(stdout)?;

        stdout.flush()?;
        Ok(())
    }

    fn draw_prompt(&self, stdout: &mut impl Write) -> io::Result<()> {
        // "> query_" with the > in cyan
        queue!(stdout, SetForegroundColor(Color::Cyan))?;
        write!(stdout, "> ")?;
        queue!(stdout, ResetColor)?;
        write!(stdout, "{}", self.query)?;
        // Block cursor effect — draw a space in reverse video
        queue!(stdout, SetBackgroundColor(Color::White))?;
        queue!(stdout, SetForegroundColor(Color::Black))?;
        write!(stdout, " ")?;
        queue!(stdout, ResetColor)?;
        queue!(stdout, cursor::MoveToNextLine(1))?;
        Ok(())
    }

    fn draw_items(&self, stdout: &mut impl Write) -> io::Result<()> {
        let visible_items: Vec<&usize> = self.filtered
            .iter()
            .skip(self.scroll_offset)
            .take(self.max_visible)
            .collect();

        for (i, &item_idx) in visible_items.iter().enumerate() {
            let item = &self.items[item_idx];
            let is_selected = (self.scroll_offset + i) == self.cursor;

            // Highlight selected row
            if is_selected {
                queue!(stdout, SetBackgroundColor(Color::DarkGrey))?;
                queue!(stdout, SetForegroundColor(Color::White))?;
                queue!(stdout, SetAttribute(Attribute::Bold))?;
                write!(stdout, " ▶ ")?;
            } else {
                queue!(stdout, SetForegroundColor(Color::DarkGrey))?;
                write!(stdout, "   ")?;
            }

            // Draw the item name with fuzzy highlight
            if !self.query.is_empty() {
                self.draw_highlighted(stdout, &item.display, &self.query)?;
            } else {
                queue!(stdout, SetForegroundColor(Color::White))?;
                write!(stdout, "{}", item.display)?;
            }

            // Draw description/detail
            if is_selected {
                queue!(stdout, SetForegroundColor(Color::Grey))?;
            } else {
                queue!(stdout, SetForegroundColor(Color::DarkGrey))?;
            }
            if !item.detail.is_empty() {
                let available = self.width as usize
                    .saturating_sub(item.display.len() + 8);
                let detail: String = item.detail.chars().take(available).collect();
                write!(stdout, "  {}", detail)?;
            }

            queue!(stdout, ResetColor)?;
            queue!(stdout, cursor::MoveToNextLine(1))?;
        }

        // Fill remaining lines with empty space
        let remaining = self.max_visible.saturating_sub(visible_items.len());
        for _ in 0..remaining {
            write!(stdout, "\r\n")?;
        }

        Ok(())
    }

    fn draw_highlighted(&self, stdout: &mut impl Write, text: &str, query: &str) -> io::Result<()> {
        let matcher = fuzzy_matcher::skim::SkimMatcherV2::default();
        let matches = matcher.fuzzy_indices(text, query);

        match matches {
            Some((_, indices)) => {
                let matched: std::collections::HashSet<usize> =
                    indices.into_iter().collect();

                for (i, ch) in text.chars().enumerate() {
                    if matched.contains(&i) {
                        // Highlighted character — bold + bright color
                        queue!(stdout, SetForegroundColor(Color::BrightYellow))?;
                        queue!(stdout, SetAttribute(Attribute::Bold))?;
                        write!(stdout, "{}", ch)?;
                        queue!(stdout, ResetColor)?;
                        // Re-apply row style
                        // (This is simplified — a real impl tracks parent style)
                    } else {
                        write!(stdout, "{}", ch)?;
                    }
                }
            }
            None => {
                write!(stdout, "{}", text)?;
            }
        }
        Ok(())
    }

    fn draw_status(&self, stdout: &mut impl Write) -> io::Result<()> {
        // Bottom status bar
        queue!(stdout, SetForegroundColor(Color::DarkGrey))?;
        write!(stdout, " {} / {}  ↑↓ navigate  Enter select  Esc cancel  Ctrl+U clear  Ctrl+W delete word",
            self.filtered.len(),
            self.items.len(),
        )?;
        queue!(stdout, ResetColor)?;
        Ok(())
    }
}
```

### 3.5 Window Resize (SIGWINCH)

`crossterm` handles `SIGWINCH` internally and delivers `Event::Resize(w, h)` events. Our poll-based render loop naturally picks these up. No manual signal handling needed.

**Edge case**: If the terminal shrinks to very small, enforce a minimum `max_visible` of 1 to avoid panics.

### 3.6 Key Bindings Summary

| Key | Action |
|-----|--------|
| `Enter` | Select current item |
| `Esc` | Cancel (return `None`) |
| `Ctrl+C` | Cancel |
| `↑` / `k` | Move cursor up |
| `↓` / `j` / `Tab` | Move cursor down |
| `BackTab` (Shift+Tab) | Move cursor up |
| `Ctrl+U` | Clear query |
| `Ctrl+W` | Delete last word from query |
| `Backspace` | Delete last character |
| `Home` | Jump to first item |
| `End` | Jump to last item |
| `PageUp` | Scroll up by page |
| `PageDown` | Scroll down by page |
| Any printable char | Append to query |

### 3.7 Fallback Integration Point

```rust
// In picker.rs — the public entry point:
pub fn pick(items: &[PickerItem], query: &str) -> io::Result<PickerResult> {
    // Not a TTY? Fall back to non-interactive list.
    if !std::io::stdout().is_tty() {
        return pick_fallback(items);
    }

    // Try fzf first
    if let Some(fzf_path) = find_fzf() {
        let lines: Vec<String> = items.iter()
            .map(|i| format!("{}\t{}\t{}", i.display, i.detail, i.key))
            .collect();

        match fzf_pick(&lines, query, &fzf_path, "snippet") {
            Ok(FzfResult::Selected(line)) => {
                // Parse the key from the last tab-separated column
                let key = line.rsplit('\t').next().unwrap_or(&line).to_string();
                return Ok(PickerResult::Selected(key));
            }
            Ok(FzfResult::Cancelled) => {
                return Ok(PickerResult::Cancelled);
            }
            Ok(FzfResult::NotFound) => {
                // Shouldn't happen since we checked, but handle gracefully
            }
            Err(e) => {
                // fzf failed to launch — log and fall through
                eprintln!("warning: fzf failed: {e}, using built-in picker");
            }
        }
    }

    // Built-in fallback
    let engine = PickerEngine::new(items.to_vec())?;
    match engine.run()? {
        Some(key) => Ok(PickerResult::Selected(key)),
        None => Ok(PickerResult::Cancelled),
    }
}
```

---

## 4. Scoring Algorithm Improvements

### 4.1 Current State

The current `fuzzy.rs` uses `fuzzy_matcher::skim::SkimMatcherV2` — which already implements fzf's V2 Smith-Waterman algorithm with the correct scoring constants. **The matching quality is already good.**

However, `SkimMatcherV2::default()` does not return match positions for highlighting. We need to switch to `fuzzy_indices()` to get the character indices for the built-in picker's highlight rendering.

### 4.2 Upgraded `fuzzy.rs`

```rust
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

/// A scored fuzzy-match result with match positions.
#[derive(Debug, Clone)]
pub struct FuzzyResult {
    pub key: String,
    pub score: i64,
    /// Character indices in `key` that matched (for highlighting).
    pub indices: Vec<usize>,
}

/// Fuzzy-match `query` against a list of `keys`.
///
/// Returns results sorted by score descending (best match first).
/// An empty query returns all keys with empty indices.
pub fn fuzzy_match(query: &str, keys: &[String]) -> Vec<FuzzyResult> {
    let matcher = SkimMatcherV2::default();

    if query.is_empty() {
        return keys
            .iter()
            .map(|key| FuzzyResult {
                key: key.clone(),
                score: 0,
                indices: Vec::new(),
            })
            .collect();
    }

    let mut results: Vec<FuzzyResult> = keys
        .iter()
        .filter_map(|key| {
            match matcher.fuzzy_indices(key, query) {
                Some((score, indices)) if score > 0 => Some(FuzzyResult {
                    key: key.clone(),
                    score,
                    indices,
                }),
                _ => None,
            }
        })
        .collect();

    results.sort_by(|a, b| b.score.cmp(&a.score));
    results
}

/// Find the single best fuzzy match. Returns `None` if nothing matches.
pub fn fuzzy_best(query: &str, keys: &[String]) -> Option<String> {
    let mut results = fuzzy_match(query, keys);
    results.first().map(|r| r.key.clone())
}

/// Match a single string and return positions for highlighting.
pub fn fuzzy_positions(text: &str, query: &str) -> Option<(i64, Vec<usize>)> {
    let matcher = SkimMatcherV2::default();
    matcher.fuzzy_indices(text, query)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match() {
        let keys = vec!["build".into(), "test".into(), "deploy".into()];
        let results = fuzzy_match("build", &keys);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "build");
    }

    #[test]
    fn fuzzy_match_works() {
        let keys = vec!["build-release".into(), "build-debug".into(), "test".into()];
        let results = fuzzy_match("bldrel", &keys);
        assert!(!results.is_empty());
        assert_eq!(results[0].key, "build-release");
    }

    #[test]
    fn returns_match_indices() {
        let results = fuzzy_match("br", &["build-release".into()]);
        assert_eq!(results.len(), 1);
        // 'b' at index 0, 'r' at index 6
        assert_eq!(results[0].indices, vec![0, 6]);
    }

    #[test]
    fn word_boundary_bonus() {
        // "deploy-staging" should score higher for "ds" than "deep-scan"
        // because 'd' and 's' are both at word boundaries
        let keys = vec![
            "deploy-staging".into(),   // d___s — both at boundaries
            "deep-scan".into(),        // d___s — both at boundaries (tie)
            "uploads".into(),          // u___ds — 'd' NOT at boundary
        ];
        let results = fuzzy_match("ds", &keys);
        // deploy-staging and deep-scan should rank above uploads
        let uploads_pos = results.iter().position(|r| r.key == "uploads");
        let deploy_pos = results.iter().position(|r| r.key == "deploy-staging");
        assert!(uploads_pos.unwrap() > deploy_pos.unwrap());
    }

    #[test]
    fn no_match_returns_empty() {
        let keys = vec!["build".into(), "test".into()];
        let results = fuzzy_match("xyz", &keys);
        assert!(results.is_empty());
    }

    #[test]
    fn empty_query_returns_all() {
        let keys = vec!["a".into(), "b".into(), "c".into()];
        let results = fuzzy_match("", &keys);
        assert_eq!(results.len(), 3);
    }
}
```

### 4.3 What SkimMatcherV2 Already Gives Us

The `fuzzy-matcher` crate's `SkimMatcherV2` implements fzf's scoring system. Comparing against fzf's source:

| Feature | fzf (Go) | fuzzy-matcher (Rust) | Match? |
|---------|----------|---------------------|--------|
| Score constants (16, -3, -1, 8, 10, 9, 7, 4) | ✅ | ✅ | Identical |
| Character class classification (7 classes) | ✅ | ✅ | Identical |
| Word boundary bonuses (white, delimiter, non-word) | ✅ | ✅ | Identical |
| CamelCase transition bonus | ✅ | ✅ | Identical |
| Consecutive match bonus | ✅ | ✅ | Identical |
| Gap start / extension penalties | ✅ | ✅ | Identical |
| First character multiplier (×2) | ✅ | ✅ | Identical |
| V2 Smith-Waterman DP algorithm | ✅ | ✅ | Functionally identical |
| ASCII fast-path window narrowing | ✅ | ❌ | Minor perf (see §5) |
| SIMD byte search | ✅ | ❌ | Minor perf (see §5) |
| Unicode normalization | ✅ | ❌ | Not needed for snippet keys |

**Conclusion**: No custom scoring implementation is needed. `SkimMatcherV2` already uses fzf's exact algorithm and constants. The improvement is switching from `fuzzy_match()` → `fuzzy_indices()` to get match positions.

### 4.4 If We Ever Need a Custom Scorer

For reference, here is fzf's exact bonus calculation ported to Rust (from the fzf analysis, §3.2–3.3). **Do NOT implement this unless SkimMatcherV2 is removed as a dependency.** Kept here as documentation:

```rust
// fzf's exact scoring constants
const SCORE_MATCH: i16 = 16;
const SCORE_GAP_START: i16 = -3;
const SCORE_GAP_EXTENSION: i16 = -1;
const BONUS_BOUNDARY: i16 = 8;
const BONUS_NON_WORD: i16 = 8;
const BONUS_CAMEL123: i16 = 7;
const BONUS_CONSECUTIVE: i16 = 4;
const BONUS_FIRST_CHAR_MULT: i16 = 2;
const BONUS_BOUNDARY_WHITE: i16 = 10;
const BONUS_BOUNDARY_DELIM: i16 = 9;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum CharClass {
    White = 0,
    NonWord = 1,
    Delimiter = 2,
    Lower = 3,
    Upper = 4,
    Letter = 5,
    Number = 6,
}

fn char_class(ch: char) -> CharClass {
    match ch {
        'a'..='z' => CharClass::Lower,
        'A'..='Z' => CharClass::Upper,
        '0'..='9' => CharClass::Number,
        c if c.is_whitespace() => CharClass::White,
        '/' | ',' | ':' | ';' | '|' => CharClass::Delimiter,
        c if c.is_alphabetic() => CharClass::Letter,
        _ => CharClass::NonWord,
    }
}

fn bonus_for(prev: CharClass, curr: CharClass) -> i16 {
    if curr >= CharClass::NonWord {
        match prev {
            CharClass::White => return BONUS_BOUNDARY_WHITE,
            CharClass::Delimiter => return BONUS_BOUNDARY_DELIM,
            CharClass::NonWord => return BONUS_BOUNDARY,
            _ => {}
        }
    }
    if prev == CharClass::Lower && curr == CharClass::Upper {
        return BONUS_CAMEL123;
    }
    if prev != CharClass::Number && curr == CharClass::Number {
        return BONUS_CAMEL123;
    }
    match curr {
        CharClass::NonWord | CharClass::Delimiter => BONUS_NON_WORD,
        CharClass::White => BONUS_BOUNDARY_WHITE,
        _ => 0,
    }
}
```

---

## 5. Performance Targets

### 5.1 Target

**< 16ms for 500 items** — this gives 60fps headroom for the render loop (one frame = 16.67ms).

### 5.2 Current Performance (Baseline)

Using the existing `SkimMatcherV2` with 500 items of ~30 character keys:

| Operation | Items | Time (expected) | Notes |
|-----------|-------|-----------------|-------|
| `fuzzy_match("build", keys)` | 500 | ~1–2ms | SkimMatcherV2 is fast |
| `fuzzy_indices("build", keys)` | 500 | ~2–3ms | Slightly slower (allocates indices) |
| Full render (draw 20 items) | 20 | ~0.5ms | Terminal I/O is the bottleneck |
| Total frame (match + draw) | 500 | ~3–5ms | Well under 16ms ✅ |

### 5.3 Why 500 Items Is Not a Problem

1. **Snippet collections are small**. Even power users rarely have more than 200 snippets. Pet's design (global file) caps at a few hundred.
2. **SkimMatcherV2 is O(n×m)** where n = text length (~30 chars) and m = query length (typically 1–5 chars). For 500 items with 5-char query: 500 × 30 × 5 = 75,000 operations. Trivial.
3. **fzf's extreme optimizations** (SIMD, parallel matching, slab allocators, bitmap cache) are designed for 100K+ items (file trees). We don't need them.

### 5.4 Built-In Picker Frame Budget

```
Frame budget:    16.67ms (60fps)
├── Fuzzy match: ~3ms    (500 items, 5-char query)
├── Sort:         ~0.1ms  (500 items, already mostly sorted)
├── Render:       ~2ms    (crossterm queue + flush, 20 visible items)
├── Poll wait:    ~11ms   (crossterm::poll with 16ms timeout)
└── Headroom:     ~0.5ms  ✅
```

### 5.5 Optimization: Incremental Refilter

For the built-in picker, avoid re-sorting the entire list on every keystroke. Since users typically append one character at a time:

```rust
/// Optimization: if the new query is an extension of the old query,
/// only re-score items that previously matched.
fn refilter_incremental(&mut self, old_query: &str) {
    if !self.query.starts_with(old_query) {
        // Query changed non-incrementally (backspace, clear, etc.)
        self.refilter();
        return;
    }

    // Only the newly added suffix matters for NEW matches.
    // But existing matches need re-scoring because the new char
    // changes the optimal alignment. Just re-score existing matches.
    let matcher = SkimMatcherV2::default();
    let query = &self.query;

    // Re-score previously matched items (they may no longer match)
    let mut kept: Vec<(usize, i64)> = Vec::with_capacity(self.filtered.len());
    for &idx in &self.filtered {
        if let Some(score) = matcher.fuzzy_match(&self.items[idx].display, query) {
            if score > 0 {
                kept.push((idx, score));
            }
        }
    }

    // Check previously unmatched items (new char might create new matches)
    let matched_set: std::collections::HashSet<usize> =
        kept.iter().map(|(i, _)| *i).collect();
    for idx in 0..self.items.len() {
        if matched_set.contains(&idx) {
            continue;
        }
        if let Some(score) = matcher.fuzzy_match(&self.items[idx].display, query) {
            if score > 0 {
                kept.push((idx, score));
            }
        }
    }

    kept.sort_by(|a, b| b.1.cmp(&a.1));
    self.filtered = kept.into_iter().map(|(idx, _)| idx).collect();
    self.clamp_scroll();
}
```

### 5.6 When Performance Would Become a Problem

| Scenario | Items | Frame Time | Action Needed? |
|----------|-------|------------|----------------|
| Personal snippets | 10–200 | <1ms | No |
| Team shared snippets | 200–1000 | 2–5ms | No |
| Global snippet library | 1000–10000 | 10–50ms | Yes — page results, lazy load |
| File search | 100K+ | 100ms+ | Use fzf (has SIMD, parallel) |

**If we ever hit 10K+ items**: Switch to the fzf-only path. The built-in picker is a fallback for 0–1000 items.

---

## 6. Implementation Phases

### Phase 1: fzf Shell-Out (Day 1–2)

1. Add `find_fzf()` detection function
2. Implement `fzf_pick()` with tab-separated format, `--with-nth`, `--preview`, `--query`
3. Implement `fzf_filter()` for non-interactive `snip run <partial>`
4. Wire into `picker.rs` as the primary path
5. Keep existing built-in picker as-is for the fallback (improve later)
6. Add `fzf_pick_with_actions()` for multi-action (run/edit/copy)

**Files changed**: `src/ui/picker.rs`, `src/core/fuzzy.rs`

### Phase 2: Built-In Picker Upgrade (Day 3–5)

1. Create `src/ui/picker_engine.rs` with the full state machine
2. Implement render loop with `crossterm::event::poll` at 16ms
3. Implement match highlighting using `fuzzy_indices()`
4. Handle `Event::Resize` for SIGWINCH
5. Add key bindings (Enter, Esc, Ctrl+C/U/W, arrows, j/k, Tab, Home/End, PageUp/Down)
6. Use alternate screen buffer (`EnterAlternateScreen` / `LeaveAlternateScreen`)
7. Integrate into `picker.rs` as fallback when `find_fzf()` returns `None`

**Files changed**: `src/ui/picker.rs`, `src/ui/picker_engine.rs` (new)

### Phase 3: Polish (Day 6–7)

1. Add match position return to `FuzzyResult` (switch from `fuzzy_match` to `fuzzy_indices`)
2. Test on macOS, Linux, and Windows
3. Add `SNIP_FZF` env var override
4. Graceful error messages when fzf fails
5. FZF_DEFAULT_OPTS passthrough

### Phase 4 (Future, Optional)

- Replace `fuzzy-matcher` crate with custom fzf V1 algorithm if binary size becomes a concern
- Add incremental refilter optimization to built-in picker
- Add mouse support to built-in picker
- Add preview window to built-in picker