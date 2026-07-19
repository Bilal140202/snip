# Changelog

All notable changes to this project will be documented in this file.

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