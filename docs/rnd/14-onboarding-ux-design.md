# 14 — Onboarding UX Design

> Feature Agent #14: Onboarding UX Expert
> Input: `03-tldr-analysis.md`, `src/main.rs`, all CLI/detect/ui modules
> Status: Design spec — ready for implementation

---

## 0. Current State & Gap Analysis

### What exists today

| Component | File | Status |
|-----------|------|--------|
| `snip init` | `src/cli/init.rs` | **Functional but bare**. Detects project type, creates `.snips`, prints count. No prompting, no confirmation, no delight. |
| `snip` (no args) | `src/cli/list.rs` | **Passive failure**. When no `.snips` found, prints dimmed message + suggests `snip init`. Does NOT auto-detect or offer to init. |
| Fuzzy picker | `src/ui/picker.rs` | **Skeleton**. Raw mode works, but no cursor tracking, no arrow key selection, no post-init launch. |
| Fuzzy match | `src/core/fuzzy.rs` | **Solid**. SkimMatcherV2, sorted results, works well. |
| Detectors | `src/detect/*.rs` | **5 detectors**: Node.js, Cargo, Make, Python, Docker. All functional. |
| Error messages | `src/cli/run.rs:21-24` | **Minimal**. `bail!("No .snips file found. Run snip init first.")` — no path, no suggestion, no fuzzy alternative. |

### The critical gap

Running `snip` for the first time should feel magical. Today it feels like hitting a wall. The user sees a dim message, then has to manually run `snip init`, then manually run `snip` again, then manually run `snip run <name>`. That's 4 round-trips for what should be 1.

### Design principles (stolen from tldr analysis)

1. **Zero config to value in <10 seconds** — tldr shows you the page immediately, updates later
2. **The "show first, act later" pattern** — never block the user on setup
3. **Output that teaches** — every message should reduce future confusion
4. **Progressive disclosure** — day 1 needs 3 commands, not 15

---

## 1. The 10-Second First Run

### Flow: `snip` with no `.snips` file, detected project

This is the most important UX flow in the entire product. It must feel like the tool is *helping*, not *demanding*.

#### Exact terminal output

```
$ cd my-node-project
$ snip
snip: no .snips file found.

  Detected: Node.js (package.json with 8 scripts)

  Create .snips with these commands? [Y/n]: _
```

User presses `Enter` (default Y):

```
  ✓ Created .snips with 8 commands

    dev            Start dev server
    build          Compile TypeScript
    test           Run Jest tests
    lint           Run ESLint
    format         Prettier formatting
    typecheck      Type checking
    clean          Remove dist/
    storybook      Start Storybook

  Run `snip` to list · `snip run <name>` to execute · `snip --help` for more
```

**Total wall-clock time: ~3 seconds.** Detection is instant (file stat + JSON parse). File write is instant (atomic rename). Display is instant (no network).

#### The implementation contract

**What `snip` (no args) does when no `.snips` is found:**

1. Walk up from CWD looking for `.snips` (already implemented in `snipfile::find_snipfile`)
2. If not found, run all detectors on CWD (already implemented in `detector::detect_all`)
3. If detectors found snippets:
   - Print the project type(s) detected and count
   - Prompt `Create .snips with these commands? [Y/n]`
   - On Y: write `.snips`, print the commands in a pretty table, print the "next steps" line
   - On N: print `Run 'snip init' to create one manually.` and exit 0
4. If detectors found nothing: fall through to **Section 2 (Empty Project)**

**Key behavioral rules:**

- **Never auto-create without asking.** Even though default is Y, the user sees what will happen and can say no. This respects the "tool in my project" principle.
- **The prompt must show the count.** "8 commands" sets expectations. "3 commands" is honest. "47 commands" might make them reconsider.
- **Always list the commands after creation.** This is the payoff — the user sees their project's commands organized for the first time. This IS the aha moment.
- **The trailing line is always the same.** `Run 'snip' to list · 'snip run <name>' to execute · 'snip --help' for more` — this is the mental model in one line. Every first-run ends with it.

#### Detection phrasing rules

The detection line must be specific and honest:

