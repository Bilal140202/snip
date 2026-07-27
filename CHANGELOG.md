# Changelog

All notable changes to this project will be documented in this file.

## [0.4.0] - 2026-07-27

### Added
- **Natural-language execution**: `snip "<phrase>"` runs the best-matching
  snippet by scoring the phrase against each snippet's key, command,
  description, and tags. Examples:
  ```bash
  snip "deploy staging"      → runs deploy.staging
  snip "start frontend"      → runs frontend.dev
  snip "release"             → runs the snippet tagged #release
  ```
  The matching algorithm tokenises both the query and the snippet fields
  (filtering stop-words like "the", "a", "to", "run"), rewards token overlap
  with weights (key=20, desc=15, cmd=10, tags=25), and gives large bonuses
  for exact-phrase matches in the key (+300) or description (+200). When the
  top match isn't a clear winner (margin < 30 points), snip shows the top 5
  candidates and asks the user to be more specific.
- **Enhanced `snip doctor`**: in addition to checking snippet binaries,
  doctor now reports:
  - **Env vars referenced in snippets but not set** in your shell (e.g.
    `$DEPLOY_TOKEN`, `${DATABASE_URL}`). Filters out always-set vars like
    `PATH`, `HOME`, `USER`, etc.
  - **Missing `.env` file** when env vars are referenced but no `.env`
    exists — suggests a template with the missing var names.
  - **Docker daemon not running** when any snippet uses `docker compose` or
    `docker ...`. Distinguishes between "binary not installed", "daemon not
    running", and "permission denied".
- **Picker preview pane**: both the fzf and built-in pickers now show a
  preview of the highlighted snippet's full command, tags, and variables.
  - **fzf mode**: uses `--preview` with a side-channel TSV file so the
    preview updates as you move the cursor. Window is `down:3:wrap`.
  - **Built-in mode** (no fzf): the list pane is followed by a `─────`
    separator and a preview block showing `cmd:`, `tags:`, `vars:` rows.
  - List rows now also show inline `#tag` chips in purple.
- **`PickerItem` extended** with `cmd`, `tags`, `vars` fields plus builder
  methods (`with_cmd`, `with_tags`, `with_vars`). Old call sites continue
  to work — the new fields default to empty.

### Changed
- **README rewrite**: opens with a punchy before/after value prop instead
  of a paragraph of prose:
  ```
  Before:  README → find command → copy → paste → switch back → run
  After:   $ snip dev    →    npm run dev
  ```
  The Commands table now lists `snip "<phrase>"` as a first-class way to
  execute snippets, and the `snip doctor` row mentions the new env/Docker
  checks.

### Tests
- **386 tests** (up from 375). New tests cover the NL matcher (exact key
  match, phrase-in-desc, tag match, no-match, stop-words, sorting) and
  the env-var extractor used by `snip doctor`.

## [0.3.5] - 2026-07-26

### Changed
- **Crate renamed to `snipit`** (from `snipcmd`). The crate name `snipit`
  is shorter and more memorable than `snipcmd`. Both `snip` and `snipcmd`
  alternatives were considered; `snipit` was chosen because `snip` was
  already taken on crates.io.
- Install command is now `cargo install snipit` (was `cargo install snipcmd`).
- The `snipcmd` crate on crates.io (v0.3.4) has been yanked. Users who
  installed via `cargo install snipcmd` should switch:
  ```bash
  cargo uninstall snipcmd
  cargo install snipit
  ```

## [0.3.4] - 2026-07-26

### Fixed
- **MSRV bump**: 1.85 → 1.88. The transitive dependency `home v0.5.12`
  (pulled in via `which v6`) requires rustc 1.88. Updated the CI matrix
  to test on 1.88.0.

## [0.3.3] - 2026-07-26

### Fixed
- **MSRV bump**: 1.78 → 1.85. The transitive dependency `home v0.5.12`
  (pulled in via `which v6`) requires `edition2024`, which needs Rust 1.85+.
  Updated the CI matrix to test on 1.85.0 instead of 1.78.0.
- **Windows binary naming**: release workflow no longer produces
  `snip-x86_64-windows.exe.exe` (double extension). The flatten step now
  checks whether the asset name already ends in `.exe` before appending.

## [0.3.2] - 2026-07-26

