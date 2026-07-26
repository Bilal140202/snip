<p align="center">
  <strong>snip</strong><br>
  <em>every command for this project, zero memorization</em>
</p>

<p align="center">
  <a href="https://github.com/Bilal140202/snip/releases"><img alt="GitHub release" src="https://img.shields.io/github/v/release/Bilal140202/snip?style=flat-square&color=blue"></a>
  <a href="https://github.com/Bilal140202/snip/actions"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/Bilal140202/snip/ci.yml?style=flat-square"></a>
  <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-green?style=flat-square"></a>
</p>

---

## 🎯 Why snip?

Every project has commands you always forget. *"What's the deploy command?"* *"How do I seed the database?"*
You search CONTRIBUTING.md. You scroll through terminal history. You Slack a coworker.

Not anymore. `snip` saves project-scoped command snippets in a committable `.snips` file.
Run `snip` to list them. Run `snip run <name>` to execute them. No memorization. No config.

Three things make `snip` different:

| | snip | `npm run` | `make` | `just` | `pet` |
|---|---|---|---|---|---|
| **Project-scoped & committable** | ✅ `.snips` in repo | ✅ `package.json` | ✅ `Makefile` | ✅ `justfile` | ❌ global only |
| **Fuzzy matching built-in** | ✅ always available | ❌ | ❌ | ⚠️ requires fzf | ⚠️ requires fzf |
| **Human-friendly format** | ✅ TOML | ⚠️ JSON | ⚠️ tab-indented | ⚠️ custom syntax | ✅ TOML |
| **Auto-detects existing commands** | ✅ 11 file types | N/A | N/A | ❌ | ❌ |
| **Zero config** | ✅ `snip init` | ✅ | ✅ | ✅ | ❌ |
| **Variable substitution** | ✅ `{{var}}` prompts | ❌ | ⚠️ make vars | ✅ recipe vars | ❌ |
| **Global snippets** | ✅ `~/.config/snip/global.toml` | ❌ | ❌ | ❌ | ✅ |
| **Team sharing** | ✅ commit `.snips` + gist import | ✅ | ✅ | ✅ | ❌ gist-based |
| **Cold start** | **< 5 ms** | ~150 ms | ~50 ms | ~30 ms | ~200 ms |
| **Binary size** | **~1.6 MB** | N/A | ~400 KB | ~4 MB | ~8 MB |

---

## 📦 Install

### From crates.io (recommended)

```bash
cargo install snipit
```

The package is published as `snipit` because `snip` was already taken
on crates.io. The binary is still installed as `snip`.

### From source

```bash
git clone https://github.com/Bilal140202/snip.git
cd snip
cargo install --path .
```

### Binary download

