# snip — R&D Master Plan

> Synthesized from 15 specialized R&D agents analyzing 5 open-source codebases and producing 16,300 lines of technical documentation.

## What We Studied (Real Code, Not Guessing)

| Repo | Cloned & Analyzed | Key Takeaway |
|------|-------------------|--------------|
| [casey/just](https://github.com/casey/just) | ✅ 3,400+ line parser, 14 stealable patterns | Shell out to fzf for picker. Levenshtein typo suggestions. Column-aligned listing. |
| [knqyf263/pet](https://github.com/knqyf263/pet) | ✅ 1,800 LOC Go snippet manager | Pipe-to-fzf pattern is proven. Global-only storage is why it failed at 7.5K stars. |
| [tldr-pages/tlrc](https://github.com/tldr-pages/tlrc) | ✅ 2,705 LOC Rust client | Content/client separation = growth strategy. `yansi` > `colored`. Deferred auto-update. |
| [junegunn/fzf](https://github.com/junegunn/fzf) | ✅ Full Smith-Waterman V2 algorithm | Don't reimplement — shell out to fzf. Word boundary bonuses, 64-bit sort keys, SIMD. |
| [ellie/atuinsh/atuin](https://github.com/atuinsh/atuin) | ✅ 20-crate Rust workspace | `eval "$(snip hook)"` pattern. zsh-autosuggest strategy. stderr/stdout fd-swap for TUI. |

---

## Phase 2: BUILDING NOW

### Week 1: fzf Integration + Onboarding UX

**P0 — Shell out to fzf for interactive picker** (from doc 06)
- Status: 🔨 Building
- When `snip` runs interactively and fzf is available, pipe snippets to fzf
- Format: `description\tkey\n` → `fzf --with-nth=1 --nth=1 --delimiter=$'\t'`
- Fall back to current text list if no fzf
- **File:** `src/ui/picker.rs`, `src/cli/run.rs` — add fzf detection and pipe logic
- **Est:** 4 hours

**P0 — Merge init into `snip` (no args)** (from doc 14)
- Status: 🔨 Building
- Running `snip` with no `.snips` file should auto-detect and offer to create
- This is the #1 UX improvement — reduces first-run from 4 steps to 1
- **File:** `src/cli/list.rs` — add detection + prompt before falling through to "run snip init"
- **Est:** 3 hours

**P0 — Fix all error messages** (from doc 14)
- Status: 🔨 Building
- Every error must answer: what happened, why, what to do
- Add Levenshtein typo suggestions for "did you mean?" (steal from just/src/justfile.rs:55-64)
- **File:** `src/cli/run.rs`, `src/cli/rm.rs` — improve error messages
- **Est:** 2 hours

### Week 2: Shell Integration

**P1 — Dynamic completions** (from doc 07)
- Status: 🔨 Building
- Add hidden `snip _complete <subcommand> <partial>` command
- Generate bash/zsh/fish completion scripts that call this
- `snip completions bash` outputs a script that reads .snips dynamically
- **Files:** `src/cli/completions.rs` (rewrite), new shell scripts
- **Est:** 8 hours

**P1 — `eval "$(snip hook)"` command** (from doc 07)
- Status: 🔨 Building
- Single command that sets up completions + keybindings
- Embeds shell scripts via `include_str!` at compile time
- User adds ONE line to .bashrc/.zshrc
- **Files:** new `src/cli/hook.rs`, modify `src/main.rs`
- **Est:** 4 hours

### Week 3: Smart Features

**P2 — JSON output mode** (from doc 08)
- Status: 🔨 Building
- `snip list --json` for piping to other tools
- `snip list --format "{{key}}: {{cmd}}"` for templates
- **Files:** `src/cli/list.rs` — add `--json` and `--format` flags
- **Est:** 3 hours

**P2 — Version lock in .snips** (from doc 08)
- Status: 🔨 Building
- Add `format = "1.0"` header to .snips
- Backward compatible (missing = 1.0)
- **Files:** `src/core/snipfile.rs` — read/write format header
- **Est:** 1 hour

### Week 4: Polish + Release

**P2 — `snip setup` (team onboarding wizard)** (from doc 11)
- Status: 🔨 Building
- Check for required tools (Node, Docker, etc.)
- Validate all snippets work
- Interactive fix prompts for broken ones
- **Files:** new `src/cli/setup.rs`
- **Est:** 8 hours

**P2 — Improved CI + release pipeline** (from doc 15)
- Status: 🔨 Building
- Cross-platform testing (ubuntu, macos, windows)
- Binary size gate (fail if > 4MB)
- Coverage tracking with cargo-llvm-cov
- **Files:** `.github/workflows/ci.yml` (rewrite)
- **Est:** 6 hours

---

## Phase 3: BUILDING NOW

### Directory Merge + Intelligence

**`.snips.d/` directory for team snippets** (from doc 11)
- Status: 🔨 Building
- Modular snippet files: `common.toml`, `frontend.toml`, `backend.toml`, `local.toml`
- 8-layer merge chain with priority
- `snip add --scope frontend` routes to correct file
- **Files:** `src/core/snipfile.rs`
- **Est:** 12 hours

**`snip suggest` (offline, from shell history)** (from doc 09)
- Status: 🔨 Building
- Read `.bash_history` / `.zsh_history` / `fish_history`
- Find frequently-run commands not in .snips
- Suggest adding them (recency-weighted scoring)
- **Files:** new `src/cli/suggest.rs`, new `src/core/history.rs`
- **Est:** 10 hours

**`snip explain <name>`** (from doc 09)
- Status: 🔨 Building
- Local tier: built-in explainers for common tools
- LLM tier: explain complex piped commands
- **Files:** new `src/cli/explain.rs`, new `src/core/explainer.rs`
- **Est:** 10 hours

### (Future — not yet building)

**`snip pack add <github-url>`** (from doc 10)
- GitHub IS the registry — no server needed
- Clone a repo, find .snips, merge with local
- `snip pack search <query>` searches GitHub for repos with `topic:snip-pack`
- **Est:** 15 hours

**`snip ai "<natural language>"`** (from doc 09)
- Three-tier: fuzzy match on descriptions → LLM lookup → fallback
- Ollama (local, free) + OpenAI-compatible API support
- Behind `--features ai` cargo feature flag
- **Est:** 20 hours

**Ctrl+S global keybinding** (from doc 07)
- Opens snip picker from anywhere in the terminal
- Uses atuin's stderr/stdout fd-swap trick
- Inserts selected command at cursor position
- **Est:** 17 hours

**Pre/Post hooks on snippets** (from doc 08)
- Run checks before snippet execution
- Notifications after completion
- Per-snippet and project-wide hooks
- **Est:** 12 hours

---

## Phase 4: BUILDING NOW

### Currently Building

**`snip stale` — detect unused snippets** (from doc 11)
- Status: 🔨 Building
- Analyze snippet usage frequency and last-used timestamps
- Flag snippets that haven't been run in N days
- **Files:** new `src/cli/stale.rs`, new `src/core/stale.rs`
- **Est:** 3 hours

**`snip doctor --fix` auto-fix** (from doc 11)
- Status: 🔨 Building
- Extend existing `doctor` command with automatic repair
- Fix broken snippet syntax, missing tools, stale references
- **Files:** `src/cli/doctor.rs`
- **Est:** 8 hours

**Nushell completion support** (from doc 13)
- Status: 🔨 Building
- Generate Nushell completions via `snip completions nushell`
- Support dynamic snippet name completion
- **Files:** `src/cli/completions.rs`
- **Est:** 4 hours

### (Future — not yet building)

| Feature | Source Doc | Est. Hours |
|---------|-----------|-----------|
| Snippet usage analytics (local SQLite) | doc 11 | 12 |
| Custom detector plugins (`.snips.d/*.toml`) | doc 08 | 15 |
| Built-in fuzzy picker (fallback when no fzf) | doc 06 | 20 |
| Homebrew tap distribution | doc 15 | 4 |
| npm wrapper package | doc 15 | 3 |
| Environment overrides (`SNIP_ENV`) | doc 11 | 8 |
| Registry API (static GitHub Pages) | doc 10 | 20 |

---

## Key Technical Decisions (From R&D)

| Decision | Answer | Source |
|----------|--------|--------|
| Fuzzy picker: built-in or fzf? | **Shell out to fzf**, built-in fallback | doc 06 |
| LLM dependency? | **Feature-flagged, zero required deps** | doc 09 |
| Registry: custom server or GitHub? | **GitHub IS the registry** (zero infra) | doc 10 |
| Shell integration pattern? | **`eval "$(snip hook)"`** (one line) | doc 07 |
| Snippet storage: single file or directory? | **Both**: `.snips` + `.snips.d/` merge chain | doc 11 |
| Terminal colors: `colored` or `yansi`? | **Switch to `yansi`** (lighter, NO_COLOR support) | doc 03 |
| Binary size target? | **< 3MB** (currently 1.4MB with LTO) | doc 12 |
| Cold start target? | **< 5ms** (currently ~2ms) | doc 12 |

---

## Bugs Found & Fixed (From R&D)

| Bug | File | Fix | Status |
|-----|------|-----|--------|
| `fuzzy_best()` returns WORST match | `src/core/fuzzy.rs:51` | Changed `.pop()` to `.into_iter().next()` | ✅ Fixed |
| Dead deps (`dirs`, `crossterm`) bloating binary | `Cargo.toml` | Feature-gated behind `picker` and removed `dirs` | ✅ Fixed |
| No release optimizations | `Cargo.toml` | Added LTO, strip, panic=abort, codegen-units=1 | ✅ Fixed |
| Binary was 2.5MB | — | Now 1.4MB (44% reduction) | ✅ Fixed |
| `serde_yaml` always compiled | `Cargo.toml` | Feature-gated behind `detect-docker` | ✅ Fixed |

---

## Total Engineering Estimate

| Phase | Duration | Hours |
|-------|----------|-------|
| Phase 2 (Building Now) | 4 weeks | ~39 |
| Phase 3 (Building Now) | parallel | ~32 |
| Phase 4 (Building Now) | parallel | ~15 |
| Phase 3-4 (Future) | months 2-12 | ~124 |
| **Total** | **12 months** | **~210 hours** |