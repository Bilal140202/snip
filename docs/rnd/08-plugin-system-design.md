# Plugin & Extension System Design

> Design document for `snip` Phase 2 extensibility
> Date: 2025-07-09
> Status: **Proposed** — drives Phase 2 implementation

---

## Overview

`snip` already has a clean `ProjectDetector` trait and five built-in detectors (Node, Make, Cargo, Python, Docker). This document designs four extension mechanisms that turn `snip` from a single-tool command runner into a **platform**:

1. **Detector Plugins** — custom project-type detectors via `.snips.d/`
2. **Output Plugins** — JSON, template strings, and custom formatters
3. **Pre/Post Hooks** — lifecycle hooks around snippet execution
4. **Shared Snippet Packs** — `snip pack add github:user/repo` ecosystem

Plus a **version-locking header** for forward-compatible `.snips` format evolution.

All of this is designed to work with the existing `ProjectDetector` trait, `SnipFile`/`Snippet` types, and TOML-based `.snips` format — no breaking changes.

---

## 1. Detector Plugin System

### 1.1 Directory Layout

Users create a `.snips.d/` directory next to their `.snips` file:

```
my-project/
├── .snips                  # local snippets (TOML)
├── .snips.d/
│   ├── gradle.toml         # declarative TOML detector
│   ├── kustomize.toml      # declarative TOML detector
│   └── fetch-version.sh    # script-based detector
└── ...
```

`.snips.d/` is discovered the same way as `.snips` — walking up from cwd. If both exist, they must be siblings.

### 1.2 Two Detector Types

#### Type A: TOML Declarative Detector

A `.snips.d/*.toml` file that declaratively describes how to detect and extract snippets. No code required.

```toml
# .snips.d/gradle.toml

# Human-readable name shown in `snip list` section headers
name = "Gradle"

# Detection: all conditions must be true (AND logic)
[detect]
# At least one of these files must exist
files = ["build.gradle", "build.gradle.kts", "settings.gradle"]
# All of these commands must be on $PATH
commands = ["gradle"]

# How to extract snippets
[extract]
# Run a command and parse its output
command = "gradle tasks --quiet 2>/dev/null || true"

# Parse each line of output with this regex.
# Named groups become available as {name}, {desc}, etc.
# Lines that don't match are skipped.
pattern = '^(?P<name>\S+)\s+-\s+(?P<desc>.+)$'

# Template to build the actual command from the match groups.
# {name} = "build", {desc} = "Assembles the outputs", etc.
cmd_template = "gradle {name}"

# Optional: only include tasks matching this filter regex
filter = '^(build|test|bootRun|dependencies|clean)'

# Optional: section prefix for the snippet keys
# Without this, keys would be "build", "test", etc.
# With section = "gradle", keys become "gradle.build", "gradle.test"
section = "gradle"

# Optional: tag all extracted snippets
tags = ["build", "gradle"]

# Optional: set working directory (relative to project root)
# dir = "android"

# Optional: timeout in milliseconds for the extract command
timeout = 10000
```

**Result**: Running `snip list` in a Gradle project would show:

```
  [gradle]
    gradle:build    Assembles the outputs
    gradle:test     Runs the unit tests
    gradle:clean    Deletes the build directory
```

#### Type B: Shell Script Detector

A `.snips.d/*.sh` file that outputs **JSON lines** (one JSON object per line, each representing a snippet).

```bash
#!/usr/bin/env bash
# .snips.d/fetch-version.sh
# Detects if the project has a VERSION file and generates a snippet

set -euo pipefail

# Detection: exit 0 = project detected, exit non-zero = not detected
if [ ! -f VERSION ]; then
  exit 1
fi

VERSION=$(cat VERSION | tr -d '[:space:]')

# Output JSON lines. Each line is one snippet.
# Required fields: name, cmd
# Optional fields: desc, section, tags, shell, dir
cat <<EOF
{"name":"version","cmd":"echo $VERSION","desc":"Print current version","section":"meta","tags":["version"]}
{"name":"bump-patch","cmd":"echo $((VERSION + 1)) > VERSION && git add VERSION","desc":"Bump patch version","section":"meta"}
EOF
```

**Contract**:
1. Script is run with `bash <script>` from the project root
2. Exit code 0 + no stdout → project detected, no snippets found (e.g. empty Gradle project)
3. Exit code 0 + stdout → project detected, parse each line as JSON
4. Exit code non-zero → project not detected, skip entirely

### 1.3 Rust Trait Extension

The existing `ProjectDetector` trait stays as-is. We add a new `PluginDetector` wrapper:

