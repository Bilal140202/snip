# Atuin Deep Dive: Recon Analysis for `snip`

> Cloned: `https://github.com/atuinsh/atuin.git` (v18.17.1, ~20 crates, Rust)
> Date: 2025-07-11
> Mission: Extract shell integration, sync, community, and Rust architecture patterns for `snip`

---

## A. Shell Integration Architecture

### Overview

Atuin's shell integration is the **gold standard** for invisible terminal tooling. The user runs `eval "$(atuin init zsh)"` (or bash/fish) and everything just works — no daily commands to remember.

### The Two-Layer Architecture

Atuin's shell integration has two completely separate layers:

#### Layer 1: History Capture (preexec/precmd hooks)

Every shell has a mechanism to run code before and after each command:

| Shell | Mechanism | Atuin functions |
|-------|-----------|-----------------|
| **Zsh** | `add-zsh-hook preexec _atuin_preexec` / `add-zsh-hook precmd _atuin_precmd` | `_atuin_preexec()`, `_atuin_precmd()` |
| **Bash** | `preexec_functions+=()` / `precmd_functions+=()` (via bundled bash-preexec or blesh) | `__atuin_preexec()`, `__atuin_precmd()` |
| **Fish** | `--on-event fish_preexec` / `--on-event fish_postexec` | `_atuin_preexec`, `_atuin_postexec` |

**The preexec hook** runs BEFORE the user's command:
```bash
# zsh version (simplest):
_atuin_preexec() {
    local id
    id=$(ATUIN_SHELL=zsh atuin history start --hook -- "$1" 2>/dev/null)
    export ATUIN_HISTORY_ID="$id"
    __atuin_osc133_command_executed
    __atuin_preexec_time=${EPOCHREALTIME-}
}
```

This calls `atuin history start --hook -- "<command>"` which:
1. Creates a history entry in the local SQLite DB
2. Returns a UUID as the history ID
3. The ID is exported as `ATUIN_HISTORY_ID` for the precmd hook

**The precmd hook** runs AFTER the command finishes:
```bash
_atuin_precmd() {
    local EXIT="$?" __atuin_precmd_time=${EPOCHREALTIME-}
    __atuin_osc133_wrap_prompt
    [[ -z "${ATUIN_HISTORY_ID:-}" ]] && return
    # ... calculate duration ...
    (atuin history end --hook --exit $EXIT ${duration:+--duration=$duration} -- $ATUIN_HISTORY_ID >/dev/null 2>&1 &)
    export ATUIN_HISTORY_ID=""
}
```

Key detail: `history end` runs **asynchronously** in a subshell (`&`) so it never slows down the prompt.

**Critical timing detail for bash**: They record `EPOCHREALTIME` (microsecond-precision epoch time) in both hooks and compute duration as `precmd_time - preexec_time` to get nanosecond-precision command durations.

#### Layer 2: Interactive Search (keybindings)

This is how `Ctrl+R` and `Up Arrow` invoke the atuin TUI.

**Zsh** (cleanest):
```zsh
zle -N atuin-search _atuin_search
zle -N atuin-up-search _atuin_up_search
```

The `_atuin_search` function:
1. Reads `$BUFFER` (current command line content)
2. Passes it as `ATUIN_QUERY=$BUFFER atuin search -i`
3. The `-i` flag means "interactive mode" — atuin takes over the terminal
4. The search TUI outputs the selected command to **stderr** (swapped with stdout: `3>&1 1>&2 2>&3 3>&-`)
5. The shell reads the output and replaces `LBUFFER` with it
6. If the output starts with `__atuin_accept__:`, it also calls `zle accept-line` (enter-to-execute)

**Bash** (insanely complex due to readline limitations):
- Bash 4.3+: Uses a "widget" system with intermediate key sequences (`\C-x\C-_A<n>\a`)
- Two-step macro dispatch: `KEYSEQ -> IKEYSEQ1 IKEYSEQ2` where IKEYSEQ2 binding is dynamically set to either `accept-line` or empty string
- Bash <= 4.2: Even more complex binary encoding scheme using `\C-xQ`, `\C-xR`, `\C-xS` two-byte sequences
- Supports `READLINE_LINE` and `READLINE_POINT` for reading/writing the buffer
- The `__atuin_accept_line` function manually: reprints the prompt, adds to bash history, invokes all preexec functions, runs the command with `eval`, then invokes all PROMPT_COMMAND entries

