use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::core::snippet::SnipFile;

// ── Data types ────────────────────────────────────────────────────────

/// A single entry parsed from a shell history file.
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    /// The shell command that was run.
    pub cmd: String,
    /// Unix timestamp (seconds), if available.
    pub timestamp: Option<i64>,
    /// Number of times this exact command was seen.
    pub count: u32,
}

/// A suggestion for a snippet candidate derived from shell history.
#[derive(Debug, Clone)]
pub struct Suggestion {
    /// The command to suggest as a snippet.
    pub cmd: String,
    /// How many times the command (or its base) was run.
    pub frequency: u32,
    /// Combined score (frequency × recency weight).
    pub recency_score: f64,
}

// ── Detection ─────────────────────────────────────────────────────────

/// Detect which shell history file to use based on `$SHELL` env var.
///
/// Returns the path to the history file, or `None` if detection fails.
pub fn detect_history_path() -> Option<PathBuf> {
    let shell = std::env::var("SHELL").ok()?;

    let home = dirs_home()?;

    if shell.contains("zsh") {
        let p = home.join(".zsh_history");
        if p.exists() {
            return Some(p);
        }
    }

    if shell.contains("bash") {
        let p = home.join(".bash_history");
        if p.exists() {
            return Some(p);
        }
    }

    if shell.contains("fish") {
        let p = home.join(".local/share/fish/fish_history");
        if p.exists() {
            return Some(p);
        }
    }

    // Fallback: try each in order
    let candidates = [
        home.join(".zsh_history"),
        home.join(".bash_history"),
        home.join(".local/share/fish/fish_history"),
    ];
    for c in &candidates {
        if c.exists() {
            return Some(c.clone());
        }
    }

    None
}

// ── Parsers ───────────────────────────────────────────────────────────

/// Parse a bash history file (`~/.bash_history`).
///
/// One command per line; lines starting with `#` are timestamps in the form
/// `#1590000000`.
pub fn parse_bash_history(path: &Path) -> Result<Vec<HistoryEntry>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read bash history: {}", path.display()))?;

    let mut entries: Vec<HistoryEntry> = Vec::new();
    let mut current_ts: Option<i64> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(ts_str) = trimmed.strip_prefix('#') {
            // Try to parse as timestamp
            if let Ok(ts) = ts_str.trim().parse::<i64>() {
                current_ts = Some(ts);
            }
            continue;
        }
        entries.push(HistoryEntry {
            cmd: trimmed.to_string(),
            timestamp: current_ts,
            count: 1,
        });
    }

    Ok(entries)
}

/// Parse a zsh history file (`~/.zsh_history`).
///
/// Extended history format: `: timestamp:0;command`
pub fn parse_zsh_history(path: &Path) -> Result<Vec<HistoryEntry>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read zsh history: {}", path.display()))?;

    let mut entries: Vec<HistoryEntry> = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Extended history format: : timestamp:0;command
        // Also handle multi-line commands that start with a backslash continuation.
        if let Some(rest) = trimmed.strip_prefix(':') {
            // Find the semicolon that separates metadata from command
            if let Some(semicolon_pos) = rest.find(';') {
                let meta = &rest[..semicolon_pos];
                let cmd = rest[semicolon_pos + 1..].trim();

                // Parse timestamp from meta (format: " timestamp:0")
                let ts = meta
                    .trim()
                    .split(':')
                    .next()
                    .and_then(|s| s.trim().parse::<i64>().ok());

                if !cmd.is_empty() {
                    entries.push(HistoryEntry {
                        cmd: cmd.to_string(),
                        timestamp: ts,
                        count: 1,
                    });
                }
                continue;
            }
        }

        // Fallback: treat as plain command (old-style zsh history)
        if !trimmed.starts_with('#') {
            entries.push(HistoryEntry {
                cmd: trimmed.to_string(),
                timestamp: None,
                count: 1,
            });
        }
    }

    Ok(entries)
}

