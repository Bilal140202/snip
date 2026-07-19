# snip Architecture v2 — Phase 2-4

> Date: 2025-07-11
> Supersedes: Initial architecture from Phase 1
> Covers: All 14 commands, .snips.d/ merge chain, dynamic completions, hook system, fzf integration

---

## 1. Command Tree (14 Commands)

```
snip
├── init                    # Create / detect .snips file
├── add <name> <cmd> [desc] # Add a snippet (--scope, --team in v2)
├── rm <name>               # Remove a snippet
├── edit                    # Open .snips in $EDITOR
├── list [filter]           # List snippets (--json, --format in v2)
├── run [name|query]        # Execute a snippet (fzf picker in v2)
├── import <source>         # Import snippets from another project
├── doctor [--fix]          # Validate + auto-fix snippet issues
├── completions <shell>     # Generate shell completions (dynamic in v2)
├── hook <shell>            # NEW: eval "$(snip hook)" — unified shell setup
├── setup                   # NEW: Team onboarding wizard
├── suggest                 # NEW: Suggest snippets from shell history
├── explain <name>          # NEW: Explain what a snippet does
├── stale [--days N]        # NEW: Detect unused snippets
└── _complete <sub> <partial>  # HIDDEN: Dynamic completion backend
```

### Source Map

| Command | Module | Status |
|---------|--------|--------|
| `init` | `src/cli/init.rs` | ✅ existing |
| `add` | `src/cli/add.rs` | ✅ existing, needs `--scope` flag |
| `rm` | `src/cli/rm.rs` | ✅ existing |
| `edit` | `src/cli/edit.rs` | ✅ existing |
| `list` | `src/cli/list.rs` | ✅ existing, needs `--json`/`--format` |
| `run` | `src/cli/run.rs` | ✅ existing, needs fzf integration |
| `import` | `src/cli/import.rs` | ✅ existing |
| `doctor` | `src/cli/doctor.rs` | ✅ existing, needs `--fix` |
| `completions` | `src/cli/completions.rs` | ✅ existing, needs full rewrite |
| `hook` | `src/cli/hook.rs` | 🆕 new |
| `setup` | `src/cli/setup.rs` | 🆕 new |
| `suggest` | `src/cli/suggest.rs` | 🆕 new |
| `explain` | `src/cli/explain.rs` | 🆕 new |
| `stale` | `src/cli/stale.rs` | 🆕 new |
| `_complete` | `src/cli/completions.rs` | 🆕 hidden subcommand |

---

## 2. `.snips.d/` Merge Chain (8-Layer Priority)

When `.snips.d/` exists alongside `.snips`, snip loads snippets from multiple sources and merges them. Higher layers override lower layers on key collision.

```
Priority (lowest → highest):

Layer 1:  .snips                          (legacy single file — always loaded)
Layer 2:  .snips.d/common.toml            (shared team snippets)
Layer 3:  .snips.d/<team>.toml (α-sort)   (team-specific: frontend, backend, ...)
Layer 4:  .snips.d/production.toml        (env override, if SNIP_ENV=production)
Layer 5:  .snips.d/staging.toml           (env override, if SNIP_ENV=staging)
Layer 6:  .snips.d/development.toml       (env override, if SNIP_ENV=development)
Layer 7:  .snips.d/<user>.local.toml      (user personal overrides)
Layer 8:  .snips.d/local.toml             (machine-local, gitignored — always wins)
```

### Merge Diagram

```
.snips ──────────────────────────────────────────────────┐
                                                         │
.snips.d/common.toml ────────────────────────────────────┤
                                                         │
.snips.d/backend.toml ───────────────────────────────────┤
.snips.d/frontend.toml ──────────────────────────────────┤
                                                         │  LayeredSnipFile::load()
.snips.d/production.toml ──┐                            │  (reverse-order dedup)
         (if SNIP_ENV)     │                            │
.snips.d/staging.toml ─────┤                            │
         (if SNIP_ENV)     │                            │
.snips.d/development.toml──┘                            │
                                                         │
.snips.d/alice.local.toml ──────────────────────────────┤
                                                         │
.snips.d/local.toml ─────────────────────────────────────┘
                        │
                        ▼
              Merged Snippet Map
              (highest-priority version
               of each key wins)
```