**Fish** (simplest):
```fish
function _atuin_search
    set ATUIN_H (ATUIN_SHELL=fish ATUIN_QUERY=(commandline -b) atuin search -i 3>&1 1>&2 2>&3 3>&- | string collect)
    if string match --quiet '__atuin_accept__:*' "$ATUIN_H"
        commandline -r (string replace "__atuin_accept__:" "" -- "$ATUIN_H")
        commandline -f execute
    else
        commandline -r "$ATUIN_H"
    end
end
```

#### Layer 3: zsh-autosuggestions Integration (THE KEY PATTERN FOR SNIP)

This is the most important pattern for `snip`:

```zsh
# Atuin registers itself as an autosuggestion strategy for zsh-autosuggestions
_zsh_autosuggest_strategy_atuin() {
    suggestion=$(ATUIN_QUERY="$1" atuin search --cmd-only --author '$all-user' --limit 1 --search-mode prefix 2>/dev/null)
}

if [ -n "${ZSH_AUTOSUGGEST_STRATEGY:-}" ]; then
    ZSH_AUTOSUGGEST_STRATEGY=("atuin" "${ZSH_AUTOSUGGEST_STRATEGY[@]}")
else
    ZSH_AUTOSUGGEST_STRATEGY=("atuin")
fi
```

This means: as the user types, `zsh-autosuggestions` calls `_zsh_autosuggest_strategy_atuin` with the current buffer. It runs `atuin search --cmd-only --limit 1 --search-mode prefix` and shows the result as a grayed-out suggestion.

For bash, there's also a ble.sh autosuggestion source:
```bash
function ble/complete/auto-complete/source:atuin-history {
    local suggestion
    suggestion=$(ATUIN_QUERY="$_ble_edit_str" atuin search --cmd-only --limit 1 --search-mode prefix 2>/dev/null)
    [[ $suggestion == "$_ble_edit_str"?* ]] || return 1
    ble/complete/auto-complete/enter h 0 "${suggestion:${#_ble_edit_str}}" '' "$suggestion"
}
```

#### Layer 4: OSC 133 Support (Semantic Prompts)

Atuin injects OSC 133 escape sequences into the prompt when `ATUIN_PTY_PROXY_ACTIVE` is set:
- `\033]133;A\a` — prompt start
- `\033]133;B\a` — prompt end
- `\033]133;C\a` — command executed
- `\033]133;D;{exit};history_id={id};session_id={sid}\a` — command finished

This enables the PTY proxy to capture semantic terminal events.

#### Layer 5: AI Agent Hooks

Atuin has a `hook` subcommand that integrates with AI coding agents (Claude Code, Codex, pi):
```rust
const CLAUDE_CODE: AgentSpec = AgentSpec {
    aliases: &["claude-code", "claude"],
    actor_name: "claude-code",
    install_kind: InstallKind::JsonHooks {
        config_path: &[".claude", "settings.json"],
        hook_command: "atuin hook claude-code",
        matcher: "Bash",
    },
};
```

It modifies the agent's config JSON to inject pre/post tool hooks that capture commands in atuin history.

### How `snip` Should Use These Patterns

#### 1. Auto-suggesting Snippets (THE PRIORITY FEATURE)

**For zsh**: Register a `zsh-autosuggestions` strategy:
```zsh
_zsh_autosuggest_strategy_snip() {
    suggestion=$(snip suggest --query "$1" --cwd "$PWD" 2>/dev/null)
}
ZSH_AUTOSUGGEST_STRATEGY=("snip" "${ZSH_AUTOSUGGEST_STRATEGY[@]}")
```

The `snip suggest` command would:
- Read `.snip` files from current directory, parent directories, and `~/.snips/`
- Find snippets matching the current typed prefix
- Return the best match

**For bash**: Use ble.sh's autosuggestion source (ble.sh is the only viable option for bash autosuggestions).

**For fish**: Fish has built-in autosuggestions. Register via:
```fish
function _snip_autosuggest
    # fish autosuggestions calls functions named in fish_autosuggest_buffer
end
```

#### 2. `Ctrl+S` to Open Snippet Picker Mid-Command

Mirror atuin's `Ctrl+R` pattern exactly:

**zsh**:
```zsh
zle -N snip-picker _snip_picker
bindkey '^S' snip-picker

_snip_picker() {
    emulate -L zsh
    zle -I
    local output
    output=$(snip search --query "$BUFFER" -i 3>&1 1>&2 2>&3 3>&-)
    zle reset-prompt
    if [[ -n $output ]]; then
        LBUFFER=$output
        # Optionally auto-execute: if [[ $LBUFFER == __snip_accept__:* ]] then ...
    fi
}
```