```rust
// src/detect/plugin.rs

use std::path::{Path, PathBuf};
use anyhow::Result;
use super::{DetectedSnippet, ProjectDetector};

/// A TOML-based declarative detector loaded from `.snips.d/*.toml`.
pub struct TomlDetector {
    config: TomlDetectorConfig,
    source_path: PathBuf,  // for error messages
}

/// Parsed structure of a `.snips.d/*.toml` file.
#[derive(Debug, Deserialize)]
pub struct TomlDetectorConfig {
    pub name: String,

    #[serde(default)]
    pub detect: DetectConfig,

    pub extract: ExtractConfig,

    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct DetectConfig {
    /// Files that must exist (any one = match).
    #[serde(default)]
    pub files: Vec<String>,

    /// Commands that must be on $PATH (all must exist = match).
    #[serde(default)]
    pub commands: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ExtractConfig {
    /// Shell command to run. stdout is parsed line-by-line.
    pub command: String,

    /// Regex with named groups for parsing each output line.
    pub pattern: String,

    /// Go-style template for the command. {name}, {desc}, etc.
    /// from regex named groups.
    pub cmd_template: String,

    /// Optional: only include matches matching this regex.
    #[serde(default)]
    pub filter: Option<String>,

    /// Optional: section prefix for snippet keys.
    #[serde(default)]
    pub section: Option<String>,

    /// Optional: working directory relative to project root.
    #[serde(default)]
    pub dir: Option<String>,

    /// Optional: timeout in milliseconds.
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

fn default_timeout() -> u64 { 10_000 }

impl TomlDetector {
    /// Load a TOML detector from a file path.
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: TomlDetectorConfig = toml::from_str(&content)
            .map_err(|e| anyhow::anyhow!("invalid detector TOML in {}: {}", path.display(), e))?;
        Ok(Self {
            config,
            source_path: path.to_path_buf(),
        })
    }
}

impl ProjectDetector for TomlDetector {
    fn name(&self) -> &str { &self.config.name }

    fn detect(&self, root: &Path) -> bool {
        // Files check: at least one must exist
        let files_match = if self.config.detect.files.is_empty() {
            true
        } else {
            self.config.detect.files.iter().any(|f| root.join(f).exists())
        };

        // Commands check: all must be on PATH
        let cmds_match = if self.config.detect.commands.is_empty() {
            true
        } else {
            self.config.detect.commands.iter().all(|cmd| {
                which::which(cmd).is_ok()
            })
        };

        files_match && cmds_match
    }

    fn extract(&self, root: &Path) -> Vec<DetectedSnippet> {
        let output = match crate::core::executor::execute_capture_timed(
            &self.config.extract.command,
            root,
            std::time::Duration::from_millis(self.config.extract.timeout),
        ) {
            Ok(o) => o,
            Err(_) => return Vec::new(),
        };

        let regex = match regex::Regex::new(&self.config.extract.pattern) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        let filter_regex = self.config.extract.filter.as_ref()
            .and_then(|f| regex::Regex::new(f).ok());

        let section = self.config.extract.section.clone()
            .unwrap_or_else(|| self.config.name.to_lowercase());

        let mut snippets = Vec::new();
        for line in output.lines() {
            let caps = match regex.captures(line) {
                Some(c) => c,
                None => continue,
            };

            let name = caps.name("name")
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();

            // Apply filter if present
            if let Some(ref filt) = filter_regex {
                if !filt.is_match(&name) {
                    continue;
                }
            }

            let desc = caps.name("desc")
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();

            // Build command from template
            let cmd = self.config.extract.cmd_template.clone();
            let mut cmd = cmd;
            for cap in regex.capture_names().flatten() {
                if let Some(value) = caps.name(cap) {
                    cmd = cmd.replace(&format!("{{{}}}", cap), value.as_str());
                }
            }

            snippets.push((section.clone(), name, cmd, desc));
        }

        snippets
    }
}
```

**Script-based detector:**

```rust
// src/detect/script.rs

/// A shell-script-based detector loaded from `.snips.d/*.sh`.
pub struct ScriptDetector {
    name: String,
    script_path: PathBuf,
}

impl ScriptDetector {
    pub fn from_file(path: &Path) -> Result<Self> {
        let name = path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("script")
            .to_string();

        // Validate it's executable-ish (we'll run via bash regardless)
        Ok(Self {
            name,
            script_path: path.to_path_buf(),
        })
    }
}

impl ProjectDetector for ScriptDetector {
    fn name(&self) -> &str { &self.name }

    fn detect(&self, root: &Path) -> bool {
        std::process::Command::new("bash")
            .arg(&self.script_path)
            .current_dir(root)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn extract(&self, root: &Path) -> Vec<DetectedSnippet> {
        let output = match std::process::Command::new("bash")
            .arg(&self.script_path)
            .current_dir(root)
            .output()
        {
            Ok(o) if o.status.success() => o,
            _ => return Vec::new(),
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut snippets = Vec::new();

        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() { continue; }

            match serde_json::from_str::<ScriptSnippet>(trimmed) {
                Ok(s) => {
                    let section = s.section.unwrap_or_else(|| self.name.clone());
                    snippets.push((
                        section,
                        s.name,
                        s.cmd,
                        s.desc.unwrap_or_default(),
                    ));
                }
                Err(_) => {
                    // Log warning but don't fail
                    eprintln!("warning: skipping malformed JSON line from {}", self.name);
                }
            }
        }

        snippets
    }
}

/// JSON structure output by script detectors (one per line).
#[derive(Debug, Deserialize)]
struct ScriptSnippet {
    name: String,
    cmd: String,
    desc: Option<String>,
    section: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    shell: Option<String>,
    dir: Option<String>,
}
```

### 1.4 Plugin Discovery & Loading

Modified `detect_all` to include plugins:

```rust
// src/detect/mod.rs (modified)

use std::path::PathBuf;

/// Return all detectors: built-in + plugins from `.snips.d/`.
pub fn all_detectors(root: &Path) -> Vec<Box<dyn ProjectDetector>> {
    let mut detectors: Vec<Box<dyn ProjectDetector>> = vec![
        Box::new(node::NodeDetector),
        Box::new(makefile::MakeDetector),
        Box::new(cargo::CargoDetector),
        Box::new(python::PythonDetector),
        Box::new(docker::DockerDetector),
    ];

    // Load plugins from .snips.d/
    if let Some(plugins_dir) = find_plugins_dir(root) {
        if let Ok(entries) = std::fs::read_dir(&plugins_dir) {
            let mut plugin_files: Vec<PathBuf> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_file())
                .collect();

            // Sort for deterministic ordering
            plugin_files.sort();

            for path in plugin_files {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                match ext {
                    "toml" => {
                        if let Ok(detector) = plugin::TomlDetector::from_file(&path) {
                            detectors.push(Box::new(detector));
                        }
                    }
                    "sh" => {
                        if let Ok(detector) = plugin::ScriptDetector::from_file(&path) {
                            detectors.push(Box::new(detector));
                        }
                    }
                    _ => {}  // ignore unknown extensions
                }
            }
        }
    }

    detectors
}

/// Find the `.snips.d/` directory by walking up from root,
/// looking for it alongside a `.snips` file.
fn find_plugins_dir(start: &Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        if dir.join(".snips").is_file() && dir.join(".snips.d").is_dir() {
            return Some(dir.join(".snips.d"));
        }
        if !dir.pop() { return None; }
    }
}
```

### 1.5 More TOML Detector Examples

```toml
# .snips.d/kustomize.toml
name = "Kustomize"

[detect]
files = ["kustomization.yaml", "kustomization.yml"]

[extract]
command = "find . -name 'kustomization.yaml' -not -path '*/vendor/*' | head -20"
pattern = '^\./?(?P<name>.+)/kustomization\.yaml$'
cmd_template = "kubectl apply -k {name}"
section = "k8s"
filter = "^((base|overlays)/)"
```

```toml
# .snips.d/makefile-completions.toml
name = "Makefile-completions"

[detect]
files = ["Makefile", "makefile"]

[extract]
# Extract targets that have a comment on the same or preceding line
command = "grep -E '^[a-zA-Z_-]+:.*##' Makefile 2>/dev/null || grep -B1 '^[a-zA-Z_-]+:' Makefile 2>/dev/null || true"
pattern = '^((?P<name>[a-zA-Z_-]+):[^\#]*)#*\s*(?P<desc>.+)$'
cmd_template = "make {name}"
section = "make"
```

---

## 2. Output Plugin System

### 2.1 CLI Interface

```rust
// Extended list command CLI
List {
    /// Output format: human, json, or a template string.
    #[arg(long, default_value = "human")]
    format: String,

    /// Filter snippets by tag.
    #[arg(long)]
    tag: Vec<String>,

    /// Filter snippets by section.
    #[arg(long)]
    section: Vec<String>,
}
```

Usage:
```bash
snip list                          # human-readable (current default)
snip list --json                   # machine-readable JSON
snip list --format "{{key}}: {{cmd}}"     # custom template
snip list --format '{{key}}\t{{desc}}'    # tab-separated for fzf
snip list --tag build              # filter by tag
snip list --section gradle         # filter by section
```

### 2.2 Output Formatter Trait

```rust
// src/ui/format.rs

use crate::core::snippet::{Snippet, SnipFile};
use std::io::Write;

/// A single formatted entry, ready for output.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FormattedEntry {
    pub key: String,
    pub section: String,
    pub name: String,        // key minus section prefix
    pub cmd: String,
    pub desc: String,
    pub tags: Vec<String>,
    pub source: String,      // ".snips", "detector:Gradle", "pack:rust-common"
    pub has_vars: bool,
}

/// Trait for output formatters.
pub trait OutputFormatter {
    /// Format a header/separator. Called once before entries.
    fn header(&self, _out: &mut dyn Write) -> std::io::Result<()> { Ok(()) }

    /// Format a single entry. Called once per snippet.
    fn format_entry(&self, entry: &FormattedEntry, out: &mut dyn Write) -> std::io::Result<()>;

    /// Format a footer. Called once after all entries.
    fn footer(&self, _out: &mut dyn Write) -> std::io::Result<()> { Ok(()) }
}
```

### 2.3 Built-in Formatters

#### Human Formatter (default, current behavior)

```rust
pub struct HumanFormatter {
    /// Whether to use color (auto-detected from terminal).
    color: bool,
}

impl OutputFormatter for HumanFormatter {
    fn format_entry(&self, entry: &FormattedEntry, out: &mut dyn Write) -> std::io::Result<()> {
        let desc = if entry.desc.is_empty() { &entry.cmd } else { &entry.desc };
        if self.color {
            writeln!(out, "  {:width$} {}", entry.key.cyan(), desc.dimmed(), width = 24)?;
        } else {
            writeln!(out, "  {:width$} {}", entry.key, desc, width = 24)?;
        }
        Ok(())
    }
}
```

#### JSON Formatter

```rust
pub struct JsonFormatter {
    /// Pretty-print with indentation.
    pretty: bool,
}

impl OutputFormatter for JsonFormatter {
    fn header(&self, _out: &mut dyn Write) -> std::io::Result<()> { Ok(()) }

    fn format_entry(&self, entry: &FormattedEntry, out: &mut dyn Write) -> std::io::Result<()> {
        let json = if self.pretty {
            serde_json::to_string_pretty(entry).unwrap()
        } else {
            serde_json::to_string(entry).unwrap()
        };
        // JSON Lines format: one JSON object per line (unless pretty)
        if self.pretty {
            // For pretty, buffer and output as array at footer
            write!(out, "{}", json)?;
        } else {
            writeln!(out, "{}", json)?;
        }
        Ok(())
    }
}
```

**JSON output example:**

```bash
$ snip list --json
{"key":"build","section":"build","name":"","cmd":"cargo build","desc":"Build the project","tags":[],"source":".snips","has_vars":false}
{"key":"build.release","section":"build","name":"release","cmd":"cargo build --release","desc":"Release build","tags":[],"source":".snips","has_vars":false}
{"key":"gradle.build","section":"gradle","name":"build","cmd":"gradle build","desc":"Assembles the outputs","tags":["build","gradle"],"source":"detector:Gradle","has_vars":false}
```

```bash
$ snip list --json | jq '.key'
"build"
"build.release"
"gradle.build"
```

#### Template Formatter

```rust
pub struct TemplateFormatter {
    /// The format string with {{key}}, {{cmd}}, {{desc}}, {{section}}, {{name}}, {{tags}} placeholders.
    template: String,
}

impl TemplateFormatter {
    pub fn new(template: &str) -> Self {
        Self { template: template.to_string() }
    }
}

impl OutputFormatter for TemplateFormatter {
    fn format_entry(&self, entry: &FormattedEntry, out: &mut dyn Write) -> std::io::Result<()> {
        let mut line = self.template.clone();
        line = line.replace("{{key}}", &entry.key);
        line = line.replace("{{cmd}}", &entry.cmd);
        line = line.replace("{{desc}}", &entry.desc);
        line = line.replace("{{section}}", &entry.section);
        line = line.replace("{{name}}", &entry.name);
        line = line.replace("{{tags}}", &entry.tags.join(","));
        line = line.replace("{{source}}", &entry.source);
        writeln!(out, "{}", line)?;
        Ok(())
    }
}
```

### 2.4 Custom Formatter Files

Users can place custom formatter definitions in `.snips.d/formats/`:

```
.snips.d/
├── formats/
│   ├── fzf.toml        # optimized for fzf --preview
│   ├── markdown.toml   # renders as a markdown table
│   └── csv.toml         # comma-separated values
```

```toml
# .snips.d/formats/fzf.toml
name = "fzf"
# Template string using the same placeholders as --format
template = "{{key}}\t{{desc}}"
description = "Tab-separated for piping to fzf"
```

```toml
# .snips.d/formats/csv.toml
name = "csv"
template = "{{key}},{{cmd}},{{desc}},{{tags}}"
description = "CSV output for spreadsheets"
```

```toml
# .snips.d/formats/markdown.toml
name = "markdown"
# For multi-line formats, the template is applied per-line.
# The "header" and "footer" fields provide the table bookends.
header = "| Key | Command | Description |"
separator = "|-----|---------|-------------|"
template = "| {{key}} | `{{cmd}}` | {{desc}} |"
description = "Markdown table output"
```

Usage:
```bash
snip list --format fzf       # loads .snips.d/formats/fzf.toml
snip list --format csv       # loads .snips.d/formats/csv.toml
snip list --format markdown  # loads .snips.d/formats/markdown.toml
```

**Resolution order for `--format <NAME>`:**
1. Built-in names: `human`, `json`
2. Files in `.snips.d/formats/<NAME>.toml`
3. Raw template string if it contains `{{` (detected heuristically)

### 2.5 Formatter Registry

```rust
// src/ui/format.rs (continued)

pub struct FormatRegistry {
    formatters: HashMap<String, Box<dyn OutputFormatter>>,
}

impl FormatRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            formatters: HashMap::new(),
        };
        registry.register("human", Box::new(HumanFormatter::new(true)));
        registry.register("json", Box::new(JsonFormatter::new(false)));
        registry.register("json-pretty", Box::new(JsonFormatter::new(true)));
        registry
    }

    pub fn register(&mut self, name: impl Into<String>, formatter: Box<dyn OutputFormatter>) {
        self.formatters.insert(name.into(), formatter);
    }

    /// Resolve a format name/string to a formatter.
    /// Returns the formatter or an error.
    pub fn resolve(&self, input: &str, root: &Path) -> Result<Box<dyn OutputFormatter>> {
        // 1. Check built-in registry
        if let Some(fmt) = self.formatters.get(input) {
            // We'd need to clone or use Arc here; simplified for design doc
            return Ok(/* cloned formatter */);
        }

        // 2. Check .snips.d/formats/<input>.toml
        let format_path = find_plugins_dir(root)
            .map(|d| d.join("formats").join(format!("{}.toml", input)));
        if let Some(ref path) = format_path {
            if path.exists() {
                let config: FormatConfig = /* load and parse */;
                return Ok(Box::new(TemplateFormatter::new(&config.template)));
            }
        }

        // 3. Treat as raw template if it contains {{
        if input.contains("{{") {
            return Ok(Box::new(TemplateFormatter::new(input)));
        }

        anyhow::bail!("unknown format: '{}'. Use --format human|json|<template>", input)
    }
}
```

---

## 3. Pre/Post Hooks

### 3.1 Hook Locations

Hooks are defined in three places, with this precedence (highest to lowest):

1. **Per-snippet hooks** in `.snips` (inline with the snippet definition)
2. **Project-wide hooks** in `.snips.d/hooks.toml`
3. **User-global hooks** in `~/.config/snip/hooks.toml`

### 3.2 Hook Definition in `.snips` (Per-Snippet)

Extend the `Snippet` struct with hook fields:

```rust
// Extension to Snippet struct in src/core/snippet.rs

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Snippet {
    pub cmd: String,
    pub desc: String,
    pub vars: Vec<VarDef>,
    pub tags: Vec<String>,
    pub shell: Option<String>,
    pub dir: Option<String>,

    // ── New hook fields ──
    /// Command to run before this snippet. Exit non-zero = abort.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_hook: Option<String>,

    /// Command to run after this snippet (regardless of success/failure).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_hook: Option<String>,

    /// Command to run only if the snippet fails (exit non-zero).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_fail: Option<String>,

    /// Environment variables to set when running this snippet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<std::collections::HashMap<String, String>>,
}
```

**Example `.snips` with hooks:**

```toml
[deploy.production]
cmd = "kubectl apply -f k8s/production/"
desc = "Deploy to production"
pre_hook = "kubectl version --client > /dev/null 2>&1"
post_hook = "slack-notify 'Deploy to production complete'"
on_fail = "slack-notify 'DEPLOY FAILED' --channel #alerts"

