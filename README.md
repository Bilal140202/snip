# snip

> every command for this project, zero memorization.

`snip` saves project-scoped command snippets in a committable `.snips` file.
Run `snip` to list and execute any command. No memorization. No config.

## Why?

Every project has commands you always forget.
"What's the deploy command?" "How do I seed the database?"
You search CONTRIBUTING.md. You scroll through terminal history. You Slack a coworker.

Not anymore. `snip init` detects your commands. `snip` finds them instantly.
And because `.snips` is a committable file, every developer who clones
your repo gets your commands for free.

## Install

```bash
# From source (requires Rust)
cargo install --path .

# Or build from this repo
git clone https://github.com/Bilal140202/snip.git
cd snip
cargo install
```

## 60-Second Quickstart

```bash
$ cd your-project
$ snip init                          # detect commands from package.json, Makefile, etc.
  Created .snips with 8 commands from npm

$ snip                               # list all commands
  dev            Start dev server on :3000
  test           Run tests
  test:watch     Run tests in watch mode
  build          Build for production
  lint           Run ESLint

$ snip run dev                       # execute by name
  → npm run dev

$ snip run tst                       # fuzzy match
  → npm test
```

## Commands

| Command | Description |
|---------|-------------|
| `snip` | List all snippets (grouped by section) |
| `snip init` | Detect and scaffold `.snips` from your project |
| `snip add <name> "<cmd>" [desc]` | Add a snippet |
| `snip run <name\|fuzzy>` | Execute a snippet (supports fuzzy matching) |
| `snip rm <name>` | Remove a snippet |
| `snip edit` | Open `.snips` in `$EDITOR` |
| `snip doctor` | Validate snippets (check if binaries exist) |
| `snip import <path>` | Import snippets from another project |
| `snip completions <shell>` | Generate shell completions (bash/zsh/fish) |

## The `.snips` File

A simple TOML file that lives in your project root. **Commit it to git.**

```toml
# .snips — every contributor gets your commands

[dev]
cmd = "npm run dev"
desc = "Start dev server on :3000"

[test]
cmd = "npm test -- --watch"
desc = "Run tests in watch mode"

[build]
cmd = "npm run build"
desc = "Build for production"

[deploy.staging]
cmd = "fly deploy --app myapp-staging"
desc = "Deploy to staging environment"

[deploy.production]
cmd = "fly deploy --app myapp-production"
desc = "Deploy to production"

[db.reset]
cmd = "docker compose down -v && docker compose up -d && npm run db:migrate"
desc = "Nuke and rebuild local database"

[db.seed]
cmd = "npm run db:seed"
desc = "Seed database with test data"
```

### Variables

Snippets support `{{variable}}` placeholders with optional prompts:

```toml
[deploy]
cmd = "kubectl apply -f k8s/{{env}}/"
desc = "Deploy to environment"
vars = [{ name = "env", desc = "Target environment", options = ["staging", "production"] }]
```

## Auto-Detection

`snip init` detects commands from:

| File | What it detects |
|------|----------------|
| `package.json` | npm scripts |
| `Makefile` | .PHONY targets with `##` descriptions |
| `Cargo.toml` | Common cargo commands |
| `pyproject.toml` | PDM / project scripts |
| `docker-compose.yml` | Service names |

## Shell Completions

```bash
# Bash
eval "$(snip completions bash)"

# Zsh
eval "$(snip completions zsh)"

# Fish
snip completions fish | source
```

## License

MIT