# Feature #11: Team Collaboration Design

> **Agent**: #11 — Team Collaboration Expert
> **Date**: 2025-07-12
> **Status**: Draft
> **Depends on**: Existing `.snips` file format, `SnipFile`/`Snippet` types, `find_snipfile`, `validator`, `doctor`, detector infrastructure

---

## Table of Contents

1. [`.snips.d/` Directory — Modular Snippet System](#1-snipsd-directory--modular-snippet-system)
2. [Environment Overrides](#2-environment-overrides)
3. [Snippet Staleness Detection — `snip doctor --fix`](#3-snippet-staleness-detection--snip-doctor---fix)
4. [Snippet Usage Analytics (Local, Privacy-First)](#4-snippet-usage-analytics-local-privacy-first)
5. [Team Onboarding Flow](#5-team-onboarding-flow)
6. [New Dependencies](#6-new-dependencies)
7. [Migration Path](#7-migration-path)
8. [File Tree (Proposed New Modules)](#8-file-tree-proposed-new-modules)

---

## 1. `.snips.d/` Directory — Modular Snippet System

### 1.1 Motivation

Currently `snip` reads a single `.snips` file. On a team with 5+ developers, a monolithic `.snips` becomes a merge-conflict magnet. Teams need to own their own snippet files while sharing a common base.

### 1.2 Directory Layout

```
project-root/
├── .snips              # Main project snippets (detected + hand-written)
├── .snips.d/           # Modular snippet directory (NEW)
│   ├── common.toml     # Shared team snippets          — committed
│   ├── frontend.toml   # Frontend team snippets         — committed
│   ├── backend.toml    # Backend team snippets          — committed
│   ├── infra.toml      # DevOps / infra team snippets   — committed
│   ├── production.toml # Production-specific overrides  — committed
│   └── local.toml      # Personal overrides            — .gitignored
└── .gitignore          # Must contain `.snips.d/local.toml`
```

**Gitignore rule** (added by `snip init` if not present):

```gitignore
.snips.d/local.toml
```

### 1.3 File Format

Each `.snips.d/*.toml` file uses the **exact same TOML schema** as `.snips`. No new syntax. The existing `SnipFile::from_toml_value` / `to_toml_value` parsers work unchanged.

**`.snips.d/common.toml`** — example:

```toml
[lint]
cmd = "prettier --check . && eslint ."
desc = "Run all linters"

[format]
cmd = "prettier --write ."
desc = "Auto-format all files"

[deploy.staging]
cmd = "kubectl apply -f k8s/staging/"
desc = "Deploy to staging cluster"
```

**`.snips.d/frontend.toml`** — example:

```toml
[dev]
cmd = "npm run dev -- --port 5173"
desc = "Start dev server with HMR"

[build]
cmd = "vite build"
desc = "Production bundle"

[storybook]
cmd = "npm run storybook"
desc = "Launch Storybook"
```

**`.snips.d/backend.toml`** — example:

```toml
[dev]
cmd = "cargo run"
desc = "Run backend server"

[migrate]
cmd = "sqlx migrate run"
desc = "Run pending DB migrations"

[seed]
cmd = "sqlx database reset --force && sqlx migrate run && cargo run --bin seed"
desc = "Reset DB, migrate, then seed"
```

**`.snips.d/local.toml`** — example (personal):

```toml
[dev]
cmd = "npm run dev -- --port 3000"
desc = "Dev server on port 3000 (my local override)"

[test]
cmd = "vitest run --reporter verbose"
desc = "Run tests with verbose output"
```

### 1.4 Merge Semantics at Runtime

Snip loads snippet sources in a **deterministic priority order**. Later sources override earlier ones when keys collide:

```
Priority (lowest → highest):
  1. .snips                    # project-level base file
  2. .snips.d/common.toml      # shared team base
  3. .snips.d/<team>.toml      # team-specific (alphabetical order)
  4. .snips.d/production.toml  # env overrides (if present)
  5. .snips.d/local.toml       # personal overrides (always last)
```

Within step 3, files are sorted alphabetically (e.g. `backend.toml` before `frontend.toml`).

**Merging is key-level**: when two sources define the same fully-qualified key, the later source's entire `Snippet` value wins. There is no field-level merging.

### 1.5 Architecture: `LayeredSnipFile`

A new core type that represents the merged view:

```rust
// src/core/layered.rs

use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use super::snippet::{SnipFile, Snippet};

/// A single snippet source with provenance metadata.
#[derive(Debug, Clone)]
pub struct SourceEntry {
    /// The fully-qualified key (e.g. "build.release").
    pub key: String,
    /// The snippet data.
    pub snippet: Snippet,
    /// Which file this came from (e.g. ".snips.d/frontend.toml").
    pub source_file: String,
}

/// Represents the merged view of all snippet layers.
pub struct LayeredSnipFile {
    /// All resolved entries in priority order (highest priority first).
    entries: Vec<SourceEntry>,
}

impl LayeredSnipFile {
    /// Load and merge all snippet layers from the given project root.
    ///
    /// Walks the merge chain described in §1.4 and returns the
    /// deduplicated, priority-merged result.
    pub fn load(project_root: &Path) -> Result<Self> {
        let mut seen_keys = std::collections::HashSet::new();
        let mut entries = Vec::new();

        let layers = Self::resolve_layers(project_root)?;

        // Process in reverse so that highest-priority entries end up
        // at the front of the list. This preserves insertion order for
        // display while ensuring the correct snippet is used.
        for (path, label) in layers.iter().rev() {
            if !path.exists() {
                continue;
            }
            let file = crate::core::snipfile::read_snippets(path)
                .with_context(|| format!("failed to read {}", path.display()))?;

            for (key, snippet) in file.iter() {
                // First occurrence in reverse order = highest priority.
                // We insert at the front, but only if we haven't seen it.
                if seen_keys.insert(key.clone()) {
                    entries.insert(0, SourceEntry {
                        key: key.clone(),
                        snippet: snippet.clone(),
                        source_file: label.clone(),
                    });
                }
            }
        }

        Ok(Self { entries })
    }

    /// Get a snippet by key (returns the highest-priority version).
    pub fn get(&self, key: &str) -> Option<&Snippet> {
        self.entries.iter().find(|e| e.key == key).map(|e| &e.snippet)
    }

    /// Iterate all entries.
    pub fn iter(&self) -> impl Iterator<Item = &SourceEntry> {
        self.entries.iter()
    }

    /// Number of unique snippet keys.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Return the ordered list of (path, label) layers.
    fn resolve_layers(project_root: &Path) -> Result<Vec<(PathBuf, String)>> {
        let mut layers = Vec::new();

        // Layer 1: .snips
        layers.push((project_root.join(".snips"), ".snips".into()));

        let snips_d = project_root.join(".snips.d");
        if !snips_d.is_dir() {
            return Ok(layers);
        }

        // Layer 2: common.toml
        layers.push((snips_d.join("common.toml"), ".snips.d/common.toml".into()));

        // Layer 3: team files (alphabetical, excluding special names)
        let special_names = ["common.toml", "local.toml", "production.toml", "staging.toml", "development.toml"];
        let mut team_files: Vec<String> = std::fs::read_dir(&snips_d)
            .context("failed to read .snips.d directory")?
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with(".toml") && !special_names.contains(&name.as_str()) {
                    Some(name)
                } else {
                    None
                }
            })
            .collect();
        team_files.sort();

        for name in &team_files {
            layers.push((snips_d.join(name), format!(".snips.d/{}", name)));
        }

        // Layer 4: environment overrides (if present)
        for env_name in &["production.toml", "staging.toml", "development.toml"] {
            let path = snips_d.join(env_name);
            if path.exists() {
                layers.push((path, format!(".snips.d/{}", env_name)));
            }
        }

        // Layer 5: local.toml (always last, always wins)
        layers.push((snips_d.join("local.toml"), ".snips.d/local.toml".into()));

        Ok(layers)
    }
}
```

### 1.6 `snip init` Changes

The existing `snip init` (`src/cli/init.rs`) must be extended:

```
Before:  creates .snips
After:   creates .snips + .snips.d/ + .snips.d/common.toml + .snips.d/local.toml
         appends .snips.d/local.toml to .gitignore
```

Pseudocode for the change:

```rust
// In src/cli/init.rs, after writing .snips:

let snips_d = root.join(".snips.d");
std::fs::create_dir_all(&snips_d)?;

// Create common.toml with a helpful comment header
let common_path = snips_d.join("common.toml");
if !common_path.exists() {
    std::fs::write(&common_path, COMMON_TOML_TEMPLATE)?;
}

// Create local.toml (personal overrides)
let local_path = snips_d.join("local.toml");
if !local_path.exists() {
    std::fs::write(&local_path, LOCAL_TOML_TEMPLATE)?;
}

// Ensure .gitignore includes local.toml
ensure_gitignore_entry(root, ".snips.d/local.toml")?;
```

Template for `common.toml`:

```toml
# Shared team snippets — edit freely, this file is committed.
# Format is identical to .snips. Keys here override .snips.

# [example]
# cmd = "echo hello"
# desc = "An example shared snippet"
```

Template for `local.toml`:

```toml
# Personal snippet overrides — THIS FILE IS GITIGNORED.
# Use this for your own shortcuts and local config.
# Keys here override everything else.

# [example]
# cmd = "echo hello from my machine"
# desc = "My personal override"
```

### 1.7 `snip add --scope` Changes

Extend the `Add` CLI command:

```
snip add <NAME> "<CMD>" [DESCRIPTION]
  --scope <SCOPE>     Target file: "project" (.snips), "common", "frontend",
                      "backend", "local", or any filename without .toml
```

```rust
// In the Commands enum:
Add {
    name: String,
    cmd: String,
    description: Option<String>,
    /// Target scope file (e.g. "frontend" → .snips.d/frontend.toml).
    #[arg(long, default_value = "project")]
    scope: String,
},
```

Routing logic in `cli::add`:

```rust
fn resolve_target_path(root: &Path, scope: &str) -> PathBuf {
    match scope {
        "project" => root.join(".snips"),
        "local" => root.join(".snips.d/local.toml"),
        other => root.join(".snips.d").join(format!("{}.toml", other)),
    }
}
```

The existing `add_snippet()` function in `snipfile.rs` works unchanged — it already handles create-on-missing. The only change is that the caller resolves the correct file path based on `--scope`.

### 1.8 Display: Source Attribution in `snip list`

When snippets come from `.snips.d/`, `snip list` (the default command) should show the source:

```
  dev                Start dev server with HMR             [frontend]
  build              Production bundle                      [frontend]
  lint               Run all linters                        [common]
  dev                Run backend server                     [backend]
  test               Run tests with verbose output          [local]
```

The `[source]` suffix is computed from `SourceEntry::source_file`. This requires changing `cli::list::run()` to use `LayeredSnipFile::load()` instead of `read_snippets()`.

---

## 2. Environment Overrides

### 2.1 Problem

Developers need different commands for different environments. Production deploys use different clusters/ports/paths than local dev. Currently, the only override mechanism is editing `.snips` directly.

### 2.2 Environment File Convention

Special filenames in `.snips.d/` activate as environment overrides:

| File | Trigger | Typical Contents |
|------|---------|-----------------|
| `.snips.d/production.toml` | Always loaded (if present) | Production URLs, cluster names |
| `.snips.d/staging.toml` | Always loaded (if present) | Staging-specific config |
| `.snips.d/development.toml` | Always loaded (if present) | Dev defaults (redundant if `.snips` has them) |

These are **committed** — they represent the team's shared environment config.

### 2.3 Full Priority Chain

```
Priority (lowest → highest):
  1. Detected snippets (packs) — from `detector::detect_all()`
  2. .snips                     — project-level base
  3. .snips.d/common.toml       — shared team base
  4. .snips.d/<team>.toml       — team files (alphabetical)
  5. .snips.d/production.toml   — env overrides (if present)
  6. .snips.d/staging.toml      — env overrides (if present)
  7. .snips.d/development.toml  — env overrides (if present)
  8. .snips.d/local.toml        — personal overrides (always wins)
```

This means `local.toml` is the final word for any developer. If someone needs a production URL in their local dev, they can override it in `local.toml`.

### 2.4 Example: Production Overrides

**`.snips.d/frontend.toml`** (base):

```toml
[dev]
cmd = "npm run dev -- --port 5173"
desc = "Start dev server"

[build]
cmd = "vite build"
desc = "Production build"
```

**`.snips.d/production.toml`** (overrides):

```toml
[build]
cmd = "vite build --mode production && cp -r dist/ /deploy/artifacts/"
desc = "Production build for deploy pipeline"

[deploy]
cmd = "aws s3 sync /deploy/artifacts/ s3://prod-cdn/app/ --delete"
desc = "Deploy to production CDN"
```

**`.snips.d/local.toml`** (personal):

```toml
[dev]
cmd = "npm run dev -- --port 3000"
desc = "Dev server on my preferred port"

[build]
cmd = "vite build --mode development"
desc = "Dev-mode build (faster, no minify)"
```

Result for key `dev`: local.toml wins → `npm run dev -- --port 3000`
Result for key `deploy`: production.toml → `aws s3 sync ...`
Result for key `build`: local.toml wins → `vite build --mode development`

### 2.5 Runtime Environment Variable: `SNIP_ENV`

To make environment-aware snippets even more explicit, `snip` respects an optional `SNIP_ENV` environment variable:

```bash
# Only load production.toml when explicitly requested
SNIP_ENV=production snip list

# In CI:
SNIP_ENV=production snip run deploy
```

When `SNIP_ENV` is set:

- Only the matching environment file is loaded (e.g. `production.toml`)
- Other environment files (`staging.toml`, `development.toml`) are **skipped**
- The full merge chain still applies, just with a single env layer

Implementation:

```rust
fn resolve_env_layers(snips_d: &Path) -> Vec<(PathBuf, String)> {
    let env = std::env::var("SNIP_ENV").ok();

    let env_files = match env.as_deref() {
        Some("production") => vec!["production.toml"],
        Some("staging") => vec!["staging.toml"],
        Some("development") | Some("dev") => vec!["development.toml"],
        _ => vec!["production.toml", "staging.toml", "development.toml"],
    };

    env_files
        .iter()
        .filter_map(|name| {
            let path = snips_d.join(name);
            if path.exists() {
                Some((path, format!(".snips.d/{}", name)))
            } else {
                None
            }
        })
        .collect()
}
```

---

## 3. Snippet Staleness Detection — `snip doctor --fix`

### 3.1 Problem

Snippets reference external binaries (`docker`, `kubectl`, `sqlx`), file paths (`k8s/staging/`), and project scripts (`npm run build`). When these change — a binary is uninstalled, a script is renamed, a config file is moved — snippets silently break. Nobody notices until they try to run one.

### 3.2 Current `snip doctor` (Baseline)

The existing `cli::doctor::run()` performs two checks:
1. Core validation (empty commands, undefined/unused template vars) via `validator::validate()`
2. Binary existence via `which::which(first_word_of_cmd)`

Output: pass/fail per snippet.

### 3.3 Extended Checks

#### 3.3.1 Binary Existence (existing, enhanced)

Already implemented. Enhancement: check **all** commands in a pipeline (`|`, `&&`, `;`):

```rust
/// Extract all command names from a pipeline string.
fn extract_binaries(cmd: &str) -> Vec<String> {
    let mut binaries = Vec::new();
    // Split on shell operators
    for part in cmd.split(|c| c == '|' || c == '&' || c == ';') {
        let trimmed = part.trim();
        if let Some(first) = trimmed.split_whitespace().next() {
            // Skip shell builtins and variable assignments
            if !first.contains('=') && !["cd", "echo", "export", "set", "true", "false"].contains(&first) {
                binaries.push(first.to_string());
            }
        }
    }
    binaries
}
```

#### 3.3.2 Referenced File Existence

Detect file path arguments and verify they exist:

```rust
/// Extract file paths from a command (heuristic).
/// Looks for arguments that look like paths: contain '/' or '.', end in known extensions.
fn extract_referenced_paths(cmd: &str, project_root: &Path) -> Vec<String> {
    let tokens = shell_words::split(cmd).unwrap_or_default();
    let mut paths = Vec::new();

    // Flags that should be skipped
    let skip_prefixes = ["-", "--"];

    for token in &tokens {
        if token.starts_with("--") || token.starts_with("-") || token.starts_with("$") {
            continue;
        }
        // Looks like a relative path
        if token.contains('/') || (token.contains('.') && !token.starts_with('-')) {
            let resolved = project_root.join(token);
            paths.push(token.clone());
        }
    }
    paths
}
```

#### 3.3.3 npm Script / Makefile Target Drift

The most insidious staleness: a `.snips` snippet says `npm run build` but `package.json` was refactored and the script is now `npm run compile`. Or `make test` but the Makefile target was renamed to `make tests`.

**Detection approach**: re-run detectors and compare.

```rust
/// Check if detected snippets have drifted from what's in .snips.
fn check_detected_drift(project_root: &Path, snipfile: &SnipFile) -> Vec<DriftIssue> {
    let detected = detector::detect_all(project_root);
    let mut issues = Vec::new();

    for (source, (section, name, cmd, desc)) in &detected {
        let key = if section.is_empty() {
            name.clone()
        } else {
            format!("{}.{}", section, name)
        };

        if let Some(existing) = snipfile.get(&key) {
            // Key exists but command differs
            if existing.cmd != *cmd {
                issues.push(DriftIssue {
                    key,
                    kind: DriftKind::CommandChanged {
                        old_cmd: existing.cmd.clone(),
                        new_cmd: cmd.clone(),
                        source: source.clone(),
                    },
                    auto_fixable: true,
                });
            }
        }
        // Note: missing keys are NOT drift — users may have intentionally
        // excluded detected snippets.
    }

    issues
}
```

### 3.4 `--fix` Flag

```
snip doctor          # Report issues (no changes)
snip doctor --fix    # Report + auto-fix where possible
```

Auto-fixable issues:

| Issue | Fix Action |
|-------|-----------|
| Detected command changed | Update `.snips` to match new detected command |
| Binary renamed (e.g. `docker-compose` → `docker compose`) | Substitute the new binary name in the command string |
| File path moved | If a single candidate is found in the project, update the path |

Non-auto-fixable issues (report only):

| Issue | Why Not Auto-Fix |
|-------|-----------------|
| Binary not installed | Can't guess the replacement |
| Multiple candidate file moves | Ambiguous |
| Template variable in path | Can't resolve at check time |

```rust
pub enum DriftKind {
    CommandChanged { old_cmd: String, new_cmd: String, source: String },
    BinaryMissing { binary: String },
    FileMissing { path: String },
    ScriptRenamed { file: String, old_target: String, new_target: String },
}

pub struct DriftIssue {
    pub key: String,
    pub kind: DriftKind,
    pub auto_fixable: bool,
}
```

### 3.5 Auto-Fix Implementation

```rust
/// Apply auto-fixes to the .snips file. Returns the number of fixes applied.
fn apply_fixes(project_root: &Path, issues: &[DriftIssue]) -> Result<usize> {
    let snipfile_path = find_snipfile(Some(project_root))?
        .ok_or_else(|| anyhow::anyhow!("no .snips file found"))?;

    let mut file = read_snippets(&snipfile_path)?;
    let mut fixes_applied = 0;

    for issue in issues {
        if !issue.auto_fixable {
            continue;
        }
        match &issue.kind {
            DriftKind::CommandChanged { new_cmd, .. } => {
                if let Some(snippet) = file.get_mut(&issue.key) {
                    snippet.cmd = new_cmd.clone();
                    fixes_applied += 1;
                }
            }
            DriftKind::ScriptRenamed { old_target, new_target, .. } => {
                if let Some(snippet) = file.get_mut(&issue.key) {
                    // Replace the old script/target name with the new one
                    snippet.cmd = snippet.cmd.replace(old_target, new_target);
                    fixes_applied += 1;
                }
            }
            _ => {} // Other kinds are not auto-fixable
        }
    }

    if fixes_applied > 0 {
        write_snippets(&snipfile_path, &file)?;
    }

    Ok(fixes_applied)
}
```

### 3.6 CI Integration: GitHub Actions

A GitHub Action workflow for CI:

```yaml
# .github/workflows/snip-doctor.yml
name: Snip Doctor

on:
  pull_request:
    paths:
      - '.snips'
      - '.snips.d/**'
      - 'package.json'
      - 'Makefile'
      - 'Cargo.toml'

jobs:
  doctor:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install snip
        uses: cargo-bins/cargo-binstall@main
        with:
          crates: snip

      - name: Install project dependencies
        run: npm ci  # or whatever the project needs

      - name: Run snip doctor
        run: snip doctor

      # Optional: fail on stale snippets
      # - name: Run snip doctor (strict)
      #   run: snip doctor --ci
```

A `--ci` flag could be added that:
- Exits with code 1 if any binary is missing
- Exits with code 1 if any detected command has drifted
- Does NOT attempt fixes (CI shouldn't modify files)

```rust
// Extended Commands enum:
Doctor {
    /// Auto-fix detected issues
    #[arg(long)]
    fix: bool,
    /// CI mode: exit 1 on any issue, no interactive prompts
    #[arg(long)]
    ci: bool,
},
```

---

## 4. Snippet Usage Analytics (Local, Privacy-First)

### 4.1 Design Principles

1. **All data stays on the local machine.** No telemetry, no network calls, no analytics servers.
2. **Opt-in by default.** Analytics recording starts the first time a snippet is run. It can be disabled entirely.
3. **SQLite for reliability.** Not a flat file — SQLite handles concurrent access, is queryable, and is zero-maintenance.
4. **XDG-compliant location.** `~/.local/share/snip/history.db` on Linux, `~/Library/Application Support/snip/history.db` on macOS, `%LOCALAPPDATA%\snip\history.db` on Windows.

### 4.2 Database Schema

```sql
-- ~/.local/share/snip/history.db

CREATE TABLE IF NOT EXISTS runs (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    project     TEXT    NOT NULL,       -- absolute path to project root
    key         TEXT    NOT NULL,       -- fully-qualified snippet key
    source      TEXT    NOT NULL,       -- which file the snippet came from
    cmd         TEXT    NOT NULL,       -- the command that was executed
    exit_code   INTEGER,               -- NULL if still running / timed out
    duration_ms INTEGER,               -- execution time in milliseconds
    timestamp   TEXT    NOT NULL DEFAULT (datetime('now')),
    tty         INTEGER NOT NULL DEFAULT 0  -- 1 if run from interactive terminal
);

CREATE INDEX IF NOT EXISTS idx_runs_project_key ON runs(project, key);
CREATE INDEX IF NOT EXISTS idx_runs_timestamp ON runs(timestamp);

-- Track when detected snippets were last refreshed for staleness checks
CREATE TABLE IF NOT EXISTS detection_cache (
    project     TEXT    NOT NULL,
    detector    TEXT    NOT NULL,       -- e.g. "Node.js"
    key         TEXT    NOT NULL,
    cmd         TEXT    NOT NULL,
    detected_at TEXT    NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (project, detector, key)
);
```

### 4.3 New Module: `src/core/analytics.rs`

```rust
// src/core/analytics.rs

use std::path::{Path, PathBuf};
use anyhow::{Context, Result};

/// A recorded snippet execution.
pub struct RunRecord {
    pub project: String,
    pub key: String,
    pub source: String,
    pub cmd: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub tty: bool,
}

/// Privacy-first, local-only analytics store.
pub struct Analytics {
    conn: rusqlite::Connection,
}

impl Analytics {
    /// Open (or create) the history database.
    pub fn open() -> Result<Self> {
        let db_path = Self::db_path()?;
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = rusqlite::Connection::open(&db_path)
            .with_context(|| format!("failed to open analytics DB: {}", db_path.display()))?;

        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS runs (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                project     TEXT    NOT NULL,
                key         TEXT    NOT NULL,
                source      TEXT    NOT NULL,
                cmd         TEXT    NOT NULL,
                exit_code   INTEGER,
                duration_ms INTEGER,
                timestamp   TEXT    NOT NULL DEFAULT (datetime('now')),
                tty         INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_runs_project_key ON runs(project, key);
            CREATE INDEX IF NOT EXISTS idx_runs_timestamp ON runs(timestamp);
            CREATE TABLE IF NOT EXISTS detection_cache (
                project     TEXT    NOT NULL,
                detector    TEXT    NOT NULL,
                key         TEXT    NOT NULL,
                cmd         TEXT    NOT NULL,
                detected_at TEXT    NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (project, detector, key)
            );
        ")?;

        Ok(Self { conn })
    }

    /// Record a snippet execution.
    pub fn record_run(&self, record: &RunRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO runs (project, key, source, cmd, exit_code, duration_ms, tty)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                record.project,
                record.key,
                record.source,
                record.cmd,
                record.exit_code,
                record.duration_ms as i64,
                record.tty as i32,
            ],
        )?;
        Ok(())
    }

    /// Get the XDG-compliant path to the history database.
    fn db_path() -> Result<PathBuf> {
        let data_dir = dirs::data_local_dir()
            .context("could not determine local data directory")?;
        Ok(data_dir.join("snip").join("history.db"))
    }

    /// Most-used snippets for a project.
    pub fn most_used(&self, project: &Path, limit: usize) -> Result<Vec<UsageRow>> {
        let project_str = project.to_string_lossy().to_string();
        let mut stmt = self.conn.prepare(
            "SELECT key, source, COUNT(*) as runs, MAX(timestamp) as last_used
             FROM runs WHERE project = ?1
             GROUP BY key
             ORDER BY runs DESC
             LIMIT ?2"
        )?;
        let rows = stmt.query_map(rusqlite::params![project_str, limit as i64], |row| {
            Ok(UsageRow {
                key: row.get(0)?,
                source: row.get(1)?,
                count: row.get(2)?,
                last_used: row.get(3)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Snippets not used in N days for a project.
    pub fn stale(&self, project: &Path, days: u32) -> Result<Vec<StaleRow>> {
        let project_str = project.to_string_lossy().to_string();
        let mut stmt = self.conn.prepare(
            "SELECT key, source, MAX(timestamp) as last_used
             FROM runs WHERE project = ?1
             GROUP BY key
             HAVING julianday('now') - julianday(last_used) > ?2
             ORDER BY last_used ASC"
        )?;
        let rows = stmt.query_map(rusqlite::params![project_str, days as i64], |row| {
            Ok(StaleRow {
                key: row.get(0)?,
                source: row.get(1)?,
                last_used: row.get(2)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

pub struct UsageRow {
    pub key: String,
    pub source: String,
    pub count: i64,
    pub last_used: String,
}

pub struct StaleRow {
    pub key: String,
    pub source: String,
    pub last_used: String,
}
```

### 4.4 Integration Point: Recording Runs

In `cli::run`, after executing a snippet, record the run:

```rust
// In src/cli/run.rs, after execution:

let start = std::time::Instant::now();
let result = executor::execute(&resolved_cmd);
let duration = start.elapsed();

// Record to analytics (fire-and-forget — errors are silently ignored)
if let Ok(analytics) = Analytics::open() {
    let _ = analytics.record_run(&RunRecord {
        project: project_root.to_string_lossy().to_string(),
        key: resolved_key.clone(),
        source: source_file.clone(),
        cmd: resolved_cmd.clone(),
        exit_code: result.as_ref().err().and_then(|_| Some(1)),
        duration_ms: duration.as_millis() as u64,
        tty: atty::is(atty::Stream::Stdout),
    });
}
```

### 4.5 `snip stats` Command

```
$ snip stats
  Snippet usage for ~/my-project (all time)

  #  Key               Source       Runs   Last Used
  ── ────────────────  ──────────   ────   ──────────────────
  1  dev               frontend      342   2025-07-12 09:14
  2  test              frontend      187   2025-07-12 08:55
  3  build             common        134   2025-07-11 17:30
  4  lint              common         98   2025-07-11 16:22
  5  migrate           backend        67   2025-07-10 14:11
  ...
  42 seed              backend         1   2025-06-15 10:00

  Total: 42 unique snippets, 1,247 runs
```

```rust
// src/cli/stats.rs

pub fn run() -> Result<()> {
    let project_root = find_project_root()?;
    let analytics = Analytics::open()?;
    let rows = analytics.most_used(&project_root, 20)?;

    if rows.is_empty() {
        println!("No usage data yet. Run some snippets!");
        return Ok(());
    }

    println!("  Snippet usage for {} (all time)\n", project_root.display());
    println!("  {:>2}  {:<20}  {:<12}  {:>5}  {}", "#", "Key", "Source", "Runs", "Last Used");
    println!("  {:>2}  {:─<20}  {:─<12}  {:─>5}  {}", "──", "────────────────────", "────────────", "─────", "──────────────────");

    for (i, row) in rows.iter().enumerate() {
        println!("  {:>2}  {:<20}  {:<12}  {:>5}  {}",
            i + 1,
            row.key,
            row.source,
            row.count,
            row.last_used,
        );
    }

    Ok(())
}
```

### 4.6 `snip stale` Command

```
$ snip stale
  Stale snippets (not used in 30 days) for ~/my-project

  Key               Source       Last Used
  ────────────────  ──────────   ──────────────────
  seed              backend      2025-05-20 10:00
  storybook         frontend     2025-05-10 14:22
  deploy.staging    common       2025-04-30 09:00

  3 stale snippet(s). Consider removing or updating them.
```

```rust
// src/cli/stale.rs

pub fn run(days: u32) -> Result<()> {
    let project_root = find_project_root()?;
    let analytics = Analytics::open()?;
    let rows = analytics.stale(&project_root, days)?;

    if rows.is_empty() {
        println!("  No stale snippets (all used within {} days).", days);
        return Ok(());
    }

    println!("  Stale snippets (not used in {} days) for {}\n", days, project_root.display());
    println!("  {:<20}  {:<12}  {}", "Key", "Source", "Last Used");
    println!("  {:─<20}  {:─<12}  {}", "────────────────────", "────────────", "──────────────────");

    for row in &rows {
        println!("  {:<20}  {:<12}  {}", row.key, row.source, row.last_used);
    }

    println!("\n  {} stale snippet(s). Consider removing or updating them.", rows.len());
    Ok(())
}
```

CLI registration:

```rust
// In the Commands enum:
/// Show snippet usage statistics
Stats,

/// Show stale (unused) snippets
Stale {
    /// Number of days of inactivity to consider stale
    #[arg(long, default_value_t = 30)]
    days: u32,
},

/// Clear all local usage data
#[command(hide = true)]
AnalyticsReset,
```

### 4.7 Privacy Guarantees

| Concern | Guarantee |
|---------|-----------|
| Data leaves the machine? | **Never.** No `reqwest`, no HTTP client, no telemetry. |
| Data readable by other users? | SQLite file permissions: `0600` (owner read/write only). |
| Can it be disabled? | `snip config analytics.enabled false` (future) or simply delete `~/.local/share/snip/history.db`. |
| What if the DB is corrupt? | `Analytics::open()` creates tables with `IF NOT EXISTS`. On corrupt DB, the open call errors gracefully; snip still works, just no analytics. |
| How big does it get? | Each row is ~200 bytes. At 100 runs/day, that's ~7 MB/year. Negligible. |

---

## 5. Team Onboarding Flow

### 5.1 The Problem

A new hire clones the repo. They don't know:
- What tools to install (Docker, Node 20, Rust nightly, etc.)
- What the common commands are
- Whether their setup works

The current onboarding process: a wiki page that's 6 months out of date.

### 5.2 The Snip Onboarding Experience

```
Step 1: Clone repo
  $ git clone git@github.com:acme/app.git
  $ cd app

Step 2: See all commands (already works today)
  $ snip
  [common]
    lint              Run all linters
    format            Auto-format all files
  [frontend]
    dev               Start dev server with HMR
    build             Production bundle
    storybook         Launch Storybook
  [backend]
    dev               Run backend server
    migrate           Run pending DB migrations
    seed              Reset DB, migrate, then seed

Step 3: Verify setup
  $ snip doctor
  ✓ lint — prettier --check . && eslint .
  ✓ format — prettier --write .
  ✓ frontend:dev — npm run dev -- --port 5173
  ✗ backend:dev — cargo run (binary 'cargo' not found)
  ✗ backend:migrate — sqlx migrate run (binary 'sqlx' not found)
  ! 2 valid, 2 broken

Step 4: Interactive setup wizard (NEW)
  $ snip setup

  Welcome to acme/app! Let's get your environment ready.
  ══════════════════════════════════════════════

  Checking required tools...

  ✗ Rust toolchain (rustc, cargo)
      → Install: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
      → Or: brew install rust
  ✗ sqlx CLI
      → Install: cargo install sqlx-cli
  ✓ Node.js v20.11.0
  ✓ Docker 24.0.7
  ✓ kubectl v1.29.0

  2 issue(s) found. Fix now? [Y/n] y

  Installing Rust toolchain...
  (runs the install command)

  Installing sqlx CLI...
  (runs cargo install sqlx-cli)

  Verifying fixes...
  ✓ Rust toolchain installed
  ✓ sqlx CLI installed

  Running snip doctor to verify...
  ✓ All 8 snippet(s) are valid.

  Setup complete! Run `snip` to see available commands.
```

### 5.3 Setup Requirements File: `.snips.setup.toml`

The onboarding wizard is driven by a committed config file:

```toml
# .snips.setup.toml — Onboarding requirements for this project
# This file is committed and shared with the team.

[tools.node]
required = true
check = "node --version"
expected_pattern = "^v20\\."
install_hint = "Install via nvm: nvm install 20\nOr: brew install node@20"
friendly_name = "Node.js"

[tools.cargo]
required = true
check = "cargo --version"
install_hint = "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
friendly_name = "Rust toolchain"

[tools.sqlx]
required = true
check = "sqlx --version"
install_hint = "cargo install sqlx-cli --features postgres"
friendly_name = "sqlx CLI"

[tools.docker]
required = false
check = "docker --version"
install_hint = "Install Docker Desktop: https://docs.docker.com/get-docker/"
friendly_name = "Docker"

[tools.kubectl]
required = false
check = "kubectl version --client --short 2>/dev/null || kubectl version --client"
install_hint = "brew install kubectl\nOr: curl -LO https://dl.k8s.io/release/v1.29.0/bin/linux/amd64/kubectl"
friendly_name = "kubectl"

[post_setup]
# Commands to run after all tools are installed
commands = [
    "npm ci",
    "cargo build",
    "sqlx database create",
    "sqlx migrate run",
]
```

### 5.4 Architecture: `src/cli/setup.rs`

```rust
// src/cli/setup.rs

use std::path::Path;
use anyhow::{Context, Result};
use serde::Deserialize;
use colored::Colorize;

#[derive(Debug, Deserialize)]
pub struct SetupConfig {
    pub tools: std::collections::BTreeMap<String, ToolReq>,
    #[serde(default)]
    pub post_setup: Option<PostSetup>,
}

#[derive(Debug, Deserialize)]
pub struct ToolReq {
    pub required: bool,
    pub check: String,
    pub friendly_name: String,
    #[serde(default)]
    pub expected_pattern: Option<String>,
    #[serde(default)]
    pub install_hint: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct PostSetup {
    #[serde(default)]
    pub commands: Vec<String>,
}

pub fn run() -> Result<()> {
    let project_root = std::env::current_dir().context("failed to get cwd")?;
    let config_path = project_root.join(".snips.setup.toml");

    if !config_path.exists() {
        println!("{}", "No .snips.setup.toml found. Nothing to set up.".dimmed());
        println!("Create one to define tool requirements for new team members.");
        return Ok(());
    }

    let config_content = std::fs::read_to_string(&config_path)
        .with_context(|| format!("failed to read {}", config_path.display()))?;
    let config: SetupConfig = toml::from_str(&config_content)
        .with_context("failed to parse .snips.setup.toml")?;

    println!("Welcome to {}! Let's get your environment ready.", 
        project_root.file_name().unwrap_or_default().to_string_lossy().bold());
    println!("{}", "══════════════════════════════════════════════".dimmed());
    println!();
    println!("Checking required tools...\n");

    let mut failures = Vec::new();

    for (name, tool) in &config.tools {
        let output = std::process::Command::new("sh")
            .args(["-c", &tool.check])
            .output();

        let (success, version_output) = match output {
            Ok(out) => (out.status.success(), String::from_utf8_lossy(&out.stdout).trim().to_string()),
            Err(_) => (false, String::new()),
        };

        // Check expected_pattern if specified
        let pattern_ok = match (&tool.expected_pattern, &success) {
            (Some(pattern), true) => {
                regex::Regex::new(pattern).map(|re| re.is_match(&version_output)).unwrap_or(false)
            }
            _ => success,
        };

        if pattern_ok {
            println!("  {} {}", "✓".green(), tool.friendly_name);
            if !version_output.is_empty() {
                println!("      {}", version_output.dimmed());
            }
        } else {
            println!("  {} {}", "✗".red(), tool.friendly_name);
            if let Some(pattern) = &tool.expected_pattern {
                if !version_output.is_empty() {
                    println!("      {} (expected: {})", version_output.dimmed(), pattern.dimmed());
                }
            }
            if !tool.install_hint.is_empty() {
                for line in tool.install_hint.lines() {
                    println!("      {} {}", "→".yellow(), line);
                }
            }
            if tool.required {
                failures.push(name.clone());
            }
        }
    }

    println!();
    if failures.is_empty() {
        println!("{}", "✓ All tools are installed!".green());
        run_post_setup(&config.post_setup)?;
    } else {
        println!("{} {} required tool(s) missing.", "!".yellow(), failures.len());
        // In interactive mode, ask if user wants to fix
        if atty::is(atty::Stream::Stdout) {
            println!("\n  Fix now? [Y/n] ");
            // Read user input...
            // For each failure, offer to run the install_hint commands
        }
    }

    // Final: run snip doctor to verify everything works
    println!();
    println!("Running snip doctor to verify...");
    crate::cli::doctor::run_at(&project_root)?;

    Ok(())
}

fn run_post_setup(post_setup: &Option<PostSetup>) -> Result<()> {
    let Some(post) = post_setup else {
        return Ok(());
    };

    println!("\nRunning post-setup commands...\n");
    for cmd in &post.commands {
        println!("  {} {}", "$".dimmed(), cmd.cyan());
        let status = std::process::Command::new("sh")
            .args(["-c", cmd])
            .status()?;
        if status.success() {
            println!("  {} Done", "✓".green());
        } else {
            println!("  {} Failed (exit code {:?})", "✗".red(), status.code());
        }
    }

    Ok(())
}
```

### 5.5 CLI Registration

```rust
// New commands in the Commands enum:
/// Interactive onboarding wizard
Setup,
```

### 5.6 The Complete New-Hire Experience

| Step | Command | What Happens |
|------|---------|-------------|
| 1 | `git clone ... && cd app` | `.snips`, `.snips.d/`, `.snips.setup.toml` are already in the repo |
| 2 | `snip` | Shows all commands grouped by team, with source labels |
| 3 | `snip doctor` | Verifies all snippet commands work on their machine |
| 4 | `snip setup` | Interactive wizard: checks tools, installs missing ones, runs post-setup commands |
| 5 | `snip` | Everything works. No wiki page needed. |

### 5.7 `snip init --team` Flag

For project bootstrapping, add a `--team` flag to `snip init` that creates the full team structure:

```bash
$ snip init --team
  Created .snips (8 detected snippets)
  Created .snips.d/
  Created .snips.d/common.toml
  Created .snips.d/local.toml
  Created .snips.setup.toml
  Updated .gitignore (added .snips.d/local.toml)

  Team structure ready! Commit these files and share with your team.
```

This creates the `.snips.setup.toml` from a template that the team can customize:

```toml
# .snips.setup.toml — generated by snip init --team
# Define tool requirements for onboarding.
# Remove this file if onboarding checks aren't needed.

[tools.node]
required = false
check = "node --version"
friendly_name = "Node.js"
```

---

## 6. New Dependencies

| Crate | Version | Purpose | Added By |
|-------|---------|---------|----------|
| `rusqlite` | `0.31` | SQLite for analytics history DB | §4 |
| `regex` | `1` | Pattern matching for tool version checks | §5 |
| `atty` | `0.2` | Detect interactive TTY for prompts | §5 |

Note: `dirs` is already in `Cargo.toml`. `rusqlite` should use the `bundled` feature to avoid system SQLite dependency:

```toml
rusqlite = { version = "0.31", features = ["bundled"] }
```

---

## 7. Migration Path

### 7.1 Backward Compatibility

- **No breaking changes.** If `.snips.d/` doesn't exist, `snip` behaves exactly as today — reads `.snips` only.
- The `LayeredSnipFile::load()` function falls back gracefully: if `.snips.d/` is absent, only `.snips` is in the layer list.
- All existing commands (`snip init`, `snip add`, `snip run`, `snip doctor`, `snip list`) continue to work unchanged.

### 7.2 Migration Steps for Existing Projects

1. **Run `snip init --team`** — creates `.snips.d/` and `.snips.setup.toml`, updates `.gitignore`.
2. **Optionally split `.snips`** — move team-specific snippets into `.snips.d/<team>.toml` files.
3. **Commit everything** — `.snips`, `.snips.d/common.toml`, `.snips.d/frontend.toml`, etc.
4. **Each developer creates `.snips.d/local.toml`** — personal overrides, automatically gitignored.

### 7.3 Feature Flags

To allow incremental rollout, the `.snips.d/` feature is gated behind a config option:

```toml
# ~/.config/snip/config.toml (global user config)
features = { layered_snippets = true }
```

When `layered_snippets` is `false` (default for now), `.snips.d/` is ignored. When `true`, the full layer merge applies.

---

## 8. File Tree (Proposed New Modules)

```
src/
├── main.rs                      # Updated: new Commands variants
├── cli/
│   ├── mod.rs                   # Updated: pub mod setup, stats, stale
│   ├── add.rs                   # Modified: --scope flag
│   ├── doctor.rs                # Modified: --fix, --ci flags, staleness checks
│   ├── init.rs                  # Modified: --team flag, creates .snips.d/
│   ├── list.rs                  # Modified: uses LayeredSnipFile, shows source
│   ├── setup.rs                 # NEW: onboarding wizard
│   ├── stats.rs                 # NEW: usage statistics
│   ├── stale.rs                 # NEW: stale snippet detection
│   └── ... (existing unchanged)
├── core/
│   ├── mod.rs                   # Updated: pub mod layered, analytics
│   ├── snippet.rs               # Unchanged
│   ├── snipfile.rs              # Unchanged (single-file ops still work)
│   ├── layered.rs               # NEW: LayeredSnipFile, SourceEntry, merge logic
│   ├── analytics.rs             # NEW: Analytics, RunRecord, SQLite history
│   ├── staleness.rs             # NEW: drift detection, auto-fix logic
│   ├── validator.rs             # Unchanged (structural validation)
│   ├── detector.rs              # Unchanged
│   └── ... (existing unchanged)
├── detect/                      # Unchanged
└── utils/                       # Unchanged

docs/
└── rnd/
    └── 11-team-collaboration-design.md  # This file
```

---

## Summary of Changes by Feature

| Feature | New Files | Modified Files | New CLI Commands | New Flags |
|---------|-----------|---------------|------------------|-----------|
| §1 `.snips.d/` | `core/layered.rs` | `cli/init.rs`, `cli/add.rs`, `cli/list.rs`, `main.rs` | — | `--scope`, `--team` |
| §2 Env Overrides | (uses `layered.rs`) | `core/layered.rs` | — | `SNIP_ENV` env var |
| §3 Staleness | `core/staleness.rs` | `cli/doctor.rs`, `main.rs` | — | `--fix`, `--ci` |
| §4 Analytics | `core/analytics.rs`, `cli/stats.rs`, `cli/stale.rs` | `cli/run.rs`, `main.rs` | `snip stats`, `snip stale` | `--days` |
| §5 Onboarding | `cli/setup.rs` | `cli/init.rs`, `main.rs` | `snip setup` | `--team` |