[db.reset]
cmd = "dropdb mydb && createdb mydb && migrate up"
desc = "Reset the database"
env = { PGDATABASE = "mydb" }
pre_hook = "echo 'WARNING: This will destroy all data!' && read -p 'Continue? (yes/no) ' CONFIRM && [ \"$CONFIRM\" = 'yes' ]"
```

### 3.3 Project-Wide Hooks (`.snips.d/hooks.toml`)

```toml
# .snips.d/hooks.toml

# Global pre-run hook: runs before ANY snippet.
# $SNIP_KEY and $SNIP_CMD are set to the snippet being run.
[pre_run]
command = "docker info > /dev/null 2>&1"
# "required" means the snippet is aborted if this fails.
required = true
# "message" is shown if the hook fails.
message = "Docker is not running. Start Docker first."

# Global post-run hook: runs after ANY snippet.
[post_run]
command = ""
# Leave command empty to skip. Or set a notification:
# command = "osascript -e 'display notification \"Done: $SNIP_KEY\" with title \"snip\"'"

# Snippet-specific overrides by key pattern (glob-style).
# These run IN ADDITION to per-snippet hooks.
[[hooks]]
# Match snippets whose key matches this glob pattern.
pattern = "deploy.*"
pre = "echo 'Deploying...' && kubectl version --client > /dev/null 2>&1"
post = "slack-notify 'Deployed $SNIP_KEY successfully'"
```

### 3.4 Hook Trait & Execution Engine

```rust
// src/core/hooks.rs