| Detected | Output |
|----------|--------|
| Node.js with scripts | `Detected: Node.js (package.json with 8 scripts)` |
| Cargo (no custom scripts) | `Detected: Rust/Cargo (Cargo.toml — 6 standard commands)` |
| Cargo (with metadata.scripts) | `Detected: Rust/Cargo (Cargo.toml with 3 custom scripts)` |
| Makefile with ## comments | `Detected: Make (Makefile with 5 documented targets)` |
| Python with PDM scripts | `Detected: Python (pyproject.toml with 4 PDM scripts)` |
| Docker Compose | `Detected: Docker (docker-compose.yml with 3 services)` |
| Node.js + Docker | `Detected: Node.js (8 scripts) + Docker (3 services)` |
| Nothing | *(falls through to Empty Project — Section 2)* |

For multi-detection, show the two biggest contributors only: `Detected: Node.js (8 scripts) + Make (5 targets)`. If more, append `+ N more`.

#### Color specification

```
"snip:"                    → dimmed white
"no .snips file found."    → dimmed white
"Detected:"                → bold green
"Node.js"                  → bold (terminal default)
"(package.json with 8 scripts)" → dimmed
"[Y/n]"                    → bold yellow
"✓ Created .snips..."      → bold green
command name column         → cyan, bold
description column          → dimmed
"Run 'snip' to list..."    → dimmed
```

All color respects `NO_COLOR` and `!is_terminal()` (piped output = no color).

---

## 2. The Empty Project Experience

### Flow: `snip` with no `.snips` file, no project detected

```
$ cd ~/new-project
$ snip
snip: no .snips file found.
No supported project detected.

  Create an empty .snips? [Y/n]: _
```

User presses `Enter`:

```
  ✓ Created empty .snips

  Add your first command:
    snip add dev "npm run dev" "Start dev server"

  Or edit .snips directly:
    snip edit
```

User types `n`:

```
  Run 'snip init' when you're ready, or create .snips manually.
  Format: https://github.com/Bilal140202/snip#format
```

#### Design decisions

1. **Still offer to create.** An empty `.snips` is not useless — it's an invitation. The user can `snip add` into it immediately. A `.snips` file that exists is a signal that "this project uses snip."

2. **The empty-state message teaches the format by example.** `snip add dev "npm run dev" "Start dev server"` is a complete, copy-pasteable example. The user doesn't need to read docs.

3. **"Or edit .snips directly" — always offer both paths.** Some people are editors, some are CLI people. Both should feel supported.

4. **When they say no, give a URL.** They might come back later. The URL should point to the format spec, not a marketing page.

#### Edge case: git repo with no detectable project

```
$ mkdir my-api && cd my-api && git init
$ snip
snip: no .snips file found.
No supported project detected.

  Create an empty .snips? [Y/n]: y
  ✓ Created empty .snips
  ...
```

Same flow. The fact that it's a git repo doesn't change anything — we don't detect VCS, only project types.

---

## 3. The "Aha Moment"

### The moment that makes people go "oh, this is useful"

The aha moment is: **you just created your `.snips` file and your commands are RIGHT THERE, ready to run.** The flow after first-run creation should immediately demonstrate value, not end with a blank prompt.

#### The ideal flow (interactive terminal)

```
$ snip
snip: no .snips file found.

  Detected: Node.js (package.json with 8 scripts)

  Create .snips with these commands? [Y/n]: y

  ✓ Created .snips with 8 commands

    dev            Start dev server
    build          Compile TypeScript
    test           Run Jest tests
    lint           Run ESLint
    format         Prettier formatting
    typecheck      Type checking
    clean          Remove dist/
    storybook      Start Storybook

  > _
```

After showing the list, the fuzzy picker launches automatically. The `>` prompt appears and the user can type 2 letters:

```
  > de_
    dev            Start dev server          ██████████ best match
    deploy         Deploy to staging
    clean          Remove dist/
```

User presses `Enter`:

```
  → npm run dev

  > Ready on http://localhost:3000
  ...
```

**Total time from `snip` to running a command: under 5 seconds.** This is the magic. The user typed 6 characters total: `s`, `n`, `i`, `p`, `Enter`, `d`, `e`, `Enter` — 8 keystrokes to go from "no idea what commands this project has" to "running the dev server."

#### Implementation: auto-launch picker after first-run creation

The key change is in `src/main.rs` — when `snip` (no args) triggers first-run creation, it should NOT return to the shell. Instead, it should flow into the fuzzy picker.