**bash**: Use the same widget/macro pattern atuin uses for `Ctrl+R`, but bound to `Ctrl+S`.

**fish**: `bind ctrl-s _snip_search`

#### 3. Tab Completion That Reads From .snips

For zsh, generate completion functions from `.snip` files:
```zsh
# snip completions would be generated by `snip init zsh` and output something like:
# _snip_completions() {
#     local -a commands
#     commands=(${(f)"$(snip list --names-only --cwd $PWD 2>/dev/null)"})
#     _describe 'snippets' commands
# }
# compdef _snip_completions <TARGET_COMMAND>
```

Or more practically, hook into `zsh-autosuggestions` as described above — this is what atuin does, and it's much more natural than tab completion.

---

## B. Sync System

### Architecture

Atuin uses a **client-encrypted, append-only record store** with a centralized server.

#### Data Model: The Record Store

Everything is stored as a `Record<EncryptedData>`:
```rust
pub struct Record<Data> {
    pub id: RecordId,           // UUIDv7 (time-ordered)
    pub idx: RecordIdx,          // Monotonic integer, unique per (host, tag)
    pub host: Host,              // Which machine created this
    pub timestamp: u64,          // Nanoseconds since unix epoch
    pub version: String,         // Schema version
    pub tag: String,             // Type: "history", "alias", "var", "kv", "script"
    pub data: Data,              // Either EncryptedData or DecryptedData
}
```

Records are organized as **append-only logs** identified by `(HostId, tag)` pairs. Each record has a monotonically increasing `idx` within its `(host, tag)` stream.

#### Local Storage

- **History DB**: SQLite at `~/.local/share/atuin/history.db` — stores searchable, decrypted history
- **Record Store**: SQLite at `~/.local/share/atuin/record.db` — stores encrypted records for sync
- **Encryption Key**: `~/.local/share/atuin/key` — 256-bit XSalsa20-Poly1305 key, **generated client-side, never sent to server**

#### Encryption Model

```
Client generates key -> encrypts all data with XSalsa20-Poly1305 -> sends ciphertext to server
Server stores opaque blobs -> returns them to other clients -> clients decrypt with shared key
```

Key insight: The server **never sees plaintext**. Encryption uses PASETO V4 with the record metadata as additional authenticated data (AAD).

#### Sync Protocol

1. **Client builds local index**: `RecordStatus = HashMap<HostId, HashMap<String, RecordIdx>>` — for each (host, tag), the highest `idx` we have
2. **Client fetches remote index**: `GET /api/v0/record` returns server's `RecordStatus`
3. **Diff calculation**: Compare local vs remote to get `Vec<Diff>` — each diff has `(host, tag, local_idx, remote_idx)`
4. **Operation resolution**: Each diff becomes Upload, Download, or Noop
5. **Execution**: Upload/download in pages of 100 records

**Conflict resolution**: There is NONE in the traditional sense. Each host owns its own append-only stream. Records from host A with tag "history" can only be appended by host A. This means:
- No merge conflicts possible
- Records are ordered by UUIDv7 (time-ordered)
- If a host is lost, its stream is gone from that host but preserved on the server

#### Server Backend

Two implementations sharing a common `Database` trait:
- **PostgreSQL** (`atuin-server-postgres`): For production/self-hosted
- **SQLite** (`atuin-server-sqlite`): For development/testing

Server is an Axum web app with endpoints:
- `GET /api/v0/record` — get index (tail record IDs)
- `POST /api/v0/record` — upload records
- `GET /api/v0/record/next?host=X&tag=Y&start=Z&count=N` — download records page

#### Auth

Two auth modes:
- **Legacy**: Username/password registration → session token → `Authorization: Token <token>`
- **Hub**: OAuth via `hub.atuin.sh` → bearer token → `Authorization: Bearer <token>`

The Hub also provides a CLI auth flow: request code → poll for completion (similar to GitHub device flow).

### How `snip` Could Use This

#### For Syncing .snips Across Team Repos

**Simple approach (v1)**: Don't sync at all — `.snip` files live in the git repo. This is actually BETTER for most teams because:
- Git is already the sync mechanism
- No server to maintain
- Snippets are code-reviewed with PRs
- Works offline