use std::collections::HashMap;
use std::path::Path;
use anyhow::Result;

/// Context passed to hook execution.
#[derive(Debug, Clone)]
pub struct HookContext {
    /// The fully-qualified snippet key (e.g. "deploy.production").
    pub key: String,
    /// The resolved command (after variable substitution).
    pub cmd: String,
    /// The project root directory.
    pub project_root: std::path::PathBuf,
    /// Environment variables from the snippet definition.
    pub env: HashMap<String, String>,
}

/// A hook definition.
#[derive(Debug, Clone, Default)]
pub struct HookDef {
    /// Shell command to execute.
    pub command: String,
    /// Whether failure aborts the snippet.
    pub required: bool,
    /// Message to show on failure.
    pub message: Option<String>,
    /// Timeout in milliseconds (default: 30 seconds).
    pub timeout_ms: u64,
}

/// Lifecycle hook point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookPoint {
    PreRun,
    PostRun,
    OnFail,
}

/// Trait for hook providers. The engine collects hooks from all providers
/// and runs them in order at each hook point.
pub trait HookProvider {
    /// Return hooks for the given hook point and snippet key.
    /// Multiple hooks can be returned; they run in order.
    fn hooks_for(&self, point: HookPoint, key: &str) -> Vec<HookDef>;
}

