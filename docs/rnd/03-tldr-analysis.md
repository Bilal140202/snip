# tlrc (tldr) Deep Source Code Analysis

> Cloned: `tldr-pages/tlrc` v1.13.1 — the official Rust tldr client
> Full codebase: 7 source files, ~1,600 lines of Rust total. Remarkably small and focused.

---

## A. The Discovery/Adoption Playbook

### How tldr got 50K+ GitHub stars

The `tldr-pages/tldr` **content repo** has ~50K stars. The client (`tlrc`) is one of many clients. The growth strategy is:

1. **The content repo IS the product.** The tldr-pages repo is a community-maintained collection of ~3,000 markdown pages covering standard CLI tools. People star the *pages*, not the client. The client is just the delivery vehicle.

2. **Solves a universal pain point.** `man` pages are verbose and intimidating. `tldr` gives you "just the examples." This is something literally every developer encounters daily.

3. **Low-friction contribution model.** Pages are plain markdown files. PRs are trivial to create. The barrier to entry is nearly zero.

4. **Multi-client ecosystem.** There are official clients in Node.js, Python, Rust, plus dozens of unofficial ones. This means users pick their preferred language but the content is shared.

5. **Package manager ubiquity.** `tlrc` is in: Homebrew, Nix, AUR, Void, openSUSE, Winget, Scoop, MacPorts, NetBSD pkgsrc, Conda, crates.io, and as a `.deb`. This is the widest distribution possible for a CLI tool.

6. **The `tldr` command name itself** is the killer brand. It's become a verb: "just tldr it."

### Cache System Architecture

**File:** `src/cache.rs` (936 lines — the largest file in the project)

The cache system is the core of tlrc. Here's exactly how it works:

#### Cache Location
```rust
// src/cache.rs:38-40
pub fn locate() -> PathBuf {
    dirs::cache_dir().unwrap().join(env!("CARGO_PKG_NAME"))
    // e.g. ~/.cache/tlrc/
}
```

#### Cache Structure on Disk
```
~/.cache/tlrc/
  tldr.sha256sums          # checksums of all zip archives (age tracker)
  pages.en/                # English pages (always downloaded)
    common/                # cross-platform tools
      git.md
      tar.md
      ...
    linux/                 # Linux-specific tools
      apt.md
      ...
    osx/
    windows/
    ...
  pages.de/                # German pages (optional)
  pages.pl/                # Polish pages (optional)
```

#### Download & Update Strategy
```rust
// src/cache.rs:79-147  download_and_verify()
// 1. Download tldr.sha256sums from GitHub Releases
// 2. Parse it to get {language -> sha256} mapping
// 3. Compare with LOCAL checksum file
// 4. Only download zips for languages whose checksums changed
// 5. Verify SHA256 of each downloaded zip
// 6. Save new checksums file
```

Key design decisions:
- **Incremental updates**: Only downloads zips for languages that changed (SHA256 comparison). Skips entirely if all checksums match.
- **Full replacement per language**: When updating a language, the old `pages.XX/` directory is `remove_dir_all`'d and re-extracted. No per-file granularity.
- **SHA256 verification**: Uses `ring::digest::SHA256` (not a hand-rolled hash). Downloaded archives are verified against the checksum file.

#### Offline Usage
```rust
// src/main.rs:120-146
// Three-tier strategy:
// 1. If cache is EMPTY and offline -> error "cache does not exist"
// 2. If cache is STALE (>max_age) and offline -> warning, use stale cache
// 3. If cache is FRESH -> use it silently
```

The `--offline` flag completely prevents network access. The `defer_auto_update` config option shows the page FIRST, then updates in the background (deferred auto-update). This is clever: the user never waits for an update.

#### Cache Age Tracking
```rust
// src/cache.rs:723-739
pub fn age(&self) -> Result<Duration> {
    let sumfile = self.dir.join(CHECKSUM_FILE);
    let metadata = if sumfile.is_file() {
        fs::metadata(&sumfile)
    } else {
        fs::metadata(self.dir)  // fallback to directory mtime
    }?;
    metadata.modified()?.elapsed()
}
```

Default max age: **336 hours (2 weeks)**. Configurable via `cache.max_age` in the config.

---

## B. Content/Registry System