```
Pseudo-flow for `snip` (no args):

1. find_snipfile(cwd)
   ├─ Found → list snippets (current behavior, but launch picker if TTY)
   └─ Not found
       ├─ Detect project → show prompt
       │   ├─ Y → create .snips → show list → launch picker (if TTY)
       │   └─ N → print hint, exit 0
       └─ No project detected → show empty prompt
           ├─ Y → create empty .snips → print add example, exit 0
           └─ N → print hint, exit 0
```

#### TTY vs non-TTY behavior

```
# TTY (interactive terminal):
$ snip
  → launches picker after list

# Non-TTY (piped, CI, script):
$ snip | head
  dev            Start dev server
  build          Compile TypeScript
  test           Run Jest tests
  ...
```

When not a TTY: never prompt, never launch picker. Just list. This makes `snip | fzf` work naturally as a fallback.

#### The picker after creation: special first-run mode

The picker should have a **first-run mode** that's slightly different from the normal picker:

| Aspect | Normal picker | First-run picker |
|--------|--------------|-----------------|
| Header | `>` | `> Type to filter (Esc to quit)` |
| Item format | `key — description` | `key            description` (aligned columns) |
| Footer | (none) | `Enter to run · Esc to quit` |
| Timeout | none | none, but first-run hint fades after first keypress |

The hint text `Type to filter (Esc to quit)` disappears as soon as the user types their first character. This teaches without cluttering.

---

## 4. Progressive Disclosure

### The learning curve design

The goal: a developer should be productive with 3 commands on day 1. Every additional command should be *discovered*, not *taught*.

#### Level 1 — Day 1: "Get stuff done" (3 commands)

```
snip              → list / fuzzy-pick commands
snip run <name>   → execute a command
snip --help       → (safety net)
```

**How Level 1 is taught:** The first-run output. The trailing line says:
```
Run `snip` to list · `snip run <name>` to execute · `snip --help` for more
```

That's it. No tutorial. No walkthrough. Three commands, one line.

#### Level 2 — Week 1: "Manage your commands" (3 new commands)

```
snip add <name> "<cmd>" "[desc]"   → add a new snippet
snip rm <name>                     → remove a snippet
snip edit                          → open .snips in $EDITOR
```

**How Level 2 is discovered:**

1. The empty-project first-run shows `snip add dev "npm run dev" "Start dev server"` as an example.
2. `snip --help` lists all commands grouped by category.
3. When a user runs `snip add` with a key that already exists, the message says: `Snippet 'dev' already exists. Use 'snip edit' to modify it, or 'snip rm dev' to remove it first.`
4. The `snip doctor` output (Level 3) may suggest adding missing commands, teaching `snip add` by implication.

#### Level 3 — Month 1: "Power user" (3 new commands)

```
snip doctor        → validate .snips, check for issues, suggest improvements
snip import <path> → import snippets from another project
snip completions   → generate shell completions
```

**How Level 3 is discovered:**

1. `snip --help` always shows everything — there's no hidden command.
2. `snip doctor` is suggested when `snip` detects potential issues:
   ```
   $ snip
   ⚠  2 issues found. Run 'snip doctor' for details.
   
     dev            Start dev server
     ...
   ```
3. `snip completions` is mentioned in the install script output (Section 7).
4. `snip import` is shown when cloning a repo that has a `.snips` template in a different location.

#### Level 4 — Month 3+: "Advanced" (3 new commands, future)

```
snip pack          → export snippets as a shareable template
snip ai            → generate snippets from natural language
snip suggest       → suggest commands based on project files
```

**How Level 4 is discovered:**

1. `snip --help` lists them under a `[advanced]` section header.
2. They are not shown in the first-run output or any error message.
3. They appear in the README under "Advanced Usage" — discoverable by search, not pushed.
4. `snip doctor` may hint: `Tip: 'snip suggest' can auto-detect more commands from your project.`

#### The `--help` layout that enables progressive disclosure

