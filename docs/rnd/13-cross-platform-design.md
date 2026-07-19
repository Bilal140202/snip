# 13 — Cross-Platform Design

> **Status:** Design document — no code changes yet.
> **Scope:** Full Windows, macOS, and Linux support for `snip`.
> **Current state:** The codebase has basic `cfg!(target_os = "windows")` guards in
> `executor.rs` (falling back to `cmd /C`) and `shell.rs` (falling back to `"vi"`),
> but everything else assumes a Unix-like environment.

---

## Table of Contents

1. [Current Cross-Platform Gaps](#1-current-cross-platform-gaps)
2. [Windows Support](#2-windows-support)
3. [macOS Specifics](#3-macos-specifics)
4. [Linux Specifics](#4-linux-specifics)
5. [Shell Detection Matrix](#5-shell-detection-matrix)
6. [Shell Detection — Concrete Implementation](#6-shell-detection--concrete-implementation)
7. [Command Execution Per Platform](#7-command-execution-per-platform)
8. [Editor Detection Per Platform](#8-editor-detection-per-platform)
9. [CI/CD Testing Matrix](#9-cicd-testing-matrix)
10. [`.snips` File Portability](#10-snips-file-portability)
11. [Dependency Additions](#11-dependency-additions)
12. [Migration Path](#12-migration-path)

---

## 1. Current Cross-Platform Gaps

### 1.1 `executor.rs` — Partial Windows Support

The current code has `cfg!(target_os = "windows")` guards but **always** uses `sh -c`
on Unix and `cmd /C` on Windows. This ignores the user's actual shell and means:

- Windows users get `cmd.exe` semantics even if they use PowerShell or WSL.
- The snippet `shell` field (`Option<String>` on `Snippet`) is parsed but **never
  consulted** during execution.
- No pipe/redirection support differences are handled.

### 1.2 `shell.rs` — Unix-Only Editor Detection

```rust
// Current: falls back to "vi" — does not exist on Windows
pub fn default_editor() -> String {
    std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| "vi".to_string())
}
```

- `$SHELL` is queried but never used for execution.
- No Windows fallback editor (`notepad.exe`).
- `open_in_editor` spawns the editor directly (no terminal-aware handling on macOS).

### 1.3 `completions.rs` — Missing Nushell

The `Shell` enum supports `Bash`, `Zsh`, `Fish`, `Elvish`, `PowerShell`. Nushell is
missing despite its rapid adoption. `clap_complete` supports it natively.

### 1.4 `picker.rs` — ANSI Assumptions

`crossterm` handles most cross-platform terminal work, but the `colored` crate needs
ANSI support enabled on Windows (see [Section 2.5](#25-ansi-color-support)).

### 1.5 `doctor.rs` — Unicode Icons

Uses `✓`, `✗`, `⚠` which may not render in legacy Windows consoles (pre-Windows 10
v1511). The `which` crate is cross-platform and is fine.

### 1.6 `snipfile.rs` — No Line-Ending Enforcement

The atomic write uses `fs::write_all` with whatever `toml::to_string_pretty` produces.
No LF enforcement — a user on Windows may introduce CRLF.

---

## 2. Windows Support

### 2.1 Command Execution Strategy

The current `cmd /C` fallback is the **lowest common denominator**. Windows users
increasingly use PowerShell (7+), and some use WSL. The execution strategy should be:

| Scenario | Shell | Invocation |
|----------|-------|-----------|
| Default Windows | `cmd.exe` | `cmd /C <cmd>` |
| PowerShell preferred | `pwsh.exe` | `pwsh -Command <cmd>` |
| Windows PowerShell | `powershell.exe` | `powershell -Command <cmd>` |
| WSL | `wsl` | `wsl sh -c <cmd>` |
| Snippet `shell` override | explicit | User-specified |

**Decision tree:**

1. If the snippet has a `shell` field, use it directly.
2. If `$SHELL` is set (WSL or Git Bash), use `$SHELL -c`.
3. If `pwsh.exe` or `powershell.exe` is found on PATH, use it.
4. Fall back to `cmd /C`.

```rust
// src/core/executor.rs — redesigned

use std::process::Command;
use anyhow::{Context, Result};

/// Platform-aware shell resolution.
pub fn resolve_shell(override_shell: Option<&str>) -> (String, Vec<String>) {
    // 1. Explicit per-snippet override
    if let Some(shell) = override_shell {
        if cfg!(target_os = "windows") {
            // On Windows, "bash" might mean Git Bash
            return (shell.to_string(), vec!["-c".to_string()]);
        }
        return (shell.to_string(), vec!["-c".to_string()]);
    }

    if cfg!(target_os = "windows") {
        resolve_windows_shell()
    } else {
        // Unix: respect $SHELL, fall back to "sh"
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string());
        (shell, vec!["-c".to_string()])
    }
}

/// Resolve the best available shell on Windows.
fn resolve_windows_shell() -> (String, Vec<String>) {
    // Check $SHELL first (set by Git Bash, MSYS2, WSL interop)
    if let Ok(shell) = std::env::var("SHELL") {
        return (shell, vec!["-c".to_string()]);
    }

    // Check $SNIP_SHELL for explicit user preference
    if let Ok(shell) = std::env::var("SNIP_SHELL") {
        return match shell.as_str() {
            "powershell" | "pwsh" => ("pwsh.exe".to_string(), vec!["-Command".to_string()]),
            "cmd" => ("cmd".to_string(), vec!["/C".to_string()]),
            other => (other.to_string(), vec!["-c".to_string()]),
        };
    }

    // Prefer pwsh (PowerShell 7+) over cmd
    if which::which("pwsh.exe").is_ok() {
        return ("pwsh.exe".to_string(), vec!["-Command".to_string()]);
    }
    if which::which("pwsh").is_ok() {
        return ("pwsh".to_string(), vec!["-Command".to_string()]);
    }

    // Fall back to cmd.exe
    ("cmd".to_string(), vec!["/C".to_string()])
}

/// Execute a shell command string using the best available shell.
pub fn execute(cmd: &str) -> Result<()> {
    execute_with_shell(cmd, None, None)
}

/// Execute a shell command with optional shell override and working directory.
pub fn execute_with_shell(
    cmd: &str,
    override_shell: Option<&str>,
    dir: Option<&std::path::Path>,
) -> Result<()> {
    let (shell, args) = resolve_shell(override_shell);
    let mut command = Command::new(&shell);
    command.args(&args);
    command.arg(cmd);

    if let Some(d) = dir {
        command.current_dir(d);
    }

    // On Windows, don't spawn a new console window
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    let status = command
        .status()
        .with_context(|| format!("spawning shell '{}'", shell))?;

    if status.success() {
        Ok(())
    } else {
        let code = status.code().unwrap_or(-1);
        anyhow::bail!("command exited with status {}", code);
    }
}

/// Execute a shell command and capture stdout.
pub fn execute_capture(cmd: &str) -> Result<String> {
    let (shell, args) = resolve_shell(None);
    let output = Command::new(&shell)
        .args(&args)
        .arg(cmd)
        .output()
        .with_context(|| format!("spawning shell '{}'", shell))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let code = output.status.code().unwrap_or(-1);
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("command exited with status {}: {}", code, stderr);
    }
}
```

### 2.2 Editor Detection

```rust
// src/utils/shell.rs — platform-aware editor detection

/// Return the user's editor, with platform-aware fallbacks.
pub fn default_editor() -> String {
    // 1. Explicit environment variables (works on all platforms)
    if let Ok(editor) = std::env::var("VISUAL") {
        return editor;
    }
    if let Ok(editor) = std::env::var("EDITOR") {
        return editor;
    }

    if cfg!(target_os = "windows") {
        // 2. Windows: try common editors in order
        for candidate in &["code.cmd", "code", "notepad++.exe", "notepad.exe"] {
            if which::which(candidate).is_ok() {
                return candidate.to_string();
            }
        }
        "notepad.exe".to_string()
    } else if cfg!(target_os = "macos") {
        // 3. macOS: try VS Code, then TextEdit via `open -t`
        for candidate in &["code", "vim", "nvim"] {
            if which::which(candidate).is_ok() {
                return candidate.to_string();
            }
        }
        // `open -t` opens in TextEdit (the default macOS GUI editor)
        // We return a special sentinel handled in open_in_editor
        "open -t".to_string()
    } else {
        // 4. Linux: try common editors, then xdg-open, then sensible-editor
        for candidate in &["code", "vim", "nvim", "nano"] {
            if which::which(candidate).is_ok() {
                return candidate.to_string();
            }
        }
        if which::which("sensible-editor").is_ok() {
            return "sensible-editor".to_string();
        }
        if which::which("xdg-open").is_ok() {
            return "xdg-open".to_string();
        }
        "vi".to_string()
    }
}

/// Open a file in the user's editor.
pub fn open_in_editor(path: &std::path::Path) -> anyhow::Result<()> {
    let editor = default_editor();

    // Handle composite commands like "open -t" (macOS) or "code --wait"
    let parts: Vec<&str> = editor.split_whitespace().collect();
    let (program, args) = parts.split_first().ok_or_else(|| {
        anyhow::anyhow!("empty editor command")
    })?;

    let mut cmd = Command::new(program);
    cmd.args(args);
    cmd.arg(path);

    // On Windows, don't spawn a new console window
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    let status = cmd
        .status()
        .map_err(|e| anyhow::anyhow!("failed to open editor '{}': {}", editor, e))?;

    if status.success() {
        Ok(())
    } else {
        let code = status.code().unwrap_or(-1);
        anyhow::bail!("editor exited with status {}", code)
    }
}
```

### 2.3 Shell Completions

The current `completions.rs` already supports PowerShell via `clap_complete`. Add
Nushell support:

```rust
// src/cli/completions.rs — extended Shell enum

#[derive(Debug, Clone, ValueEnum)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
    Elvish,
    PowerShell,
    Nushell,
}

impl Shell {
    fn to_clap_shell(&self) -> shells::Shell {
        match self {
            Shell::Bash => shells::Shell::Bash,
            Shell::Zsh => shells::Shell::Zsh,
            Shell::Fish => shells::Shell::Fish,
            Shell::Elvish => shells::Shell::Elvish,
            Shell::PowerShell => shells::Shell::PowerShell,
            Shell::Nushell => shells::Shell::Nushell,
        }
    }
}
```

**PowerShell install instructions** (for `snip completions` help text):

```powershell
# Add to $PROFILE (not $PROFILE.CurrentUserAllHosts — that's too broad)
snip completions powershell | Out-String | Invoke-Expression
```

### 2.4 Path Handling

Windows paths introduce backslashes, drive letters (`C:\`), and UNC paths
(`\\server\share\file`). The `.snips` file format must be agnostic (see
[Section 10](#10-snips-file-portability)). At runtime:

```rust
// src/utils/path.rs — new file

use std::path::{Path, PathBuf};

/// Convert a path from .snips format (forward slashes) to the native OS format.
///
/// `.snips` files always use `/` as path separator. This function converts
/// them to `\` on Windows, and is a no-op on Unix.
pub fn to_native_path(snips_path: &str) -> PathBuf {
    if cfg!(target_os = "windows") {
        // Replace forward slashes with backslashes for the Windows filesystem,
        // but preserve UNC path prefixes (// → \\)
        let path = snips_path.replace('/', "\\");
        PathBuf::from(path)
    } else {
        PathBuf::from(snips_path)
    }
}

/// Convert a native path to .snips format (always forward slashes).
pub fn to_snips_path(native: &Path) -> String {
    let s = native.to_string_lossy();
    if cfg!(target_os = "windows") {
        s.replace('\\', "/")
    } else {
        s.into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_native_windows_path() {
        // These tests only verify the non-Windows behavior in CI.
        // Windows-specific tests run in the windows-latest CI job.
        #[cfg(not(target_os = "windows"))]
        {
            assert_eq!(to_native_path("src/main.rs"), PathBuf::from("src/main.rs"));
        }
    }

    #[test]
    fn to_snips_always_forward() {
        assert_eq!(to_snips_path(Path::new("src/main.rs")), "src/main.rs");
    }
}
```

### 2.5 ANSI Color Support

Windows 10+ supports ANSI escape codes, but they must be explicitly enabled for
console applications. The `colored` crate (currently used) handles this automatically
via its `wincon` backend. However, **the process must have a valid console**.

Two issues to address:

1. **`CREATE_NO_WINDOW` flag:** When executing commands with the
   `0x08000000` (CREATE_NO_WINDOW) flag, no console is attached, so ANSI output
   from child processes is lost. Solution: only set this flag for
   `execute_capture()` (which reads stdout directly), not for `execute()`
   (which inherits stdio).

2. **`colored` crate initialization:** Add explicit initialization in `main.rs`:

```rust
// src/main.rs — top of main()

fn main() -> anyhow::Result<()> {
    // Enable ANSI color support on Windows 10+
    #[cfg(target_os = "windows")]
    colored::control::set_virtual_terminal(true).ok();

    let cli = Cli::parse();
    // ... rest of main
}
```

### 2.6 File Watching (Future: `snip watch`)

The `notify` crate provides cross-platform file system events:

```toml
# Cargo.toml addition
notify = { version = "7", features = ["macos_fsevent"] }
```

```rust
// Future: src/cli/watch.rs (conceptual)

use notify::{Watcher, RecursiveMode, watcher};

// notify abstracts over:
// - Windows: ReadDirectoryChangesW
// - macOS:  FSEvents (kqueue as fallback)
// - Linux:  inotify
let (tx, rx) = std::sync::mpsc::channel();
let mut watcher = watcher(tx, std::time::Duration::from_secs(1))?;
watcher.watch(".snips", RecursiveMode::NonRecursive)?;
```

The key advantage of `notify` is that it handles the platform differences
transparently. No special Windows/macOS/Linux code is needed.

---

## 3. macOS Specifics

### 3.1 Editor Detection

macOS has two unique editor pathways:

| Method | Command | Notes |
|--------|---------|-------|
| VS Code | `code --wait` | Most common for developers |
| TextEdit | `open -t` | System default GUI editor |
| Vim | `vim` | Pre-installed, CLI-safe |
| Xcode | `xed` | Only useful for Swift/Xcode projects |

The `open -t` command is special: it opens the file in whichever app is registered
for `.txt` files (usually TextEdit). This is a **composite command** that must be
handled by `open_in_editor` (see [Section 2.2](#22-editor-detection)).

### 3.2 Apple Silicon vs Intel Binary Distribution

| Architecture | Target | Rust Target | Notes |
|-------------|--------|-------------|-------|
| Apple Silicon | `aarch64-apple-darwin` | `aarch64-apple-darwin` | M1/M2/M3/M4 |
| Intel | `x86_64-apple-darwin` | `x86_64-apple-darwin` | Older Macs |
| Universal | Fat binary | Both, then `lipo` | Optional |

**Strategy:** Ship separate binaries. Universal binaries add complexity and double
the download size. The GitHub Release workflow should build both targets.

```yaml
# .github/workflows/release.yml (conceptual)
- name: Build (macOS aarch64)
  run: cargo build --release --target aarch64-apple-darwin
- name: Build (macOS x86_64)
  run: cargo build --release --target x86_64-apple-darwin
```

### 3.3 Keychain Integration (Future)

macOS Keychain can store sensitive snippet variables (API keys, tokens) securely.
This is a **future feature** — design placeholder:

```rust
// Future: src/utils/keychain.rs (macOS only)

#[cfg(target_os = "macos")]
pub fn store_secret(key: &str, value: &str) -> Result<()> {
    // Use `security add-generic-password` CLI
    let status = Command::new("security")
        .args(["add-generic-password", "-a", "snip", "-s", key, "-w", value])
        .status()?;
    // ...
}

#[cfg(target_os = "macos")]
pub fn get_secret(key: &str) -> Result<Option<String>> {
    // Use `security find-generic-password -w -s <key>`
    let output = Command::new("security")
        .args(["find-generic-password", "-a", "snip", "-s", key, "-w"])
        .output()?;
    // ...
}
```

### 3.4 Homebrew Installation

```ruby
# Formula placeholder (homebrew-tap)
class Snip < Formula
  desc "Project-scoped command snippets with built-in fuzzy finder"
  homepage "https://github.com/Bilal140202/snip"
  url "https://github.com/Bilal140202/snip/releases/download/v#{version}/snip-#{version}-aarch64-macos.tar.gz"
  sha256 "..."
  license "MIT"

  def install
    bin.install "snip"
  end
end
```

---

## 4. Linux Specifics

### 4.1 Editor Detection

| Priority | Editor | Reason |
|----------|--------|--------|
| 1 | `$VISUAL` | User preference (GUI editors like `code --wait`) |
| 2 | `$EDITOR` | User preference (CLI editors like `vim`) |
| 3 | `code` | VS Code — most common developer editor |
| 4 | `vim` / `nvim` | Ubiquitous on servers |
| 5 | `nano` | Ubiquitous, beginner-friendly |
| 6 | `sensible-editor` | Debian/Ubuntu default (runs `$EDITOR` or `nano`) |
| 7 | `xdg-open` | Opens in GUI (not ideal for terminal snippets) |

### 4.2 Shell Implementation Support

Linux has more shell diversity than any other platform. The snippet `shell` field
must support all of these:

| Shell | Invocation | Notes |
|-------|-----------|-------|
| bash | `bash -c "<cmd>"` | Default on most systems |
| zsh | `zsh -c "<cmd>"` | macOS default, popular on Linux |
| fish | `fish -c "<cmd>"` | Different syntax — **not POSIX** |
| nushell | `nu -c "<cmd>"` | Structured data — **not POSIX** |
| dash | `dash -c "<cmd>"` | `/bin/sh` on Ubuntu/Debian |
| ash/busybox | `ash -c "<cmd>"` | Alpine Linux, embedded |

**Critical:** Fish and Nushell are not POSIX-compatible. A snippet written with
`&&`, `|`, or `$VAR` syntax will break in these shells. The `shell` field on each
snippet handles this — users can set `shell = "bash"` to force POSIX semantics.

### 4.3 musl vs glibc — Static Binary Strategy

| Distro | libc | Strategy |
|--------|------|----------|
| Ubuntu, Debian, Fedora | glibc | Dynamic linking (default) |
| Alpine, Arch (musl) | musl | Static linking needed |

**Recommendation:** Use `cargo-zigbuild` for cross-compilation with musl targets.
Ship a `x86_64-unknown-linux-musl` binary alongside the default glibc binary.

```bash
# Build static musl binary using zig as linker
cargo zigbuild --release --target x86_64-unknown-linux-musl
```

This produces a single binary with zero system dependencies, working on Alpine,
Busybox containers, and older glibc systems.

### 4.4 Package Manager Distribution

| Manager | Format | Repository |
|---------|--------|-----------|
| APT | `.deb` | Personal Package Archive (PPA) or GitHub Releases |
| RPM | `.rpm` | COPR (Fedora) or OpenSUSE Build Service |
| AUR | `PKGBUILD` | Arch User Repository (community-maintained) |
| Nix | `flake.nix` | nixpkgs or personal flake |
| Homebrew | Ruby formula | `homebrew-core` or personal tap |

For the initial release, **direct binary download from GitHub Releases** is
sufficient. Package manager support can be added incrementally.

---

## 5. Shell Detection Matrix

### 5.1 Identification

| Shell | Process Name | Env Var | Config File | Detection Method |
|-------|-------------|---------|-------------|-----------------|
| bash | `bash` | `$BASH_VERSION` | `~/.bashrc` | Check `$SHELL`, parse process name |
| zsh | `zsh` | `$ZSH_VERSION` | `~/.zshrc` | Check `$SHELL`, parse process name |
| fish | `fish` | `$FISH_VERSION` | `~/.config/fish/config.fish` | Check `$SHELL`, parse process name |
| nushell | `nu` | `$NU_VERSION` | `~/.config/nushell/env.nu` | Check `$SHELL`, parse process name |
| PowerShell | `pwsh` / `powershell` | `$PSVersionTable` | `$PROFILE` | Check `$SHELL`, Windows-only default |
| cmd | `cmd` | N/A | N/A | Windows fallback only |
| dash | `dash` | N/A | N/A | Check `/bin/sh` symlink |

### 5.2 Completion Style

| Shell | Completion System | Integration Point | Generated By |
|-------|------------------|-------------------|-------------|
| bash | `compgen` / `complete` | `.bashrc` or `.bash_completion` | `clap_complete::Shell::Bash` |
| zsh | `compdef` / `compinit` | `.zshrc` (in `fpath`) | `clap_complete::Shell::Zsh` |
| fish | `complete` | `~/.config/fish/completions/snip.fish` | `clap_complete::Shell::Fish` |
| nushell | custom completer | `~/.config/nushell/config.nu` | `clap_complete::Shell::Nushell` |
| PowerShell | `Register-ArgumentCompleter` | `$PROFILE` | `clap_complete::Shell::PowerShell` |
| cmd | DOSKEY macros | N/A (no real completion) | Manual / not supported |

### 5.3 Syntax Differences Affecting Snippets

| Feature | bash/zsh/dash | fish | nushell | PowerShell | cmd |
|---------|--------------|------|---------|-----------|-----|
| Variable | `$VAR` | `$VAR` | `$env.VAR` | `$VAR` | `%VAR%` |
| Command substitution | `$(cmd)` or `` `cmd` `` | `(cmd)` | `(cmd)` | `$(cmd)` or `` `cmd` `` | N/A |
| AND operator | `&&` | `&&` | `and` | `-and` | `&` |
| OR operator | `\|\|` | `\|\|` | `or` | `-or` | N/A |
| Pipes | `\|` | `\|` | `\|` | `\|` | `\|` |
| String quoting | `"..."`, `'...'` | `"..."`, `'...'` | `"..."`, `'...'` | `"..."`, `'...'` | `"..."` |
| Line continuation | `\` | `\` | `(` multi-line `)` | `` ` `` | `^` |
| Comments | `#` | `#` | `#` | `#` | `REM` / `::` |
| Environment | `VAR=val cmd` | `env VAR=val cmd` | `with-env {VAR: val} {cmd}` | `$env:VAR="val"; cmd` | `set VAR=val & cmd` |

---

## 6. Shell Detection — Concrete Implementation

```rust
// src/utils/shell.rs — shell detection system

use std::process::Command;

/// Represents a detected shell environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectedShell {
    Bash,
    Zsh,
    Fish,
    Nushell,
    PowerShell,
    Cmd,
    Dash,
    Sh,
    Unknown(String),
}

impl DetectedShell {
    /// The shell's human-readable name.
    pub fn name(&self) -> &str {
        match self {
            Self::Bash => "bash",
            Self::Zsh => "zsh",
            Self::Fish => "fish",
            Self::Nushell => "nushell",
            Self::PowerShell => "powershell",
            Self::Cmd => "cmd",
            Self::Dash => "dash",
            Self::Sh => "sh",
            Self::Unknown(name) => name,
        }
    }

    /// The binary name to invoke this shell.
    pub fn binary(&self) -> &str {
        match self {
            Self::Bash => "bash",
            Self::Zsh => "zsh",
            Self::Fish => "fish",
            Self::Nushell => "nu",
            Self::PowerShell => {
                if cfg!(target_os = "windows") {
                    if which::which("pwsh.exe").is_ok() || which::which("pwsh").is_ok() {
                        "pwsh"
                    } else {
                        "powershell"
                    }
                } else {
                    "pwsh"
                }
            }
            Self::Cmd => "cmd",
            Self::Dash => "dash",
            Self::Sh => "sh",
            Self::Unknown(name) => name,
        }
    }

    /// The flag to pass a command string to this shell.
    pub fn command_flag(&self) -> &str {
        match self {
            Self::PowerShell => "-Command",
            Self::Cmd => "/C",
            _ => "-c",
        }
    }

    /// Whether this shell is POSIX-compatible (supports $VAR, &&, ||, etc.)
    pub fn is_posix(&self) -> bool {
        matches!(self, Self::Bash | Self::Zsh | Self::Dash | Self::Sh)
    }

    /// The config file path for installing completions (if applicable).
    pub fn completion_hint(&self) -> Option<String> {
        match self {
            Self::Bash => Some("Add to ~/.bashrc or ~/.bash_completion".into()),
            Self::Zsh => Some("Add to a directory in $fpath, then run compinit".into()),
            Self::Fish => Some("Save to ~/.config/fish/completions/snip.fish".into()),
            Self::Nushell => Some("Add to ~/.config/nushell/config.nu".into()),
            Self::PowerShell => Some("Add to $PROFILE".into()),
            _ => None,
        }
    }
}

/// Detect the current shell from the environment.
///
/// Detection order:
/// 1. `$SHELL` env var (set by terminal emulators on Unix)
/// 2. Parent process name (works when $SHELL is not set)
/// 3. Platform defaults
pub fn detect_shell() -> DetectedShell {
    // 1. Check $SHELL
    if let Ok(shell_path) = std::env::var("SHELL") {
        if let Some(name) = shell_path.rsplit('/').next() {
            if let Some(detected) = classify_shell_name(name) {
                return detected;
            }
        }
    }

    // 2. Check parent process name
    #[cfg(unix)]
    {
        if let Some(parent) = parent_process_name() {
            if let Some(detected) = classify_shell_name(&parent) {
                return detected;
            }
        }
    }

    // 3. Check $COMSPEC (Windows)
    #[cfg(target_os = "windows")]
    {
        if let Ok(comspec) = std::env::var("COMSPEC") {
            if comspec.to_lowercase().contains("cmd.exe") {
                return DetectedShell::Cmd;
            }
        }
        // Check if running inside PowerShell
        if let Ok(ps) = std::env::var("PSModulePath") {
            if !ps.is_empty() {
                return DetectedShell::PowerShell;
            }
        }
    }

    // 4. Platform defaults
    if cfg!(target_os = "windows") {
        DetectedShell::Cmd
    } else {
        DetectedShell::Sh
    }
}

/// Classify a shell binary name into a `DetectedShell` variant.
fn classify_shell_name(name: &str) -> Option<DetectedShell> {
    let name_lower = name.to_lowercase();
    match name_lower.as_str() {
        "bash" => Some(DetectedShell::Bash),
        "zsh" => Some(DetectedShell::Zsh),
        "fish" => Some(DetectedShell::Fish),
        "nu" | "nushell" => Some(DetectedShell::Nushell),
        "pwsh" | "pwsh.exe" | "powershell" | "powershell.exe" => {
            Some(DetectedShell::PowerShell)
        }
        "cmd" | "cmd.exe" => Some(DetectedShell::Cmd),
        "dash" => Some(DetectedShell::Dash),
        "sh" => Some(DetectedShell::Sh),
        other => Some(DetectedShell::Unknown(other.to_string())),
    }
}

/// Get the name of the parent process (Unix only).
#[cfg(unix)]
fn parent_process_name() -> Option<String> {
    use std::fs;

    let pid = std::os::unix::process::parent_id();
    let comm_path = format!("/proc/{}/comm", pid);
    fs::read_to_string(&comm_path)
        .ok()
        .map(|s| s.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_known_shells() {
        assert_eq!(classify_shell_name("bash"), Some(DetectedShell::Bash));
        assert_eq!(classify_shell_name("zsh"), Some(DetectedShell::Zsh));
        assert_eq!(classify_shell_name("fish"), Some(DetectedShell::Fish));
        assert_eq!(classify_shell_name("nu"), Some(DetectedShell::Nushell));
        assert_eq!(classify_shell_name("pwsh"), Some(DetectedShell::PowerShell));
        assert_eq!(classify_shell_name("cmd.exe"), Some(DetectedShell::Cmd));
        assert_eq!(classify_shell_name("dash"), Some(DetectedShell::Dash));
        assert_eq!(classify_shell_name("sh"), Some(DetectedShell::Sh));
    }

    #[test]
    fn classify_case_insensitive() {
        assert_eq!(classify_shell_name("BASH"), Some(DetectedShell::Bash));
        assert_eq!(classify_shell_name("Zsh"), Some(DetectedShell::Zsh));
    }

    #[test]
    fn posix_shells() {
        assert!(DetectedShell::Bash.is_posix());
        assert!(DetectedShell::Zsh.is_posix());
        assert!(DetectedShell::Dash.is_posix());
        assert!(!DetectedShell::Fish.is_posix());
        assert!(!DetectedShell::Nushell.is_posix());
        assert!(!DetectedShell::PowerShell.is_posix());
    }

    #[test]
    fn command_flags() {
        assert_eq!(DetectedShell::Bash.command_flag(), "-c");
        assert_eq!(DetectedShell::PowerShell.command_flag(), "-Command");
        assert_eq!(DetectedShell::Cmd.command_flag(), "/C");
    }
}
```

---

## 7. Command Execution Per Platform

### 7.1 Integrated Execution with Shell Detection

The executor should use the detected shell when no per-snippet override is
provided. Here's the refined execution flow:

```rust
// src/core/executor.rs — final design

use std::process::Command;
use anyhow::{Context, Result};
use crate::utils::shell::{detect_shell, DetectedShell};

/// Execute a snippet's command.
///
/// Resolution order:
/// 1. Snippet's `shell` field (explicit override)
/// 2. User's detected shell (from $SHELL / parent process)
/// 3. Platform default
pub fn execute_snippet(cmd: &str, snippet_shell: Option<&str>, dir: Option<&std::path::Path>) -> Result<()> {
    let (shell_bin, flag) = if let Some(override_shell) = snippet_shell {
        // Explicit per-snippet shell override
        let shell = match override_shell {
            "bash" => "bash",
            "zsh" => "zsh",
            "fish" => "fish",
            "nu" | "nushell" => "nu",
            "powershell" | "pwsh" => {
                if cfg!(target_os = "windows") {
                    if which::which("pwsh.exe").is_ok() { "pwsh.exe" } else { "powershell.exe" }
                } else {
                    "pwsh"
                }
            }
            "cmd" => {
                if cfg!(target_os = "windows") { "cmd" } else { "cmd" }
            }
            other => other,
        };
        (shell.to_string(), shell_command_flag(override_shell).to_string())
    } else {
        // Use detected shell
        let detected = detect_shell();
        (detected.binary().to_string(), detected.command_flag().to_string())
    };

    let mut command = Command::new(&shell_bin);
    command.arg(&flag);
    command.arg(cmd);

    if let Some(d) = dir {
        command.current_dir(d);
    }

    // Windows: prevent console window popup for GUI editors / non-CLI tools
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // Only suppress window for cmd/powershell background execution
        // Don't suppress for interactive editors
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    let status = command
        .status()
        .with_context(|| format!("spawning '{} {} {}'", shell_bin, flag, cmd))?;

    if status.success() {
        Ok(())
    } else {
        let code = status.code().unwrap_or(-1);
        anyhow::bail!("command exited with status {}", code);
    }
}

fn shell_command_flag(shell: &str) -> &str {
    match shell {
        "powershell" | "pwsh" => "-Command",
        "cmd" => "/C",
        _ => "-c",
    }
}
```

### 7.2 Windows-Specific: `CREATE_NO_WINDOW` Decision

| Scenario | Flag | Reason |
|----------|------|--------|
| `snip run` (foreground) | None | Inherit parent console |
| `snip run` (capture stdout) | `CREATE_NO_WINDOW` | No console needed |
| `snip edit` (editor) | None | Editor needs console/terminal |
| `snip doctor` (which check) | `CREATE_NO_WINDOW` | Silent binary check |

---

## 8. Editor Detection Per Platform

### 8.1 Complete Detection Chain

```rust
/// Platform-specific editor fallback chain.
///
/// Returns the editor command string (may contain arguments like "open -t").
pub fn detect_editor() -> String {
    // Priority 1: Explicit user configuration
    if let Ok(v) = std::env::var("SNIP_EDITOR") {
        return v;
    }

    // Priority 2: Standard environment variables
    if let Ok(v) = std::env::var("VISUAL") {
        return v;
    }
    if let Ok(v) = std::env::var("EDITOR") {
        return v;
    }

    // Priority 3: Platform-specific discovery
    if cfg!(target_os = "windows") {
        detect_windows_editor()
    } else if cfg!(target_os = "macos") {
        detect_macos_editor()
    } else {
        detect_linux_editor()
    }
}

fn detect_windows_editor() -> String {
    // Check Git for Windows: sets EDITOR=vim, but vim might not be on PATH
    for candidate in ["code.cmd", "code.exe", "code", "notepad++.exe", "notepad.exe"] {
        if which::which(candidate).is_ok() {
            return candidate.to_string();
        }
    }
    "notepad.exe".to_string()
}

fn detect_macos_editor() -> String {
    for candidate in ["code", "vim", "nvim", "vim"] {
        if which::which(candidate).is_ok() {
            // VS Code needs --wait to block until file is closed
            if candidate == "code" {
                return "code --wait".to_string();
            }
            return candidate.to_string();
        }
    }
    // macOS fallback: open in default GUI text editor
    "open -t".to_string()
}

fn detect_linux_editor() -> String {
    for candidate in ["code", "vim", "nvim", "nano"] {
        if which::which(candidate).is_ok() {
            if candidate == "code" {
                return "code --wait".to_string();
            }
            return candidate.to_string();
        }
    }
    // Debian/Ubuntu: sensible-editor is a wrapper that respects $EDITOR
    // or falls back to nano
    if which::which("sensible-editor").is_ok() {
        return "sensible-editor".to_string();
    }
    "vi".to_string()
}
```

---

## 9. CI/CD Testing Matrix

### 9.1 GitHub Actions Matrix

```yaml
# .github/workflows/ci.yml

name: CI
on: [push, pull_request]

jobs:
  test:
    name: Test (${{ matrix.os }}, ${{ matrix.rust }})
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
        rust: [stable, beta]
        include:
          - os: ubuntu-latest
            rust: stable
            target: x86_64-unknown-linux-gnu
          - os: macos-latest
            rust: stable
            target: aarch64-apple-darwin
          - os: windows-latest
            rust: stable
            target: x86_64-pc-windows-msvc
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@master
        with:
          toolchain: ${{ matrix.rust }}
          targets: ${{ matrix.target }}
      - run: cargo test --all
      - run: cargo test --all -- --ignored  # slow / integration tests

  lint:
    name: Lint
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - run: cargo fmt -- --check
      - run: cargo clippy --all-targets -- -D warnings

  musl:
    name: Test (musl)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: sudo apt-get update && sudo apt-get install -y musl-tools
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: x86_64-unknown-linux-musl
      - run: cargo build --release --target x86_64-unknown-linux-musl
      - run: cargo test --target x86_64-unknown-linux-musl
```

### 9.2 Testing Interactive Features in CI

Interactive features (picker, prompts) cannot run in a TTY-less CI environment.
Strategy:

1. **Feature gates:** All interactive code paths behind `is_stdout_tty()` checks
   (already implemented in `picker.rs`).

2. **Fallback testing:** The `pick_fallback` path (non-interactive) is tested
   normally. The TTY path is tested with a pseudo-terminal.

3. **PTY testing (optional):** Use the `pty-process` crate for integration tests
   that simulate a terminal:

```rust
// tests/integration/interactive.rs (future)

#[cfg(all(unix, not(ci)))]
#[test]
fn test_picker_with_pty() {
    use pty_process::Pty;
    let mut pty = Pty::new().unwrap();
    pty.spawn_command("target/debug/snip").unwrap();
    pty.write_all(b"build\n").unwrap();
    // read output, verify selection
}
```

4. **Snapshot testing for terminal output:** Use `insta` for cross-platform
   snapshot tests. Separate snapshots per platform:

```rust
// tests/snapshots/render_test.rs

#[test]
fn test_snippet_entry_render() {
    let output = capture_terminal_output(|| {
        crate::ui::render::snippet_entry(
            "build.release",
            "cargo build --release",
            "Build in release mode",
            &["rust".to_string()],
        );
    });
    // insta handles platform-specific newlines (\r\n vs \n)
    insta::assert_snapshot!(output);
}
```

### 9.3 Platform-Specific Test Concerns

| Concern | Linux/macOS | Windows | Mitigation |
|---------|------------|---------|-----------|
| Path separators in output | `/` | `\` | Normalize in snapshot comparisons |
| Line endings | `\n` | `\r\n` | `.trim()` or `strip_cr()` in assertions |
| Unicode rendering | Full | Partial (legacy console) | Test with `colored::control::set_virtual_terminal(true)` |
| `which` binary names | `vim`, `nu` | `vim.exe`, `nu.exe` | `which` crate handles extensions automatically |
| Temp file paths | `/tmp/...` | `C:\Users\...\AppData\...` | Use `tempfile` crate (already a dependency) |
| Shell invocation | `sh -c` | `cmd /C` | Test both paths via `cfg!(target_os)` |

---

## 10. `.snips` File Portability

The `.snips` TOML format must work identically on all platforms. A `.snips` file
committed to a Git repo and cloned on Windows, macOS, and Linux must behave the
same.

### 10.1 Line Endings: Enforce LF

CRLF line endings from Windows editors can break TOML parsing (though the `toml`
crate handles both). Enforce LF at write time:

```rust
// src/core/snipfile.rs — write_snippets() modification

pub fn write_snippets(path: &Path, data: &SnipFile) -> Result<()> {
    let toml_value = data.to_toml_value();
    let mut toml_str = toml::to_string_pretty(&toml_value)
        .context("failed to serialize .snips data to TOML")?;

    // Enforce LF line endings for cross-platform portability.
    // Git on Windows may convert to CRLF on checkout; that's fine for reading.
    // But we always write LF to ensure the canonical form.
    if cfg!(target_os = "windows") {
        toml_str = toml_str.replace("\r\n", "\n");
    }

    // ... rest of write logic unchanged

    // Atomic write via temp file.
    let tmp_path = path.with_extension("snips.tmp");
    {
        let mut file = fs::File::create(&tmp_path)
            .with_context(|| format!("failed to create temp file: {}", tmp_path.display()))?;
        file.write_all(toml_str.as_bytes())
            .context("failed to write .snips data")?;
    }
    fs::rename(&tmp_path, path).with_context(|| {
        format!(
            "failed to rename temp file {} -> {}",
            tmp_path.display(),
            path.display()
        )
    })?;

    Ok(())
}
```

Additionally, add a `.gitattributes` entry to the project template:

```
# .snips files always use LF
.snips text eol=lf
```

### 10.2 Path Separators: Always `/` in Storage

The `.snips` file must use forward slashes for all paths. The `dir` field on
`Snippet` stores paths relative to the project root:

```toml
# CORRECT — portable
[build]
cmd = "cargo build"
dir = "src/lib"

# WRONG — Windows-only
[build]
cmd = "cargo build"
dir = "src\lib"
```

Conversion happens at **execution time only**, not storage time:

```rust
// In executor.rs, when resolving the `dir` field:

fn resolve_working_dir(snippet: &Snippet, project_root: &Path) -> Option<PathBuf> {
    snippet.dir.as_ref().map(|relative| {
        // .snips always uses /, convert to native path at runtime
        let native_relative = if cfg!(target_os = "windows") {
            relative.replace('/', std::path::MAIN_SEPARATOR.to_string().as_str())
        } else {
            relative.clone()
        };
        project_root.join(native_relative)
    })
}
```

### 10.3 Command Escaping: Applied at Execution, Not Storage

Commands in `.snips` are stored as-is (no platform-specific escaping). Shell
escaping is applied at execution time based on the target shell:

```rust
/// Escape a command argument for the given shell.
///
/// This is used when constructing commands programmatically (e.g., from
/// variable substitution), NOT for commands written verbatim in .snips.
pub fn shell_escape(value: &str, shell: &DetectedShell) -> String {
    match shell {
        // POSIX shells: single-quote wrapping
        DetectedShell::Bash | DetectedShell::Zsh | DetectedShell::Dash | DetectedShell::Sh => {
            if value.contains('\'') {
                // Replace ' with '\'' (end quote, escaped quote, start quote)
                format!("'{}'", value.replace('\'', "'\\''"))
            } else {
                format!("'{}'", value)
            }
        }
        // Fish uses the same single-quote escaping
        DetectedShell::Fish => {
            if value.contains('\'') {
                format!("'{}'", value.replace('\'', "'\\''"))
            } else {
                format!("'{}'", value)
            }
        }
        // Nushell: double-quote wrapping
        DetectedShell::Nushell => {
            format!("\"{}\"", value.replace('"', "\\\""))
        }
        // PowerShell: single-quote wrapping (no escape mechanism for single quotes!)
        // Use double-quotes with backtick escaping
        DetectedShell::PowerShell => {
            let escaped = value
                .replace('`', "``")
                .replace('"', '`"');
            format!("\"{}\"", escaped)
        }
        // cmd.exe: double-quote wrapping, escape " with \"
        DetectedShell::Cmd => {
            let escaped = value.replace('"', "\\\"");
            format!("\"{}\"", escaped)
        }
        DetectedShell::Unknown(_) => {
            // Safe default: single-quote wrapping
            format!("'{}'", value.replace('\'', "'\\''"))
        }
    }
}
```

**Key principle:** The `.snips` file stores the command exactly as the user wrote
it. When a `{{variable}}` is substituted, the substituted value is escaped for the
target shell. The base command template is passed through verbatim to the shell.

---

## 11. Dependency Additions

```toml
# Cargo.toml — new dependencies

[dependencies]
# Existing (no changes needed to these):
# clap, clap_complete, toml, serde, serde_json, serde_yaml,
# crossterm, fuzzy-matcher, shell-words, dirs, colored, anyhow, which

# New:
notify = { version = "7", optional = true }  # For future `snip watch`
insta = { version = "1", optional = true }    # For snapshot testing

[dev-dependencies]
tempfile = "3"      # Already present
assert_cmd = "2"    # Already present
predicates = "3"    # Already present
insta = "1"         # Snapshot testing
```

**No new required dependencies.** The cross-platform support is achieved through:

- `which` (already present) — binary discovery
- `crossterm` (already present) — terminal handling
- `colored` (already present) — ANSI color with Windows support
- `std::process::Command` — shell execution
- `std::env` — environment variable reading
- `cfg!(target_os = "...")` — compile-time platform detection

---

## 12. Migration Path

### Phase 1: Foundation (No Breaking Changes)

1. **Refactor `executor.rs`** — Add `resolve_shell()`, `execute_snippet()`.
   Keep old `execute()` and `execute_capture()` as thin wrappers.
2. **Refactor `shell.rs`** — Add `detect_shell()`, `DetectedShell` enum,
   platform-aware `detect_editor()`. Keep old `default_editor()` as wrapper.
3. **Fix `snipfile.rs`** — Enforce LF in `write_snippets()`.
4. **Add `main.rs` ANSI init** — `colored::control::set_virtual_terminal(true)`
   on Windows.
5. **Extend `completions.rs`** — Add `Nushell` variant.

### Phase 2: Enhanced Platform Support

6. **Add `src/utils/path.rs`** — `to_native_path()`, `to_snips_path()`.
7. **Update executor** — Use `resolve_working_dir()` with native path conversion.
8. **Add `.gitattributes`** — Enforce LF for `.snips`.
9. **CI matrix** — Add `windows-latest` and `macos-latest` to GitHub Actions.

### Phase 3: Polish

10. **`snip doctor` platform info** — Show detected shell, OS, editor in output.
11. **Snapshot tests** — Add `insta` for cross-platform terminal output testing.
12. **File watching** — Add `notify` for `snip watch` (future command).

### Phase 4: Distribution

13. **musl static binary** — `cargo zigbuild` for Alpine/container support.
14. **Homebrew tap** — Formula for macOS installation.
15. **AUR package** — `PKGBUILD` for Arch Linux.
16. **Winget/scoop** — Windows package managers.