### Core Type

```rust
// src/core/layered.rs (NEW)

pub struct LayeredSnipFile {
    /// All resolved entries in priority order (highest priority first).
    entries: Vec<SourceEntry>,
}

pub struct SourceEntry {
    pub key: String,
    pub snippet: Snippet,
    pub source_file: String,  // e.g. ".snips.d/frontend.toml"
}

impl LayeredSnipFile {
    pub fn load(project_root: &Path) -> Result<Self>;
    pub fn get(&self, key: &str) -> Option<&Snippet>;
    pub fn all(&self) -> &[SourceEntry];
    pub fn is_empty(&self) -> bool;
}
```

---

## 3. Dynamic Completion Flow

### Architecture

```
User types:  snip run buil<TAB>
                         │
                         ▼
              Shell calls completion function
              (registered by eval "$(snip hook)")
                         │
                         ▼
              Completion function invokes:
                snip _complete run "buil" "$PWD"
                         │
                         ▼
              ┌─────────────────────────────┐
              │  Rust binary (cold start)    │
              │                              │
              │  1. Find .snips in $PWD      │
              │  2. Parse all snippets       │
              │  3. Filter keys by "buil"    │
              │  4. Print matching keys       │
              │     (one per line, stdout)   │
              │  5. Exit                      │
              └─────────────────────────────┘
                         │
                         ▼
              Shell consumes names, shows menu
              User selects: build
```

### Shell Script (embedded via `include_str!`)

**Bash** (`eval "$(snip hook bash)"` emits):

```bash
_snip_completions() {
    local cur="${COMP_WORDS[COMP_CWORD]}"
    local prev="${COMP_WORDS[COMP_CWORD-1]}"

    # Position 2 = subcommand argument (e.g. snippet name after "run")
    if (( COMP_CWORD == 2 )); then
        case "${COMP_WORDS[1]}" in
            run|rm|edit|explain)
                local names
                names=$(snip _complete "${COMP_WORDS[1]}" "$cur" 2>/dev/null)
                if [[ -n "$names" ]]; then
                    COMPREPLY=($(compgen -W "$names" -- "$cur"))
                    return
                fi
                ;;
        esac
    fi

    # Position 1 = subcommand completion
    if (( COMP_CWORD == 1 )); then
        COMPREPLY=($(compgen -W "init add rm edit list run import doctor completions hook setup suggest explain stale" -- "$cur"))
        return
    fi
}
complete -F _snip_completions snip
```

**Zsh** and **Fish** follow equivalent patterns with their native completion APIs.

### Performance Target

- `snip _complete` must exit in **< 50ms** for projects with 1-100 snippets
- Reads `.snips` directly (no git, no network, no external deps)
- Single-purpose: parse → filter → print → exit

---

## 4. Hook System Architecture

### Concept

One line in the user's shell config activates everything:

```bash
# ~/.bashrc or ~/.zshrc
eval "$(snip hook)"        # auto-detects shell from $SHELL
# or explicitly:
eval "$(snip hook bash)"
eval "$(snip hook zsh)"
eval "$(snip hook fish)"
```

### Flow Diagram

```
User's .bashrc / .zshrc
        │
        ▼
   eval "$(snip hook bash)"
        │
        ▼
┌─────────────────────────────────┐
│  snip binary                    │
│                                 │
│  Detects shell = "bash"         │
│  Loads embedded script from     │
│  include_str!("hook.bash")      │
│  Prints to stdout               │
└─────────────────────────────────┘
        │
        ▼
   Shell eval's the output:
   ┌─────────────────────────────────┐
   │  1. Register _snip_completions  │
   │     function                    │
   │  2. complete -F _snip snip      │
   │  3. (Future) bind Ctrl+S to     │
   │     snip picker invocation      │
   └─────────────────────────────────┘
        │
        ▼
   User now has:
   • TAB completion for all snip subcommands
   • TAB completion for snippet names (dynamic)
   • (Future) Ctrl+S global keybinding
```