/// Parse a fish history file (`~/.local/share/fish/fish_history`).
///
/// Fish uses a YAML-like format. We simply grep for `- cmd: ...` lines
/// and the following `- when: ...` lines for timestamps.
pub fn parse_fish_history(path: &Path) -> Result<Vec<HistoryEntry>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read fish history: {}", path.display()))?;

    let mut entries: Vec<HistoryEntry> = Vec::new();
    let mut last_cmd_idx: Option<usize> = None;

    for line in content.lines() {
        let trimmed = line.trim();

        if let Some(cmd_str) = trimmed.strip_prefix("- cmd: ") {
            let cmd = cmd_str.trim().to_string();
            if !cmd.is_empty() {
                last_cmd_idx = Some(entries.len());
                entries.push(HistoryEntry {
                    cmd,
                    timestamp: None,
                    count: 1,
                });
            }
            continue;
        }

        let when_str = trimmed
            .strip_prefix("- when: ")
            .or_else(|| trimmed.strip_prefix("when: "));
        if let Some(when_str) = when_str {
            // Fish stores timestamps as epoch seconds (integer or float)
            let ts_str = when_str.trim();
            // Parse as f64 first, then convert to i64
            if let Ok(f) = ts_str.parse::<f64>() {
                if let Some(idx) = last_cmd_idx {
                    entries[idx].timestamp = Some(f as i64);
                    last_cmd_idx = None;
                }
            }
            continue;
        }

        // Reset on new top-level keys
        if !trimmed.starts_with(' ') && !trimmed.is_empty() && trimmed.ends_with(':') {
            last_cmd_idx = None;
        }
    }

    Ok(entries)
}

/// Parse a history file by auto-detecting its format from the path name.
pub fn parse_history(path: &Path) -> Result<Vec<HistoryEntry>> {
    let file_name = path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();

    if file_name.contains("zsh") {
        return parse_zsh_history(path);
    }
    if file_name.contains("fish") {
        return parse_fish_history(path);
    }

    // Default to bash format
    parse_bash_history(path)
}

// ── Suggestion logic ──────────────────────────────────────────────────

/// Commands that are too trivial to suggest as snippets.
const BUILTIN_BLACKLIST: &[&str] = &[
    "ls", "cd", "pwd", "echo", "clear", "exit", "history", "man", "which",
    "whoami", "date", "uname", "uptime", "cat", "less", "more", "head",
    "tail", "wc", "sort", "uniq", "tr", "cut", "tee", "xargs",
    "true", "false", "test", "type", "source", "alias", "unalias",
    "export", "unset", "set", "env", "printenv", "read", "declare",
    "local", "return", "shift", "wait", "sleep", "logout", "resize",
    "reset", "bindkey", "bg", "fg", "jobs", "disown", "kill", "trap",
    "dirs", "pushd", "popd", "hash", "builtin", "command", "exec",
    "shopt", "setopt", "opt", "compinit", "compdef",
    "vim", "vi", "nano", "emacs", "code",
    "ssh", "scp", "sftp",
    "snip",
];

/// Check if a command should be filtered out.
fn should_filter(cmd: &str) -> bool {
    let trimmed = cmd.trim();

    // Skip empty commands
    if trimmed.is_empty() {
        return true;
    }

    // Skip single-word commands (just a binary name)
    let first_word = trimmed.split_whitespace().next().unwrap_or("");
    if !first_word.contains('/') && !trimmed.contains(' ') {
        return true;
    }

    // Skip builtins / trivial commands
    let base = first_word.rsplit('/').next().unwrap_or(first_word);
    if BUILTIN_BLACKLIST.contains(&base) {
        return true;
    }

    // Skip commands with pipes — likely one-off pipelines
    if trimmed.contains('|') {
        return true;
    }

    // Skip commands with redirections that look one-off
    if trimmed.contains(">>") || trimmed.contains("2>") || trimmed.contains("&>") {
        return true;
    }

    // Skip very short commands (less than 10 chars total)
    if trimmed.len() < 10 {
        return true;
    }

    false
}

