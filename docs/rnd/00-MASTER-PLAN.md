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

## Phase 2: Immediate Build (Weeks 1-4)

### Week 1: fzf Integration + Onboarding UX

**P0 — Shell out to fzf for interactive picker** (from doc 06)
- When `snip` runs interactively and fzf is available, pipe snippets to fzf
- Format: `description\tkey\n` → `fzf --with-nth=1 --nth=1 --delimiter=$'\t'`
- Fall back to current text list if no fzf
- **File:** `src/cli/list.rs` — add fzf detection and pipe logic
- **Est:** 4 hours

**P0 — Merge init into `snip` (no args)** (from doc 14)
- Running `snip` with no `.snips` file should auto-detect and offer to create
- This is the #1 UX improvement — reduces first-run from 4 steps to 1
- **File:** `src/cli/list.rs` — add detection + prompt before falling through to "run snip init"
- **Est:** 3 hours

**P0 — Fix all error messages** (from doc 14)
- Every error must answer: what happened, why, what to do
- Add Levenshtein typo suggestions for "did you mean?" (steal from just/src/justfile.rs:55-64)
- **File:** `src/cli/run.rs`, `src/cli/rm.rs` — improve error messages
- **Est:** 2 hours

### Week 2: Shell Integration

**P1 — Dynamic completions** (from doc 07)
- Add hidden `snip _complete <subcommand> <partial>` command
- Generate bash/zsh/fish completion scripts that call this
- `snip completions bash` outputs a script that reads .snips dynamically
- **Files:** `src/cli/completions.rs` (rewrite), new shell scripts
- **Est:** 8 hours

**P1 — `eval "$(snip hook)"` command** (from doc 07)
- Single command that sets up completions + keybindings
- Embeds shell scripts via `include_str!` at compile time
- User adds ONE line to .bashrc/.zshrc
- **Files:** new `src/cli/hook.rs`, modify `src/main.rs`
- **Est:** 4 hours

### Week 3: Smart Features

**P2 — `snip suggest` (offline, from shell history)** (from doc 09)
- Read `.bash_history` / `.zsh_history` / `fish_history`
- Find frequently-run commands not in .snips
- Suggest adding them (recency-weighted scoring)
- **Files:** new `src/cli/suggest.rs`, new `src/core/history.rs`
- **Est:** 10 hours

**P2 — JSON output mode** (from doc 08)
- `snip list --json` for piping to other tools
- `snip list --format "{{key}}: {{cmd}}"` for templates
- **Files:** `src/cli/list.rs` — add `--json` and `--format` flags
- **Est:** 3 hours

**P2 — Version lock in .snips** (from doc 08)
- Add `format = "1.0"` header to .snips
- Backward compatible (missing = 1.0)
- **Files:** `src/core/snipfile.rs` — read/write format header
- **Est:** 1 hour

### Week 4: Polish + Release

**P2 — `snip setup` (team onboarding wizard)** (from doc 11)
- Check for required tools (Node, Docker, etc.)
- Validate all snippets work
- Interactive fix prompts for broken ones
- **Files:** new `src/cli/setup.rs`
- **Est:** 8 hours

**P2 — Improved CI + release pipeline** (from doc 15)
- Cross-platform testing (ubuntu, macos, windows)
- Binary size gate (fail if > 4MB)
- Coverage tracking with cargo-llvm-cov
- **Files:** `.github/workflows/ci.yml` (rewrite)
- **Est:** 6 hours

---

## Phase 3: Growth Features (Months 2-4)

### Month 2: Community

**`snip pack add <github-url>`** (from doc 10)
- GitHub IS the registry — no server needed
- Clone a repo, find .snips, merge with local
- `snip pack search <query>` searches GitHub for repos with `topic:snip-pack`
- **Est:** 15 hours

**`.snips.d/` directory for team snippets** (from doc 11)
- Modular snippet files: `common.toml`, `frontend.toml`, `backend.toml`, `local.toml`
- 8-layer merge chain with priority
- `snip add --scope frontend` routes to correct file
- **Est:** 12 hours

### Month 3: Intelligence

**`snip ai "<natural language>"`** (from doc 09)
- Three-tier: fuzzy match on descriptions → LLM lookup → fallback
- Ollama (local, free) + OpenAI-compatible API support
- Behind `--features ai` cargo feature flag
- **Est:** 20 hours

**`snip explain <name>`** (from doc 09)
- Local tier: built-in explainers for common tools
- LLM tier: explain complex piped commands
- **Est:** 10 hours

### Month 4: Platform

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

## Phase 4: Ecosystem (Months 5-12)

| Feature | Source Doc | Est. Hours |
|---------|-----------|-----------|
| Snippet usage analytics (local SQLite) | doc 11 | 12 |
| `snip doctor --fix` auto-fix | doc 11 | 8 |
| Custom detector plugins (`.snips.d/*.toml`) | doc 08 | 15 |
| Built-in fuzzy picker (fallback when no fzf) | doc 06 | 20 |
| Nushell completion support | doc 13 | 4 |
| `snip stale` — detect unused snippets | doc 11 | 3 |
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
| Phase 2 (Immediate) | 4 weeks | ~50 |
| Phase 3 (Growth) | 3 months | ~86 |
| Phase 4 (Ecosystem) | 7 months | ~97 |
| **Total** | **12 months** | **~233 hours** |