### Rust Implementation

```rust
// src/cli/hook.rs (NEW)

use std::env;

const HOOK_BASH: &str = include_str!("../assets/hook.bash");
const HOOK_ZSH:  &str = include_str!("../assets/hook.zsh");
const HOOK_FISH: &str = include_str!("../assets/hook.fish");

pub fn run(shell: Option<&str>) -> anyhow::Result<()> {
    let shell = match shell {
        Some(s) => s,
        None => {
            // Auto-detect from $SHELL
            let shell_path = env::var("SHELL")?;
            Path::new(&shell_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("bash")
        }
    };

    let script = match shell {
        "bash" => HOOK_BASH,
        "zsh"  => HOOK_ZSH,
        "fish" => HOOK_FISH,
        _ => anyhow::bail!("unsupported shell: {shell}"),
    };

    print!("{script}");
    Ok(())
}
```

---

## 5. fzf Integration Flow

### Decision: Hybrid — fzf First, Built-In Fallback

```
snip run [query]
       │
       ▼
  ┌────────────────────┐
  │ Is terminal        │
  │ interactive?       │
  └────────┬───────────┘
           │
     ┌─────┴─────┐
     │ No        │ Yes
     ▼           ▼
  Text list   ┌──────────────────┐
  (current    │ Is fzf on $PATH? │
   behavior)  └────────┬─────────┘
                      │
                ┌─────┴─────┐
                │ No        │ Yes
                ▼           ▼
          Built-in     ┌─────────────────────┐
          picker       │ Pipe snippets to    │
          (Phase 4)    │ fzf via stdin       │
                       │                     │
                       │ Format per line:    │
                       │   "desc\tkey\n"     │
                       │                     │
                       │ fzf args:           │
                       │   --with-nth=1      │
                       │   --nth=1           │
                       │   --delimiter=$'\t' │
                       │   --query=<query>   │
                       └──────────┬──────────┘
                                  │
                                  ▼
                         ┌─────────────────┐
                         │ User selects    │
                         │ in fzf          │
                         └────────┬────────┘
                                  │
                         ┌────────┴────────┐
                         │                 │
                      Selected         Aborted
                         │            (Ctrl+C)
                         ▼                 ▼
                   Execute          Silent
                   snippet          exit
```

### Rust Implementation (Primary Path)

```rust
// src/ui/picker.rs

use std::process::{Command, Stdio};

pub struct FzfResult {
    pub key: String,
    pub action: FzfAction,
}

pub enum FzfAction {
    Run,
    Print,       // Ctrl+P — print command to clipboard
    Edit,        // Ctrl+E — open in $EDITOR
}

pub fn fzf_pick(
    items: &[String],           // ["description\tkey", ...]
    query: &str,                // partial from "snip run <query>"
    fzf_path: &Path,            // path to fzf binary
) -> anyhow::Result<Option<FzfResult>> {
    let mut child = Command::new(fzf_path)
        .arg("--with-nth=1")
        .arg("--nth=1")
        .arg(format!("--delimiter=\t"))
        .arg(format!("--query={query}"))
        .arg("--expect=ctrl-e,ctrl-p")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())  // fzf renders to TTY
        .spawn()?;

    // Write items to stdin
    {
        let stdin = child.stdin.as_mut().expect("piped");
        for item in items {
            writeln!(stdin, "{item}")?;
        }
    }

    // Read selection from stdout
    let output = child.wait_with_output()?;
    let stdout = String::from_utf8(output.stdout)?;

    // Parse: first line = action key, second line = selected item
    let mut lines = stdout.lines();
    let action_line = lines.next().unwrap_or("");
    let item_line = lines.next().unwrap_or("");

    if item_line.is_empty() {
        return Ok(None); // User aborted
    }

    // Extract key from "description\tkey" format
    let key = item_line.rsplit('\t').next().unwrap_or(item_line).to_string();

    let action = match action_line {
        "ctrl-e" => FzfAction::Edit,
        "ctrl-p" => FzfAction::Print,
        _ => FzfAction::Run,
    };

    Ok(Some(FzfResult { key, action }))
}
```