/// Built-in hook provider that reads `.snips.d/hooks.toml`.
pub struct FileHookProvider {
    config: HooksConfig,
}

/// Parsed `.snips.d/hooks.toml` structure.
#[derive(Debug, Deserialize, Default)]
pub struct HooksConfig {
    pre_run: Option<GlobalHook>,
    post_run: Option<GlobalHook>,
    #[serde(default)]
    hooks: Vec<PatternHook>,
}

#[derive(Debug, Deserialize)]
pub struct GlobalHook {
    command: String,
    #[serde(default)]
    required: bool,
    message: Option<String>,
    #[serde(default = "default_hook_timeout")]
    timeout_ms: u64,
}

#[derive(Debug, Deserialize)]
pub struct PatternHook {
    pattern: String,
    pre: Option<String>,
    post: Option<String>,
    on_fail: Option<String>,
}

fn default_hook_timeout() -> u64 { 30_000 }

/// The hook execution engine.
pub struct HookEngine {
    providers: Vec<Box<dyn HookProvider>>,
}

impl HookEngine {
    pub fn new() -> Self {
        Self { providers: Vec::new() }
    }

    pub fn add_provider(&mut self, provider: Box<dyn HookProvider>) {
        self.providers.push(provider);
    }

    /// Run all pre-run hooks for the given key.
    /// Returns Ok(()) if all required hooks passed.
    pub fn run_pre(&self, ctx: &HookContext) -> Result<()> {
        for provider in &self.providers {
            for hook in provider.hooks_for(HookPoint::PreRun, &ctx.key) {
                self.execute_hook(&hook, ctx)?;
            }
        }
        Ok(())
    }