/// Extract a "base command" key for grouping similar commands.
///
/// This strips arguments that vary between runs, keeping only the
/// structural parts of the command.
fn base_command_key(cmd: &str) -> String {
    let parts: Vec<&str> = cmd.trim().split_whitespace().collect();
    if parts.is_empty() {
        return String::new();
    }

    let mut key_parts = Vec::new();

    // Always include the program name
    key_parts.push(parts[0].to_string());

    // For known subcommand-based tools, include the subcommand but strip flags
    let subcommand_tools = [
        "cargo", "npm", "yarn", "pnpm", "docker", "kubectl", "git",
        "go", "pip", "pip3", "poetry", "npm", "npx", "yarn",
        "bundle", "rake", "rails", "make", "cmake",
        "terraform", "ansible-playbook", "aws", "gcloud", "az",
        "pytest", "jest", "mocha", "vitest", "cargo",
    ];

    let base = parts[0].rsplit('/').next().unwrap_or(parts[0]);

    if subcommand_tools.contains(&base) {
        // Include subcommands (positional args before any flag)
        for part in parts.iter().skip(1) {
            if part.starts_with('-') {
                break;
            }
            key_parts.push(part.to_string());
        }

        // Also include --flag=value style flags
        let mut i = 1;
        while i < parts.len() {
            let part = parts[i];
            if part.starts_with("--") && !part.starts_with("--no-") {
                // Include --flag=value style (keep the flag part only)
                if let Some(eq_pos) = part.find('=') {
                    key_parts.push(part[..eq_pos].to_string());
                }
                // Skip standalone --flag (like --release, --verbose)
            }
            i += 1;
        }
    } else {
        // For other commands, include all non-flag arguments and --flag=value pairs
        for part in parts.iter().skip(1) {
            if part.starts_with('-') {
                if let Some(eq_pos) = part.find('=') {
                    key_parts.push(part[..eq_pos].to_string());
                }
                // Skip standalone flags for non-subcommand tools to group more aggressively
            } else {
                key_parts.push(part.to_string());
            }
        }
    }

    key_parts.join(" ")
}

/// Calculate a recency weight multiplier for a given timestamp.
///
/// - Last 7 days: 2.0x
/// - Last 30 days: 1.5x
/// - Older: 1.0x
fn recency_weight(timestamp: Option<i64>, now: i64) -> f64 {
    match timestamp {
        Some(ts) => {
            let age_secs = now.saturating_sub(ts);
            let age_days = age_secs / 86400;
            if age_days <= 7 {
                2.0
            } else if age_days <= 30 {
                1.5
            } else {
                1.0
            }
        }
        None => 1.0,
    }
}