**Enhanced approach (v2)**: A `snip pack` concept:
```bash
# Subscribe to a shared snippet pack (just a git repo)
snip pack add github:myorg/snippets-rust
snip pack add github:myorg/snippets-terraform

# This adds to ~/.config/snip/packs.toml:
# [packs]
# "github:myorg/snippets-rust" = { branch = "main" }
# "github:myorg/snippets-terraform" = { branch = "main" }
```

`snip sync` would just be `git pull` on these repos. No encryption needed since snippets are meant to be shared.

**Advanced approach (v3)**: If you want a server, steal atuin's record store pattern:
- Each `(host, tag)` stream = each snippet source
- Tags could be: `"snip-local"`, `"snip-pack-myorg"`, etc.
- Client-side encryption optional (snippets aren't usually secret)

---

## C. Community/Social Features

### What Atuin Has

Atuin's "community" features are minimal but well-executed:

1. **Hub (hub.atuin.sh)**: A web dashboard for authenticated users with:
   - Account management
   - OAuth-based login
   - AI features (Atuin AI with LLM integration)
   - Session linking between CLI and Hub accounts

2. **Wrapped (`atuin wrapped`)**: A Spotify Wrapped-style yearly stats summary:
   - Top commands
   - Command vocabulary size
   - Error rate analysis
   - Package management stats
   - Busiest hour
   - Command evolution (first half vs second half of year)
   - Night owl / early bird detection

3. **Stats (`atuin stats`)**: General command statistics with configurable:
   - `common_prefix` (strip "sudo")
   - `common_subcommands` (e.g., "git checkout" → "git")
   - `ignored_commands` (hide "cd", "ls")

4. **Dotfiles sync**: Sync shell aliases and environment variables across machines (also encrypted).

5. **Scripts**: A newer feature for storing and running named scripts.

### What Atuin Does NOT Have

- No "shared history" between users
- No "snippet packs" or community-contributed content
- No social features (no following, no sharing, no comments)
- No marketplace

### How `snip` Could Do Better

**Shared Snippet Packs** is the killer feature atuin doesn't have:

```bash
# Install from a registry or git repo
snip pack install aws-common
snip pack install myorg/devops
snip pack install --git github:someuser/k8s-snippets

# Create your own
snip pack init myteam-snippets
snip pack publish  # pushes to git

# Browse
snip pack search kubernetes
snip pack list
```

Each pack is just a directory of `.snip` files in a git repo. No server needed. No encryption needed. Snippets are meant to be shared.

---

## D. Rust Architecture

### Crate Structure (Workspace)

```
crates/
├── atuin/                    # Main CLI binary
├── atuin-client/             # Client library (settings, DB, API, encryption, import)
├── atuin-common/             # Shared types (Record, API, utils)
├── atuin-server/             # Server binary (Axum)
├── atuin-server-database/    # Database trait (Postgres/SQLite impl)
├── atuin-server-postgres/    # PostgreSQL implementation
├── atuin-server-sqlite/      # SQLite implementation
├── atuin-daemon/             # Background daemon (gRPC/protobuf)
├── atuin-pty-proxy/          # PTY proxy for semantic capture
├── atuin-dotfiles/           # Dotfiles sync (aliases, vars)
├── atuin-kv/                 # Key-value store
├── atuin-scripts/            # Named scripts
├── atuin-history/            # History stats computation
├── atuin-ai/                 # AI features (LLM integration)
└── atuin-nucleo/             # Fuzzy matcher (fork of nucleo)
```

### CLI Structure (clap)

```rust
// main.rs — minimal, just parses and runs
struct Atuin {
    #[command(subcommand)]
    atuin: AtuinCmd,
}

// command/mod.rs — uses #[command(flatten)] for client commands
pub enum AtuinCmd {
    Client(client::Cmd),       // Most subcommands live here
    PtyProxy(...),             // PTY proxy subcommand
    Uuid,                      // Generate UUID
    GenCompletions(...),       # Shell completions
    External(Vec<String>),     # Fallback to external subcommands
}

// command/client/mod.rs — all user-facing commands
pub enum Cmd {
    Search, History, Sync, Login, Register, Logout,
    Stats, Info, Init, Import, List, Doctor,
    Daemon, Store, Config, Kv, Dotfiles, Hook,
    Scripts, Wrapped, Account, DefaultConfig,
}
```

Key patterns:
- `#[command(infer_subcommands = true)]` — allows partial subcommand matching
- `#[command(flatten)]` — client commands are flattened into the top level
- Custom help template with styled headers
- Feature flags gate entire subsystems: `#[cfg(feature = "client")]`, `#[cfg(feature = "sync")]`, `#[cfg(feature = "daemon")]`

### Configuration

Uses the `config` crate with layered sources:
1. **Defaults** in code (via `#[serde(default)]`)
2. **Config file**: `~/.config/atuin/config.toml` (with `include_str!("../config.toml")` as reference)
3. **Environment variables**: `ATUIN_*` prefix (via `config::Environment`)

```rust
impl Settings {
    pub fn new() -> Result<Self> {
        let config_dir = settings_dir();
        let config_path = config_dir.join("config.toml");

        Settings::builder()
            .add_source(ConfigFile::with_name(&config_path.to_string_lossy()).required(false))
            .add_source(Environment::with_prefix("ATUIN").separator("_"))
            .build()?
            .try_deserialize()
    }
}
```

Config is a single flat struct with ~60 fields, using nested structs for organized sections (`[stats]`, `[keys]`, `[preview]`, `[daemon]`, `[search]`, `[tmux]`, `[ui]`, `[meta]`, `[ai]`).

### Key Dependencies

| Crate | Purpose |
|-------|---------|
| `clap` 4.5 | CLI argument parsing (derive) |
| `config` 0.15 | Config file + env loading |
| `serde` + `serde_json` | Serialization |
| `ratatui` 0.30 | Terminal TUI |
| `crossterm` 0.29 | Terminal raw mode / events |
| `tokio` | Async runtime |
| `reqwest` | HTTP client (for sync API) |
| `sqlx` | Database (SQLite + Postgres) |
| `axum` | Server framework |
| `uuid` (v4, v7) | Unique IDs (v7 for time-ordering) |
| `crypto_secretbox` | XSalsa20-Poly1305 encryption |
| `eyre` | Error handling |
| `tracing` | Logging |
| `indicatif` | Progress bars |
| `directories` | XDG paths |
| `typed-builder` | Builder pattern for records |
| `time` | Date/time handling |
| `regex` | History/cwd filtering |

### Notable Rust Patterns

1. **Typed Builder** for records: `Record::builder().host(h).tag(t).data(d).build()`
2. **Feature flags** for conditional compilation: sync, client, daemon, pty-proxy, ai
3. **`async_trait`** for the Database trait
4. **Static singletons** via `OnceLock` / `OnceCell` for global state (data dir, meta store)
5. **`include_str!`** to embed shell scripts at compile time
6. **External subcommand fallback**: `#[command(external_subcommand)]` forwards unknown commands
7. **Error handling**: `eyre::Result` everywhere, with `thiserror` for library error types
8. **UUIDv7** for time-ordered unique IDs

---

## E. What We Should STEAL for `snip`

### 1. Shell Hook Integration Pattern (CRITICAL — for auto-suggesting snippets)

**Steal**: Atuin's zsh-autosuggestions strategy registration.

```zsh
# In `snip init zsh` output:
_zsh_autosuggest_strategy_snip() {
    suggestion=$(snip suggest --query "$1" --cwd "$PWD" 2>/dev/null)
}
ZSH_AUTOSUGGEST_STRATEGY=("snip" "${ZSH_AUTOSUGGEST_STRATEGY[@]}")
```

This is the #1 feature that makes snip "invisible muscle memory." As the user types `dock`, the suggestion `docker compose up -d` appears in gray. They press `→` to accept.

**Implementation for `snip suggest`**:
- Takes `--query` (current buffer) and `--cwd` (current directory)
- Walks up from CWD looking for `.snip` files
- Also checks `~/.config/snip/` and any installed packs
- Filters by prefix match against query
- Returns the best match (most recently used? most specific to cwd?)

### 2. Keybinding Pattern (for `Ctrl+S` snippet picker)

**Steal**: Atuin's exact TUI invocation pattern:
```bash
# In `snip init zsh` output:
zle -N snip-picker _snip_picker
bindkey '^S' snip-picker

_snip_picker() {
    emulate -L zsh
    zle -I
    local output
    output=$(snip search --query "$BUFFER" -i 3>&1 1>&2 2>&3 3>&-)
    zle reset-prompt
    echo -n ${zle_bracketed_paste[1]} >/dev/tty
    if [[ -n $output ]]; then
        LBUFFER=$output
        if [[ $LBUFFER == __snip_accept__:* ]]; then
            LBUFFER=${LBUFFER#__snip_accept__:}
            zle accept-line
        fi
    fi
}
```

The `-i` flag + stderr/stdout swap is the magic. The TUI writes to the real terminal (stderr), and the selected result goes to stdout for the shell to read.

### 3. `snip init <shell>` Pattern

**Steal**: The entire `atuin init zsh|bash|fish` approach:
- Embeds shell scripts via `include_str!`
- Takes `--disable-ctrl-s` and `--disable-up-arrow` flags
- Checks environment variables like `SNIP_NOBIND` for user control
- The init command can also inject dotfiles/aliases if configured

### 4. Sync Architecture (for future team/cloud features)

**Don't steal the full sync system** — it's overkill for snippets. Instead:

**Steal the pattern of**: records as append-only logs with (host, tag) organization.

For `snip`, a simpler approach:
- `.snip` files are the source of truth
- `snip pack` is just git repos of `.snip` files
- `snip sync` = `git pull` on subscribed packs
- No encryption needed (snippets are shared by design)
- No server needed (GitHub/GitLab IS the server)

### 5. Config Management

**Steal**: The layered config approach:
1. Defaults in code
2. `~/.config/snip/config.toml` for user preferences
3. `SNIP_*` env vars for overrides
4. `.snip.toml` in project root for project-level settings

Use the `config` crate with `Environment::with_prefix("SNIP")`.

### 6. TUI Search (ratatui + crossterm)

**Steal**: The pattern of:
- `ratatui` for the TUI rendering
- `crossterm` for raw terminal mode
- The stderr/stdout swap trick for embedding in shell keybindings
- Inline mode (limited height) vs full-screen mode

But build a MUCH simpler TUI. Atuin's interactive search is 3200+ lines. For `snip`, we need:
- A list of snippets
- Fuzzy search filtering
- Single key accept (enter → insert, ctrl+enter → insert+execute)
- Show the snippet name, command, and description
- Maybe 200-300 lines total

### 7. Rust Patterns

**Steal**:
- `clap` with `derive` and `infer_subcommands`
- `eyre` for error handling
- `directories` for XDG paths
- `serde` + `toml` for config
- `include_str!` for embedding shell scripts
- Feature flags for optional features
- `typed-builder` for complex structs
- Workspace crate organization (keep it flat though — don't over-modularize)

---

## F. The Key Lesson for `snip`

### Why Atuin Succeeded

Atuin made shell history **BETTER** without changing how people work:

1. **Zero new commands to remember**: The user doesn't type `atuin` daily. They just press `Ctrl+R` (which they already know) and get a better experience.
2. **Zero config to get started**: `eval "$(atuin init zsh)"` and done.
3. **Invisible operation**: History capture happens in preexec/precmd hooks — the user never thinks about it.
4. **Enhances existing patterns**: It replaces `Ctrl+R` and Up Arrow, not adds new keybindings alongside them.
5. **Gradual depth**: Basic users get better Ctrl+R. Power users get sync, stats, AI, dotfiles.

### How `snip` Must Follow This Pattern

**The cardinal rule**: `snip` must NOT require the user to type `snip` as a separate command during their daily workflow.

| Atuin approach | snip equivalent |
|----------------|-----------------|
| Replace `Ctrl+R` with better search | Show snippet suggestions as you type (zsh-autosuggestions integration) |
| Replace Up Arrow with better history | `Ctrl+S` opens snippet picker (only when you want it) |
| Auto-capture history via preexec hooks | Auto-suggest snippets based on CWD and what you're typing |
| `atuin init zsh` sets everything up | `snip init zsh` sets everything up |
| History sync across machines | Snippet packs via git repos |
| `atuin search -i` for full TUI | `snip search -i` for full snippet browser |

### The Invisible Integration Manifesto for `snip`

1. **`snip init zsh|bash|fish`** — one line in `.zshrc`, everything works
2. **zsh-autosuggestions** — as you type `dock`, `docker compose up -d` appears in gray
3. **`Ctrl+S`** — opens snippet picker with fuzzy search, pre-filtered by CWD
4. **Tab completion** — after accepting a snippet, tab cycles through placeholder arguments
5. **`.snip` files in repo** — no database, no server, just files that travel with the code
6. **`snip pack add <repo>`** — subscribe to shared snippet collections, zero config sync

The user should feel like their shell just "got smarter" — like the terminal knows what they're about to type. That's the atuin magic, and that's what `snip` must achieve.