    /// Run all post-run hooks for the given key.
    pub fn run_post(&self, ctx: &HookContext) {
        for provider in &self.providers {
            for hook in provider.hooks_for(HookPoint::PostRun, &ctx.key) {
                if let Err(e) = self.execute_hook(&hook, ctx) {
                    eprintln!("warning: post-hook failed: {}", e);
                    // Post hooks never abort; they're best-effort.
                }
            }
        }
    }

    /// Run all on-fail hooks for the given key.
    pub fn run_on_fail(&self, ctx: &HookContext, error: &anyhow::Error) {
        for provider in &self.providers {
            for hook in provider.hooks_for(HookPoint::OnFail, &ctx.key) {
                if let Err(e) = self.execute_hook(&hook, ctx) {
                    eprintln!("warning: on-fail hook failed: {}", e);
                }
            }
        }
    }

    fn execute_hook(&self, hook: &HookDef, ctx: &HookContext) -> Result<()> {
        if hook.command.is_empty() { return Ok(()); }

        let cmd = hook.command
            .replace("$SNIP_KEY", &ctx.key)
            .replace("$SNIP_CMD", &ctx.cmd);

        let result = std::process::Command::new("sh")
            .args(["-c", &cmd])
            .current_dir(&ctx.project_root)
            .envs(&ctx.env)
            .timeout(std::time::Duration::from_millis(hook.timeout_ms))
            .status();

        match result {
            Ok(status) if status.success() => Ok(()),
            Ok(_) => {
                if hook.required {
                    let msg = hook.message.as_deref()
                        .unwrap_or(&format!("pre-hook '{}' failed", hook.command));
                    anyhow::bail!("{}", msg);
                }
                Ok(())
            }
            Err(e) => {
                if hook.required {
                    anyhow::bail!("hook execution error: {}", e);
                }
                Ok(())
            }
        }
    }
}
```

### 3.5 Integration with Executor

The modified run flow:

```rust
// Pseudocode for the modified run path in cli/run.rs

pub fn run(name: &str) -> Result<()> {
    let file = read_snippets(&snipfile_path)?;
    let snippet = file.get(name)?;

    let cmd = resolve_variables(snippet)?;
    let ctx = HookContext {
        key: name.to_string(),
        cmd: cmd.clone(),
        project_root: project_root.clone(),
        env: snippet.env.clone().unwrap_or_default(),
    };

    // 1. Pre-run hooks (can abort)
    hook_engine.run_pre(&ctx)?;

    // 2. Execute the snippet
    let result = executor::execute(&cmd);

    // 3. Post-run or on-fail hooks
    match &result {
        Ok(()) => hook_engine.run_post(&ctx),
        Err(e) => {
            // Check for per-snippet on_fail hook
            if let Some(ref fail_cmd) = snippet.on_fail {
                let fail_ctx = HookContext { cmd: fail_cmd.clone(), ..ctx };
                hook_engine.run_on_fail(&fail_ctx, e);
            }
        }
    }

    result
}
```

---

## 4. Shared Snippet Packs

### 4.1 Concept

Snippet packs are shareable `.snips` files hosted on GitHub (or any git repo). Users install them, and the snippets are **merged** into their local snippet list. Local snippets always win on key conflicts.

This is how `snip` gets a community ecosystem — similar to `nvm`'s `.nvmrc`, `asdf`'s plugin system, or `zsh`'s oh-my-zsh themes.

### 4.2 Pack Repository Format

A pack repo contains a `.snips` file (or multiple in subdirectories):

```
github.com/snip-packs/rust-common/
├── .snips              # The pack's snippets
├── README.md           # Description for `snip pack info`
└── .snips-pack.toml    # Pack metadata
```

```toml
# .snips-pack.toml (pack metadata)
name = "rust-common"
version = "1.2.0"
description = "Common Rust/Cargo snippets for all projects"
author = "snip-packs"
tags = ["rust", "cargo", "language"]
```

**Example pack `.snips`:**

```toml
[check]
cmd = "cargo check"
desc = "Type-check without building"
tags = ["rust", "fast"]

[clippy]
cmd = "cargo clippy -- -D warnings"
desc = "Run linter with strict warnings"
tags = ["rust", "lint"]

[doc]
cmd = "cargo doc --no-deps --open"
desc = "Build and open documentation"
tags = ["rust", "docs"]

[audit]
cmd = "cargo audit"
desc = "Check for known security vulnerabilities"
tags = ["rust", "security"]
```

### 4.3 Pack Storage

Installed packs are stored in `~/.config/snip/packs/`:

```
~/.config/snip/
├── config.toml          # global snip config
├── hooks.toml           # global hooks
└── packs/
    ├── registry.toml    # list of installed packs
    └── github.com/
        └── snip-packs/
            └── rust-common/
                ├── .snips
                └── .snips-pack.toml
```

**`registry.toml`:**

```toml
# ~/.config/snip/packs/registry.toml

[[packs]]
source = "github:snip-packs/rust-common"
installed_at = "2025-07-09T14:30:00Z"
commit = "abc1234"
version = "1.2.0"
enabled = true

[[packs]]
source = "github:snip-packs/docker-compose"
installed_at = "2025-07-09T15:00:00Z"
commit = "def5678"
version = "0.5.0"
enabled = true
```

### 4.4 CLI Interface

```rust
// src/cli/pack.rs

