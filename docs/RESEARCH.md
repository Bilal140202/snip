# Competitive Research & Feature Specification

> **Project**: `snip` — Project-scoped command snippet manager (Rust CLI)
> **Date**: 2025
> **Author**: Competitive Research Agent

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Competitive Landscape](#competitive-landscape)
3. [Tool-by-Tool Analysis](#tool-by-tool-analysis)
   - [just (casey/just)](#1-just-caseyjust)
   - [make / Makefile](#2-make--makefile)
   - [Taskfile (go-task/task)](#3-taskfile-go-tasktask)
   - [pet (knqyf263/pet)](#4-pet-knqyf263pet)
   - [direnv](#5-direnv)
   - [npm run](#6-npm-run)
   - [tldr](#7-tldr)
4. [Competitive Comparison Matrix](#competitive-comparison-matrix)
5. [Key Gaps in the Market](#key-gaps-in-the-market)
6. [Feature Specification for `snip`](#feature-specification-for-snip)
   - [.snips File Format Specification](#1-snips-file-format-specification-toml-schema)
   - [Command-by-Command Specification](#2-command-by-command-specification)
   - [`snip init` Auto-Detection](#3-snip-init-auto-detection)
   - [Fuzzy Matching Algorithm Requirements](#4-fuzzy-matching-algorithm-requirements)
   - [Shell Completion Specification](#5-shell-completion-specification)
   - [Edge Cases](#6-edge-cases-to-handle)
7. [Lessons from tldr's Success](#lessons-from-tldrs-success)
8. [References](#references)

---

## Executive Summary

The CLI task-running space is crowded but incomplete. Every existing tool fails on at least one of these axes:

- **Discovery** (can a newcomer find what commands exist?)
- **Descriptions** (do commands explain what they do?)
- **Zero config** (does it work on first run with no setup?)
- **Fuzzy finding** (can you find a command without remembering its exact name?)
- **Project-scoped & committable** (does the config live in the repo?)
- **Simple format** (is the config file easy to read and edit?)

`snip` targets the intersection of all six. It is the only tool that uses **TOML** (human-friendly, comments, no YAML whitespace pitfalls), provides **built-in fuzzy matching** (no fzf dependency), requires **zero config**, and lives in a **committable `.snips` file** in the project root.

The closest competitor is `just` (~20K stars), which uses a custom Makefile-like syntax, requires fzf for fuzzy selection, and lacks a structured data format. The most direct conceptual competitor is `pet` (~7.5K stars), which uses TOML but is **global** (not project-scoped) and depends on GitHub Gists for sync.

---

## Competitive Landscape

| Tool | GitHub Stars | Language | File Format | Project-Scoped | Built-in Fuzzy | Tab Completion | Descriptions |
|------|-------------|----------|-------------|----------------|----------------|----------------|--------------|
| `make` | ~45K (GNU) | C | Makefile | Yes | No | Partial (bash) | No (hack only) |
| `just` | ~20K | Rust | justfile | Yes | Via fzf | Yes | Yes (comments) |
| `Taskfile` | ~14K | Go | YAML | Yes | No | Yes | Yes (`desc`) |
| `direnv` | ~14K | Go | `.envrc` | Yes | N/A | N/A | N/A |
| `npm run` | N/A (npm) | JS | package.json | Yes | No | Partial | No |
| `tldr` | ~50K | Community | Markdown | N/A | Some clients | N/A | Yes (examples) |
| `pet` | ~7.5K | Go | TOML | **No** (global) | Via fzf | No | Yes |
| **`snip`** | — | Rust | **TOML** | **Yes** | **Yes (built-in)** | **Yes** | **Yes** |

---

## Tool-by-Tool Analysis

### 1. just (casey/just)

**GitHub Stars**: ~20,000+
**Language**: Rust
**File**: `justfile` / `Justfile` (custom Makefile-like syntax)

#### What It Does Well

- **Clean recipe syntax**: Recipes are easy to read and write, with none of Make's cryptic pitfalls (tab vs spaces, silent failures).
- **`just --list`**: Lists all recipes with their descriptions. A new contributor runs `just -l` and discovers everything.
- **`just --choose`**: Opens an interactive fzf-based fuzzy selector for choosing recipes to run.
- **Shell completion**: Built-in completion scripts for bash, zsh, fish, and PowerShell.
- **Always runs from project root**: You can run `just` from any subdirectory and it finds the `justfile` at the project root.
- **Cross-platform**: Works on Windows, macOS, and Linux.
- **Zero first-run config**: No config file needed. Drop a `justfile` in the repo and go.
- **Rich features**: Aliases, dependencies between recipes, variables, conditionals, shebang support for non-shell languages, `--dry-run`, `--summary`.
- **Committable**: `justfile` lives in the repo.

#### Commands for Listing/Running

| Command | Behavior |
|---------|----------|
| `just` | Run the default recipe |
| `just <recipe>` | Run a specific recipe |
| `just --list` / `just -l` | List all recipes with descriptions |
| `just --list --unsorted` | List without alphabetical sorting |
| `just --choose` | Interactive fzf-based recipe selector |
| `just --choose --chooser "fzf --preview 'just --show {}'"` | Custom chooser with preview |
| `just --summary` | Compact list (names only, no descriptions) |
| `just --show <recipe>` | Show the recipe's source |
| `just --init` | Create a template justfile |

#### First-Run Experience

Zero config. Install `just` and drop a `justfile` in the project root. No global config, no hooks, no setup steps.

#### How Descriptions Work

```justfile
# Build the project in debug mode
build:
    cargo build

# Run all tests
test: build
    cargo test
```

The `# comment` on the line before a recipe becomes its description. This is elegant but has limitations: there's no structured metadata, no tags, and the description is embedded in the execution syntax.

#### Weaknesses & Common Complaints

1. **Custom syntax, not a data format**: The `justfile` uses a proprietary syntax that's not a standard serialization format. You can't programmatically parse/generate it with standard tools.
2. **No subcommand/module system** ([#383](https://recipes, but many users want namespaced subcommands like `just db migrate` or `just deploy staging`)): `just` supports `.`-separated recipe names, but many users want namespaced subcommands.
3. **Fuzzy finding requires fzf**: `just --choose` shells out to fzf. If fzf isn't installed, it fails. There's no built-in fuzzy matching.
4. **Not executable as a single file** ([#367](https://github.com/casey/just/issues/367)): Unlike a shell script, you can't `chmod +x justfile && ./justfile`.
5. **No structured metadata**: Descriptions are comments, not first-class data fields. No tags, no categories, no additional metadata.
6. **Silent `--list` on no justfile**: Running `just --list` in a directory without a justfile shows nothing useful (no error guidance).

---

### 2. make / Makefile

**Stars**: Ubiquitous (GNU Make ~45K, but pre-installed on virtually every system)
**Language**: C
**File**: `Makefile` (tab-indented)

#### What It Does Well

- **Universal availability**: Pre-installed on every Linux/macOS system. No installation required.
- **Build system features**: Dependency graph, incremental builds, parallel execution (`-j`).
- **Convention**: Everyone knows what a `Makefile` is. The concept is universal.
- **Committable**: Lives in the repo.

#### Commands for Listing/Running

| Command | Behavior |
|---------|----------|
| `make` | Run the first target |
| `make <target>` | Run a specific target |
| `make -n <target>` | Dry run (show commands without executing) |
| `make -j4` | Run with 4 parallel jobs |
| `make TAB TAB` | Bash completion of targets (if bash-completion installed) |

#### Discovery: Where It Fails

**Make has no native `--list` command with descriptions.** This is its single biggest failure for command discovery. The StackOverflow thread "How do you get the list of targets in a makefile?" has dozens of answers, each a different hack:

- `make -pn` (parse output for targets — fragile)
- `grep '^[^ ]*:' Makefile` (misses include files, pattern rules)
- Custom `help` target (the most common workaround)

The famous [prwhite/help Makefile pattern](https://gist.github.com/prwhite/8168133) is a workaround that requires adding `##` comments and a `help` recipe that parses them:

```makefile
## build: Build the project
build:
    cargo build

## test: Run all tests
test:
    cargo test

help: ## Show this help
    @grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-15s\033[0m %s\n", $$1, $$2}'
```

This is a **community workaround for a fundamental missing feature**. Every project reimplements it. The fact that this gist has 3K+ stars demonstrates the demand.

#### First-Run Experience

Zero config for basic use. But learning Makefile syntax is a rite of passage with many pitfalls.

#### Weaknesses & Common Complaints

1. **Tab indentation**: Makefiles require literal tab characters. Mixing tabs and spaces causes cryptic errors ("missing separator").
2. **Silent failures**: Many Makefile mistakes fail silently with no error message. Debugging is painful.
3. **No native descriptions**: The most requested feature that doesn't exist.
4. **Designed for builds, not commands**: Make is a build system. Using it as a command runner is a misappropriation that everyone does anyway.
5. **No fuzzy finding**: None at all.
6. **Phony target boilerplate**: Every non-file target needs `.PHONY:` declarations.

---

### 3. Taskfile (go-task/task)

**GitHub Stars**: ~14,000+
**Language**: Go
**File**: `Taskfile.yml` (YAML)

#### What It Does Well

- **Structured YAML format**: First-class `desc:` field, `vars:`, `env:`, `deps:`, `cmds:` — everything is a named field.
- **`task --list`**: Clean output showing task name + description.
- **Cross-platform**: Works on Windows, macOS, Linux. Handles path separators.
- **Shell completion**: For bash, zsh, fish, PowerShell.
- **Variable templating**: `{{.VAR}}` syntax for variables.
- **Task dependencies**: `deps:` field for ordering.
- **Includes**: Can include other Taskfiles for modularity.
- **Output control**: `silent:`, `interactive:`, `internal:` flags per task.
- **Committable**: `Taskfile.yml` lives in the repo.

#### Commands for Listing/Running

| Command | Behavior |
|---------|----------|
| `task` | Run the default task |
| `task <name>` | Run a specific task |
| `task --list` / `task -l` | List all tasks with descriptions |
| `task -a` | List all tasks including hidden/internal ones |
| `task --list-all` | List including tasks from included files |
| `task --init` | Create a template Taskfile.yml |
| `task -s <name>` | Show task details |
| `task --watch` | Watch for file changes and re-run |

#### First-Run Experience

Zero config. Install `task` and drop a `Taskfile.yml` in the repo. No global setup.

#### How Descriptions Work

```yaml
version: '3'
tasks:
  build:
    desc: Build the Go binary
    cmds:
      - go build -o bin/app ./...

  test:
    desc: Run all tests with coverage
    cmds:
      - go test -cover ./...
```

The `desc:` field is first-class data. This is better than `just`'s comment-based approach for programmatic access.

#### Weaknesses & Common Complaints

1. **YAML verbosity and pitfalls**: YAML has well-known issues — significant whitespace, type coercion (`on`/`yes`/`true` all become boolean `true`), multiline strings are awkward.
2. **No built-in fuzzy finding**: No `--choose` equivalent. You must type the full task name or use tab completion.
3. **Global taskfile completion issues** ([#1574](https://github.com/go-task/task/issues/1574)): Tab completion doesn't work for global Taskfiles.
4. **Variable leaking** ([#2488](https://github.com/go-task/task/discussions/2488)): Variables in included Taskfiles leak to the parent.
5. **Fish shell completion bugs** ([#2591](https://github.com/go-task/task/issues/2591)): Completion includes whitespace.
6. **Heavier than needed**: Taskfile is powerful but heavy for just saving a few command snippets.

---

### 4. pet (knqyf263/pet)

**GitHub Stars**: ~7,500
**Language**: Go
**File**: TOML (`~/.config/pet/snippet.toml` by default)

#### What It Does Well

- **TOML format**: Clean, human-readable, supports comments. Same format `snip` will use.
- **Tagging**: Snippets can be tagged for categorization.
- **Search**: Fuzzy search through snippets (via fzf).
- **Execute**: Can search and execute snippets directly.
- **Sync**: Can sync snippets via GitHub Gists or GitLab Snippets.
- **Edit**: `pet edit` opens the TOML file in `$EDITOR`.

#### Commands for Listing/Running

| Command | Behavior |
|---------|----------|
| `pet list` | List all snippets |
| `pet new` | Create a new snippet interactively |
| `pet edit` | Open snippet file in editor |
| `pet search` | Fuzzy search snippets (uses fzf) |
| `pet exec` | Search and execute a snippet |
| `pet sync` | Sync snippets with Gist/GitLab |
| `pet configure` | Set up config (Gist token, etc.) |

#### First-Run Experience

**Requires config.** You must run `pet configure` or manually create `~/.config/pet/config.toml` with at minimum a `SnippetFile` path. For Gist sync, you need a GitHub token. This is a significant onboarding friction.

#### How Descriptions Work

```toml
[[snippets]]
description = "List all Docker containers"
command = "docker ps -a"
tags = ["docker", "containers"]

[[snippets]]
description = "Kill process on port 3000"
command = "lsof -ti:3000 | xargs kill -9"
tags = ["networking", "process"]
```

Descriptions are first-class TOML fields. Tags are supported. This is structurally excellent.

#### Why Only 7.5K Stars: Critical Weaknesses

1. **Not project-scoped**: This is the #1 reason `pet` hasn't gained wider adoption. Snippets are stored in `~/.config/pet/snippet.toml` — a global file. You can't have project-specific snippets that live in the repo. Every developer on a team has their own global snippets, with no sharing mechanism other than Gist sync (which is still global, not per-project).
2. **Gist dependency for sync**: Syncing requires a GitHub token and Gist setup. This creates friction and excludes users who can't use GitHub (enterprise restrictions, air-gapped environments, GitLab users).
3. **Multi-line snippet issues** ([#116](https://github.com/knqyf263/pet/issues/116)): Multi-line commands don't work well. You have to manually add `\n` in TOML.
4. **Config file crashes** ([#321](https://github.com/knqyf263/pet/discussions/321)): Omitting `SnippetFile` in config causes a panic.
5. **No shell completion**: There's no tab completion for snippet names.
6. **Stale project**: Last significant update was years ago. Low maintenance activity.
7. **No init detection**: Doesn't detect existing commands from package.json, Makefile, etc.

---

### 5. direnv

**GitHub Stars**: ~14,000+
**Language**: Go (originally Shell)
**File**: `.envrc` (shell script)

#### Relevance to snip

`direnv` is not a command runner — it's an environment variable manager. However, it's highly relevant because it pioneered the **project-scoped config file in repo root** pattern that `snip` follows.

#### What It Does Well

- **Project-scoped `.envrc`**: Each project directory can have an `.envrc` file that sets environment variables.
- **Automatic loading/unloading**: When you `cd` into a directory, direnv loads the `.envrc`. When you leave, it unloads.
- **Shell hooks**: Supports bash, zsh, fish, tcsh, elvish.
- **Security**: Requires `direnv allow` to trust a new `.envrc`.

#### First-Run Experience

**High friction.** Requires two steps:
1. Add shell hook to your `.bashrc`/`.zshrc`: `eval "$(direnv hook zsh)"`
2. Restart shell
3. For each project: create `.envrc`, then run `direnv allow`

The `direnv allow` step is a common point of confusion ([Medium troubleshooting post](https://medium.com/@nayeem.ridoy/how-i-fix-the-direnv-allow-error-troubleshooting-envrc-7b0135553503)).

#### Lessons for snip

- **Zero config is critical**: `direnv`'s hook requirement means many users bounce before they experience the value.
- **The "allow" step is a blocker**: Any security gate before first use reduces adoption. `snip` must work immediately on install.
- **`.envrc` is committable**: Teams commit `.envrc` to git (with `.env` in `.gitignore`). This pattern works.

---

### 6. npm run

**Stars**: N/A (part of npm itself, 24M+ weekly downloads)
**Language**: JavaScript
**File**: `package.json` (JSON `scripts` section)

#### What It Does Well

- **Universal in Node.js**: Every Node project has `package.json`. Zero additional installation.
- **Conventional**: `npm run dev`, `npm run build`, `npm test` — these are universal commands.
- **Lifecycle scripts**: `preinstall`, `postinstall`, `prepare` hooks.
- **Committable**: `package.json` is always in the repo.

#### Commands for Listing/Running

| Command | Behavior |
|---------|----------|
| `npm run` | List all scripts (names only, no descriptions) |
| `npm run <script>` | Run a specific script |
| `npm test` | Run the `test` script (shortcut) |
| `npm start` | Run the `start` script (shortcut) |
| `npm run --silent <script>` | Suppress output |

#### Discovery Experience: Why It's Bad

Running `npm run` produces output like this:

```
Lifecycle scripts included in my-project:
  test
  start

available via `npm run-script`:
  build
  dev
  lint
  typecheck
```

**There are no descriptions.** You see `build`, `dev`, `lint`, `typecheck` — and you have no idea what any of them does without opening `package.json` and reading the command strings.

This has been a [known issue since 2016](https://github.com/npm/npm/issues/9952) (npm issue #9952, titled "Descriptions for npm Scripts"). The npm team has acknowledged it but never implemented it. Third-party tools like `npm-scripts-info` (which uses a `scripts-info` field in `package.json`) exist as workarounds but have very low adoption.

#### Weaknesses

1. **No descriptions**: The #1 complaint. You must read the JSON to understand what a script does.
2. **No fuzzy finding**: None at all.
3. **Noisy output**: npm scripts print a lot of npm-related noise before/after the actual command.
4. **JSON limitations**: No comments in JSON. Can't annotate scripts. Multiline commands are awkward.
5. **Platform-dependent**: Script commands may not work cross-platform (e.g., `rm -rf` on Windows).
6. **Not general-purpose**: Only works in Node.js projects.

---

### 7. tldr

**GitHub Stars**: ~50,000+
**Language**: Community (multiple client implementations)
**File**: Markdown pages organized by command name

#### Why tldr Succeeded Where man Pages Didn't

tldr is not a command runner, but its success story is directly applicable to `snip`'s strategy:

1. **Examples over reference**: `man tar` gives you every flag in alphabetical order. `tldr tar` gives you 5 common use cases. tldr succeeded because it answers "how do I use this?" not "what are all the options?"

2. **Community-driven content**: ~5,000+ contributors. Low barrier to contribution. Anyone can submit a page.

3. **Multiple client implementations**: The core is the page format (Markdown), not a specific tool. This led to clients in Node.js, Python, Rust, Go, and even web-based versions.

4. **Simple format**: Each page is ~20 lines of Markdown. No complex markup. This made contribution trivial.

5. **Solved a universal pain point**: Everyone who uses the command line has been frustrated by man pages. tldr's value proposition is immediately obvious.

#### Lessons for snip

- **Solve a universal pain point**: "What commands does this project have?" is asked by every developer on every project. It's the `man tar` problem applied to project commands.
- **Simple format lowers barrier**: TOML is as simple as Markdown for config. Anyone can read/write it.
- **Community contribution is key**: If `snip` makes it trivial to add a `.snips` file, adoption spreads organically.
- **Multiple implementations aren't needed**: Unlike tldr, `snip` is a single tool. But the file format should be simple enough that anyone could write a parser.

---

## Competitive Comparison Matrix

### Discovery Experience (How easy is it to find what commands exist?)

| Tool | List Command | Shows Descriptions? | Fuzzy Find? | First-Run Config? |
|------|-------------|-------------------|-------------|-------------------|
| `make` | None native | No | No | None |
| `just` | `just -l` | Yes | `--choose` (needs fzf) | None |
| `Taskfile` | `task -l` | Yes | No | None |
| `npm run` | `npm run` | **No** | No | None |
| `pet` | `pet list` | Yes | `pet search` (needs fzf) | Config needed |
| **`snip`** | `snip list` | **Yes** | **Built-in** | **None** |

### Developer Experience (How pleasant is it to use?)

| Tool | File Format | Comments in File? | Structured Metadata? | Tab Completion? | Cross-Platform? |
|------|------------|-------------------|----------------------|-----------------|-----------------|
| `make` | Makefile | Yes (`#`) | No | Partial | Partial (Windows issues) |
| `just` | justfile | Yes (`#`) | No | Yes | Yes |
| `Taskfile` | YAML | Yes (`#`) | Yes | Yes | Yes |
| `npm run` | JSON | **No** | **No** | Partial | Partial |
| `pet` | TOML | Yes (`#`) | Yes | **No** | Yes |
| **`snip`** | **TOML** | **Yes (`#`)** | **Yes** | **Yes** | **Yes** |

### Team Fit (How well does it work for teams?)

| Tool | Project-Scoped? | Committable? | Shareable via Git? | No External Deps? |
|------|----------------|-------------|-------------------|-------------------|
| `make` | Yes | Yes | Yes | Yes |
| `just` | Yes | Yes | Yes | Yes |
| `Taskfile` | Yes | Yes | Yes | Yes |
| `npm run` | Yes | Yes | Yes | Yes |
| `pet` | **No** (global) | **No** | **No** (needs Gist) | **No** (needs fzf, Gist) |
| **`snip`** | **Yes** | **Yes** | **Yes** | **Yes** |

---

## Key Gaps in the Market

These are the specific gaps that `snip` fills:

### Gap 1: No tool has both built-in fuzzy finding AND project-scoped TOML config

- `just` has project-scoping but uses custom syntax and requires fzf
- `pet` has TOML and fuzzy finding but is global and requires fzf
- `Taskfile` has project-scoping but uses YAML and has no fuzzy finding
- **`snip`**: TOML + project-scoped + built-in fuzzy finding

### Gap 2: No tool auto-detects existing commands

None of `just`, `make`, `Taskfile`, or `pet` will scan your `package.json`, `Makefile`, `Cargo.toml`, or `pyproject.toml` and generate a starter config. You always start from scratch.

- **`snip init`**: Scans for known project files and generates a `.snips` file pre-populated with detected commands.

### Gap 3: No snippet manager is project-first

`pet` is the closest conceptually but is fundamentally global. Every other tool is a task runner (not a snippet manager). `snip` is specifically a **project-scoped snippet manager** — snippets that travel with the project.

### Gap 4: No tool uses TOML for task running

`pet` uses TOML but isn't project-scoped. No task runner uses TOML. TOML is the sweet spot between JSON (no comments, strict) and YAML (whitespace pitfalls, type coercion). Rust's own ecosystem standardizes on TOML (`Cargo.toml`, `rustfmt.toml`, `clippy.toml`).

---

## Feature Specification for `snip`

### 1. `.snips` File Format Specification (TOML Schema)

The `.snips` file lives in the project root directory. It uses TOML format with the following schema:

```toml
# .snips - Project command snippets
# Generated by snip, edited by humans

version = "1"

# Optional: metadata about the project
[meta]
project = "my-project"
description = "A web application"

# Commands are defined as [[commands]] entries
# Each command has: name, description (optional), command, tags (optional), env (optional)

# --- Development ---
[[commands]]
name = "dev"
description = "Start the development server with hot reload"
command = "cargo run"
tags = ["dev", "server"]

[[commands]]
name = "build"
description = "Build the project in release mode"
command = "cargo build --release"
tags = ["build"]

[[commands]]
name = "test"
description = "Run all tests with output"
command = "cargo test -- --nocapture"
tags = ["test"]

# --- Database ---
[[commands]]
name = "db-migrate"
description = "Run all pending database migrations"
command = "sqlx migrate run"
tags = ["db", "migrate"]

[[commands]]
name = "db-reset"
description = "Reset database and re-run all migrations"
command = "sqlx database reset && sqlx migrate run"
tags = ["db", "reset"]

# --- Docker ---
[[commands]]
name = "docker-up"
description = "Start all Docker services"
command = "docker compose up -d"
tags = ["docker"]

[[commands]]
name = "docker-down"
description = "Stop all Docker services and remove volumes"
command = "docker compose down -v"
tags = ["docker"]

# --- Code Quality ---
[[commands]]
name = "lint"
description = "Run the linter"
command = "cargo clippy -- -D warnings"
tags = ["lint", "quality"]

[[commands]]
name = "fmt"
description = "Format all code"
command = "cargo fmt --all"
tags = ["format", "quality"]

# --- Multi-line commands (use triple-quoted strings) ---
[[commands]]
name = "release"
description = "Create a new release: bump version, build, tag, push"
command = """
cargo set-version {{version}}
cargo build --release
git add -A
git commit -m "Release {{version}}"
git tag v{{version}}
git push && git push --tags
"""
tags = ["release"]
```

#### Schema Rules

1. **`version`** (string, required): File format version. Currently `"1"`. Allows future format changes.
2. **`[meta]`** (table, optional): Project metadata. Not used by `snip` itself; for human/tooling consumption.
3. **`[[commands]]`** (array of tables, required — at least one entry):
   - **`name`** (string, required): The command name. Used as the primary identifier and for matching. Must be unique. May contain hyphens, underscores, and alphanumeric characters. Regex: `^[a-zA-Z0-9][a-zA-Z0-9_-]*$`
   - **`description`** (string, optional): Human-readable description. Shown in `snip list` and used for fuzzy matching.
   - **`command`** (string, required): The shell command to execute. Supports multiline via TOML triple-quoted strings (`"""..."""`). Supports template variables (see below).
   - **`tags`** (array of strings, optional): Tags for filtering. Shown in `snip list --tags`.
   - **`env`** (table, optional): Environment variables to set before running the command.
   - **`dir`** (string, optional): Working directory relative to project root. Defaults to project root.

4. **Template variables**: Commands support `{{var}}` syntax. Variables are resolved from:
   - Command-line arguments: `snip run release version=2.0.0`
   - Environment variables: `{{env.HOME}}`
   - Shell expansion at runtime (fallback)

5. **Comments**: Standard TOML `#` comments are preserved and encouraged.

6. **Ordering**: Commands appear in the order they are defined. `snip list` preserves this order by default.

7. **Empty `.snips` file**: A `.snips` file with only `version = "1"` and no `[[commands]]` is valid. `snip list` prints "No snippets defined. Add some to .snips".

#### TOML Example: Minimal

```toml
version = "1"

[[commands]]
name = "dev"
command = "npm run dev"

[[commands]]
name = "test"
command = "npm test"
```

#### TOML Example: With All Features

```toml
version = "1"

[meta]
project = "my-api"
description = "REST API service"

[[commands]]
name = "dev"
description = "Start development server"
command = "cargo run"
tags = ["dev"]
dir = "server"
env = { RUST_LOG = "debug", PORT = "3000" }

[[commands]]
name = "deploy-staging"
description = "Deploy to staging environment"
command = """
echo "Deploying to staging..."
fly deploy --app my-api-staging
echo "Done!"
"""
tags = ["deploy", "staging"]
```

---

### 2. Command-by-Command Specification

#### `snip init` — Initialize a new `.snips` file

```
snip init [OPTIONS]
```

| Flag | Short | Description |
|------|-------|-------------|
| `--force` | `-f` | Overwrite existing `.snips` file |
| `--dry-run` | `-n` | Show what would be detected without writing |

**Behavior**:
1. Scan current directory (recursively, max depth 2) for known project files.
2. For each detected file, extract known commands.
3. Generate a `.snips` file with detected commands, each with a generated description.
4. Print summary: "Detected 12 commands from 3 project files."

**Detection rules** (see [Section 3](#3-snip-init-auto-detection) for full details):
- `package.json` → extract `scripts`
- `Makefile` → extract `.PHONY` targets and `##` documented targets
- `Cargo.toml` → extract common cargo commands (build, test, run, clippy, fmt)
- `pyproject.toml` → extract `[tool.pytest]`, `[tool.ruff]`, `[tool.mypy]` commands
- `docker-compose.yml` / `docker-compose.yaml` → extract services
- `Taskfile.yml` → extract tasks with descriptions
- `justfile` / `Justfile` → extract recipes with descriptions

**Edge cases**:
- If `.snips` already exists: print error and suggest `--force`. Never silently overwrite.
- If no known project files found: create empty `.snips` with helpful comments.
- If run outside a git repo: warn but proceed.

#### `snip list` — List all snippets

```
snip list [OPTIONS] [QUERY]
```

| Flag | Short | Description |
|------|-------|-------------|
| `--tags` | `-t` | Show tags alongside each command |
| `--raw` | `-r` | Output in machine-readable format (JSON) |
| `--all` | `-a` | Include hidden/internal commands |
| `--quiet` | `-q` | Only print command names (no descriptions) |

**Arguments**:
- `QUERY` (optional): Filter commands by fuzzy match against name + description.

**Output format** (default):
```
  dev          Start the development server with hot reload
  build        Build the project in release mode
  test         Run all tests with output
  db-migrate   Run all pending database migrations
  lint         Run the linter
  fmt          Format all code
```

Columns are padded to align descriptions. Uses terminal colors (cyan for names, dim for descriptions).

**With `--tags`**:
```
  dev          Start the development server with hot reload    [dev, server]
  build        Build the project in release mode               [build]
```

**With `--raw`**:
```json
[
  {"name": "dev", "description": "Start the development server with hot reload", "tags": ["dev", "server"]},
  {"name": "build", "description": "Build the project in release mode", "tags": ["build"]}
]
```

**With QUERY** (fuzzy filter):
```
$ snip list "db"
  db-migrate   Run all pending database migrations
  db-reset     Reset database and re-run all migrations
```

#### `snip run` — Execute a snippet

```
snip run [OPTIONS] <NAME_OR_QUERY> [VARS...]
```

| Flag | Short | Description |
|------|-------|-------------|
| `--dry-run` | `-n` | Print command without executing |
| `--verbose` | `-v` | Show command before executing |
| `--shell` | `-s` | Shell to use (default: `$SHELL` or `sh`) |
| `--confirm` | `-c` | Prompt for confirmation before running |

**Arguments**:
- `NAME_OR_QUERY` (required): Either an exact command name or a fuzzy query. If the query matches exactly one command, run it. If it matches multiple, show a picker. If it matches none, show error with suggestions.
- `VARS...` (optional): Key=value pairs for template variables. E.g., `snip run deploy env=staging version=2.0.0`

**Behavior**:
1. Parse `.snips` file.
2. Match `NAME_OR_QUERY` against command names (exact first, then fuzzy).
3. If exact match: execute immediately.
4. If single fuzzy match: execute immediately.
5. If multiple fuzzy matches: display interactive picker (numbered list, user selects).
6. If no matches: print error "Unknown command: 'foo'. Did you mean one of: bar, baz?"

**Template variable resolution**:
```bash
$ snip run deploy env=staging
# Resolves {{env}} to "staging" in the command template
```

#### `snip add` — Add a new snippet

```
snip add [OPTIONS] <NAME> <COMMAND>
```

| Flag | Short | Description |
|------|-------|-------------|
| `--description` | `-d` | Description for the command |
| `--tags` | `-t` | Comma-separated tags |

**Behavior**:
1. If `.snips` doesn't exist, create it with `version = "1"`.
2. Append a new `[[commands]]` entry.
3. Preserve TOML formatting and comments.
4. Print confirmation: "Added command 'name' to .snips".

**Interactive mode** (when called without arguments):
```
$ snip add
Command name: deploy-staging
Description: Deploy to staging environment
Command: fly deploy --app my-app-staging
Tags (comma-separated): deploy, staging
Added command 'deploy-staging' to .snips
```

#### `snip edit` — Open `.snips` in editor

```
snip edit [OPTIONS]
```

| Flag | Short | Description |
|------|-------|-------------|
| `--editor` | `-e` | Editor to use (default: `$EDITOR` or `$VISUAL`) |

**Behavior**: Opens `.snips` in the user's editor. After the editor closes, validates the file and reports any parsing errors.

#### `snip show` — Show a single command

```
snip show <NAME>
```

**Behavior**: Print the full details of a command: name, description, tags, and the command string.

```
$ snip show deploy-staging
Name:        deploy-staging
Description: Deploy to staging environment
Tags:        deploy, staging
Command:
  fly deploy --app my-app-staging
```

#### `snip remove` / `snip rm` — Remove a snippet

```
snip remove <NAME>
```

**Behavior**: Remove the command entry from `.snips`. Ask for confirmation unless piped. Preserve all other entries and formatting.

#### `snip completion` — Generate shell completion scripts

```
snip completion <SHELL>
```

**Shells**: `bash`, `zsh`, `fish`, `powershell`, `elvish`

**Behavior**: Print the completion script to stdout. User pipes to a file or evals:
```bash
# bash
eval "$(snip completion bash)"

# zsh
eval "$(snip completion zsh)"

# fish
snip completion fish | source
```

#### `snip --help` — Built-in help

```
$ snip --help
snip - Project-scoped command snippets

Usage: snip [COMMAND]

Commands:
  init       Initialize a new .snips file
  list       List all snippets
  run        Execute a snippet
  add        Add a new snippet
  edit       Open .snips in editor
  show       Show details of a snippet
  remove     Remove a snippet
  completion  Generate shell completion

Options:
  -h, --help     Print help
  -V, --version  Print version
```

#### `snip` (no arguments) — Default behavior

When run with no arguments in a directory with a `.snips` file: equivalent to `snip list`. This mirrors `just`'s pattern of `default: just --list` and gives immediate value on first run.

---

### 3. `snip init` Auto-Detection

`snip init` scans for the following project files and extracts commands:

#### package.json Detection

```json
{
  "scripts": {
    "dev": "next dev",
    "build": "next build",
    "start": "next start",
    "lint": "next lint",
    "test": "jest --coverage",
    "typecheck": "tsc --noEmit"
  }
}
```

Generates:
```toml
[[commands]]
name = "dev"
description = "Run dev script from package.json"
command = "npm run dev"
tags = ["npm"]

[[commands]]
name = "build"
description = "Run build script from package.json"
command = "npm run build"
tags = ["npm"]
# ... etc
```

**Detection rules**:
- Skip `pre*` and `post*` lifecycle scripts (they run automatically).
- Skip common npm internal scripts (`install`, `uninstall`, `prepare`, `pack`, `publish`).
- Use script name as `snip` command name.
- Description: "Run {name} script from package.json" (since npm scripts lack descriptions).

#### Makefile Detection

Parses Makefile for:
1. `.PHONY` target names (these are typically "command" targets, not file targets).
2. `## comment` patterns before targets (the prwhite/help convention).

```makefile
## build: Build the project
build:
    cargo build

## test: Run all tests
test:
    cargo test

.PHONY: build test
```

Generates:
```toml
[[commands]]
name = "build"
description = "Build the project"
command = "make build"
tags = ["make"]

[[commands]]
name = "test"
description = "Run all tests"
command = "make test"
tags = ["make"]
```

**Detection rules**:
- Extract `## description` comments before target definitions.
- Prioritize `.PHONY` targets.
- Skip targets that look like file targets (contain `.` extension like `.o`, `.js`).
- Wrap in `make <target>` so the Makefile remains the source of truth.

#### Cargo.toml Detection

Doesn't parse for custom commands (Cargo.toml doesn't have a scripts section), but detects the project type and generates standard cargo commands:

```toml
[[commands]]
name = "build"
description = "Build the Rust project"
command = "cargo build"
tags = ["cargo"]

[[commands]]
name = "test"
description = "Run all Rust tests"
command = "cargo test"
tags = ["cargo"]

[[commands]]
name = "run"
description = "Run the Rust binary"
command = "cargo run"
tags = ["cargo"]

[[commands]]
name = "lint"
description = "Run Clippy linter"
command = "cargo clippy -- -D warnings"
tags = ["cargo", "lint"]

[[commands]]
name = "fmt"
description = "Format Rust code"
command = "cargo fmt --all"
tags = ["cargo", "format"]
```

#### pyproject.toml Detection

Parses for:
- `[tool.pytest.ini_options]` → generate `test` command
- `[tool.ruff]` → generate `lint` and `fmt` commands
- `[tool.mypy]` → generate `typecheck` command
- `[project.scripts]` → generate entry point commands

```toml
[[commands]]
name = "test"
description = "Run pytest"
command = "pytest -v"
tags = ["python", "test"]

[[commands]]
name = "lint"
description = "Run ruff linter"
command = "ruff check ."
tags = ["python", "lint"]

[[commands]]
name = "fmt"
description = "Format Python code with ruff"
command = "ruff format ."
tags = ["python", "format"]

[[commands]]
name = "typecheck"
description = "Run mypy type checker"
command = "mypy ."
tags = ["python", "types"]
```

#### docker-compose.yml / docker-compose.yaml Detection

Parses for service names:

```yaml
services:
  web:
    build: .
  db:
    image: postgres:16
  redis:
    image: redis:7
```

Generates:
```toml
[[commands]]
name = "docker-up"
description = "Start all Docker Compose services"
command = "docker compose up -d"
tags = ["docker"]

[[commands]]
name = "docker-down"
description = "Stop all Docker Compose services"
command = "docker compose down"
tags = ["docker"]

[[commands]]
name = "docker-logs"
description = "View Docker Compose service logs"
command = "docker compose logs -f"
tags = ["docker"]
```

#### Taskfile.yml / justfile Detection

Parses existing task runner configs and imports their tasks:

- `Taskfile.yml`: Extract `tasks.*.desc` and `tasks.*.cmds`
- `justfile`: Extract recipe names and `#` descriptions

Generates commands that delegate to the original tool:
```toml
[[commands]]
name = "build"
description = "Build the go binary"  # from Taskfile desc
command = "task build"
tags = ["taskfile"]
```

---

### 4. Fuzzy Matching Algorithm Requirements

`snip` must include a **built-in fuzzy matcher** (no external dependency on fzf). The algorithm should be:

#### Requirements

1. **Substring matching with character gaps**: Query `"dbm"` should match `"db-migrate"`. Each character in the query must appear in the target in order, but not necessarily contiguously.

2. **Consecutive character bonus**: Matching consecutive characters scores higher. `"dbm"` matching `"db-migrate"` scores higher if `d-b` and `m` are closer together.

3. **Word boundary bonus**: Characters at word boundaries (start of string, after `-`, after `_`) score higher. Query `"dm"` against `"db-migrate"` should prefer matches at word starts.

4. **Case-insensitive**: All matching is case-insensitive.

5. **Prefix bonus**: Matches at the start of the string get a bonus. `"dev"` against `"dev-server"` scores higher than against `"run-dev-server"`.

6. **Description matching**: If the query doesn't match the command name well, also match against the description. A match in the description scores lower than a match in the name.

7. **Scoring function** (pseudocode):

```
fuzzy_score(query, target_name, target_description):
    name_score = smith_waterman_score(query, target_name)
    desc_score = smith_waterman_score(query, target_description) * 0.5
    return max(name_score, desc_score)
```

Where `smith_waterman_score` is a simplified Smith-Waterman local alignment:
- Match: +2 (or +3 at word boundary)
- Mismatch: -1
- Gap: -1

8. **Minimum threshold**: If the best score is below a threshold (e.g., 30% of max possible), return "no match" rather than a poor match.

9. **Performance**: Must complete matching against 100+ commands in < 10ms. The algorithm is O(n*m) where n = query length and m = target length, which is negligible for command names (typically < 50 chars).

10. **Tie-breaking**: When multiple commands have the same score, prefer:
    1. Shorter name (more specific match)
    2. Alphabetical order
    3. Definition order in `.snips`

#### Implementation Notes

- Use the `fuzzy-matcher` crate (Rust) or implement a simplified Smith-Waterman.
- The `fuzzy-matcher` crate implements a skim-like algorithm (used by `skim`, the Rust fzf alternative).
- For the interactive picker (when `snip run` matches multiple results), display results sorted by score.

---

### 5. Shell Completion Specification

`snip completion <SHELL>` generates completion scripts for the following shells:

#### Bash

```bash
_snip() {
    local cur prev commands
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"

    # Complete subcommands
    if [[ ${COMP_CWORD} -eq 1 ]]; then
        COMPREPLY=($(compgen -W "init list run add edit show remove completion --help --version" -- "${cur}"))
        return
    fi

    # Complete 'run' with command names from .snips
    if [[ ${prev} == "run" ]]; then
        commands=$(snip list --quiet 2>/dev/null)
        COMPREPLY=($(compgen -W "${commands}" -- "${cur}"))
        return
    fi

    # Complete 'show' and 'remove' with command names
    if [[ ${prev} == "show" || ${prev} == "remove" || ${prev} == "rm" ]]; then
        commands=$(snip list --quiet 2>/dev/null)
        COMPREPLY=($(compgen -W "${commands}" -- "${cur}"))
        return
    fi
}
complete -F _snip snip
```

#### Zsh

```zsh
#compdef snip

_snip() {
    local -a commands
    commands=(
        'init:Initialize a new .snips file'
        'list:List all snippets'
        'run:Execute a snippet'
        'add:Add a new snippet'
        'edit:Open .snips in editor'
        'show:Show details of a snippet'
        'remove:Remove a snippet'
        'completion:Generate shell completion'
    )

    if (( CURRENT == 2 )); then
        _describe 'command' commands
    elif (( CURRENT == 3 )); then
        case $words[2] in
            run|show|remove|rm)
                local -a snip_commands
                snip_commands=(${(f)"$(snip list --quiet 2>/dev/null)"})
                _describe 'snippet' snip_commands
                ;;
        esac
    fi
}
```

#### Fish

```fish
complete -c snip -f

complete -c snip -n '__fish_use_subcommand' -a init -d 'Initialize a new .snips file'
complete -c snip -n '__fish_use_subcommand' -a list -d 'List all snippets'
complete -c snip -n '__fish_use_subcommand' -a run -d 'Execute a snippet'
complete -c snip -n '__fish_use_subcommand' -a add -d 'Add a new snippet'
complete -c snip -n '__fish_use_subcommand' -a edit -d 'Open .snips in editor'
complete -c snip -n '__fish_use_subcommand' -a show -d 'Show details of a snippet'
complete -c snip -n '__fish_use_subcommand' -a remove -d 'Remove a snippet'
complete -c snip -n '__fish_use_subcommand' -a rm -d 'Remove a snippet'
complete -c snip -n '__fish_use_subcommand' -a completion -d 'Generate shell completion'

# Dynamic completion for command names
complete -c snip -n '__fish_seen_subcommand_from run show remove rm' -a '(snip list --quiet)'
```

#### PowerShell

```powershell
Register-ArgumentCompleter -Native -CommandName snip -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)

    $commands = @('init', 'list', 'run', 'add', 'edit', 'show', 'remove', 'completion', '--help', '--version')

    if ($commandAst.CommandElements.Count -eq 2) {
        $commands | Where-Object { $_ -like "$wordToComplete*" }
    } elseif ($commandAst.CommandElements[1].Value -match '^(run|show|remove|rm)$') {
        $snipCommands = snip list --quiet 2>$null
        if ($snipCommands) {
            $snipCommands | Where-Object { $_ -like "$wordToComplete*" }
        }
    }
}
```

#### Completion Behavior Requirements

1. **Dynamic**: Completion for command names is dynamic — it reads `.snips` at completion time, so new commands appear immediately after editing the file.
2. **Fast**: Must complete in < 100ms. The `snip list --quiet` command must be optimized for this.
3. **Graceful degradation**: If `.snips` doesn't exist or is malformed, completion for subcommands still works; command name completion returns empty.
4. **Descriptive** (zsh/fish): Where possible, show descriptions alongside command names in the completion menu.

---

### 6. Edge Cases to Handle

#### Nested Sections / Namespacing

Users may want namespaced commands like `db migrate` or `deploy staging`. `snip` handles this through naming conventions (hyphens: `db-migrate`, `deploy-staging`) rather than nested TOML tables, which keeps the format flat and simple.

However, the TOML format supports an optional `[groups]` section for organizational display:

```toml
[groups.dev]
title = "Development"
# Commands with tag "dev" are shown under this group in `snip list`

[groups.db]
title = "Database"
# Commands with tag "db" are shown under this group in `snip list`
```

**Decision**: Groups are optional. By default, `snip list` shows a flat list. With `--grouped`, it groups by tags that have matching `[groups.*]` entries.

#### Special Characters in Commands

- **Dollar signs**: `$VAR` in command strings — TOML requires no escaping for `$` in basic strings, but `$$` is needed in basic strings to represent a literal `$`. Prefer TOML literal strings (`'...'`) or basic strings with proper escaping.
- **Backslashes**: Path separators on Windows. TOML basic strings treat `\` as escape. Use `\\` or literal strings.
- **Quotes in commands**: Commands containing single/double quotes must use TOML escaping or triple-quoted strings.
- **Template variable collision**: If a command legitimately contains `{{...}}` that isn't a template variable, use `{{"{{"}}` escaping (TOML inline table trick) or a raw string.

**Recommendation**: For the `command` field, document that triple-quoted basic strings (`"""..."""`) are preferred for complex commands. The parser should handle common escaping patterns.

#### Cross-Platform

- **Path separators**: Use `/` in commands; document that `snip` doesn't transform paths. Users on Windows should use commands that work on Windows (or use `sh -c`).
- **Shell**: Default to `$SHELL` on Unix, `cmd.exe` or `powershell` on Windows. The `--shell` flag overrides this.
- **Line endings**: `.snips` file should use LF line endings (`.gitattributes` can enforce this).
- **File encoding**: UTF-8 only.

#### Large `.snips` Files

- Performance: Parsing and listing 1000+ commands must complete in < 50ms.
- Display: `snip list` should paginate or truncate if output exceeds terminal height.
- Fuzzy matching: Must remain fast with large command sets (O(n*m) is fine for n < 1000).

#### Missing/Empty `.snips` File

- **No `.snips` file**: `snip list` prints: "No .snips file found. Run `snip init` to create one."
- **Empty commands**: `snip list` prints: "No snippets defined. Add commands with `snip add`."
- **Malformed TOML**: Print clear error with line number and character position.

#### Command Name Conflicts

- Duplicate `name` entries: Error on parse. "Duplicate command name 'build' at line 15 (first defined at line 5)."
- Names that shadow `snip` subcommands: Warning only. `snip run init` runs the user's `init` command, not `snip init`. To run the subcommand, use `snip -- init`.

#### Concurrent Access

- If `.snips` is being edited while `snip` reads it: read the file as-is (no locking). This matches `git`'s behavior.
- `snip add` appends atomically where possible (write to temp file, rename).

#### Environment Variable Handling

- Commands inherit the current environment.
- Per-command `env` table adds/overrides variables.
- `SNIP_FILE` env var: Override the default `.snips` file path (for testing or monorepo setups).

#### Non-interactive / Piped Mode

- When stdout is not a TTY: disable colors, disable interactive picker.
- If `snip run` matches multiple commands and stdin is not a TTY: print error "Ambiguous query 'foo'. Matches: bar, baz. Use an interactive terminal or be more specific."

---

## Lessons from tldr's Success

tldr grew to 50K+ stars by doing exactly one thing better than an existing tool. Here's how `snip` should apply those lessons:

| tldr Lesson | snip Application |
|-------------|-------------------|
| Replace `man` pages (too verbose) with examples (just what you need) | Replace "open package.json and read scripts" with `snip list` (just what you need) |
| Simple Markdown format → anyone can contribute | Simple TOML format → anyone can add commands |
| Multiple client implementations → format is the standard | Single implementation but file format is simple enough for others to parse |
| "I just want to know how to tar a directory" | "I just want to know what commands this project has" |
| Community-curated pages | Team-curated `.snips` files (committed to git) |
| Solved a pain point everyone has | Solves a pain point every developer has, every day |

**The key insight**: tldr didn't try to replace `man`. It complemented it by answering the "how do I use this?" question that `man` was bad at. Similarly, `snip` doesn't try to replace `make` or `just`. It complements them by answering the "what commands does this project have?" question that all of them are mediocre at.

---

## References

### Tools
- [casey/just](https://github.com/casey/just) — Command runner (Rust, ~20K stars)
- [go-task/task](https://github.com/go-task/task) — Task runner (Go, ~14K stars)
- [knqyf263/pet](https://github.com/knqyf263/pet) — Snippet manager (Go, ~7.5K stars)
- [direnv/direnv](https://github.com/direnv/direnv) — Env manager (Go, ~14K stars)
- [tldr-pages/tldr](https://github.com/tldr-pages/tldr) — Simplified man pages (~50K stars)
- [npm/npm](https://github.com/npm/npm) — Package manager (Issue #9952: Descriptions for npm Scripts)

### Articles
- [LWN: Just: a command runner](https://lwn.net/Articles/1047715) — Overview of just
- [Powerful `just` features](https://liam.rs/posts/powerful-just-features) — Deep dive into just features
- [3 reasons I'm choosing Taskfile over Make](https://www.reddit.com/r/golang/comments/1stdwxz/) — Taskfile advocacy
- [Make vs Just vs Mise vs go-task for .NET Apps in 2026](https://mehdihadeli.com/blog/task-runners-comparison-2026) — Direct comparison
- [Managing command snippets is hard](https://itnext.io/managing-command-snippets-is-hard-but-there-is-hope-dc6f046759bc) — Pet use case
- [Makefile help target](https://nedbatchelder.com/blog/201804/makefile_help_target) — Make's discovery failure
- [prwhite/help Makefile pattern](https://gist.github.com/prwhite/8168133) — The community workaround for Make

### Key Issues
- [npm/npm#9952](https://github.com/npm/npm/issues/9952) — Descriptions for npm Scripts (open since 2016)
- [casey/just#383](https://github.com/casey/just/issues/383) — Modules and subcommands
- [casey/just#367](https://github.com/casey/just/issues/367) — Executable Justfiles
- [go-task/task#1574](https://github.com/go-task/task/issues/1574) — Completion for global taskfile
- [knqyf263/pet#116](https://github.com/knqyf263/pet/issues/116) — Multi-line snippet support