### How Content Is Sourced

The tldr ecosystem has a **sharp client/content separation**:

```
tldr-pages/tldr      -> The CONTENT repo (50K stars, community pages)
tldr-pages/tlrc      -> The Rust CLIENT repo (what we studied)
```

Content lives at: `https://github.com/tldr-pages/tldr`

Pages are distributed as **pre-built zip archives** from GitHub Releases:
```
https://github.com/tldr-pages/tldr/releases/latest/download/tldr-pages.en.zip
https://github.com/tldr-pages/tldr/releases/latest/download/tldr-pages.de.zip
https://github.com/tldr-pages/tldr/releases/latest/download/tldr.sha256sums
```

This is a release-based distribution model, not a git-clone model. The mirror is configurable:
```toml
# config.toml
[cache]
mirror = "https://github.com/tldr-pages/tldr/releases/latest/download"
```

### Multi-Language Handling

```rust
// src/config.rs:416-422
if cfg.cache.languages.is_empty() {
    util::get_languages_from_env(&mut cfg.cache.languages);
}
// English is ALWAYS downloaded and searched (appended last as fallback)
cfg.cache.languages.push("en".to_string());
```

Language detection from env vars (per CLIENT-SPECIFICATION.md):
```rust
// src/util.rs:89-113
// 1. Read $LANGUAGE (colon-separated list, e.g. "de:pl:en")
// 2. Read $LANG (e.g. "en_US.UTF-8")
// 3. Parse ll_CC format -> add both "ll_CC" and "ll"
// 4. Parse ll format -> add "ll"
```

Page resolution order: user-specified languages first, then English as fallback.

### Versioning

No per-page versioning. The entire content repo is versioned via GitHub Releases. Each release generates new zip archives with new SHA256 sums. Clients compare sums to decide if they need to re-download.

### Page Resolution (Find Algorithm)

```rust
// src/cache.rs:380-432  find()
// Given: name, languages[], platform
// 1. Try: pages.{lang}/{platform}/{name}.md  (exact platform)
// 2. Fallback: pages.{lang}/common/{name}.md  (always searched)
// 3. Fallback: pages.{lang}/{other_platform}/{name}.md  (alphabetical)
// For each step, try languages in user-specified order, then English
```

This means if you're on Linux and `tar` only has a `common` page, you still get it. If it has both `linux` and `common`, the `linux` one wins.

---

## C. Terminal Rendering

**File:** `src/output.rs` (501 lines)

### The Rendering Pipeline

tlrc uses a **line-by-line streaming renderer** that processes markdown on-the-fly:

```rust
// src/output.rs:477-500  render()
fn render(&mut self) -> Result<()> {
    while self.next_line()? != 0 {
        if self.current_line.starts_with("# ")  { self.add_title()?;   }
        else if self.current_line.starts_with("> ") { self.add_desc()?;   }
        else if self.current_line.starts_with("- ") { self.add_bullet()?; }
        else if self.current_line.starts_with('`')   { self.add_example()?; }
        else if whitespace                         { self.add_newline()?; }
        else { return Err(parse_error); }
    }
    self.add_newline()?;
    Ok(self.stdout.flush()?)
}
```

The tldr page format is a strict subset of markdown:
- `# title` — Command name
- `> description` — What the command does
- `- bullet` — Example description
- `` `example` `` — The actual command
- Empty lines — Spacing

### Color/Formatting System

**Crate: `yansi`** (not `colored`, not `termcolor`, not `crossterm`)

```rust
// src/output.rs:22-30
struct RenderStyles {
    title: Style,       // magenta, bold
    desc: Style,        // magenta
    bullet: Style,      // green
    example: Style,     // cyan
    url: Style,         // red, italic
    inline_code: Style, // yellow, italic
    placeholder: Style, // red, italic
}
```

All 7 style elements are fully configurable via TOML:
```toml
[style.title]
color = "magenta"
bold = true
# Also: background, underline, italic, dim, strikethrough
# Plus 256-color and RGB support
```