/// Manage shared snippet packs.
#[derive(Subcommand)]
pub enum PackCmd {
    /// Install a snippet pack from a GitHub repo.
    /// Usage: snip pack add github:user/repo[@version]
    Add {
        /// Pack source: github:user/repo or a git URL.
        source: String,
    },

    /// Remove an installed pack.
    Rm {
        /// Pack name or source.
        name: String,
    },

    /// List installed packs.
    #[command(alias = "ls")]
    List,

    /// Show details about a pack.
    Info {
        /// Pack name.
        name: String,
    },

    /// Update all (or specific) packs to latest.
    Update {
        /// Specific pack to update (updates all if omitted).
        name: Option<String>,
    },

    /// Enable or disable a pack without removing it.
    Enable {
        name: String,
    },

    /// Disable a pack temporarily.
    Disable {
        name: String,
    },
}
```

### 4.5 Pack Merging

The merge algorithm is straightforward — **last-write-wins with local priority**:

```rust
// src/core/pack.rs

use crate::core::snippet::{SnipFile, Snippet};

/// Merge multiple SnipFiles into one.
/// Later files override earlier files on key conflicts.
/// The local .snips is always last (highest priority).
pub fn merge_snipfiles(files: Vec<(&str, &SnipFile)>) -> SnipFile {
    let mut merged = SnipFile::new();

    for (source, file) in files {
        for (key, snippet) in file.iter() {
            // Only insert if not already present (first-write-wins
            // except local .snips which is last)
            if merged.get(key).is_none() {
                merged.insert(key.clone(), snippet.clone());
            }
        }
    }

    merged
}

/// Collect all snippet sources in priority order:
/// 1. Installed packs (in order, alphabetical by name)
/// 2. Detector plugins
/// 3. Local .snips (highest priority)
pub fn collect_all_snippets(
    root: &Path,
    local_snipfile: Option<&SnipFile>,
) -> SnipFile {
    let mut sources: Vec<(&str, SnipFile)> = Vec::new();

    // 1. Packs (lowest priority)
    let packs = load_enabled_packs();
    for (pack_name, pack_snipfile) in &packs {
        sources.push((pack_name, pack_snipfile.clone()));
    }

    // 2. Detector snippets
    let detected: SnipFile = detect_to_snipfile(root);
    if !detected.is_empty() {
        sources.push(("detected", detected));
    }

    // 3. Local .snips (highest priority — overrides everything)
    if let Some(local) = local_snipfile {
        sources.push(("local", local.clone()));
    }

    merge_snipfiles(sources)
}
```

### 4.6 Pack Source Resolution

```rust
/// Resolve a pack source string to a git clone URL.
///
/// Supported formats:
///   - "github:user/repo"        → https://github.com/user/repo
///   - "github:user/repo@v1.2"   → https://github.com/user/repo (checkout v1.2 tag)
///   - "https://example.com/repo" → direct git URL
pub fn resolve_pack_source(source: &str) -> Result<(String, Option<String>)> {
    if let Some(rest) = source.strip_prefix("github:") {
        let (repo, version) = if let Some(at) = rest.find('@') {
            (&rest[..at], Some(rest[at + 1..].to_string()))
        } else {
            (rest, None)
        };
        Ok((format!("https://github.com/{}.git", repo), version))
    } else if source.starts_with("http://") || source.starts_with("https://") {
        Ok((source.to_string(), None))
    } else {
        anyhow::bail!("unsupported pack source: '{}'. Use github:user/repo or a URL.", source)
    }
}
```

### 4.7 Example Workflow

```bash
# Install a community pack
$ snip pack add github:snip-packs/rust-common
Installed pack 'rust-common' (v1.2.0) from github:snip-packs/rust-common

# See what you got
$ snip list
  [local]
    local:deploy      Deploy to staging

  [pack:rust-common]
    check             Type-check without building
    clippy            Run linter with strict warnings
    doc               Build and open documentation
    audit             Check for security vulnerabilities

  [detector:Gradle]
    gradle:build      Assembles the outputs
    gradle:test       Runs the unit tests

# Override a pack snippet locally
$ snip add clippy "cargo clippy -- -D warnings --all-targets"
# Now 'clippy' comes from local .snips, not the pack

# List installed packs
$ snip pack list
  rust-common  v1.2.0  github:snip-packs/rust-common  enabled
  docker       v0.5.0  github:snip-packs/docker-compose  enabled

# Update all packs
$ snip pack update
Updated rust-common: v1.2.0 → v1.3.0
docker-compose is already up to date.

# Disable a pack temporarily
$ snip pack disable rust-common
Disabled pack 'rust-common'

# Remove a pack
$ snip pack rm docker-compose
Removed pack 'docker-compose'
```

---

## 5. Version Locking

### 5.1 Problem

The `.snips` TOML format will evolve. New fields will be added (hooks, env, etc.). Old `snip` binaries need to handle new files gracefully, and new binaries need to handle old files.

### 5.2 Design: `format` Header in `.snips`

```toml
# .snips
format = "1.0"

[build]
cmd = "cargo build"
desc = "Build the project"
```

The `format` key is a top-level string at the root of the TOML document. It is **not** a snippet (it has no `cmd` field), so the existing `walk_table` parser naturally skips it.

### 5.3 Version Registry

```rust
// src/core/version.rs