### Changed
- **Crate renamed to `snipcmd`** for crates.io publication. The crate name
  `snip` was already taken on crates.io (by Daniel McGraw's "Text snippets
  on the command line" from 2021). The binary is still installed as `snip`
  — only the cargo package name changed.
- Install command is now `cargo install snipcmd` (was `cargo install snip`).
- Added `[[bin]] name = "snip"` so the binary keeps its name.
- Added `homepage`, `documentation`, and `readme` fields to `Cargo.toml`
  for better crates.io metadata.

## [0.3.1] - 2026-07-26

### Fixed
- **MSRV bump**: 1.75 → 1.78. Cargo.lock version 4 (default since Rust 1.78)
  is not understood by older Cargo, breaking the 1.75.0 CI matrix.
- **macOS path canonicalization**: `find_snipfile_in_cwd` test now
  canonicalizes both paths before comparing, because macOS symlinks
  `/var` → `/private/var` (current_dir returns canonical, tempdir doesn't).
- **Release workflow**: the flatten step now renames binaries by their
  target architecture instead of leaving them all named `snip` (which
  caused macOS x86_64 and aarch64 binaries to overwrite each other).
  Now the release correctly contains all 4 binaries:
  `snip-x86_64-linux`, `snip-x86_64-macos`, `snip-aarch64-macos`,
  `snip-x86_64-windows.exe`.

## [0.3.0] - 2026-07-26

### Added
- **6 new auto-detectors**: Go (`go.mod`), Deno (`deno.json`/`deno.jsonc`),
  Taskfile (`Taskfile.yml`), just (`justfile`), Rake (`Rakefile`), and
  Elixir Mix (`mix.exs`). `snip init` now recognises 11 project types
  (up from 5).
- **`snip search <query>`**: full-text search across snippet key, command,
  description, and tags. Supports `--json` for piping to other tools.
- **`snip tag <tag>`**: list snippets by tag, with `--run <name>` to execute
  a filtered snippet, and `--json` output. Suggests existing tags on
  no-match.
- **`snip rename <old> <new>`**: rename a snippet while preserving all
  metadata (vars, tags, shell, dir).
- **`snip mv <name> <section>`**: move a snippet to a different section
  while preserving its leaf name. Use `_` or `-` as section to move to
  top-level.
- **`snip export <name>`**: copy a snippet to the clipboard as TOML or
  just the command (`--format cmd`). Falls back to stdout (`--stdout`)
  when no clipboard tool is available. Supports wl-copy / xclip / xsel /
  pbcopy / clip.
- **`snip run --dry-run` / `--print`**: print the resolved command without
  executing it. Useful for verifying variable substitution.
- **`snip import <url>`**: import snippets from a GitHub gist URL
  (`https://gist.github.com/<user>/<id>`). Auto-picks the first `.toml`
  or `.snips` file in the gist; override with `--file <name>`.
- **Global snippets**: `~/.config/snip/global.toml` (XDG) or `~/.snips`
  (legacy) is automatically merged into every project at the lowest
  priority. Personal cross-project commands without copy-paste.

### Changed
- `read_all_snippets` now merges global snippets first (lowest priority),
  then `.snips.d/*.toml`, then `.snips`, then `.snips.d/_local.toml`.
- Cleaned up 49 dead-code warnings; added `#![allow(dead_code)]` at the
  bin crate root for the lib's public API surface.
- Fixed pre-existing bug where `docker.rs` called `serde_yaml` even when
  the `detect-docker` feature was disabled. Now uses a naive line-based
  parser as a fallback so `--no-default-features --features picker`
  compiles cleanly.
- Moved `rust-version` from `[profile.release]` (where it was a warning)
  to `[package]` where it belongs.
- Clippy now passes with `-D warnings` on `--all-targets --all-features`.

### Tests
- **375 tests** (up from 165) — every new feature has unit tests covering
  happy path, edge cases, and error paths.

## [0.2.0] - 2026-07-19

### Added
- **fzf integration**: Interactive picker shells out to fzf when available
- **Dynamic shell completions**: Bash, Zsh, Fish completions read .snips dynamically via `snip _complete`
- **`snip hook`**: One-line shell setup via `eval "$(snip hook)"`
- **`snip suggest`**: Analyze shell history and suggest snippet candidates
- **`snip explain`**: Break down what a snippet command does
- **`snip stale`**: Detect unused or outdated snippets
- **`snip setup`**: Interactive team onboarding wizard
- **JSON output**: `snip list --json` for piping to other tools
- **Format templates**: `snip list --format "{{key}}: {{cmd}}"`
- **`.snips.d/` directory**: Modular snippet files with priority merge chain
- **Version lock**: `format = "1.0"` header in .snips
- **Auto-init**: Running `snip` with no .snips auto-detects and offers to create
- **Levenshtein suggestions**: "did you mean?" for typos in snippet names
- **`doctor --fix`**: Auto-fix common snippet issues
- **Nushell completions**: Full completion support for Nushell
- **CI/CD**: GitHub Actions with cross-platform testing and release pipeline

### Changed
- Completions system rewritten from static clap_complete to dynamic .snips-aware completions
- Error messages now include actionable suggestions

## [0.1.0] - 2026-07-18

### Added
- Initial MVP with 9 commands: init, add, rm, edit, list, run, import, doctor, completions
- TOML-based .snips file format
- Fuzzy matching for command discovery
- Auto-detection from package.json, Makefile, Cargo.toml, pyproject.toml, docker-compose.yml
- Variable substitution with {{var}} placeholders
- 91 tests passing