Color initialization respects `NO_COLOR`:
```rust
// src/util.rs:116-128
pub fn init_color(color_mode: ColorChoice) {
    match color_mode {
        ColorChoice::Always => {}
        ColorChoice::Never => yansi::disable(),
        ColorChoice::Auto => {
            let no_color = env::var_os("NO_COLOR").is_some_and(|x| !x.is_empty());
            if no_color || !io::stdout().is_terminal() || !io::stderr().is_terminal() {
                yansi::disable();
            }
        }
    }
}
```

### Syntax Highlighting (within lines)

Three inline highlighting functions, all using string splitting:

1. **`hl_code()`** — Highlights text between backticks (`` ` ``):
   ```rust
   // Split on '`', highlight odd-indexed parts
   "use `git log` to see" -> "use " + yellow_italic("git log") + " to see"
   ```

2. **`hl_url()`** — Highlights URLs in angle brackets:
   ```rust
   // Split on "<http", find closing ">"
   "<https://example.com>" -> red_italic("https://example.com")
   ```

3. **`hl_placeholder()`** — Highlights `{{placeholder}}` in examples:
   ```rust
   // Split on "{{", find "}}" (using rsplit for }}} edge cases)
   // Special: handles option selection {{[-s|--long]}}
   "tar -czf {{archive.tar.gz}} {{file1 file2 ...}}"
   // -> red_italic("archive.tar.gz") and red_italic("file1 file2 ...")
   ```

The option placeholder logic is particularly clever:
```rust
// src/output.rs:157-174
if self.cfg.output.option_style != OptionStyle::Both
    && inside.starts_with('[') && inside.ends_with(']')
    && let Some((short, long)) = inside.split_once('|')
{
    if self.cfg.output.option_style == OptionStyle::Short {
        write_paint!(buf, &short[1..].paint(style_normal)); // "-s"
    } else {
        write_paint!(buf, &long[..long.len() - 1].paint(style_normal)); // "--long"
    }
}
```

### Line Wrapping

```rust
// src/output.rs:187-273  splitln()
// Only wraps Desc and Bullet lines (NOT examples — commands should not be broken)
// Uses unicode-width for correct CJK width calculation
// Handles wrapping across backtick-highlighted regions
// Inserts style reset/reapply across line breaks (prevents bg color bleeding)
```

Terminal width detection:
```rust
// src/output.rs:293-294
max_len: NonZero::new(cfg.output.line_length)
    .or_else(|| terminal_size().and_then(|x| NonZero::new(x.0.0 as usize))),
```

### Output Buffering

Uses `BufWriter<io::StdoutLock<'static>>` for stdout — writes are buffered until flush. This is important for performance when printing many pages or search results.

### Custom `write_paint!` Macro

```rust
// src/output.rs:55-60
macro_rules! write_paint {
    ($buf:expr, $what:expr) => {
        let _ = write!($buf, "{}", $what);
    };
}
```

Appends `yansi::Painted` values to a `String` without allocation. The `yansi` library's `Paint` writes ANSI codes directly into the buffer. This avoids creating intermediate `String`s for each highlighted segment.

---

## D. The `tldr` -> `snip` Connection

### The Fundamental Insight

| | tldr | snip |
|---|---|---|
| **Problem** | "How do I use this STANDARD tool?" | "How do I run THIS project's commands?" |
| **Scope** | ~3,000 well-known CLI tools | Every project's unique commands |
| **Content source** | Central GitHub repo | Local `.snip.yml` files |
| **Content author** | Community contributors | Project maintainers |
| **Discovery** | `tldr tar` | `snip` (in any project dir) |

tldr solves the **bottom-up** problem (learn standard tools). snip solves the **top-down** problem (discover project-specific commands). They're complementary, not competing.

### Community Snippet Registry for snip?

**Should snip have one?** Yes, but with critical differences from tldr's model:

#### What tldr does RIGHT (steal these):
1. **Plain markdown content** — Dead simple to author and review
2. **GitHub-based PR workflow** — Leverages existing GitHub accounts
3. **Zip-based distribution** — Efficient, cacheable, verifiable
4. **Multi-language support** — Shows the value of i18n even in CLI tools
5. **Client spec document** — `CLIENT-SPECIFICATION.md` lets anyone build a client

#### What snip should do DIFFERENTLY:
1. **Don't require a central registry at launch.** Start with local-only. A registry can come later.
2. **Project-scoped, not global.** snip snippets belong to projects. A registry would be per-ecosystem (npm packages, crates, PyPI, etc.) not a single monorepo.
3. **Declarative config-first.** tldr pages are hand-written prose. snip snippets are machine-parseable commands that can actually be executed.
4. **No platform fallbacks needed.** snip commands are project-specific, not OS-specific (usually).

#### Proposed Registry Architecture for snip:

```
Phase 1 (local only):
  .snip.yml in project root -> snip reads it -> done

Phase 2 (ecosystem registries):
  For npm packages: snip pulls from registry.npmjs.org/snip-commands/{package}
  For crates: snip pulls from crates.io/api/v1/crates/{name}/snip
  These are OPTIONAL overlays on top of local .snip.yml

Phase 3 (community contributions):
  GitHub repo per ecosystem:
    snip-commands/npm/
      express.json  -> {"commands": [...]}
      next.json     -> {"commands": [...]}
    snip-commands/crates/
      tokio.json    -> {"commands": [...]}
  PR-based contribution model (same as tldr)
```

The key insight from tldr: **the content repo should be SEPARATE from the client.** This lets multiple clients exist and the content be useful independently.

---

## E. Rust-Specific Techniques

### Crate Dependency Analysis

| Crate | Version | Purpose | snip should use? |
|-------|---------|---------|-----------------|
| `clap` | 4.6 | CLI arg parsing (derive) | **YES** — industry standard |
| `dirs` | 6.0 | XDG/cache/config dirs | **YES** — cross-platform paths |
| `yansi` | 1.0 | Terminal colors (zero-dep) | **YES** — lighter than `colored` |
| `terminal_size` | 0.4 | Get terminal width | **MAYBE** — only if wrapping needed |
| `unicode-width` | 0.2 | Correct CJK char width | **YES** — if wrapping needed |
| `ureq` | 3.3 | HTTP client (rustls) | **MAYBE** — for registry |
| `rustls` + `ring` | 0.23/0.17 | TLS | Only with HTTP |
| `zip` | 8.6 | Zip extraction | Only for zip-based registry |
| `toml` | 1.1 | Config file parsing | **YES** — if using TOML config |
| `serde` | 1.0 | Serialization | **YES** — everywhere |
| `once_cell` | 1.21 | Lazy initialization | **YES** — `OnceCell` pattern |
| `log` | 0.4 | Logging facade | **YES** |
| `assert_cmd` | 2.2 | CLI integration tests | **YES** — for testing |

### Notable Rust Patterns

#### 1. Zero-cost color with `yansi`
`yansi` is a "zero-dependency" styling library. It writes ANSI escape codes at runtime and can be globally disabled. When disabled, the overhead is a single branch check. This is lighter than `colored` (which pulls in `lazy_static`/`once_cell`) and `termcolor` (which is more complex).

```rust
// src/util.rs:116-128 — Global enable/disable
yansi::disable(); // when NO_COLOR or piped
```

#### 2. BufWriter on StdoutLock
```rust
// src/output.rs:290
stdout: BufWriter<io::StdoutLock<'static>>,
```
Locking stdout prevents interleaved output. BufWriter amortizes syscalls. The `'static` lifetime is safe because `stdout()` returns a static reference.

#### 3. OnceCell for Lazy Platform Discovery
```rust
// src/cache.rs:26
platforms: OnceCell<Vec<String>>,
```
Platform directories are discovered once (first access) and cached. This avoids repeated `read_dir` calls. Uses `once_cell::unsync::OnceCell` (not `std::sync::OnceLock`) because `Cache` is not `Sync`.

#### 4. Builder Pattern for HTTP Agent
```rust
// src/cache.rs:84-99
let agent = ureq::Agent::config_builder()
    .user_agent(USER_AGENT)
    .timeout_resolve(HTTP_TIMEOUT)
    .timeout_connect(HTTP_TIMEOUT)
    .tls_config(TlsConfig::builder()
        .unversioned_rustls_crypto_provider(...)
        .root_certs(RootCerts::PlatformVerifier)
        .build())
    .build()
    .into();
```
Uses `platform_verifier` for TLS (trusts system certs, not bundled). Separate timeouts for resolve and connect (no global read timeout — was causing issues for some users).

#### 5. Typed Error Kinds with Exit Codes
```rust
// src/error.rs:14-20, 108-119
pub enum ErrorKind { ParseToml, ParsePage, Download, Io, Other }
impl Error {
    pub fn exit_code(self) -> ExitCode {
        match self.kind {
            Io | Other => 1,
            ParseToml => 3,
            Download => 4,
            ParsePage => 5,
        }.into()
    }
}
```
Each error type maps to a unique exit code. This is excellent for scripting.

#### 6. Descriptive Error Chaining
```rust
// src/error.rs:55-62
pub fn describe<T>(mut self, description: T) -> Self
where T: Display {
    self.message = format!("{} {description}", self.message);
    self
}
// Usage:
Err(Error::new("page not found.").describe(Error::desc_page_does_not_exist(cache_age?)))
```
Builder-style error enrichment without allocations until the error actually occurs.

#### 7. Build Script for Version String
```rust
// build.rs:6
const CLIENT_SPEC: &str = "2.3";
// Generates: "v1.13.1 (implementing the tldr client specification v2.3)"
```
Embeds the spec version at compile time. Debug builds also include git commit hash.

#### 8. Custom Logger (no dependency)
```rust
// src/util.rs:15-57
pub struct Logger;
impl log::Log for Logger {
    fn log(&self, record: &log::Record) {
        // Color-coded level prefix, writes to stderr
    }
}
```
A 40-line logger implementation. No need for `env_logger` or `pretty-env-logger`.

#### 9. Inline Status Messages (info_start/info_end macros)
```rust
// src/util.rs:61-86
macro_rules! info_start {
    ($($arg:tt)*) => {
        // In non-verbose mode: write to stderr without newline
        // In verbose mode: use normal log::info!
    };
}
macro_rules! info_end {
    ($($arg:tt)*) => {
        // Complete the line started by info_start
    };
}
// Produces: "info: downloading 'tldr-pages.en.zip'... 1.23 MiB"
```
This creates progress-indicator-style messages during downloads: the "downloading..." part appears first, then the size is appended. In verbose mode, it falls back to standard `log::info!` messages.

#### 10. Release Profile Optimization
```toml
# Cargo.toml:48-53
[profile.release]
lto = true
strip = true
codegen-units = 1
panic = "abort"
opt-level = 3
```
Maximum optimization: LTO, symbol stripping, single codegen unit, no unwinding. The resulting binary is tiny.

### Codebase Structure

```
src/
  main.rs     (206 lines) — Entry point, orchestration, config merging
  cache.rs    (936 lines) — Download, extract, find, search, list, age
  output.rs   (501 lines) — Markdown rendering, syntax highlighting, line wrapping
  config.rs   (480 lines) — TOML config, style definitions, color types
  args.rs     (136 lines) — clap CLI definition
  error.rs    (136 lines) — Error types, exit codes
  util.rs     (310 lines) — Logger, color init, helpers, tests
  total:      ~2,705 lines
```

The separation is clean:
- `cache.rs` = data layer
- `output.rs` = presentation layer
- `config.rs` = configuration layer
- `main.rs` = orchestration
- `args.rs` + `error.rs` + `util.rs` = infrastructure

---

## F. What We Should STEAL for snip

### 1. Terminal Rendering: `yansi` + Custom Renderer

**STEAL:** The entire rendering approach from `src/output.rs`.

```rust
// snip should have a SnipRenderer with:
// - Configurable styles (title, description, command, placeholder, tag)
// - Line wrapping with unicode-width awareness
// - ANSI color disable via NO_COLOR / piped detection
// - BufWriter<StdoutLock> for buffered output
// - Zero-dep yansi instead of heavier alternatives
```

Specific techniques to copy:
- `RenderStyles` struct with `yansi::Style` for each element (`output.rs:22-30`)
- `init_color()` respecting `NO_COLOR` and `is_terminal()` (`util.rs:116-128`)
- `splitln()` for word-wrapping descriptions (`output.rs:187-273`)
- `write_paint!` macro for zero-alloc highlighting (`output.rs:55-60`)
- `OutputColor` enum with named, 256-color, RGB, and hex support (`config.rs:54-79`)

### 2. Config System: TOML with Full Defaults

**STEAL:** The config pattern from `src/config.rs`.

```rust
// snip should support:
// ~/.config/snip/config.toml  (or platform equivalent)
// $SNIP_CONFIG env var override
// --config CLI override
// All with sensible defaults (zero-config required)
```

Specific techniques:
- `#[serde(default)]` on all config structs for zero-config operation (`config.rs:108`)
- `Config::locate()` using `dirs::config_dir()` (`config.rs:436-446`)
- `--gen-config` to print default config (`config.rs:460-473`)
- `hex_to_rgb` custom deserializer for color values (`config.rs:18-52`)

### 3. Cache/Registry Pattern

**STEAL:** The download-and-verify pattern from `src/cache.rs`.

For snip's future community registry:
- SHA256 verification of downloads (`cache.rs:126-138`)
- Incremental updates via checksum comparison (`cache.rs:120-123`)
- Configurable mirror URL (`config.rs:247`)
- `dirs::cache_dir()` for storage (`cache.rs:38-40`)
- Age tracking via file mtime (`cache.rs:723-739`)
- Deferred auto-update (show first, update after) (`main.rs:137-139`)

### 4. Error Handling

**STEAL:** Typed error kinds with unique exit codes.

```rust
// snip should have:
enum SnipErrorKind { Config, Parse, Io, NotFound, Network }
// Each mapping to a unique exit code for scripting
```

And the `.describe()` builder pattern for error context chaining (`error.rs:55-62`).

### 5. CLI Design

**STEAL:**
- `clap` with derive macros (`args.rs`)
- `arg_required_else_help = true` — shows help when run with no args
- `ArgGroup` for mutually exclusive operations (`args.rs:20`)
- `override_usage` for custom help formatting (`args.rs:21`)
- Shell completions for bash, zsh, fish (`completions/`)
- Man page (`tldr.1`)
- `--verbose` with count (specifiable multiple times) (`args.rs:122`)

### 6. Distribution Strategy

**STEAL:** The package metadata approach.

```toml
# Cargo.toml:55-66
[package.metadata.deb]
section = "utils"
assets = [
    ["target/release/snip", "usr/bin/", "755"],
    ["snip.1", "usr/share/man/man1/", "644"],
    ["completions/snip.bash", "usr/share/bash-completion/completions/snip", "644"],
    ...
]
```

### 7. Testing Strategy

**STEAL:** Integration tests using `assert_cmd` + golden files.

```rust
// tests/tests.rs:17-20
fn snip(cfg: &str, page: &str) -> Command {
    let mut cmd = Command::cargo_bin("snip").unwrap();
    cmd.args(["--config", cfg, "--render", page]);
    cmd
}
// Golden file comparison: actual output vs expected file
```

### 8. Page Format (Adapted)

**ADAPT:** tldr's markdown format for snip's display:

```markdown
# project-name

> Short description of the project.

- Build the project:

`cargo build --release`

- Run tests:

`cargo test {{test_name}}`

- Deploy to production:

`snip deploy --env {{environment}}`
```

snip already has its own format (YAML), but the RENDERED output should look like tldr's: colored title, description, indented bullets with commands, and highlighted placeholders.

### 9. The "Show First, Update Later" Pattern

**STEAL:** Deferred auto-update (`main.rs:137-139, 198-202`).

For snip, this means: if a `.snip.yml` references a remote snippet registry, show the local cached version immediately, then update in the background. The user never waits.

### 10. The Content/Client Separation

**STEAL:** Keep snip's snippet format documented in a spec file, so:
- Other tools can parse `.snip.yml`
- Other clients can be built
- The format can outlive any single implementation

---

## Summary: Key Metrics

| Metric | tlrc Value |
|--------|-----------|
| Total source lines | ~2,705 |
| Number of source files | 7 |
| External dependencies | 12 |
| Build dependencies | 0 (pure Rust) |
| Release binary size | ~500KB (LTO + strip) |
| Config format | TOML |
| Content format | Markdown (strict subset) |
| Terminal color crate | `yansi` (zero-dep) |
| HTTP crate | `ureq` (rustls) |
| CLI parsing | `clap` (derive) |
| Test framework | `assert_cmd` + golden files |
| Platforms supported | Linux, macOS, Windows, BSD, Android |
| Shell completions | bash, zsh, fish |
| Man page | Yes |
| Package managers | 10+ |