Pre-built binaries for Linux, macOS, and Windows are available on the
[Releases](https://github.com/Bilal140202/snip/releases) page:

```bash
# Linux (x86_64)
curl -sL https://github.com/Bilal140202/snip/releases/latest/download/snip-x86_64-linux.tar.gz | tar xz

# macOS (Apple Silicon)
curl -sL https://github.com/Bilal140202/snip/releases/latest/download/snip-aarch64-macos.tar.gz | tar xz
```

### Homebrew

```bash
brew install Bilal140202/snip/snip
```

---

## 🚀 Quick Start

```bash
# 1. Navigate to any project
cd your-project

# 2. Auto-detect commands from package.json, Makefile, Cargo.toml, etc.
snip init
# ✅ Created .snips with 8 commands from npm

# 3. List all commands — grouped by section
snip
#   dev            Start dev server on :3000
#   test           Run tests
#   test:watch     Run tests in watch mode
#   build          Build for production
#   lint           Run ESLint

# 4. Run any command by name (or fuzzy match)
snip run dev
# → npm run dev

snip run tst          # fuzzy match — "did you mean test?"
# → npm test

# 5. Commit .snips so every teammate gets your commands
git add .snips && git commit -m "add snip commands"
```

---

## 📋 Commands

| Command | Description |
|---------|-------------|
| `snip` | List all snippets, grouped by section |
| `snip init` | Auto-detect commands and scaffold `.snips` |
| `snip add <name> "<cmd>" [desc]` | Add a new snippet |
| `snip rm <name>` | Remove a snippet |
| `snip rename <old> <new>` | Rename a snippet (preserves all metadata) |
| `snip mv <name> <section>` | Move a snippet to a different section |
| `snip edit` | Open `.snips` in `$EDITOR` |
| `snip list` | List snippets (alias: `snip ls`) |
| `snip search <query>` | Full-text search across snippets |
| `snip tag <tag> [--run <name>]` | List snippets by tag, optionally run one |
| `snip run <name> [--dry-run]` | Execute a snippet (supports fuzzy matching) |
| `snip export <name>` | Copy a snippet to clipboard (TOML or `--format cmd`) |
| `snip import <path-or-url>` | Import from `.snips` file or GitHub gist URL |
| `snip doctor` | Validate snippets — check if binaries exist |
| `snip completions <shell>` | Generate shell completions (bash/zsh/fish/nushell) |
| `snip hook` | One-line shell setup — completions + keybindings |
| `snip suggest` | Analyze shell history and suggest snippet candidates |
| `snip explain <name>` | Break down what a snippet command does |
| `snip stale` | Detect unused or outdated snippets |
| `snip setup` | Interactive team onboarding wizard |

---

## 📄 `.snips` File Format

A TOML file that lives in your project root. **Commit it to git.**

```toml
format = "1.0"

[dev]
cmd = "npm run dev"
desc = "Start dev server on :3000"

[test]
cmd = "npm test -- --watch"
desc = "Run tests in watch mode"
tags = ["ci", "qa"]

[build]
cmd = "npm run build"
desc = "Build for production"
dir = "frontend"                     # run from a subdirectory

[deploy.staging]
cmd = "fly deploy --app myapp-staging"
desc = "Deploy to staging environment"

[deploy.production]
cmd = "fly deploy --app myapp-production"
desc = "Deploy to production"
tags = ["deploy", "release"]

[lint.fix]
cmd = "npx eslint --fix 'src/**/*.{ts,tsx}'"
desc = "Auto-fix lint issues"
shell = "bash"                       # explicit shell override

[db.reset]
cmd = "docker compose down -v && docker compose up -d && npm run db:migrate"
desc = "Nuke and rebuild local database"
tags = ["db"]

[release]
cmd = "gh release create {{version}} --title {{version}} --notes-from-tag"
desc = "Create a GitHub release"
vars = [
  { name = "version", desc = "Release version (e.g. 1.2.0)" }
]
```

### Features at a glance

| Feature | Syntax |
|---------|--------|
| **Sections** | `[deploy.staging]` — dot-notation creates nested groups |
| **Descriptions** | `desc = "..."` — shown in `snip list` and completions |
| **Tags** | `tags = ["deploy", "release"]` — for filtering |
| **Variables** | `vars = [{ name = "env", ... }]` with `{{env}}` placeholders |
| **Shell override** | `shell = "bash"` — run in a specific shell |
| **Working directory** | `dir = "frontend"` — run from a subdirectory |
| **Version lock** | `format = "1.0"` — forward-compatibility header |

---

## 🔗 Shell Integration

Add one line to your `~/.bashrc`, `~/.zshrc`, or `~/.config/fish/config.fish`:

```bash
eval "$(snip hook)"
```

That's it. This enables:

- **Dynamic tab completions** — snippet names update when you edit `.snips`
- **Keybindings** (future) — Ctrl+S to open the snippet picker from anywhere

<details>
<summary>Manual completion setup (alternative)</summary>

```bash
# Bash
eval "$(snip completions bash)"

# Zsh
eval "$(snip completions zsh)"

# Fish
snip completions fish | source

# Nushell
snip completions nushell | save -f ~/.cache/snip/completions.nu
```

</details>

---

## ✨ Variable Substitution

Snippets support `{{variable}}` placeholders. When you run one, `snip` prompts you for values:

```toml
[deploy]
cmd = "kubectl apply -f k8s/{{env}}/ --namespace {{ns}}"
desc = "Deploy to environment"
vars = [
  { name = "env", desc = "Target environment", options = ["staging", "production"] },
  { name = "ns", desc = "Kubernetes namespace", default = "default" }
]
```

```bash
$ snip run deploy

  ? env: Target environment (staging, production): staging
  ? ns: Kubernetes namespace (default): myapp

  → kubectl apply -f k8s/staging/ --namespace myapp
```

Features:
- **Options** — restrict to a list of allowed values
- **Defaults** — skip the prompt by providing a default value
- **Space-tolerant** — `{{ var }}` and `{{var}}` both work

---

## 🔍 Fuzzy Matching

You don't need to remember exact snippet names. `snip` uses fuzzy matching to find what you mean:

```bash
$ snip run tst       # matches "test"
$ snip run dply stg  # matches "deploy.staging"
$ snip run bld       # matches "build"
```

If nothing matches closely, snip suggests the closest alternative:

```
  ✗ No snippet found for "tset"
  → Did you mean "test"?
```

When [fzf](https://github.com/junegunn/fzf) is installed, `snip list` opens an interactive
picker automatically. Select and press Enter to run.

---

## 🧠 Advanced Features

### `.snips.d/` Directory

For teams and larger projects, split snippets into modular files:

```
.snips                 # base snippets
.snips.d/
  common.toml          # shared commands
  frontend.toml        # frontend team commands
  backend.toml         # backend team commands
  local.toml           # personal (git-ignored) snippets
```

Files are merged with a priority chain — later files override earlier ones. Add
`local.toml` to `.gitignore` for personal snippets that don't get committed.

### `snip suggest` — History-Based Suggestions

Analyzes your shell history to find frequently-run commands not yet in `.snips`:

```bash
$ snip suggest
  💡 You run this often but it's not in .snips:
    1. npm run test:watch   (ran 47 times)
    2. docker compose up -d (ran 23 times)

  Add them? (y/n)
```

### `snip explain` — Command Breakdown

Understand what a snippet does before running it:

```bash
$ snip explain db.reset

  docker compose down -v     # Stop all containers and remove volumes
  &&                         # then
  docker compose up -d       # Start containers in detached mode
  &&                         # then
  npm run db:migrate         # Run database migrations
```

### `snip stale` — Detect Unused Snippets

Find snippets that haven't been run in a while:

```bash
$ snip stale
  ⚠ These snippets haven't been run in 30+ days:
    - legacy.build    (last run: 92 days ago)
    - old.lint        (last run: never)
```

### JSON Output Mode

Pipe snippet data to other tools:

```bash
$ snip list --json
[
  {"key":"dev","cmd":"npm run dev","desc":"Start dev server on :3000"},
  {"key":"test","cmd":"npm test","desc":"Run tests"}
]

$ snip list --format "{{key}}: {{cmd}}"
dev: npm run dev
test: npm test
```

### Auto-Detection

`snip init` detects commands from your existing project files (11 file types):

| File | What it detects |
|------|----------------|
| `package.json` | npm scripts |
| `Makefile` | `.PHONY` targets with `##` descriptions |
| `Cargo.toml` | Common cargo commands (build, test, run, clippy) |
| `pyproject.toml` | PDM / project scripts |
| `docker-compose.yml` | Service names |
| `go.mod` | Common go commands (build, test, run, fmt, vet, mod) |
| `deno.json` / `deno.jsonc` | Deno tasks (with comment-stripping for JSONC) |
| `Taskfile.yml` / `Taskfile.yaml` | Top-level tasks |
| `justfile` / `Justfile` | Recipes with `# doc` comments |
| `Rakefile` | Tasks with `desc "..."` or `# doc` comments |
| `mix.exs` | Common mix commands (compile, test, fmt, deps, phx, ecto, release) |

Running `snip` with no `.snips` file auto-detects and offers to create one.

---

## 🛠 Development

```bash
# Build
cargo build --release

# Test (375 tests)
cargo test

# Install locally
cargo install --path .

# Lint (CI gate)
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings

# Run with debug output
RUST_LOG=debug cargo run -- run dev
```

Contributions welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

---

## 📄 License

[MIT](LICENSE) &copy; 2025-present