### Integration in `snip run`

```rust
// src/cli/run.rs (modified)

pub fn run(name: &str) -> anyhow::Result<()> {
    let snippets = LayeredSnipFile::load(&current_dir()?)?;

    // Exact match? Execute immediately.
    if let Some(snippet) = snippets.get(name) {
        return executor::run(snippet);
    }

    // No exact match → try fuzzy picker
    let fzf_path = which("fzf");

    if is_terminal() {
        if let Some(path) = fzf_path {
            // fzf path: pipe all snippets, pre-fill query
            let items: Vec<String> = snippets.all()
                .iter()
                .map(|e| format!("{}\t{}", e.snippet.description, e.key))
                .collect();

            match picker::fzf_pick(&items, name, &path)? {
                Some(result) => {
                    return match result.action {
                        FzfAction::Run => executor::run(snippets.get(&result.key).unwrap()),
                        FzfAction::Edit => editor::open_snippet(&result.key),
                        FzfAction::Print => clipboard::copy_snippet(&result.key),
                    };
                }
                None => return Ok(()), // User cancelled
            }
        } else {
            // No fzf — use built-in fuzzy match
            if let Some(entry) = fuzzy_best(snippets.all(), name) {
                return executor::run(&entry.snippet);
            }
        }
    }

    // Nothing matched
    bail!(
        "No snippet found matching {:?}{}",
        name,
        levenshtein_suggestion(name, &snippet_keys)
    );
}
```

---

## 6. Module Layout (v2)

```
src/
├── main.rs                      # CLI entry, command dispatch
├── lib.rs                       # Library root
├── cli/
│   ├── mod.rs                   # Re-exports all subcommands
│   ├── add.rs                   # snip add (+ --scope)
│   ├── completions.rs           # snip completions + snip _complete (rewrite)
│   ├── doctor.rs                # snip doctor --fix
│   ├── edit.rs                  # snip edit
│   ├── explain.rs               # 🆕 snip explain
│   ├── hook.rs                  # 🆕 snip hook
│   ├── import.rs                # snip import
│   ├── init.rs                  # snip init
│   ├── list.rs                  # snip list --json --format
│   ├── rm.rs                    # snip rm
│   ├── run.rs                   # snip run (fzf integration)
│   ├── setup.rs                 # 🆕 snip setup
│   ├── stale.rs                 # 🆕 snip stale
│   └── suggest.rs               # 🆕 snip suggest
├── core/
│   ├── mod.rs                   # Re-exports
│   ├── snippet.rs               # Snippet struct
│   ├── snipfile.rs              # Single-file .snips parse/write
│   ├── layered.rs               # 🆕 LayeredSnipFile, .snips.d/ merge
│   ├── detector.rs              # Project type detection
│   ├── executor.rs              # Shell command execution
│   ├── fuzzy.rs                 # Built-in fuzzy matching
│   ├── validator.rs             # Snippet validation
│   ├── explainer.rs             # 🆕 Command explanation engine
│   ├── history.rs               # 🆕 Shell history parser
│   └── stale.rs                 # 🆕 Staleness detection
├── ui/
│   ├── mod.rs
│   ├── picker.rs                # fzf shell-out + built-in picker
│   ├── prompt.rs                # Interactive prompts
│   └── render.rs                # Terminal output formatting
├── detect/
│   ├── mod.rs
│   ├── cargo.rs
│   ├── node.rs
│   ├── python.rs
│   ├── makefile.rs
│   └── docker.rs
├── utils/
│   ├── mod.rs
│   ├── shell.rs                 # Shell detection, quoting
│   ├── git.rs                   # Git operations
│   └── fs.rs                    # File system helpers
└── assets/                      # 🆕 Embedded shell scripts
    ├── hook.bash                # Bash completion + keybindings
    ├── hook.zsh                 # Zsh completion + keybindings
    └── hook.fish                # Fish completion + keybindings
```