```
snip — project-scoped command snippets

Usage: snip [COMMAND]

Commands:
  (no command)    List snippets / launch fuzzy picker

  Core:
    run <name>    Execute a snippet
    add           Add a new snippet
    rm            Remove a snippet
    edit          Open .snips in $EDITOR

  Utilities:
    doctor        Validate and diagnose .snips
    import        Import snippets from another project
    init          Create .snips from project config
    completions   Generate shell completions

  Advanced:
    pack          Export snippets as a shareable template
    ai            Generate snippets from natural language
    suggest       Suggest commands based on project files

  Help:
    --help        Show this help message
    --version     Show version

Run 'snip' in any project directory to get started.
```

The grouping is the teaching mechanism. New users see "Core" and stop reading. Power users discover "Utilities." Advanced users find "Advanced."

---

## 5. Error Messages That Teach

### Design principles

1. **Every error must answer three questions:** What happened? Why? What should I do?
2. **Never blame the user.** "Invalid input" → "Command 'xyz' not recognized."
3. **Offer the fix, not the documentation.** "Did you mean X?" not "See the docs."
4. **Use color to separate signal from noise.** The command/error in bold, the suggestion in dimmed.

### Error catalog

#### E1: No `.snips` file found

```
# Current (BAD):
No .snips file found in this directory or any parent.
To get started, run:
  snip init

# Designed (GOOD):
snip: no .snips file found in /home/user/my-project or any parent directory.

  Run 'snip init' to create one from your project config.
  Or create .snips manually: https://github.com/Bilal140202/snip#format
```

