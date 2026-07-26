# Changelog

All notable changes to this project will be documented in this file.

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