/// Analyze history entries and suggest snippet candidates.
///
/// - Groups similar commands using `base_command_key`
/// - Filters out trivial commands and one-offs
/// - Scores by frequency × recency_weight
/// - Excludes commands already present in the existing `SnipFile`
pub fn suggest_from_history(
    entries: &[HistoryEntry],
    existing: &SnipFile,
    limit: usize,
) -> Vec<Suggestion> {
    let now = chrono_now();

    // Collect all existing commands for dedup
    let existing_cmds: Vec<String> = existing.iter().map(|(_, s)| s.cmd.clone()).collect();

    // Group by base command key
    let mut groups: HashMap<String, (u32, f64, String)> = HashMap::new();

    for entry in entries {
        if should_filter(&entry.cmd) {
            continue;
        }

        let key = base_command_key(&entry.cmd);
        if key.is_empty() {
            continue;
        }

        // Check if this command is already in the snipfile
        if existing_cmds.iter().any(|ec| ec == &entry.cmd) {
            continue;
        }

        let weight = recency_weight(entry.timestamp, now);

        let (freq, score, best_cmd) = groups.entry(key).or_insert((0, 0.0, String::new()));
        *freq += entry.count;
        *score += entry.count as f64 * weight;

        // Keep the most detailed (longest) command as the representative
        if entry.cmd.len() > best_cmd.len() {
            *best_cmd = entry.cmd.clone();
        }
    }

    // Build suggestions
    let mut suggestions: Vec<Suggestion> = groups
        .into_iter()
        .map(|(_key, (freq, score, cmd))| Suggestion {
            cmd,
            frequency: freq,
            recency_score: (score * 10.0).round() / 10.0,
        })
        .collect();

    // Sort by score descending
    suggestions.sort_by(|a, b| {
        b.recency_score
            .partial_cmp(&a.recency_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Apply limit
    suggestions.truncate(limit);

    suggestions
}

/// Get current unix timestamp. Separated for testability.
fn chrono_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Get the user's home directory.
fn dirs_home() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_bash_history(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, "{}", content).unwrap();
        f
    }

    fn make_zsh_history(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, "{}", content).unwrap();
        f
    }

    fn make_fish_history(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, "{}", content).unwrap();
        f
    }

    #[test]
    fn test_parse_bash_basic() {
        let f = make_bash_history("cargo build\ncargo test\nls\n");
        let entries = parse_bash_history(f.path()).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].cmd, "cargo build");
        assert_eq!(entries[1].cmd, "cargo test");
    }

    #[test]
    fn test_parse_bash_timestamps() {
        let f = make_bash_history("#1590000000\ncargo build\n#1590001000\ncargo test\n");
        let entries = parse_bash_history(f.path()).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].timestamp, Some(1590000000));
        assert_eq!(entries[1].timestamp, Some(1590001000));
    }

    #[test]
    fn test_parse_bash_empty_lines() {
        let f = make_bash_history("cargo build\n\ncargo test\n");
        let entries = parse_bash_history(f.path()).unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_parse_zsh_basic() {
        let f = make_zsh_history(": 1590000000:0;cargo build\n: 1590001000:0;cargo test\n");
        let entries = parse_zsh_history(f.path()).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].cmd, "cargo build");
        assert_eq!(entries[0].timestamp, Some(1590000000));
    }

    #[test]
    fn test_parse_zsh_plain() {
        let f = make_zsh_history("cargo build\ncargo test\n");
        let entries = parse_zsh_history(f.path()).unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_parse_fish_basic() {
        let f = make_fish_history(
            r#"- cmd: cargo build
  when: 1590000000
- cmd: cargo test
  when: 1590001000
"#,
        );
        let entries = parse_fish_history(f.path()).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].cmd, "cargo build");
        assert_eq!(entries[0].timestamp, Some(1590000000));
        assert_eq!(entries[1].cmd, "cargo test");
    }

    #[test]
    fn test_parse_fish_float_timestamp() {
        let f = make_fish_history("- cmd: echo hello\n  when: 1590000000.123\n");
        let entries = parse_fish_history(f.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].timestamp, Some(1590000000));
    }

    #[test]
    fn test_should_filter_single_word() {
        assert!(should_filter("ls"));
        assert!(should_filter("cargo"));
        assert!(!should_filter("cargo build"));
    }

    #[test]
    fn test_should_filter_pipes() {
        assert!(should_filter("cat file | grep foo"));
        assert!(should_filter("ls | sort | uniq"));
    }

    #[test]
    fn test_should_filter_builtins() {
        assert!(should_filter("vim file.txt"));
        assert!(should_filter("echo hello"));
        assert!(should_filter("snip list"));
    }

    #[test]
    fn test_should_filter_short() {
        assert!(should_filter("cd foo"));
        assert!(should_filter("git s")); // too short
        assert!(!should_filter("cargo build --release"));
    }

    #[test]
    fn test_should_filter_redirections() {
        assert!(should_filter("cargo build 2>/dev/null"));
        assert!(should_filter("echo hello >> file.txt"));
    }

    #[test]
    fn test_base_command_key() {
        assert_eq!(base_command_key("cargo build --release"), "cargo build");
        assert_eq!(base_command_key("cargo test -- --nocapture"), "cargo test");
        assert_eq!(base_command_key("npm run build"), "npm run build");
        assert_eq!(base_command_key("npm run dev --port 3000"), "npm run dev");
    }

    #[test]
    fn test_recency_weight() {
        let now = 1700000000;
        assert_eq!(recency_weight(Some(now), now), 2.0); // now = 0 days
        assert_eq!(recency_weight(Some(now - 5 * 86400), now), 2.0); // 5 days
        assert_eq!(recency_weight(Some(now - 10 * 86400), now), 1.5); // 10 days
        assert_eq!(recency_weight(Some(now - 25 * 86400), now), 1.5); // 25 days
        assert_eq!(recency_weight(Some(now - 60 * 86400), now), 1.0); // 60 days
        assert_eq!(recency_weight(None, now), 1.0);
    }

    #[test]
    fn test_suggest_from_history_basic() {
        let now = 1700000000;
        let entries = vec![
            HistoryEntry { cmd: "cargo build --release".to_string(), timestamp: Some(now - 86400), count: 1 },
            HistoryEntry { cmd: "cargo build --release".to_string(), timestamp: Some(now - 2 * 86400), count: 1 },
            HistoryEntry { cmd: "cargo build --release".to_string(), timestamp: Some(now - 3 * 86400), count: 1 },
            HistoryEntry { cmd: "cargo test -- --nocapture".to_string(), timestamp: Some(now - 86400), count: 1 },
            HistoryEntry { cmd: "ls".to_string(), timestamp: Some(now), count: 5 },
            HistoryEntry { cmd: "git log | head -20".to_string(), timestamp: Some(now), count: 10 },
        ];

        let existing = crate::core::snippet::SnipFile::new();
        let suggestions = suggest_from_history(&entries, &existing, 10);

        // ls and piped commands should be filtered
        assert!(!suggestions.iter().any(|s| s.cmd == "ls"));
        assert!(!suggestions.iter().any(|s| s.cmd.contains('|')));

        // cargo build should be there with frequency 3
        let build = suggestions.iter().find(|s| s.cmd.contains("cargo build"));
        assert!(build.is_some());
        assert_eq!(build.unwrap().frequency, 3);

        // cargo test should be there with frequency 1
        let test = suggestions.iter().find(|s| s.cmd.contains("cargo test"));
        assert!(test.is_some());
    }

    #[test]
    fn test_suggest_excludes_existing() {
        let now = 1700000000;
        let entries = vec![
            HistoryEntry { cmd: "cargo build --release".to_string(), timestamp: Some(now), count: 5 },
            HistoryEntry { cmd: "cargo test -- --nocapture".to_string(), timestamp: Some(now), count: 3 },
        ];

        let mut existing = crate::core::snippet::SnipFile::new();
        existing.insert(
            "build",
            crate::core::snippet::Snippet::new("cargo build --release"),
        );

        let suggestions = suggest_from_history(&entries, &existing, 10);

        // cargo build should be excluded since it's in the snipfile
        assert!(!suggestions.iter().any(|s| s.cmd.contains("cargo build")));

        // cargo test should still be there
        assert!(suggestions.iter().any(|s| s.cmd.contains("cargo test")));
    }

    #[test]
    fn test_suggest_limit() {
        let now = 1700000000;
        let entries: Vec<HistoryEntry> = (0..20)
            .map(|i| HistoryEntry {
                cmd: format!("make target-{}", i),
                timestamp: Some(now - i as i64 * 86400),
                count: 1,
            })
            .collect();

        let existing = crate::core::snippet::SnipFile::new();
        let suggestions = suggest_from_history(&entries, &existing, 5);

        assert_eq!(suggestions.len(), 5);
    }

    #[test]
    fn test_suggest_sorts_by_score() {
        let now = 1700000000;
        let entries = vec![
            // Old command run once
            HistoryEntry { cmd: "docker compose up --build".to_string(), timestamp: Some(now - 60 * 86400), count: 1 },
            // Recent command run 3 times
            HistoryEntry { cmd: "cargo build --release".to_string(), timestamp: Some(now - 86400), count: 1 },
            HistoryEntry { cmd: "cargo build --release".to_string(), timestamp: Some(now - 2 * 86400), count: 1 },
            HistoryEntry { cmd: "cargo build --release".to_string(), timestamp: Some(now - 3 * 86400), count: 1 },
        ];

        let existing = crate::core::snippet::SnipFile::new();
        let suggestions = suggest_from_history(&entries, &existing, 10);

        // cargo build should rank higher due to frequency + recency
        assert!(suggestions[0].cmd.contains("cargo build"));
        assert!(suggestions[0].recency_score > suggestions[1].recency_score);
    }

    #[test]
    fn test_parse_history_auto_detect() {
        // zsh path: create a temp file with "zsh" in the name
        let dir = tempfile::tempdir().unwrap();
        let zsh_path = dir.path().join("test_zsh_history");
        std::fs::write(&zsh_path, ": 1590000000:0;cargo build\n").unwrap();
        let entries = parse_history(&zsh_path).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].cmd, "cargo build");

        // fish path: create a temp file with "fish" in the name
        let fish_path = dir.path().join("test_fish_history");
        std::fs::write(&fish_path, "- cmd: cargo test\n  when: 1590000000\n").unwrap();
        let entries = parse_history(&fish_path).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].cmd, "cargo test");
    }

    #[test]
    fn test_history_entry_debug_clone() {
        let e = HistoryEntry {
            cmd: "cargo build".to_string(),
            timestamp: Some(123),
            count: 5,
        };
        let e2 = e.clone();
        assert_eq!(e.cmd, e2.cmd);
        assert_eq!(e.count, e2.count);
        let _ = format!("{:?}", e);
    }

    #[test]
    fn test_suggestion_debug_clone() {
        let s = Suggestion {
            cmd: "cargo test".to_string(),
            frequency: 10,
            recency_score: 15.0,
        };
        let s2 = s.clone();
        assert_eq!(s.cmd, s2.cmd);
        let _ = format!("{:?}", s);
    }
}