**Why:** Shows the exact search path (user can verify it's looking in the right place). Two options (auto-create or manual). No dead end.

Color: `snip:` = dimmed, path = dimmed, `snip init` = cyan bold, URL = dimmed underline.

#### E2: Snippet not found (exact query)

```
# Current (BAD):
Error: No snippet matching 'deploy' found

# Designed (GOOD):
snip: 'deploy' not found.

  Available commands:
    deploy.staging     Deploy to staging
    deploy.production  Deploy to production
    deploy.canary      Deploy canary build

  Run 'snip' to see all commands, or be more specific.
```

**Implementation:** When no exact match, run fuzzy match. If fuzzy finds results, show up to 5 with their descriptions. The word "Available commands" is better than "Did you mean" — it doesn't assume.

If fuzzy also finds nothing:

```
snip: 'xyz' not found. No similar commands.

  Run 'snip' to see all available commands.
```

#### E3: Ambiguous match

```
# Current (BAD):
Multiple matches found:
  deploy.staging
  deploy.production
Be more specific or use the full name.

# Designed (GOOD):
snip: 'deploy' matches 3 commands:

    deploy.staging     Deploy to staging
    deploy.production  Deploy to production
    deploy.canary      Deploy canary build

  Be more specific, e.g. 'snip run deploy.staging'
```

Added the example command. The user can copy-paste it.

#### E4: `.snips` file exists but is empty

```
# Current (BAD):
No snippets defined.
Add snippets with:
  snip add <name> "<command>" [description]

# Designed (GOOD):
snip: .snips exists but has no commands.

  Add your first command:
    snip add dev "npm run dev" "Start dev server"

  Or edit .snips directly:
    snip edit
```

Same as the empty-project first-run, but acknowledges the file already exists. The example is still copy-pasteable.

#### E5: `.snips` parse error

```
# Current (BAD):
Error: failed to parse .snips file: /home/user/project/.snips

# Designed (GOOD):
snip: error parsing .snips at line 14.

  Expected '=' after key name, found ':'
  → build: "cargo build"
           ^

  Run 'snip doctor' for detailed diagnostics.
  Format reference: https://github.com/Bilal140202/snip#format
```

Shows the exact line, the exact character, the expected vs actual tokens. Links to format docs. Suggests `snip doctor` for more help.

#### E6: `.snips` file already exists (during `snip init`)

```
# Current (BAD):
A .snips file already exists at /home/user/project/.snips
Use `snip add` to add snippets or edit the file directly.

# Designed (GOOD):
snip: .snips already exists (12 commands defined).

  Use 'snip add' to add commands, 'snip edit' to open in $EDITOR,
  or delete .snips and run 'snip init' to re-detect.
```

Shows the count so the user knows the file is populated. Offers three paths forward. The "delete and re-detect" option is important for people who want to regenerate.

#### E7: Command execution failure

```
# Current (BAD):
(whatever the shell error is, unformatted)

# Designed (GOOD):
snip: command exited with code 1 (0.3s)

  → npm run build
  error TS2307: Cannot find module './types'
    src/index.ts:3:10

  Run manually to see full output:
    npm run build
```

Separates snip's output from the command's output. Shows the wall-clock time. Shows a truncated error (first 5 lines). Offers the manual run command for full output.

#### E8: No $EDITOR set (for `snip edit`)

```
# Designed:
snip: no $EDITOR set.

  Set it in your shell:
    export EDITOR="vim"        # add to ~/.bashrc or ~/.zshrc

  Or edit .snips directly:
    code .snips                # VS Code
    vim .snips                 # Vim
```

Doesn't just say "$EDITOR not set" — gives the exact fix and two common alternatives.

### Error message template

Every snip error message follows this structure:

```
snip: <what happened>

  <why it happened / context>
  <suggested action, copy-pasteable>

  <optional: link to docs or related command>
```

The `snip:` prefix is always dimmed. The first line is always a short, complete sentence. The suggested action always uses a code-formatted command that can be copy-pasted.

---

## 6. The README-as-Marketing Document

### Structure and exact content

```markdown
# snip

> Your project's commands, one `snip` away.

Forget `npm run`, `make`, `cargo`, `just`, and `cat README.md`.
`snip` detects your project's commands and gives you a fuzzy finder
to run any of them in seconds.

```
$ snip
  dev            Start dev server          ████████
  build          Compile TypeScript
  test           Run tests
  lint           Lint code
```

## Install

```sh
curl -fsSL https://snip.sh/install.sh | sh
```

Or with Cargo:

```sh
cargo install snip
```

## 60-Second Quickstart

```sh
# In any project directory:
snip                  # auto-detects commands, creates .snips
snip                  # shows fuzzy picker
snip run build        # runs a command directly
```

That's it. Three commands. You're done.

## Why snip?

| | snip | Makefile | just | npm scripts |
|---|---|---|---|---|
| **Zero config** | Auto-detects | Write it yourself | Write it yourself | Already there |
| **Fuzzy finder** | Built-in | No | No | No |
| **Any project type** | Yes | Unix-centric | Unix-centric | Node.js only |
| **Template vars** | `{{env}}` | `$(ENV)` | `just env=staging` | No |
| **Nested commands** | `deploy.staging` | No | No | No |
| **Completions** | bash/zsh/fish | No | bash/zsh/fish | No |
| **Binary size** | ~1 MB | N/A | ~2 MB | N/A |
| **Written in** | Rust | C | Rust | JavaScript |

## How it works

1. `snip` looks for a `.snips` file in your project (or any parent directory)
2. If none exists, it detects your project type (Node.js, Rust, Python, Docker, Make)
3. It offers to create `.snips` with your project's commands
4. You run commands with `snip run <name>` or the built-in fuzzy picker

## Commands

```
snip                  List commands / fuzzy picker
snip run <name>       Execute a command
snip add <name> ...   Add a new command
snip rm <name>        Remove a command
snip edit             Open .snips in $EDITOR
snip init             Create/recreate .snips
snip doctor           Validate and diagnose
snip import <path>    Import from another project
snip completions      Shell completions
```

## License

MIT
```

### Design decisions for the README

1. **3-line description.** "Your project's commands, one snip away." is the hook. The second line names the pain points. The third line is the value proposition. Total reading time: 3 seconds.

2. **The terminal recording goes right below the description.** Before install, before quickstart. The user should see what snip DOES before they install it. This is the most important visual in the README.

3. **60-second quickstart is 3 commands.** Not a tutorial. Not a walkthrough. Three commands that demonstrate the full loop. If the user can't be productive in 60 seconds, the quickstart failed.

4. **Comparison table answers "why not X?"** Every alternative a user might consider is in the table. The table is honest — npm scripts ARE zero-config for Node.js. snip's advantage is the fuzzy finder, multi-language support, and nested commands.

5. **No GIF, use asciinema.** Terminal recordings should be asciinema links (or inline if the platform supports it). GIFs of terminals are blurry and large. asciinema is copy-pasteable text.

6. **The "How it works" section is 4 numbered steps.** Maximum clarity. No paragraphs. The user should understand the mental model in 10 seconds.

---

## 7. Install Script

### Exact design for `curl -fsSL https://snip.sh/install.sh | sh`

```sh
#!/usr/bin/env sh
# snip installer — detects platform, checks for cargo, downloads binary
# Usage: curl -fsSL https://snip.sh/install.sh | sh

set -e

# ── Version ────────────────────────────────────────────────────────
SNIP_VERSION="${SNIP_VERSION:-latest}"
REPO="Bilal140202/snip"

# ── Colors (respects NO_COLOR) ────────────────────────────────────
if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
    GREEN='\033[0;32m'
    CYAN='\033[0;36m'
    DIM='\033[2m'
    BOLD='\033[1m'
    RESET='\033[0m'
else
    GREEN='' CYAN='' DIM='' BOLD='' RESET=''
fi

info()  { printf "${GREEN}✓${RESET} %s\n" "$1"; }
hint()  { printf "  ${DIM}%s${RESET}\n" "$1"; }
ask()   { printf "${BOLD}${CYAN}>${RESET} %s " "$1"; }

# ── Platform detection ────────────────────────────────────────────
detect_platform() {
    OS="$(uname -s)"
    ARCH="$(uname -m)"

    case "$OS" in
        Linux*)  OS="linux"  ;;
        Darwin*) OS="macos"  ;;
        MINGW*|MSYS*|CYGWIN*) OS="windows" ;;
        *)       echo "Unsupported OS: $OS"; exit 1 ;;
    esac

    case "$ARCH" in
        x86_64|amd64) ARCH="x86_64" ;;
        aarch64|arm64) ARCH="aarch64" ;;
        *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
    esac

    # Windows needs .exe suffix
    EXT=""
    if [ "$OS" = "windows" ]; then EXT=".exe"; fi

    echo "${OS}-${ARCH}"
}

# ── Install location ──────────────────────────────────────────────
install_dir() {
    # Prefer ~/.local/bin (standard XDG), fall back to ~/bin
    if [ -d "${HOME}/.local/bin" ] || mkdir -p "${HOME}/.local/bin" 2>/dev/null; then
        echo "${HOME}/.local/bin"
    elif mkdir -p "${HOME}/bin" 2>/dev/null; then
        echo "${HOME}/bin"
    else
        echo "/usr/local/bin"
    fi
}

# ── Check if snip is already installed ────────────────────────────
check_existing() {
    if command -v snip >/dev/null 2>&1; then
        CURRENT="$(snip --version 2>/dev/null || echo 'unknown')"
        printf "snip is already installed (%s). Reinstall? [Y/n]: " "$CURRENT"
        read -r answer
        case "$answer" in
            n|N) echo "Aborted."; exit 0 ;;
        esac
    fi
}

# ── Try cargo install ─────────────────────────────────────────────
try_cargo_install() {
    if command -v cargo >/dev/null 2>&1; then
        printf "Rust/Cargo detected. Install via 'cargo install snip'? [Y/n]: "
        read -r answer
        case "$answer" in
            n|N) return 1 ;;
        esac
        info "Installing via cargo..."
        cargo install snip --locked
        info "Installed via cargo"
        return 0
    fi
    return 1
}

# ── Download from GitHub Releases ─────────────────────────────────
download_binary() {
    PLATFORM="$(detect_platform)"
    DIR="$(install_dir)"
    BINARY="${DIR}/snip"

    # Resolve version
    if [ "$SNIP_VERSION" = "latest" ]; then
        DOWNLOAD_URL="https://github.com/${REPO}/releases/latest/download/snip-${PLATFORM}.tar.gz"
    else
        DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${SNIP_VERSION}/snip-${PLATFORM}.tar.gz"
    fi

    printf "${BOLD}Downloading snip ${SNIP_VERSION} (${PLATFORM})...${RESET}\n"

    # Download + extract atomically
    TMPDIR="$(mktemp -d)"
    trap 'rm -rf "$TMPDIR"' EXIT

    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$DOWNLOAD_URL" | tar xz -C "$TMPDIR"
    elif command -v wget >/dev/null 2>&1; then
        wget -qO- "$DOWNLOAD_URL" | tar xz -C "$TMPDIR"
    else
        echo "Error: need curl or wget to download"
        exit 1
    fi

    # Move binary to install location
    chmod +x "${TMPDIR}/snip"
    mv "${TMPDIR}/snip" "$BINARY"

    info "Installed to ${BINARY}"
}

# ── Shell hook setup ──────────────────────────────────────────────
setup_completions() {
    # Detect shell
    SHELL_NAME="$(basename "${SHELL:-}")"
    RC_FILE=""

    case "$SHELL_NAME" in
        zsh)  RC_FILE="${HOME}/.zshrc" ;;
        bash) RC_FILE="${HOME}/.bashrc" ;;
        fish) RC_FILE="${HOME}/.config/fish/config.fish" ;;
        *)    return 0 ;;  # Unknown shell, skip
    esac

    # Check if already configured
    if [ -f "$RC_FILE" ] && grep -q 'snip completions' "$RC_FILE" 2>/dev/null; then
        return 0
    fi

    printf "Add shell completions for %s? [Y/n]: " "$SHELL_NAME"
    read -r answer
    case "$answer" in
        n|N) return 0 ;;
    esac

    case "$SHELL_NAME" in
        fish)
            echo 'snip completions fish | source' >> "$RC_FILE"
            ;;
        *)
            echo 'eval "$(snip completions bash)"' >> "$RC_FILE"
            ;;
    esac

    info "Added completions to ${RC_FILE}"
    hint "Restart your shell or run: source ${RC_FILE}"
}

# ── Main ──────────────────────────────────────────────────────────
main() {
    printf "\n${BOLD}snip${RESET} — project-scoped command snippets\n\n"

    check_existing

    # Try cargo first, fall back to binary download
    if ! try_cargo_install; then
        download_binary
    fi

    # Ensure install dir is in PATH
    DIR="$(install_dir)"
    case ":${PATH}:" in
        *":${DIR}:"*) ;;  # Already in PATH
        *)
            hint "Add to your PATH:"
            hint "  export PATH=\"${DIR}:\$PATH\""
            # Offer to add to RC file
            SHELL_RC="${HOME}/.$(basename "${SHELL:-bash}")rc"
            if [ -f "$SHELL_RC" ]; then
                printf "Add to %s? [Y/n]: " "$SHELL_RC"
                read -r answer
                case "$answer" in
                    n|N) ;;
                    *) echo "export PATH=\"${DIR}:\$PATH\"" >> "$SHELL_RC"
                       info "Added to ${SHELL_RC}"
                       hint "Restart your shell or run: source ${SHELL_RC}"
                       ;;
                esac
            fi
            ;;
    esac

    setup_completions

    printf "\n"
    info "Done! Run 'snip' in any project to get started.\n"
}

main
```

### Install script terminal output (happy path, macOS, no cargo)

```
$ curl -fsSL https://snip.sh/install.sh | sh

snip — project-scoped command snippets

Downloading snip latest (macos-aarch64)...
✓ Installed to /Users/dev/.local/bin/snip
  Add to your PATH:
    export PATH="/Users/dev/.local/bin:$PATH"
  Add to /Users/dev/.zshrc? [Y/n]: y
✓ Added to /Users/dev/.zshrc
  Restart your shell or run: source /Users/dev/.zshrc
Add shell completions for zsh? [Y/n]: y
✓ Added completions to /Users/dev/.zshrc
  Restart your shell or run: source /Users/dev/.zshrc

✓ Done! Run 'snip' in any project to get started.
```

### Install script terminal output (cargo available)

```
$ curl -fsSL https://snip.sh/install.sh | sh

snip — project-scoped command snippets

Rust/Cargo detected. Install via 'cargo install snip'? [Y/n]: y
✓ Installing via cargo...
  (cargo output...)
✓ Installed via cargo
Add shell completions for zsh? [Y/n]: y
✓ Added completions to /Users/dev/.zshrc
  Restart your shell or run: source /Users/dev/.zshrc

✓ Done! Run 'snip' in any project to get started.
```

### Install script terminal output (already installed)

```
$ curl -fsSL https://snip.sh/install.sh | sh

snip — project-scoped command snippets
snip is already installed (0.1.0). Reinstall? [Y/n]: y
...
```

### Install script design decisions

1. **Every prompt defaults to Y.** The install script should be a one-command experience for people who trust it. Every `read -r` shows `[Y/n]` and accepts Enter as yes.

2. **Never silently modify shell config.** Every `>> "$RC_FILE"` is preceded by a prompt. The user always knows what's being added and where.

3. **Cargo is preferred when available.** `cargo install` handles updates naturally (`cargo install snip` again later). Binary downloads need version management.

4. **Atomic install via temp directory.** Download to a tmpdir, extract, move. If anything fails, the trap cleans up. No half-installed state.

5. **PATH detection is explicit.** After install, we check if the install dir is in PATH. If not, we tell the user AND offer to fix it. We don't just print "make sure it's in your PATH."

6. **The final message is always the same.** `Done! Run 'snip' in any project to get started.` — this is the CTA. It tells the user exactly what to do next.

7. **No `sudo`.** Installs to `~/.local/bin` or `~/bin`, never to `/usr/local/bin` without permission. This avoids the "why is curl asking for my password?" moment.

---

## Appendix A: Complete First-Run Flow — All Paths

### Decision tree

```
snip (no args)
│
├─ .snips found (in cwd or parent)
│   ├─ TTY → show list → launch fuzzy picker
│   └─ pipe → show list (plain text)
│
└─ .snips NOT found
    │
    ├─ Detectors found commands
    │   │
    │   ├─ TTY → "Create .snips with N commands? [Y/n]"
    │   │   ├─ Y → create → show list → launch picker
    │   │   └─ N → "Run 'snip init' to create manually." exit 0
    │   │
    │   └─ pipe → silent, exit 1
    │       (non-interactive cannot prompt)
    │
    └─ Detectors found NOTHING
        │
        ├─ TTY → "Create an empty .snips? [Y/n]"
        │   ├─ Y → create → show add example → exit 0
        │   └─ N → "Run 'snip init' when ready." exit 0
        │
        └─ pipe → silent, exit 1
```

### Exit codes

| Code | Meaning |
|------|---------|
| 0 | Success, or user declined init (non-error) |
| 1 | No `.snips` found and not a TTY (cannot prompt) |
| 2 | `.snips` parse error |
| 3 | Snippet not found (no fuzzy match) |
| 4 | Command execution failed (propagates child exit code) |

---

## Appendix B: Implementation Priority

The changes needed to implement this design, in priority order:

### P0 — Must have for first impression

1. **Merge first-run into `snip` (no args)** — `src/cli/list.rs` and `src/cli/init.rs`
   - When no `.snips` found, run detectors, prompt, create, show list
   - This is the single biggest UX improvement

2. **Auto-launch picker after first-run creation** — `src/main.rs` / `src/cli/list.rs`
   - After creating `.snips`, flow into the fuzzy picker
   - This creates the "aha moment"

3. **Improve "no .snips" error in `snip run`** — `src/cli/run.rs:21-24`
   - Show the full path searched
   - Offer `snip init` as copy-pasteable command
   - This is the most commonly hit error

### P1 — Should have for polish

4. **Fuzzy suggestion in `snip run` errors** — `src/cli/run.rs:42-43`
   - When no match, show "Did you mean..." with fuzzy results
   - Already has fuzzy infrastructure, just needs to be wired to error output

5. **Better `.snips` parse errors** — `src/core/snipfile.rs:47`
   - Line number, character position, expected vs actual
   - Requires TOML error parsing improvement

6. **Color system unification** — currently using `colored`, should migrate to `yansi`
   - Per the tldr analysis: `yansi` is zero-dep, globally disableable
   - This is a refactor, not a feature, but enables NO_COLOR support

### P2 — Nice to have for delight

7. **First-run picker hint text** — `src/ui/picker.rs`
   - "Type to filter (Esc to quit)" that disappears on first keypress
   - "Enter to run · Esc to quit" footer

8. **Detection phrasing** — `src/cli/init.rs`
   - "Detected: Node.js (package.json with 8 scripts)" instead of "Created .snips with 8 commands from Node.js"

9. **Ambiguous match improvement** — `src/cli/run.rs:56-61`
   - Show descriptions, add example command

### P3 — Future

10. **`snip doctor` suggestions** — `src/cli/doctor.rs`
    - Suggest `snip add` for missing commands
    - Hint at `snip suggest` (Level 4)

11. **Install script** — new file `install.sh`
    - Per Section 7 design

12. **`snip ai`, `snip suggest`, `snip pack`** — new commands
    - Level 4 features, not needed for launch