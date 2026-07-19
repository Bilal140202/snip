# AI-Powered Features — Design Document

> **Feature Agent**: #9 — AI Integration Expert
> **Project**: `snip` — Project-scoped command snippet manager (Rust CLI)
> **Date**: 2025-07-09
> **Status**: Design — Build target: 2–4 weeks

---

## Table of Contents

1. [Design Principles](#1-design-principles)
2. [`snip ai "<natural language>"`](#2-snip-ai-natural-language)
3. [`snip suggest`](#3-snip-suggest)
4. [`snip explain <name>`](#4-snip-explain-name)
5. [Smart Variable Inference](#5-smart-variable-inference)
6. [LLM Provider Architecture](#6-llm-provider-architecture)
7. [Implementation Timeline](#7-implementation-timeline)
8. [Dependency & Cargo Impact](#8-dependency--cargo-impact)

---

## 1. Design Principles

All AI features share these constraints:

| Principle | Rule |
|-----------|------|
| **Opt-in only** | No AI feature runs unless the user has configured a provider in `~/.config/snip/config.toml`. Core `snip` works with zero AI dependencies. |
| **Privacy-first** | Shell history and `.snips` content are sent to LLMs only when explicitly invoked. `snip suggest` runs entirely offline. |
| **Graceful degradation** | If an LLM call fails (timeout, 401, model missing), snip falls back to non-AI behavior with a clear message. Never hangs. |
| **No required crates** | All LLM interaction uses `reqwest` (already a reasonable HTTP client) and `tokio` at most behind a cargo feature flag. The binary without `--features ai` has no HTTP stack. |
| **Provider-agnostic** | Every AI feature goes through a single `LlmProvider` trait. Adding a new provider is ~100 lines of code. |

### Feature flag strategy

```toml
[features]
default = []
ai = ["reqwest", "tokio", "serde_json", "async-trait"]
```

When `ai` is not enabled, all AI subcommands print:

```
AI features are not compiled in. Install with: cargo install snip --features ai
```

When enabled but no provider is configured:

```
No LLM provider configured. Add one to ~/.config/snip/config.toml:
  [ai]
  provider = "ollama"   # or "openai"
  model = "llama3.1"
```

---

## 2. `snip ai "<natural language>"`

### 2.1 Goal

Translate a natural-language intent into a snippet command. This is the **hero feature** — it makes snip feel like a command-line assistant that knows your project.

### 2.2 UX Flow

```
User types:  snip ai "run the database migration for staging"
                  │
                  ▼
        ┌─────────────────────┐
        │  1. Load .snips     │
        │  2. Build index of  │
        │     key + desc +    │
        │     cmd             │
        └────────┬────────────┘
                 │
                 ▼
        ┌─────────────────────┐
        │  3. Fuzzy match     │
        │  query against      │
        │  descriptions &     │
        │  keys (SkimMatcher) │
        └────────┬────────────┘
                 │
          ┌──────┴──────┐
          │ score > 100 │
          │  (high      │──── Yes ───► 4a. Show match
          │  confidence)│              "Found: deploy.db-migrate"
          └──────┬──────┘              "Command: make db-migrate ENV=staging"
                 │                     "Run? [y/N] "
                 │ No
                 ▼
        ┌─────────────────────┐
        │  4b. LLM lookup    │
        │  (if provider       │
        │   configured)       │
        └────────┬────────────┘
                 │
          ┌──────┴──────────────┐
          │ LLM available?     │
          └──────┬──────────────┘
            Yes  │  │  No
                 ▼  ▼
          ┌──────────┐  ┌──────────────────┐
          │ Send to  │  │ "No match found. │
          │ LLM with │  │  Add a snippet   │
          │ context  │  │  with: snip add  │
          │          │  │  db-migrate ..." │
          └────┬─────┘  └──────────────────┘
               │
               ▼
        ┌─────────────────────┐
        │  5. Parse LLM       │
        │  response (JSON)    │
        │  { command, key,    │
        │    confidence }     │
        └────────┬────────────┘
                 │
          ┌──────┴──────┐
          │ confidence  │
          │  > 0.7?     │──── Yes ───► 6a. Show + confirm
          └──────┬──────┘
                 │ No
                 ▼
        ┌─────────────────────┐
        │  6b. "Best guess:   │
        │  <cmd>. Run? [y/N]" │
        └────────┬────────────┘
                 │
                 ▼
        ┌─────────────────────┐
        │  7. Execute on "y"  │
        │     (reuses         │
        │      executor)      │
        └─────────────────────┘
```

### 2.3 LLM Prompt Design

The prompt sent to the LLM includes:

```text
You are a command-line assistant. Given a user's intent and the following
available snippets, return the best matching command.

Available snippets:
---

[deploy.staging]
cmd = "kubectl apply -f k8s/staging/ --wait"
desc = "Deploy to staging Kubernetes cluster"

[deploy.production]
cmd = "kubectl apply -f k8s/production/ --wait --dry-run=client"
desc = "Deploy to production (dry-run first)"

[db.migrate]
cmd = "DATABASE_URL=postgres://... make migrate ENV={{env}}"
desc = "Run database migrations for a given environment"

---

User intent: "run the database migration for staging"

Respond in JSON only:
{"command": "DATABASE_URL=postgres://... make migrate ENV=staging", "key": "db.migrate", "confidence": 0.95}

If no snippet matches, respond:
{"command": "make db-migrate ENV=staging", "key": null, "confidence": 0.5, "note": "No exact snippet found; this is a best guess"}
```

### 2.4 Fuzzy Match Enhancement

The existing `fuzzy_match` only matches against keys. For `snip ai`, we extend it to match against a **concatenated search string**: `"{key} {desc} {cmd_prefix}"`.

```rust
// New function in src/core/fuzzy.rs
pub fn fuzzy_match_descriptions(
    query: &str,
    entries: &[(String, &Snippet)],  // (key, snippet) pairs
) -> Vec<FuzzyResult> {
    let matcher = SkimMatcherV2::default();
    let mut results: Vec<FuzzyResult> = entries
        .iter()
        .filter_map(|(key, snippet)| {
            // Build a searchable string from key + desc + first 80 chars of cmd
            let haystack = format!(
                "{} {} {}",
                key,
                snippet.desc,
                &snippet.cmd.chars().take(80).collect::<String>()
            );
            let score = matcher.fuzzy_match(&haystack, query)?;
            if score > 0 {
                Some(FuzzyResult { key: key.clone(), score })
            } else {
                None
            }
        })
        .collect();
    results.sort_by(|a, b| b.score.cmp(&a.score));
    results
}
```

### 2.5 Error Handling

| Scenario | Behavior |
|----------|----------|
| No `.snips` file found | Print "No .snips file. Run `snip init` first." and exit 1. |
| Fuzzy match found (score > 100) | Skip LLM entirely. Show match, confirm, run. |
| LLM timeout (>10s) | Fall back to "No confident match. Try `snip add`." |
| LLM returns malformed JSON | Log to stderr, fall back to fuzzy-only results. |
| LLM 401 / auth error | Print "LLM auth failed. Check your API key in `~/.config/snip/config.toml`." |
| User declines execution | Print "Cancelled." and exit 0. |

### 2.6 Code Architecture

```
src/
├── cli/
│   └── ai.rs              # `snip ai` subcommand handler
├── core/
│   └── ai/
│       ├── mod.rs          # Public re-exports
│       ├── matcher.rs      # Fuzzy match against descriptions
│       ├── resolver.rs     # Orchestrate: fuzzy → LLM → confirm → execute
│       └── prompt.rs       # LLM prompt templates
└── ai/
    ├── mod.rs              # Provider trait + config loading
    ├── ollama.rs           # Ollama provider impl
    └── openai.rs           # OpenAI + compatible provider impl
```

The `cli/ai.rs` is a thin shell:

```rust
// src/cli/ai.rs
pub fn run(query: &str) -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;

    let snipfile_path = match find_snipfile(Some(&cwd))? {
        Some(p) => p,
        None => anyhow::bail!("No .snips file found. Run `snip init` first."),
    };
    let file = read_snippets(&snipfile_path)?;

    let result = resolver::resolve(query, &file)?;

    match result {
        Resolution::ExactMatch { key, cmd } => {
            confirm_and_execute(&key, &cmd)
        }
        Resolution::LlmMatch { cmd, confidence, note } => {
            if confidence >= 0.7 {
                confirm_and_execute("AI suggestion", &cmd)
            } else {
                // Low confidence — show with warning
                confirm_and_execute_low_confidence(&cmd, note.as_deref())
            }
        }
        Resolution::NoMatch => {
            println!("No match found. Add a snippet with: snip add <name> \"<cmd>\"");
            Ok(())
        }
    }
}
```

---

## 3. `snip suggest`

### 3.1 Goal

Automatically grow the `.snips` file by detecting commands the user runs frequently that aren't yet captured. This is **offline-only** — no LLM needed.

### 3.2 How It Works

```
User types:  snip suggest
                  │
                  ▼
        ┌─────────────────────────┐
        │  1. Read shell history  │
        │  - $HOME/.bash_history  │
        │  - $HOME/.zsh_history   │
        │  - $HOME/.local/share/  │
        │    fish/fish_history    │
        │  (detect active shell   │
        │   from $SHELL env var)  │
        └────────┬────────────────┘
                 │
                 ▼
        ┌─────────────────────────┐
        │  2. Parse & normalize   │
        │  - Strip timestamps     │
        │  - De-duplicate exact   │
        │  - Remove trivial cmds  │
        │    (ls, cd, clear, vim, │
        │    git status, etc.)    │
        └────────┬────────────────┘
                 │
                 ▼
        ┌─────────────────────────┐
        │  3. Frequency count     │
        │  - Count occurrences    │
        │  - Weight recency:      │
        │    last 7 days = 3x,    │
        │    last 30 days = 1x    │
        └────────┬────────────────┘
                 │
                 ▼
        ┌─────────────────────────┐
        │  4. Filter out commands │
        │  already in .snips     │
        │  (exact cmd match or   │
        │   substring of existing│
        │   snippet cmd)         │
        └────────┬────────────────┘
                 │
                 ▼
        ┌─────────────────────────┐
        │  5. Rank by score:      │
        │    score = freq ×       │
        │    recency_weight ×     │
        │    complexity_bonus     │
        │  (complexity = number  │
        │   of args, pipes, etc.)│
        └────────┬────────────────┘
                 │
                 ▼
        ┌─────────────────────────┐
        │  6. Show top 5:         │
        │                         │
        │  You run 'docker        │
        │  compose up -d' 3x/day. │
        │  Add to .snips? [y/N]   │
        │                         │
        │  Suggested key: docker  │
        │  Suggested desc: Start  │
        │  Docker Compose in      │
        │  detached mode          │
        └────────┬────────────────┘
                 │
                 ▼
        ┌─────────────────────────┐
        │  7. On "y": auto-add    │
        │  to .snips with         │
        │  generated key + desc   │
        └─────────────────────────┘
```

### 3.3 History File Parsing

Shell history formats are annoyingly different:

| Shell | File | Format | Parsing Strategy |
|-------|------|--------|-----------------|
| Bash | `~/.bash_history` | One cmd per line (simple) or `#<timestamp>\n<cmd>` | Split on newlines, skip lines starting with `#` |
| Zsh | `~/.zsh_history` | `: <timestamp>:<duration>:<cmd>` | Split on `:\d+:\d+:`, extract 3rd field |
| Fish | `~/.local/share/fish/fish_history` | YAML-like: `- cmd: <cmd>\n  when: <timestamp>` | Parse with simple line scanner, extract after `cmd: ` |

```rust
// src/core/suggest/history.rs

pub struct HistoryEntry {
    pub command: String,
    pub timestamp: Option<i64>,  // unix epoch, if available
}

pub fn read_history(shell: &str) -> anyhow::Result<Vec<HistoryEntry>> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("no home dir"))?;

    match shell {
        "bash" | "sh" => read_bash_history(&home),
        "zsh" => read_zsh_history(&home),
        "fish" => read_fish_history(&home),
        _ => {
            // Try all three, return whichever has entries
            try_all_histories(&home)
        }
    }
}

fn read_bash_history(home: &Path) -> Result<Vec<HistoryEntry>> {
    let path = home.join(".bash_history");
    if !path.exists() { return Ok(vec![]); }
    let content = fs::read_to_string(&path)?;
    Ok(content
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| HistoryEntry {
            command: line.to_string(),
            timestamp: None,
        })
        .collect())
}

fn read_zsh_history(home: &Path) -> Result<Vec<HistoryEntry>> {
    let path = home.join(".zsh_history");
    if !path.exists() { return Ok(vec![]); }
    let content = fs::read_to_string(&path)?;
    // Format: ": timestamp:duration:command"
    let mut entries = Vec::new();
    for line in content.lines() {
        if let Some(cmd) = line.split(':').nth(3) {
            if !cmd.trim().is_empty() {
                entries.push(HistoryEntry {
                    command: cmd.to_string(),
                    timestamp: line.split(':').nth(1).and_then(|t| t.trim().parse().ok()),
                });
            }
        }
    }
    Ok(entries)
}
```

### 3.4 Filtering Heuristics

Commands to **exclude** (not useful as snippets):

```rust
const SKIP_PREFIXES: &[&str] = &[
    "ls ", "cd ", "pwd", "clear", "exit", "history",
    "vim ", "nvim ", "nano ", "code ", "vi ",
    "git status", "git log", "git diff",
    "echo ", "cat ", "less ", "more ",
    "man ", "which ", "where ",
    "snip ",  // Don't suggest snip commands themselves
];

const SKIP_EXACT: &[&str] = &[
    "ls", "cd", "pwd", "clear", "exit", "ll", "la",
];

fn is_trivial(cmd: &str) -> bool {
    let trimmed = cmd.trim();
    SKIP_EXACT.contains(&trimmed)
        || SKIP_PREFIXES.iter().any(|p| trimmed.starts_with(p))
        || trimmed.len() < 10  // Too short to be useful
}
```

### 3.5 Key & Description Generation

Without an LLM, we use heuristics to generate a good snippet key and description:

```rust
fn suggest_key(cmd: &str) -> String {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    if parts.is_empty() { return "unnamed".into(); }

    let binary = Path::new(parts[0])
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(parts[0]);

    // If it's a subcommand like "docker compose up", use the subcommand
    if parts.len() >= 2 {
        let sub = parts[1];
        // Common subcommands
        match sub {
            "up" | "down" | "build" | "run" | "start" | "stop" | "restart"
            | "deploy" | "migrate" | "test" | "lint" | "format" | "install"
            | "apply" | "delete" | "create" | "push" | "pull" => {
                return format!("{}.{}", binary, sub);
            }
            _ => {}
        }
    }

    binary.to_string()
}

fn suggest_description(cmd: &str) -> String {
    // Simple template: "<binary> <action>"
    let parts: Vec<&str> = cmd.split_whitespace().take(5).collect();
    format!("{}", parts.join(" "))
}
```

With an LLM configured, we can send a batch of 5 candidates and get better keys/descriptions back in one call.

### 3.6 Smart Deduplication

Before suggesting, check if the command is **already captured** in `.snips`:

```rust
fn is_already_snipped(cmd: &str, file: &SnipFile) -> bool {
    for (_, snippet) in file.iter() {
        // Exact match
        if snippet.cmd == cmd { return true; }
        // The .snips cmd is a template that contains this exact command
        // e.g. snippet cmd is "docker compose up -d {{service}}"
        // and history cmd is "docker compose up -d web"
        let stripped = snippet.cmd.replace("{{", "").replace("}}", "");
        if stripped.trim() == cmd.trim() { return true; }
    }
    false
}
```

### 3.7 UX: The Suggestion Prompt

```
$ snip suggest

? You run 'docker compose up -d' 12 times in the last week.
  Add to .snips? [y/N/e] y
  → Added 'docker.up' to .snips

? You run 'kubectl get pods -n staging' 8 times in the last week.
  Add to .snips? [y/N/e] y
  → Added 'kubectl.pods-staging' to .snips

? You run 'npm run build && npm run test' 5 times in the last week.
  Add to .snips? [y/N/e] e    ← edit key before adding
  Key [build.test]: build.check
  → Added 'build.check' to .snips

? You run 'cargo build --release 2>&1 | tee build.log' 4 times in the last week.
  Add to .snips? [y/N/e] N

  3 snippet(s) added. Run `snip list` to see them.
```

The `e` option lets users edit the suggested key. This is critical — auto-generated keys won't always be right.

### 3.8 Data Flow Diagram

```
┌──────────────┐     ┌──────────────────┐     ┌───────────────┐
│ Shell History │────►│ Parse & Normalize│────►│ Frequency     │
│ (.bash_history│     │ (strip timestamps│     │ Counter       │
│  .zsh_history │     │  de-dup, filter  │     │ (cmd → count) │
│  fish_history)│     │  trivial cmds)   │     └───────┬───────┘
└──────────────┘     └──────────────────┘             │
                                                          │
                                                          ▼
┌──────────────┐     ┌──────────────────┐     ┌───────────────┐
│  .snips file  │────►│ Dedup Filter     │◄────│  Score & Rank │
│ (existing     │     │ (remove already  │     │ (freq × recency│
│  snippets)    │     │  captured cmds)  │     │  × complexity) │
└──────────────┘     └──────────────────┘     └───────┬───────┘
                                                          │
                                                          ▼
                                               ┌───────────────┐
                                               │ Interactive    │
                                               │ Suggestion     │
                                               │ Prompt (top 5) │
                                               └───────────────┘
```

---

## 4. `snip explain <name>`

### 4.1 Goal

Explain what a snippet command does in plain English. Ideal for onboarding new team members who see `deploy.staging` in `.snips` and want to understand it without reading shell man pages.

### 4.2 Two-Tier Approach

**Tier 1: Local explanation (no LLM)** — Works for simple commands using a built-in parser.

**Tier 2: LLM explanation** — Used for complex commands (pipes, subshells, env vars, xargs, etc.)

```
User types:  snip explain deploy.staging
                  │
                  ▼
        ┌─────────────────────┐
        │  1. Load .snips     │
        │  2. Find snippet    │
        │     "deploy.staging"│
        └────────┬────────────┘
                 │
                 ▼
        ┌─────────────────────┐
        │  3. Complexity      │
        │  analysis:          │
        │  - Count pipes (|)  │
        │  - Count subshells  │
        │  - Count redirects  │
        │  - Has env vars?    │
        │  - Has xargs/awk/   │
        │    sed/perl?        │
        └────────┬────────────┘
                 │
          ┌──────┴──────┐
          │ Simple?     │──── Yes ───► 4a. Local explanation
          │ (< 2 pipes, │              (tokenize + describe)
          │  no awk/sed)│
          └──────┬──────┘
                 │ No
                 ▼
        ┌─────────────────────┐
        │  4b. LLM explain    │
        │  (if configured)    │
        │                     │
        │  OR fallback:       │
        │  "This command is   │
        │  complex. Configure │
        │  an LLM for a       │
        │  detailed explain." │
        └─────────────────────┘
```

### 4.3 Complexity Scoring

```rust
fn complexity_score(cmd: &str) -> u8 {
    let mut score = 0u8;
    score += cmd.matches('|').count() as u8 * 3;        // pipes are complex
    score += cmd.matches("&&").count() as u8 * 2;       // chaining
    score += cmd.matches("||").count() as u8 * 2;       // or-chaining
    score += cmd.matches('$(').count() as u8 * 3;       // subshells
    score += cmd.matches(">`").count() as u8 * 1;       // redirects
    score += cmd.matches("xargs").count() as u8 * 3;    // xargs
    score += cmd.matches("awk").count() as u8 * 3;      // awk
    score += cmd.matches("sed").count() as u8 * 2;      // sed
    score += cmd.matches("find ").count() as u8 * 2;    // find
    score += cmd.matches("grep ").count() as u8 * 1;    // grep is common
    score += cmd.contains("{{") as u8 * 1;              // has variables
    score.min(20)  // cap it
}
```

Commands scoring `< 5` get local explanation. `>= 5` get LLM explanation.

### 4.4 Local Explanation (Tier 1)

Tokenize the command and generate a description from known patterns:

```rust
fn explain_locally(cmd: &str) -> String {
    let tokens = shell_words::split(cmd).unwrap_or_default();
    if tokens.is_empty() { return "Empty command".into(); }

    let binary = &tokens[0];
    let args = &tokens[1..];

    match binary.as_str() {
        "cargo" => explain_cargo(args),
        "docker" => explain_docker(args),
        "kubectl" => explain_kubectl(args),
        "npm" | "yarn" | "pnpm" => explain_package_manager(args, binary),
        "make" => explain_make(args),
        "go" => explain_go(args),
        "pytest" | "jest" | "vitest" => explain_test_runner(args, binary),
        _ => format!("Runs `{}` with arguments: {}", binary, args.join(" ")),
    }
}

fn explain_kubectl(args: &[String]) -> String {
    match args.first().map(|s| s.as_str()) {
        Some("apply") => {
            let file = args.iter().find(|a| a.starts_with("-f"))
                .and_then(|a| a.strip_prefix("-f"));
            match file {
                Some(f) => format!(
                    "Applies Kubernetes configuration from '{}'. \
                     This creates or updates resources defined in the file.",
                    f
                ),
                None => "Applies Kubernetes configuration from stdin.".into(),
            }
        }
        Some("get") => {
            let resource = args.get(1).map(|s| s.as_str()).unwrap_or("resources");
            format!("Lists Kubernetes {} in the cluster.", resource)
        }
        // ... etc
        _ => format!("Runs `kubectl {}`.", args.join(" ")),
    }
}
```

This covers the 80% case (simple commands). It's not perfect, but it's **instant** and **offline**.

### 4.5 LLM Explanation (Tier 2)

Prompt:

```text
Explain this shell command in plain English, as if talking to a junior developer.

Command: kubectl apply -f k8s/staging/ --wait && kubectl rollout status deployment/web -n staging

Explanation (2-3 sentences):
```

Expected response:

```
This applies all Kubernetes manifests from the `k8s/staging/` directory to the cluster
and waits for them to be created. Then it monitors the `web` deployment in the `staging`
namespace until the rollout completes, printing its progress.
```

### 4.6 Example Output

```
$ snip explain deploy.staging

  deploy.staging
  ──────────────────────────────────────────────
  Command:
    kubectl apply -f k8s/staging/ --wait && kubectl rollout status deployment/web -n staging

  Explanation:
    Applies all Kubernetes manifests from k8s/staging/ to the cluster and waits for
    the resources to be created. Then monitors the web deployment in the staging
    namespace until the rollout completes.

  Variables:
    (none)

  Tags:
    kubernetes, deployment
```

### 4.7 Onboarding Use Case

A new team member runs `snip explain` on every snippet:

```bash
# Quick onboarding — explain all snippets
snip list --quiet | while read key; do
  echo "=== $key ==="
  snip explain "$key"
  echo
done
```

Or we could add `snip explain --all` as a future enhancement that prints a README-like document for the project.

---

## 5. Smart Variable Inference

### 5.1 Goal

When a user adds a snippet, automatically detect `$VARIABLE` patterns and offer to convert them into `{{var}}` template variables with prompts.

### 5.2 Current State

The existing `snip add` command stores the command verbatim. Users must manually write `{{var}}` syntax and define `vars` in the TOML. This is friction.

### 5.3 Enhanced `snip add` Flow

```
User types:  snip add deploy "kubectl apply -f k8s/$ENV/"
                  │
                  ▼
        ┌─────────────────────────┐
        │  1. Parse command for   │
        │  variable patterns:    │
        │  - $NAME               │
        │  - ${NAME}             │
        │  - $NAME/default       │
        └────────┬────────────────┘
                 │
          ┌──────┴──────────────┐
          │ Variables found?    │
          └──────┬──────────────┘
            Yes  │  │  No
                 ▼  ▼
          ┌──────────┐  ┌──────────────────┐
          │ 2. For   │  │ Save as-is       │
          │ each var:│  │ (current behavior)│
          │ prompt   │  └──────────────────┘
          │ user     │
          └────┬─────┘
               │
               ▼
        ? Found variable $ENV.
          Description: Target environment
          Options: [staging, production, development]
          Default: [staging]: <user types or enters>
          Add as template variable? [y/N]: y
               │
               ▼
        ┌─────────────────────────┐
        │  3. Rewrite command:    │
        │  "kubectl apply -f     │
        │   k8s/{{ENV}}/"        │
        │                         │
        │  4. Store with VarDef:  │
        │  vars = [{             │
        │    name = "ENV",       │
        │    desc = "Target env",│
        │    default = "staging",│
        │    options = [...]     │
        │  }]                    │
        └─────────────────────────┘
```

### 5.4 Variable Detection Regex

```rust
use regex::Regex;

/// Detect shell-style variables in a command string.
/// Matches: $VAR, ${VAR}, ${VAR:-default}
fn detect_shell_variables(cmd: &str) -> Vec<DetectedVar> {
    let re = Regex::new(
        r#"\$(\{(?P<braced>[A-Za-z_][A-Za-z0-9_]*)(?::-(?P<default>[^}]*))?\}|(?P<simple>[A-Za-z_][A-Za-z0-9_]*))"#
    ).unwrap();

    let mut vars = Vec::new();
    for cap in re.captures_iter(cmd) {
        let name = cap.name("braced")
            .or_else(|| cap.name("simple"))
            .unwrap()
            .as_str()
            .to_string();

        let default = cap.name("default")
            .map(|m| m.as_str().to_string());

        // Skip well-known env vars that shouldn't be prompts
        if is_common_env_var(&name) { continue; }

        vars.push(DetectedVar { name, default });
    }
    vars
}

struct DetectedVar {
    name: String,
    default: Option<String>,
}

/// Env vars that are typically set by the environment, not the user.
const COMMON_ENV_VARS: &[&str] = &[
    "HOME", "USER", "PATH", "SHELL", "TERM", "LANG", "PWD",
    "EDITOR", "VISUAL", "PAGER", "TMPDIR", "XDG_",
];

fn is_common_env_var(name: &str) -> bool {
    COMMON_ENV_VARS.iter().any(|v| name.starts_with(v))
        || name.starts_with("SNIP_")
        || name == "_"  // shell's last argument
}
```

### 5.5 Command Rewriting

Convert `$VAR` / `${VAR}` → `{{VAR}}`:

```rust
fn rewrite_shell_vars_to_templates(cmd: &str, vars: &[DetectedVar]) -> String {
    let mut result = cmd.to_string();
    for var in vars {
        // Replace ${VAR} first (more specific)
        result = result.replace(&format!("${{{}}}", var.name), &format!("{{{{{}}}}}", var.name));
        // Then $VAR (but not $$VAR or inside quotes already)
        result = result.replace(&format!("${}", var.name), &format!("{{{{{}}}}}", var.name));
    }
    result
}
```

### 5.6 Options Inference

For common variable names, suggest likely options:

```rust
fn suggest_options(name: &str) -> Option<Vec<String>> {
    let name_upper = name.to_uppercase();
    match name_upper.as_str() {
        "ENV" | "ENVIRONMENT" | "STAGE" => Some(vec![
            "development".into(), "staging".into(), "production".into()
        ]),
        "REGION" | "AWS_REGION" => Some(vec![
            "us-east-1".into(), "us-west-2".into(), "eu-west-1".into()
        ]),
        "SERVICE" | "SVC" => {
            // Try to read docker-compose services
            try_infer_docker_services()
        }
        _ => None,
    }
}
```

### 5.7 LLM-Assisted Description (Optional)

If an LLM is configured and the user accepts a detected variable, we can generate a better description:

```text
Given this command: kubectl apply -f k8s/$ENV/
And this variable: ENV
Suggest a short, human-readable description for the ENV variable (5 words max).
```

Response: `"Target deployment environment"`

### 5.8 Integration Point

This hooks into the existing `snip add` command. The modification is minimal:

```rust
// In src/cli/add.rs — enhanced version
pub fn run(name: &str, cmd: &str, description: Option<&str>) -> Result<()> {
    let detected_vars = detect_shell_variables(cmd);

    if !detected_vars.is_empty() {
        let rewritten_cmd = rewrite_shell_vars_to_templates(cmd, &detected_vars);
        let var_defs = interactive_var_prompts(&detected_vars)?;
        let snippet = Snippet::new(&rewritten_cmd)
            .with_desc(description.unwrap_or(cmd))
            .with_vars(var_defs);
        // ... save snippet
    } else {
        // Current behavior — no changes
        let snippet = Snippet::new(cmd).with_desc(description.unwrap_or(cmd));
        // ... save snippet
    }
}
```

### 5.9 Output Example

```
$ snip add deploy "kubectl apply -f k8s/$ENV/ --wait"

? Found variable $ENV
  Description: Target deployment environment
  Options: development / staging / production
  Default: staging

  Add as template variable? [y/N]: y

✓ Added 'deploy' to .snips
  Command: kubectl apply -f k8s/{{ENV}}/ --wait
  Variable: ENV (default: staging) [development, staging, production]
```

---

## 6. LLM Provider Architecture

### 6.1 Config File

Location: `~/.config/snip/config.toml`

```toml
[ai]
# "ollama", "openai", or "openai-compatible"
provider = "ollama"

# Model name (provider-specific)
model = "llama3.1"

# Request timeout in seconds (default: 10)
timeout = 10

# Maximum tokens for responses (default: 500)
max_tokens = 500

# ---- Provider-specific settings ----

[ai.ollama]
# Base URL (default: http://localhost:11434)
base_url = "http://localhost:11434"

[ai.openai]
api_key = "sk-..."    # Or set OPENAI_API_KEY env var
base_url = "https://api.openai.com/v1"  # Overridable for compatible APIs

[ai.openai-compatible]
# For vLLM, text-generation-webui, LM Studio, etc.
api_key = "not-needed"
base_url = "http://localhost:8080/v1"
```

### 6.2 Config Loading

```rust
// src/ai/config.rs

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct AiConfig {
    pub ai: Option<AiSection>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AiSection {
    pub provider: String,
    pub model: String,
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    pub ollama: Option<OllamaConfig>,
    pub openai: Option<OpenAiConfig>,
    #[serde(rename = "openai-compatible")]
    pub openai_compatible: Option<OpenAiCompatibleConfig>,
}

fn default_timeout() -> u64 { 10 }
fn default_max_tokens() -> u32 { 500 }

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("snip")
        .join("config.toml")
}

pub fn load_config() -> AiConfig {
    let path = config_path();
    if !path.exists() { return AiConfig::default(); }
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    toml::from_str(&content).unwrap_or_default()
}

pub fn is_ai_enabled() -> bool {
    let config = load_config();
    config.ai.is_some()
}
```

### 6.3 Provider Trait

```rust
// src/ai/provider.rs

use async_trait::async_trait;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Send a chat completion request and return the assistant's response text.
    async fn complete(
        &self,
        system_prompt: &str,
        user_message: &str,
        max_tokens: u32,
    ) -> Result<String, LlmError>;
}

#[derive(Debug)]
pub enum LlmError {
    /// Provider is not configured
    NotConfigured,
    /// Network / connection error
    Connection(String),
    /// HTTP error (4xx, 5xx)
    Http { status: u16, body: String },
    /// Response couldn't be parsed
    Parse(String),
    /// Timeout
    Timeout,
}

impl std::fmt::Display for LlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmError::NotConfigured => write!(f, "No LLM provider configured"),
            LlmError::Connection(e) => write!(f, "Connection error: {}", e),
            LlmError::Http { status, body } => {
                write!(f, "HTTP {} — {}", status, body.chars().take(200).collect::<String>())
            }
            LlmError::Parse(e) => write!(f, "Failed to parse LLM response: {}", e),
            LlmError::Timeout => write!(f, "LLM request timed out"),
        }
    }
}
```

### 6.4 Ollama Provider

```rust
// src/ai/ollama.rs

use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct OllamaProvider {
    base_url: String,
    model: String,
    timeout: u64,
    client: Client,
}

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    system: String,
    stream: bool,
    options: OllamaOptions,
}

#[derive(Serialize)]
struct OllamaOptions {
    num_predict: u32,
}

#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
}

impl OllamaProvider {
    pub fn new(config: &AiSection, ollama: &OllamaConfig) -> Self {
        Self {
            base_url: ollama.base_url.clone().unwrap_or_else(|| {
                "http://localhost:11434".into()
            }),
            model: config.model.clone(),
            timeout: config.timeout,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    async fn complete(
        &self,
        system_prompt: &str,
        user_message: &str,
        max_tokens: u32,
    ) -> Result<String, LlmError> {
        let request = OllamaRequest {
            model: self.model.clone(),
            system: system_prompt.to_string(),
            prompt: user_message.to_string(),
            stream: false,
            options: OllamaOptions { num_predict: max_tokens },
        };

        let url = format!("{}/api/generate", self.base_url);
        let response = self.client
            .post(&url)
            .json(&request)
            .timeout(std::time::Duration::from_secs(self.timeout))
            .send()
            .await
            .map_err(|e| LlmError::Connection(e.to_string()))?;

        let status = response.status();
        let body = response.text().await
            .map_err(|e| LlmError::Parse(e.to_string()))?;

        if !status.is_success() {
            return Err(LlmError::Http {
                status: status.as_u16(),
                body,
            });
        }

        let parsed: OllamaResponse = serde_json::from_str(&body)
            .map_err(|e| LlmError::Parse(e.to_string()))?;

        Ok(parsed.response)
    }
}
```

### 6.5 OpenAI Provider (covers OpenAI + compatible)

```rust
// src/ai/openai.rs

pub struct OpenAiProvider {
    base_url: String,
    api_key: String,
    model: String,
    timeout: u64,
    client: Client,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    max_tokens: u32,
    temperature: f32,
}

#[derive(Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

impl OpenAiProvider {
    pub fn new(config: &AiSection, openai: &OpenAiConfig) -> Self {
        let api_key = openai.api_key.clone().unwrap_or_else(|| {
            std::env::var("OPENAI_API_KEY")
                .unwrap_or_default()
        });

        Self {
            base_url: openai.base_url.clone().unwrap_or_else(|| {
                "https://api.openai.com/v1".into()
            }),
            api_key,
            model: config.model.clone(),
            timeout: config.timeout,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    async fn complete(
        &self,
        system_prompt: &str,
        user_message: &str,
        max_tokens: u32,
    ) -> Result<String, LlmError> {
        let request = ChatRequest {
            model: self.model.clone(),
            max_tokens,
            temperature: 0.3,  // Low temperature for factual responses
            messages: vec![
                ChatMessage {
                    role: "system".into(),
                    content: system_prompt.into(),
                },
                ChatMessage {
                    role: "user".into(),
                    content: user_message.into(),
                },
            ],
        };

        let url = format!("{}/chat/completions", self.base_url);
        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .timeout(std::time::Duration::from_secs(self.timeout))
            .send()
            .await
            .map_err(|e| LlmError::Connection(e.to_string()))?;

        let status = response.status();
        let body = response.text().await
            .map_err(|e| LlmError::Parse(e.to_string()))?;

        if !status.is_success() {
            return Err(LlmError::Http {
                status: status.as_u16(),
                body,
            });
        }

        let parsed: ChatResponse = serde_json::from_str(&body)
            .map_err(|e| LlmError::Parse(e.to_string()))?;

        parsed.choices
            .first()
            .map(|c| c.message.content.clone())
            .ok_or_else(|| LlmError::Parse("No choices in response".into()))
    }
}
```

### 6.6 Provider Factory

```rust
// src/ai/mod.rs

pub fn create_provider(config: &AiConfig) -> Result<Box<dyn LlmProvider>, LlmError> {
    let ai = config.ai.as_ref().ok_or(LlmError::NotConfigured)?;

    match ai.provider.as_str() {
        "ollama" => {
            let ollama_conf = ai.ollama.as_ref()
                .unwrap_or(&OllamaConfig { base_url: None });
            Ok(Box::new(OllamaProvider::new(ai, ollama_conf)))
        }
        "openai" => {
            let openai_conf = ai.openai.as_ref()
                .ok_or(LlmError::NotConfigured)?;
            Ok(Box::new(OpenAiProvider::new(ai, openai_conf)))
        }
        "openai-compatible" => {
            // Same implementation, different config section
            let compat_conf = ai.openai_compatible.as_ref()
                .ok_or(LlmError::NotConfigured)?;
            Ok(Box::new(OpenAiProvider::new(ai, &compat_conf.into())))
        }
        other => {
            // Future: plugin providers via dynamic loading
            Err(LlmError::NotConfigured)  // TODO: better error
        }
    }
}

/// Synchronous wrapper for use in non-async CLI code.
/// Spawns a tokio runtime internally.
pub fn complete_sync(
    provider: &dyn LlmProvider,
    system: &str,
    user: &str,
    max_tokens: u32,
) -> Result<String, LlmError> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| LlmError::Connection(e.to_string()))?;

    rt.block_on(provider.complete(system, user, max_tokens))
}
```

### 6.7 Provider Comparison

| Aspect | Ollama | OpenAI | OpenAI-Compatible |
|--------|--------|--------|-------------------|
| **Cost** | Free (local) | ~$0.002/1K tokens | Varies |
| **Latency** | 1–10s (depends on model + GPU) | 0.5–3s | Varies |
| **Privacy** | All local — zero data leaves machine | Data sent to OpenAI | Depends on host |
| **Quality** | Good (llama3.1, mistral, codestral) | Excellent (gpt-4o-mini, gpt-4o) | Depends on model |
| **Setup** | `ollama pull llama3.1` | API key only | URL + optional key |
| **Offline** | Yes | No | Depends on host |
| **Recommended model** | `llama3.1` (4.7B, fast) | `gpt-4o-mini` (cheap + good) | Whatever is hosted |

### 6.8 Fallback Chain for `snip ai`

```
1. Fuzzy match on local snippets (always available, <1ms)
   │
   ├─ High confidence (score > 100) → return immediately
   │
   └─ Low/no confidence → continue
       │
2. Try configured LLM provider
   │
   ├─ Ollama (if configured)
   │   ├─ Connection refused → skip to next
   │   └─ Model not found → print "Run: ollama pull <model>" → skip
   │
   ├─ OpenAI (if configured)
   │   ├─ 401 → print "Check API key" → stop
   │   └─ Success → return result
   │
   └─ No provider configured → print suggestion to configure → stop
```

### 6.9 Health Check: `snip doctor` Integration

Extend the existing `snip doctor` to check AI configuration:

```
$ snip doctor

✓ All 12 snippet(s) look good.
✓ AI provider: ollama
✓ Ollama reachable at http://localhost:11434
✓ Model 'llama3.1' available
```

Implementation:

```rust
// In src/core/validator.rs — add AI check
pub fn check_ai(config: &AiConfig) -> Vec<Issue> {
    let mut issues = Vec::new();
    let ai = match &config.ai {
        Some(a) => a,
        None => return issues,  // AI not configured — not an issue
    };

    match ai.provider.as_str() {
        "ollama" => {
            // Try to hit the /api/tags endpoint
            let url = ai.ollama.as_ref()
                .and_then(|o| o.base_url.clone())
                .unwrap_or_else(|| "http://localhost:11434".into());
            match reqwest::blocking::get(&format!("{}/api/tags", url)) {
                Ok(resp) if resp.status().is_success() => {
                    // Check if the model is in the list
                    // ...
                }
                Ok(resp) => {
                    issues.push(Issue {
                        key: "ai.ollama".into(),
                        severity: Severity::Warning,
                        message: format!("Ollama returned status {}", resp.status()),
                    });
                }
                Err(e) => {
                    issues.push(Issue {
                        key: "ai.ollama".into(),
                        severity: Severity::Warning,
                        message: format!("Cannot reach Ollama at {}: {}", url, e),
                    });
                }
            }
        }
        "openai" | "openai-compatible" => {
            if let Some(openai) = ai.openai.as_ref() {
                if openai.api_key.is_none() && std::env::var("OPENAI_API_KEY").is_err() {
                    issues.push(Issue {
                        key: "ai.openai".into(),
                        severity: Severity::Error,
                        message: "No API key set. Set it in config or OPENAI_API_KEY env var.".into(),
                    });
                }
            }
        }
        _ => {}
    }
    issues
}
```

---

## 7. Implementation Timeline

### Week 1: Foundation

| Day | Task | Files |
|-----|------|-------|
| 1–2 | LLM provider trait + config loading | `src/ai/mod.rs`, `src/ai/config.rs`, `src/ai/provider.rs` |
| 2–3 | Ollama provider implementation | `src/ai/ollama.rs` |
| 3–4 | OpenAI provider implementation | `src/ai/openai.rs` |
| 4–5 | Add `ai` feature flag to `Cargo.toml`, wire into `main.rs` | `Cargo.toml`, `src/main.rs`, `src/cli/mod.rs` |
| 5 | Tests: config loading, provider factory | `src/ai/config.rs` (tests) |

### Week 2: `snip ai` + `snip explain`

| Day | Task | Files |
|-----|------|-------|
| 1 | Enhanced fuzzy match against descriptions | `src/core/fuzzy.rs` |
| 2 | LLM prompt template for `snip ai` | `src/core/ai/prompt.rs` |
| 3 | Resolution orchestrator (fuzzy → LLM → confirm) | `src/core/ai/resolver.rs` |
| 4 | `snip ai` CLI handler + tests | `src/cli/ai.rs` |
| 5 | `snip explain` — local (Tier 1) explanation | `src/cli/explain.rs`, `src/core/ai/explain.rs` |
| 5 | `snip explain` — LLM (Tier 2) explanation | `src/core/ai/explain.rs` |

### Week 3: `snip suggest` + Smart Variables

| Day | Task | Files |
|-----|------|-------|
| 1–2 | Shell history parsing (bash, zsh, fish) | `src/core/suggest/history.rs` |
| 3 | Frequency counting, filtering, scoring | `src/core/suggest/scoring.rs` |
| 4 | Interactive suggestion prompt | `src/cli/suggest.rs` |
| 5 | Smart variable detection in `snip add` | `src/cli/add.rs`, `src/core/ai/vars.rs` |

### Week 4: Polish + Testing

| Day | Task | Files |
|-----|------|-------|
| 1 | `snip doctor` AI health checks | `src/core/validator.rs`, `src/cli/doctor.rs` |
| 2 | Error handling polish (timeout, auth, fallback) | All AI files |
| 3 | Integration tests (mock LLM server) | `tests/ai_test.rs` |
| 4 | Documentation: `snip ai --help`, README section | Help text |
| 5 | Edge cases, review, release prep | — |

### Effort Estimates

| Feature | Lines of Code (est.) | Effort | Risk |
|---------|---------------------|--------|------|
| Provider architecture | ~400 | 2 days | Low |
| `snip ai` | ~300 | 2 days | Medium (prompt engineering) |
| `snip suggest` | ~400 | 3 days | Medium (history format edge cases) |
| `snip explain` | ~250 | 1.5 days | Low |
| Smart variable inference | ~200 | 1.5 days | Low |
| Config + feature flag | ~100 | 0.5 days | Low |
| Tests | ~300 | 2 days | — |
| **Total** | **~1,950** | **~12.5 days** | — |

---

## 8. Dependency & Cargo Impact

### New dependencies (behind `ai` feature flag)

```toml
[features]
ai = ["reqwest", "tokio", "regex"]

[dependencies]
# Existing — no changes
clap = { version = "4", features = ["derive"] }
# ...

# AI feature — only compiled when --features ai
reqwest = { version = "0.12", features = ["json", "blocking"], optional = true }
tokio = { version = "1", features = ["rt"], optional = true }
regex = { version = "1", optional = true }
async-trait = { version = "0.1", optional = true }
```

### Why these choices

| Crate | Why |
|-------|-----|
| `reqwest` | Industry-standard HTTP client. The `blocking` feature lets us avoid async in the CLI layer. |
| `tokio` | Required by `reqwest` for async. We only use `current_thread` runtime — no multi-threading. |
| `regex` | Needed for shell variable detection (`$VAR`, `${VAR}`, `${VAR:-default}`). |
| `async-trait` | Clean ergonomics for the `LlmProvider` trait. |

### What we deliberately avoid

| Avoided | Why |
|---------|-----|
| `ollama-rs` crate | Too much abstraction, version-pinned models. Our ~80 line HTTP impl is simpler and more debuggable. |
| `openai-rs` crate | Same reason. The OpenAI chat completions API is ~30 lines of code. |
| `tiktoken-rs` | Token counting isn't needed — we cap at `max_tokens` server-side. |
| `async-std` | Mixing `tokio` and `async-std` is a footgun. Stick with one runtime. |

### Binary size impact

| Build | Size (est.) |
|-------|------------|
| `snip` (no features) | ~1.2 MB (current) |
| `snip --features ai` | ~2.8 MB (+ reqwest/tokio/regex) |
| `snip --features ai --release` | ~1.6 MB (stripped) |

The `ai` feature adds ~400KB to the release binary. Acceptable for the value.

---

## Appendix A: Full CLI Subcommand Registration

```rust
// In src/main.rs — updated Commands enum

#[derive(Subcommand)]
enum Commands {
    // ... existing commands ...

    /// Find and run a command using natural language
    Ai {
        /// Natural language description of what you want to do
        query: String,
    },

    /// Suggest snippets based on your shell history
    Suggest,

    /// Explain a snippet's command in plain English
    Explain {
        /// Snippet key to explain
        name: String,
    },
}
```

## Appendix B: Complete Data Flow — `snip ai`

```
┌──────────┐    ┌──────────────┐    ┌──────────────┐    ┌──────────┐
│  User     │───►│  CLI parse   │───►│  Load .snips │───►│  Fuzzy   │
│  input    │    │  (clap)      │    │  (snipfile)  │    │  match   │
└──────────┘    └──────────────┘    └──────────────┘    └────┬─────┘
                                                             │
                                                     ┌───────┴───────┐
                                                     │ score > 100?  │
                                                     └───┬───────┬───┘
                                               Yes ────┘       └─── No
                                                 │                  │
                                                 ▼                  ▼
                                        ┌──────────────┐   ┌──────────────┐
                                        │  Show match  │   │  Build LLM   │
                                        │  Confirm?    │   │  prompt with │
                                        │  [y/N]       │   │  .snips ctx  │
                                        └──────┬───────┘   └──────┬───────┘
                                               │                  │
                                               ▼                  ▼
                                        ┌──────────────┐   ┌──────────────┐
                                        │  Execute     │   │  HTTP POST   │
                                        │  (executor)  │   │  to provider │
                                        └──────────────┘   └──────┬───────┘
                                                                  │
                                                          ┌───────┴───────┐
                                                          │  Parse JSON  │
                                                          │  from LLM    │
                                                          └──────┬───────┘
                                                                 │
                                                                 ▼
                                                          ┌──────────────┐
                                                          │  Show cmd    │
                                                          │  Confirm?    │
                                                          │  [y/N]       │
                                                          └──────┬───────┘
                                                                 │
                                                                 ▼
                                                          ┌──────────────┐
                                                          │  Execute     │
                                                          │  (executor)  │
                                                          └──────────────┘
```

## Appendix C: Complete Data Flow — `snip suggest`

```
┌──────────────┐    ┌──────────────────┐    ┌──────────────────┐
│ .bash_history│───►│  Parse lines     │───►│  De-duplicate    │
│ .zsh_history │    │  (strip timestamps│   │  (HashSet<cmd>) │
│ fish_history │    │   per shell fmt) │    └────────┬─────────┘
└──────────────┘    └──────────────────┘             │
                                                     ▼
┌──────────────┐    ┌──────────────────┐    ┌──────────────────┐
│  .snips file │───►│  Filter out      │◄───│  Frequency count │
│  (existing   │    │  already-snipped │    │  + recency weigh │
│   snippets)  │    └────────┬─────────┘    └──────────────────┘
└──────────────┘             │
                             ▼
                   ┌──────────────────┐    ┌──────────────────┐
                   │  Rank by score   │───►│  Show top 5      │
                   │  (freq × recency │    │  Interactive     │
                   │   × complexity)  │    │  prompt          │
                   └──────────────────┘    └────────┬─────────┘
                                                      │
                                              ┌───────┴───────┐
                                              │ y / N / e     │
                                              └───┬───┬───┬───┘
                                                  │   │   │
                                         y ───────┘   │   └──── e ──┐
                                                      │              │
                                                      ▼              ▼
                                            ┌──────────────┐  ┌──────────────┐
                                            │  Skip        │  │  Edit key,   │
                                            │              │  │  then add    │
                                            └──────────────┘  └──────────────┘
```

## Appendix D: `snip explain` Example Outputs

### Simple command (Tier 1 — local)

```
$ snip explain build.release

  build.release
  ──────────────
  cargo build --release

  Builds the Rust project in release mode with optimizations enabled.
  The resulting binary will be in target/release/.
```

### Complex command (Tier 2 — LLM)

```
$ snip explain logs.staging

  logs.staging
  ─────────────
  kubectl logs -f deployment/web -n staging --tail=100 | jq '{time: .time, level: .level, msg: .msg}'

  Streams the last 100 log lines from the 'web' deployment in the 'staging'
  namespace, then pipes each line through 'jq' to extract only the time,
  log level, and message fields. This gives you a clean, structured log
  view instead of raw JSON.
```

### Command with variables

```
$ snip explain deploy

  deploy
  ──────
  kubectl apply -f k8s/{{ENV}}/ --wait

  Applies all Kubernetes manifests from the 'k8s/{{ENV}}/' directory and waits
  for the resources to be fully created/updated. The {{ENV}} variable controls
  which environment to deploy to (e.g., staging, production).

  Variables:
    ENV: Target deployment environment [development, staging, production]
```