/// Supported .snips format versions.
pub static SUPPORTED_VERSIONS: &[&str] = &["1.0"];

/// The minimum format version this binary supports.
pub static MIN_VERSION: &str = "1.0";

/// The maximum format version this binary supports.
pub static MAX_VERSION: &str = "1.0";

/// Parse the format version from a TOML document.
/// Returns None if no format header is present (assumed v1.0).
pub fn parse_format_version(value: &toml::Value) -> Option<String> {
    value
        .as_table()
        .and_then(|t| t.get("format"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Validate that the format version is supported by this binary.
/// Returns an error with upgrade instructions if not.
pub fn validate_version(version: &str) -> Result<()> {
    if !SUPPORTED_VERSIONS.contains(&version) {
        anyhow::bail!(
            "Unsupported .snips format version '{}'. \
             This version of snip supports format versions {} to {}. \
             Upgrade snip: https://github.com/...",
            version, MIN_VERSION, MAX_VERSION
        );
    }
    Ok(())
}
```

### 5.4 Version Evolution Rules

| Scenario | Behavior |
|----------|----------|
| No `format` key | Treated as `"1.0"` (backward compatible) |
| `format = "1.0"` | Current version, all features available |
| `format = "1.1"` (new binary, old file) | New binary reads v1.0 files (backward compatible) |
| `format = "2.0"` (old binary, new file) | Old binary errors with upgrade message |
| Unknown fields in v1.0 | Silently ignored (forward compatible via `serde(deny_unknown_fields = false)`) |

### 5.5 Future Version Example

When we add hooks to the core format (v2.0), the `.snips` file might look like:

```toml
# .snips (format v2.0 — hypothetical future)
format = "2.0"

# Global hooks (v2.0 feature)
[hooks.pre_run]
command = "docker info > /dev/null 2>&1"
required = true

[deploy]
cmd = "kubectl apply -f k8s/"
desc = "Deploy to cluster"
pre_hook = "kubectl version --client"  # v2.0 feature
post_hook = "notify 'Deployed!'"
env = { CLUSTER = "production" }
```

A v1.0 binary reading this file would:
1. See `format = "2.0"` → error with "upgrade snip" message
2. Never try to parse the v2.0 fields

A v2.0 binary reading a v1.0 file would:
1. See no `format` key or `format = "1.0"` → use v1.0 parser
2. Hooks default to empty, env defaults to empty

### 5.6 Integration with SnipFile Parser

```rust
// Modified read_snippets in src/core/snipfile.rs

pub fn read_snippets(path: &Path) -> Result<SnipFile> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read .snips file: {}", path.display()))?;

    let value: toml::Value = content
        .parse::<toml::Value>()
        .with_context(|| format!("failed to parse .snips file: {}", path.display()))?;

    // Check format version
    let version = parse_format_version(&value)
        .unwrap_or_else(|| "1.0".to_string());
    validate_version(&version)?;

    // Strip the format key before parsing as SnipFile
    // (it's not a snippet, so walk_table naturally ignores it,
    //  but we validate explicitly for a clear error message)
    SnipFile::from_toml_value(&value)
}
```

---

## 6. Implementation Priority

| Priority | Component | Effort | Impact |
|----------|-----------|--------|--------|
| **P0** | Version locking (`format` header) | Small | Enables all future format evolution safely |
| **P1** | Output: `--json` flag | Small | Enables scripting, piping, CI integration |
| **P1** | Output: `--format` template strings | Small | Enables fzf integration, custom workflows |
| **P2** | TOML detector plugins (`.snips.d/*.toml`) | Medium | Users extend snip for their stack without code |
| **P2** | Shell script detectors (`.snips.d/*.sh`) | Medium | Maximum flexibility for complex extraction |
| **P2** | Pre/post hooks (per-snippet) | Small | Safety checks, notifications |
| **P3** | Global hooks (`.snips.d/hooks.toml`) | Medium | Project-wide policies |
| **P3** | Snippet packs (`snip pack add`) | Large | Community ecosystem |
| **P4** | Custom formatter files (`.snips.d/formats/`) | Small | Niche but useful for team standards |

## 7. New Dependencies

| Crate | Purpose | Version |
|-------|---------|---------|
| `which` | Check if commands exist on PATH for detector `[detect]` | 7.0 |
| `regex` | Named-group pattern matching for TOML extractors | 1.11 |
| `serde_json` | Already likely a transitive dep; needed for JSON lines output | 1.0 |
| `glob` | Pattern matching for hook `pattern` fields | 0.3 |

No other new dependencies. The design intentionally uses TOML (already a dep), shell scripts (zero-dep), and JSON (already a transitive dep).

## 8. Security Considerations

1. **Script detectors run arbitrary shell commands** — `.snips.d/*.sh` is equivalent to running `bash` on untrusted code. Only run from trusted `.snips.d/` directories (local project, not cloned from untrusted sources).

2. **Snippet packs run arbitrary snippets** — `snip pack add` clones a repo and merges its `.snips` into the user's snippet list. The commands in those snippets run with the user's shell permissions. Mitigation: `snip pack info` shows all commands before running; `snip doctor` flags suspicious commands.

3. **Hooks run with user permissions** — `pre_hook`, `post_hook`, etc. are shell commands. Same trust model as the snippets themselves.

4. **Git pack installation** — Should use `--depth 1` clones and pin to a specific commit/tag. Never auto-update without explicit `snip pack update`.