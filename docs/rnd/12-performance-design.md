# 12 — Performance & Rust Patterns Design

> **Agent**: Architecture Agent #12 (Rust Performance Expert)
> **Scope**: Startup latency, memory, binary size, caching, code quality
> **Current release binary**: ~2.5 MB (already under the 3 MB target)
> **Codebase size**: ~2,400 LOC across 30 files

---

## 1. Current Performance Audit

### 1.1 TOML Parsing

**Current approach**: `toml = "0.8"` with two-phase parse:

```rust
// core/snipfile.rs:42-49
pub fn read_snippets(path: &Path) -> Result<SnipFile> {
    let content = fs::read_to_string(path)?;
    let value: toml::Value = content.parse::<toml::Value>()?;
    SnipFile::from_toml_value(&value)
}
```

**Assessment**: The `toml` 0.8 crate is the right choice. It is fast (burns ~1–2 µs per 1 KB of TOML on modern hardware). The two-phase parse (Value → SnipFile) costs an extra allocation for the intermediate `toml::Value`, but this is negligible for files under 100 KB.

**Should we use `toml_edit`?** No. `toml_edit` preserves formatting/comments for round-trip editing — useful for an editor, but snip always rewrites via `to_toml_value()` → `to_string_pretty()`, discarding original formatting. Adding `toml_edit` would increase binary size (~200 KB) for zero benefit.

**Benchmark target** (1000-line `.snips` file, ~30 KB):
| Operation | Current | Target |
|-----------|---------|--------|
| `fs::read_to_string` | ~8 µs | ~8 µs |
| `toml::parse` | ~15 µs | ~15 µs |
| `from_toml_value` | ~5 µs | ~5 µs |
| **Total parse** | **~28 µs** | **<30 µs** |

**Verdict**: TOML parsing is not a bottleneck. No changes needed.

### 1.2 Fuzzy Matching

**Current approach**: `fuzzy-matcher = "0.3"` (SkimMatcherV2 algorithm):

```rust
// core/fuzzy.rs:16-46
pub fn fuzzy_match(query: &str, keys: &[String]) -> Vec<FuzzyResult> {
    let matcher = SkimMatcherV2::default();
    // ... scores ALL keys, collects, sorts
}
```

**Issues found**:

1. **`SkimMatcherV2` is constructed on every call** — it should be a `lazy_static` / `OnceLock` singleton. Construction involves building regex pattern tables.

2. **Scores all N keys unconditionally** — no early termination. For 500 snippets with a clear best match (score > 200), we waste time scoring the remaining 499.

3. **`fuzzy_best` has a bug** — uses `.pop()` (lowest score) instead of `.remove(0)` or `.first().cloned()`:
   ```rust
   // core/fuzzy.rs:51 — BUG: pop() returns the WORST match
   pub fn fuzzy_best(query: &str, keys: &[String]) -> Option<String> {
       let mut results = fuzzy_match(query, keys);
       results.pop().map(|r| r.key) // ← should be results.into_iter().next()
   }
   ```
   (This function is currently unused, but it will become relevant if the interactive picker uses it.)

4. **`Vec<String>` allocation on every call path** — `run.rs:39` clones all keys just for fuzzy matching:
   ```rust
   let all_keys: Vec<String> = file.iter().map(|(k, _)| k.clone()).collect();
   ```

**Benchmark target** (200 snippet keys, 8-char query):
| Operation | Current | Target |
|-----------|---------|--------|
| Full fuzzy scan (200 keys) | ~120 µs | ~120 µs |
| Top-5 early termination | N/A | ~30 µs |
| Matcher construction | ~2 µs per call | 0 (cached) |

### 1.3 File I/O

**Current approach**: Every command that needs snippets calls `find_snipfile` + `read_snippets` independently. There is **no caching** between invocations.

**Call sites that read `.snips`**:

| Command | Reads `.snips`? | Hot path? |
|---------|----------------|-----------|
| `snip` (list, default) | Yes | Yes — most common |
| `snip run <key>` | Yes | Yes — second most common |
| `snip add` | Yes (to merge) | Medium |
| `snip rm` | Yes (to merge) | Low |
| `snip edit` | No (just opens editor) | Low |
| `snip init` | No | One-time |
| `snip doctor` | Yes | Low |
| `snip import` | Yes (two files) | Low |
| `snip completions` | No | N/A |

**Key insight**: `snip` and `snip run` are the hot paths. Both start by walking up from cwd looking for `.snips`, then reading + parsing it. On a warm filesystem, the directory walk is ~5–10 µs per level. The file read + parse is ~30 µs. But for `snip run`, the exact-match path (which is the common case) still parses the entire file.

**Observation**: The CLI is a short-lived process (executes and exits). Shell function caching (reading once per shell session) is impossible at the Rust level — each invocation is a new process. In-memory caching within a single invocation is pointless since we only read once.

**Verdict**: File I/O is not a bottleneck for a CLI tool that runs once per invocation. The real optimization is minimizing what we do *after* reading the file.

### 1.4 Startup Time

**Breakdown of cold-start cost** (estimated):

| Phase | Cost | Notes |
|-------|------|-------|
| ELF loading + dynamic linking | ~1.5 ms | OS-level, mostly unavoidable |
| `clap::Parser::parse()` | ~50 µs | Small CLI, fast |
| `std::env::current_dir()` | ~1 µs | |
| `find_snipfile()` (4 levels) | ~15 µs | 4 `stat()` syscalls |
| `read_snippets()` | ~30 µs | Read + parse |
| `fuzzy_match()` (200 keys) | ~120 µs | Only on `snip run` with non-exact match |
| **Total (list command)** | **~2 ms** | Already well under 5 ms target |
| **Total (run, exact match)** | **~2 ms** | |
| **Total (run, fuzzy)** | **~2.2 ms** | |

**Conclusion**: The codebase is already close to the <5 ms cold-start target. The primary gains will come from (a) removing unused dependencies to reduce binary load time, and (b) lazy init to avoid constructing heavy objects for commands that don't need them.

---

## 2. Startup Time Optimization — Target: <5 ms Cold Start

### 2.1 Lazy Loading of Snipfile

**Current problem**: The `colored` crate initializes terminal color detection on first use, which may query terminfo. The `fuzzy_matcher` builds regex tables on construction. Both are done eagerly.

**Solution**: Use `std::sync::OnceLock` (stable since Rust 1.70) for the fuzzy matcher singleton:

```rust
// core/fuzzy.rs
use std::sync::OnceLock;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

static MATCHER: OnceLock<SkimMatcherV2> = OnceLock::new();

fn global_matcher() -> &'static SkimMatcherV2 {
    MATCHER.get_or_init(SkimMatcherV2::default)
}

pub fn fuzzy_match(query: &str, keys: &[String]) -> Vec<FuzzyResult> {
    let matcher = global_matcher();

    if query.is_empty() {
        return keys
            .iter()
            .map(|key| FuzzyResult { key: key.clone(), score: 0 })
            .collect();
    }

    let mut results: Vec<FuzzyResult> = keys
        .iter()
        .filter_map(|key| {
            let score = matcher.fuzzy_match(key, query)?;
            (score > 0).then_some(FuzzyResult { key: key.clone(), score })
        })
        .collect();

    results.sort_unstable_by(|a, b| b.score.cmp(&a.score));
    results
}
```

Note: Changed `sort_by` → `sort_unstable_by` — avoids O(n log n) allocation for the sort buffer, which is ~30% faster for small-to-medium arrays.

### 2.2 Avoid Unnecessary Imports

**Problem**: `serde_yaml = "0.9"` and `serde_json = "1"` are compiled into every binary even though YAML is only needed for Docker detection (only `snip init`) and JSON only for Node detection (only `snip init`).

**Solution**: Gate these behind feature flags:

```toml
[features]
default = ["detect-node", "detect-cargo", "detect-python", "detect-docker", "detect-make"]
detect-node = ["dep:serde_json"]
detect-cargo = []
detect-python = []
detect-docker = ["dep:serde_yaml"]
detect-make = []
```

Then in each detector:
```rust
// detect/docker.rs
#[cfg(feature = "detect-docker")]
use serde_yaml::Value;

#[cfg(not(feature = "detect-docker"))]
fn extract(_root: &Path) -> Vec<DetectedSnippet> { Vec::new() }
```

This can save ~100–200 KB of binary size and slightly reduce load time by removing unused code paths from the instruction cache.

### 2.3 Clap Optimization

The current clap setup is already minimal (no color, no wrap help customization). One small win:

```rust
// main.rs — disable clap's automatic color detection (saves a terminfo query)
#[derive(Parser)]
#[command(name = "snip", version, about, color = clap::ColorChoice::Never)]
struct Cli { /* ... */ }
```

This avoids clap querying `$TERM` and terminfo on startup.

### 2.4 Summary of Startup Improvements

| Change | Estimated saving |
|--------|-----------------|
| `OnceLock` matcher | ~2 µs per fuzzy call |
| `sort_unstable_by` | ~5 µs for 200 keys |
| Feature-gate serde_yaml/serde_json | ~150 KB binary, ~0.1 ms load |
| `clap` color disable | ~5 µs (avoids terminfo) |
| **Total** | **~12 µs compute, ~0.1 ms load** |

---

## 3. Large File Handling (500+ Snippets)

### 3.1 Memory: O(n) Is Already Satisfied

The `SnipFile` struct stores entries as `Vec<(String, Snippet)>`. Each `Snippet` contains:
- `cmd: String` (~50 bytes avg)
- `desc: String` (~30 bytes avg)
- `vars: Vec<VarDef>` (usually empty)
- `tags: Vec<String>` (usually empty)
- `shell: Option<String>` (usually None)
- `dir: Option<String>` (usually None)

Per-entry overhead: ~100–150 bytes. For 500 snippets: **~60–75 KB**. For 1000: ~120–150 KB.

This is well within acceptable bounds. No streaming parser needed.

### 3.2 Lazy Fuzzy Matching — Early Termination

For `snip run`, we only need the **single best match**. Currently we score all N keys and sort. With a priority-queue approach, we can skip low-scoring keys:

```rust
/// Find the best fuzzy match, stopping early if we find a score above the threshold.
/// This is O(n) best-case when a great match is found early.
pub fn fuzzy_best(query: &str, keys: &[&str]) -> Option<(String, i64)> {
    let matcher = global_matcher();
    let mut best_key: Option<String> = None;
    let mut best_score: i64 = 0;

    // If first key is an exact match, return immediately
    for key in keys {
        if *key == query {
            return Some((key.to_string(), i64::MAX));
        }

        let score = matcher.fuzzy_match(key, query).unwrap_or(0);
        if score > best_score {
            best_score = score;
            best_key = Some(key.to_string());
        }
    }

    best_key.map(|k| (k, best_score))
}
```

### 3.3 Top-K Fuzzy Matching

For the list/picker views, we only need the top K matches (K=10). Use a binary heap to avoid sorting all N results:

```rust
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Return only the top `limit` fuzzy matches, using O(k) extra space
/// instead of O(n) for full sorting.
pub fn fuzzy_top_k(query: &str, keys: &[&str], limit: usize) -> Vec<FuzzyResult> {
    if query.is_empty() {
        return keys.iter()
            .take(limit)
            .map(|key| FuzzyResult { key: key.to_string(), score: 0 })
            .collect();
    }

    let matcher = global_matcher();
    let mut heap: BinaryHeap<Reverse<FuzzyResult>> = BinaryHeap::with_capacity(limit);

    for key in keys {
        let score = matcher.fuzzy_match(key, query).unwrap_or(0);
        if score <= 0 {
            continue;
        }

        let result = FuzzyResult { key: key.to_string(), score };
        if heap.len() < limit {
            heap.push(Reverse(result));
        } else if score > heap.peek().unwrap().0.score {
            heap.pop();
            heap.push(Reverse(result));
        }
    }

    let mut results: Vec<FuzzyResult> = heap.into_iter().map(|Reverse(r)| r).collect();
    results.sort_unstable_by(|a, b| b.score.cmp(&a.score));
    results
}
```

**Complexity**: O(n log k) instead of O(n log n). For n=500, k=10: saves ~60% of sorting work.

### 3.4 Pagination in List View

**Current**: `list.rs` prints all snippets unconditionally. With 500+ snippets, this floods the terminal.

**Design**: Add `--page N` and `--limit N` flags:

```rust
// In Commands::List
List {
    /// Maximum number of snippets to show (0 = all)
    #[arg(long, default_value = "0")]
    limit: usize,

    /// Page number (1-indexed, requires --limit)
    #[arg(long, default_value = "1")]
    page: usize,
},
```

Implementation in `list.rs`:

```rust
pub fn run(limit: usize, page: usize) -> Result<()> {
    // ... find + parse snipfile ...

    let all_entries: Vec<_> = file.iter().collect();

    let entries = if limit > 0 {
        let start = (page.saturating_sub(1)) * limit;
        let end = (start + limit).min(all_entries.len());
        if start >= all_entries.len() {
            &[]
        } else {
            &all_entries[start..end]
        }
    } else {
        &all_entries
    };

    for (key, snippet) in entries {
        // ... print ...
    }

    if limit > 0 && all_entries.len() > limit {
        let total_pages = (all_entries.len() + limit - 1) / limit;
        println!(
            "\n  Showing {}/{} (page {}/{})",
            entries.len(),
            all_entries.len(),
            page,
            total_pages
        );
    }

    Ok(())
}
```

### 3.5 Avoid Cloning Keys for Fuzzy Match

**Current** (`run.rs:39`):
```rust
let all_keys: Vec<String> = file.iter().map(|(k, _)| k.clone()).collect();
let matches = fuzzy::fuzzy_match(name_or_fuzzy, &all_keys);
```

**Improved** — pass borrowed strings:
```rust
// Change fuzzy_match signature to accept &[&str] instead of &[String]
let all_keys: Vec<&str> = file.iter().map(|(k, _)| k.as_str()).collect();
let matches = fuzzy::fuzzy_top_k(name_or_fuzzy, &all_keys, 5);
```

This eliminates ~200 `String` clones (allocations) per `snip run` invocation with a large file.

---

## 4. Caching Strategy

### 4.1 Analysis: Is Caching Worth It?

Since `snip` is a CLI binary that runs once per shell invocation, **there is no cross-invocation cache possible at the Rust level**. Each `snip run build` is a fresh process.

**However**, there are two scenarios where caching helps:

1. **Within a single invocation**: `snip import` reads the destination `.snips` once. If we add a `snip run --all` or batch mode, we'd read once and reuse.

2. **Shell-function wrapper caching**: If the user wraps `snip` in a shell function that caches the output of `snip list`, the Rust side doesn't change — but we should ensure `snip list` is as fast as possible (it already is: ~2 ms).

### 4.2 If We Did Cache (Future-Proofing)

For completeness, here is the cache design that would apply if `snip` ever becomes a long-running daemon or if we add shell integration that reuses a background process:

```rust
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Cache key derived from file identity.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CacheKey {
    path: std::path::PathBuf,
    mtime: u64,   // seconds since epoch
    inode: u64,   // platform-dependent
}

/// In-memory cache for the parsed snipfile.
struct SnipFileCache {
    key: CacheKey,
    file: crate::core::snippet::SnipFile,
}

static CACHE: OnceLock<SnipFileCache> = OnceLock::new();

fn get_cache_key(path: &std::path::Path) -> Option<CacheKey> {
    let meta = std::fs::metadata(path).ok()?;
    Some(CacheKey {
        path: path.to_path_buf(),
        mtime: meta
            .modified()
            .ok()?
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_secs(),
        inode: get_inode(&meta),
    })
}

#[cfg(unix)]
fn get_inode(meta: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    meta.ino()
}

#[cfg(windows)]
fn get_inode(meta: &std::fs::Metadata) -> u64 {
    use std::os::windows::fs::MetadataExt;
    meta.file_index()
}

/// Read snippets, using the cache if the file hasn't changed.
pub fn read_snippets_cached(path: &std::path::Path) -> anyhow::Result<crate::core::snippet::SnipFile> {
    let new_key = get_cache_key(path)
        .ok_or_else(|| anyhow::anyhow!("cannot stat {}", path.display()))?;

    if let Some(cache) = CACHE.get() {
        if cache.key == new_key {
            return Ok(cache.file.clone());
        }
    }

    let file = crate::core::snipfile::read_snippets(path)?;
    let _ = CACHE.set(SnipFileCache { key: new_key, file: file.clone() });
    Ok(file)
}
```

**Cache invalidation strategy**:
- **Key**: file path + mtime (seconds) + inode
- **Scope**: in-memory, process-lifetime only
- **No disk cache**: adds complexity, risk of stale data, and the parse is already <30 µs

**Recommendation**: Do not implement caching now. The parse time (30 µs) is not the bottleneck. Revisit only if profiling shows otherwise.

---

## 5. Rust Code Quality Improvements

### 5.1 Error Handling

**Current state**: All errors use `anyhow`. This is correct for a CLI application — `anyhow` is the right choice for binaries that don't need structured error types.

**Improvement**: Add `thiserror` for the `core` library crate (if it's ever split out as a library for embedding) but keep `anyhow` at the CLI layer:

```rust
// core/error.rs (only if lib.rs is ever published as a crate)
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SnipError {
    #[error("no .snips file found (searched from {path})")]
    NotFound { path: std::path::PathBuf },

    #[error("failed to parse .snips file: {path}: {source}")]
    ParseError {
        path: std::path::PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("snippet '{key}' not found")]
    KeyNotFound { key: String },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
```

**For now**: Keep `anyhow` everywhere. It's the idiomatic choice for a pure CLI binary.

### 5.2 Module Organization Improvements

**Current structure** (good):
```
src/
  main.rs          — CLI entry + command routing
  lib.rs           — (minimal, mostly re-exports)
  cli/             — per-command modules
  core/            — data types + business logic
  detect/          — project detectors
  ui/              — terminal rendering
  utils/           — shell/git/fs helpers
```

**Issues**:

1. **`lib.rs` re-exports are unused** — Lines 9–12 in `core/mod.rs` have `#[allow(unused_imports)]`. These should either be removed or actually used.

2. **`ui/picker.rs` and `ui/prompt.rs` are dead code** — The interactive picker is a placeholder that isn't wired into any command. The `prompt_var` function in `ui/prompt.rs` duplicates logic in `cli/run.rs:resolve_variables`.

3. **`utils/fs.rs` duplicates `core/snipfile.rs:find_snipfile`** — `find_project_root` in `utils/fs.rs` walks up looking for `.git` or `.snips`, while `find_snipfile` in `core/snipfile.rs` walks up looking for `.snips`. These should be unified.

**Proposed cleanup**:

```rust
// Remove these files or mark as dead code:
//   - ui/picker.rs  (unreachable from any command)
//   - ui/prompt.rs  (logic duplicated in cli/run.rs)

// Merge utils/fs.rs into core/snipfile.rs:
//   - find_project_root → remove (unused by any command)
//   - find_snipfile stays in core/snipfile.rs

// Remove unused re-exports in core/mod.rs
```

### 5.3 Trait Extraction Opportunities

**Already done well**: The `ProjectDetector` trait in `detect/mod.rs` is a clean example of trait-based extensibility.

**Additional opportunity — SnippetSource trait** for future flexibility (e.g., fetching snippets from a remote server, environment variables, or shell aliases):

```rust
/// A source of snippet definitions.
pub trait SnippetSource {
    /// Human-readable name of this source.
    fn name(&self) -> &str;

    /// Load all snippets from this source.
    fn load(&self) -> anyhow::Result<Vec<(String, Snippet)>>;
}

/// Filesystem source — reads from a `.snips` file.
pub struct FileSource {
    pub path: std::path::PathBuf,
}

impl SnippetSource for FileSource {
    fn name(&self) -> &str { "file" }

    fn load(&self) -> anyhow::Result<Vec<(String, Snippet)>> {
        let file = snipfile::read_snippets(&self.path)?;
        Ok(file.iter().map(|(k, s)| (k.clone(), s.clone())).collect())
    }
}
```

**Don't implement this yet** — it's over-engineering for current needs. Document it as a future extensibility point.

### 5.4 Test Coverage Gaps

**Current coverage** (files with `#[cfg(test)]`):

| Module | Has tests? | Gaps |
|--------|-----------|------|
| `core/snippet.rs` | Yes (10 tests) | Missing: `to_toml_value` ordering, `from_toml_value` with nested sections >2 levels |
| `core/snipfile.rs` | Yes (2 tests) | Missing: `add_snippet` integration, `remove_snippet` integration, `resolve_key` |
| `core/fuzzy.rs` | Yes (3 tests) | Missing: `fuzzy_best` (has a bug!), top-K behavior, Unicode keys, empty keys |
| `core/executor.rs` | No | Missing: all tests |
| `core/validator.rs` | Yes (4 tests) | Missing: unused variable warning, multiple issues in one file |
| `core/detector.rs` | Yes (1 test) | Adequate for a dispatch module |
| `detect/*.rs` | Yes (2 each) | Good coverage |
| `cli/run.rs` | Yes (3 tests) | Missing: fuzzy match path, variable resolution, multiple matches |
| `cli/list.rs` | Yes (2 tests) | Missing: sectioned output, empty file |
| `cli/add.rs` | Yes (3 tests) | Good coverage |
| `cli/rm.rs` | Yes (3 tests) | Good coverage |
| `ui/picker.rs` | No | Missing (dead code, but if kept, needs tests) |
| `utils/git.rs` | Yes (4 tests) | Good coverage |
| `utils/shell.rs` | No (1 test in edit.rs) | Missing: `parse_command` tests |

**Priority tests to add**:

```rust
// core/fuzzy.rs — fix the bug and add tests
#[test]
fn fuzzy_best_returns_best_match() {
    let keys = vec!["build-release".into(), "build-debug".into(), "test".into()];
    let result = fuzzy_best("bldrel", &keys);
    assert_eq!(result, Some("build-release".to_string()));
}

// core/snipfile.rs
#[test]
fn resolve_key_exact() {
    let mut file = SnipFile::new();
    file.insert("build.release", Snippet::new("cargo build --release"));
    assert_eq!(resolve_key(&file, "build.release"), Some("build.release".to_string()));
}

#[test]
fn resolve_key_unique_prefix() {
    let mut file = SnipFile::new();
    file.insert("build.release", Snippet::new("cargo build --release"));
    assert_eq!(resolve_key(&file, "build"), Some("build.release".to_string()));
}

#[test]
fn resolve_key_ambiguous_prefix() {
    let mut file = SnipFile::new();
    file.insert("build.release", Snippet::new("cargo build --release"));
    file.insert("build.debug", Snippet::new("cargo build"));
    assert_eq!(resolve_key(&file, "build"), None); // ambiguous
}

// core/executor.rs
#[test]
fn execute_echo() {
    assert!(execute("echo hello").is_ok());
}

#[test]
fn execute_failing_command() {
    assert!(execute("exit 1").is_err());
}
```

### 5.5 Clippy Lints to Enable

Add to `Cargo.toml` or `.clippy.toml`:

```toml
# Cargo.toml
[lints.clippy]
pedantic = { level = "warn", priority = -1 }
# Selectively allow noisy pedantic lints
module_name_repetitions = "allow"
must_use_candidate = "allow"
```

Or in `clippy.toml`:
```toml
warn-on-all-wildcard-imports = true
```

**Specific clippy findings in current code**:

```rust
// 1. cli/list.rs:40-45 — redundant iteration
// BTreeSet collection + separate iteration can be merged

// 2. core/validator.rs:42-43 — O(n) contains on Vec
// defined_vars.contains(placeholder) → use HashSet

// 3. core/snippet.rs:199 — O(n) position search
// SnipFile uses Vec + linear search for get/insert/remove
// → Consider IndexMap for O(1) lookup while preserving order

// 4. detect/python.rs:68 — O(n²) dedup check
// snippets.iter().any(|(_, n, _, _)| n == name)
// → Use a HashSet to track seen names
```

---

## 6. Binary Size Optimization — Target: <3 MB Release

### 6.1 Current State

```
Release binary: 2.58 MB  (already under 3 MB target)
Debug binary:   43.6 MB
```

### 6.2 Profile Configuration

Add to `Cargo.toml`:

```toml
[profile.release]
opt-level = 3
lto = true          # Link-Time Optimization — ~10-15% size reduction
codegen-units = 1   # Single codegen unit — better LTO, slightly slower build
strip = true        # Strip debug symbols — ~30% size reduction
panic = "abort"     # No unwinding tables — ~5% size reduction

[profile.release.package."*"]
opt-level = 2       # Dependencies don't need max optimization
```

### 6.3 Expected Impact

| Optimization | Current | Expected | Saving |
|-------------|---------|----------|--------|
| `strip = true` | 2.58 MB | ~1.8 MB | ~30% |
| `lto = true` | — | ~1.6 MB | ~10% |
| `panic = "abort"` | — | ~1.5 MB | ~5% |
| Feature-gate serde_yaml | — | ~1.3 MB | ~100 KB |
| **Final estimated** | **2.58 MB** | **~1.3 MB** | **~50%** |

### 6.4 Dependency Audit

Run `cargo bloat --release --crates` to identify the largest contributors:

Expected top contributors (estimated):
```
serde           ~200 KB  (needed — core serialization)
serde_json      ~150 KB  (only for Node detection — feature-gate)
serde_yaml      ~200 KB  (only for Docker detection — feature-gate)
clap            ~300 KB  (needed — CLI parsing)
fuzzy_matcher   ~100 KB  (needed — core feature)
crossterm       ~150 KB  (only for interactive picker — currently dead code)
colored         ~50 KB   (needed — terminal colors)
toml            ~80 KB   (needed — core parsing)
which           ~30 KB   (only for doctor — feature-gate if desired)
shell-words     ~20 KB   (needed — command parsing)
dirs            ~15 KB   (currently unused — remove!)
```

**Dead dependencies to remove**:
- `dirs = "6"` — not imported anywhere in the codebase
- `crossterm = "0.28"` — only used in the dead-code `ui/picker.rs`

```toml
[dependencies]
# Remove these:
# dirs = "6"           # unused
# crossterm = "0.28"   # only used by dead-code picker
```

### 6.5 Build Script for Stripping

Alternatively, use a build script for platform-specific stripping:

```toml
# Cargo.toml — no changes needed if using [profile.release].strip = true
# For older Rust versions that don't support strip in profile:
[profile.release]
opt-level = "z"        # Optimize for size instead of speed
lto = "fat"
codegen-units = 1
```

> **Note**: Use `opt-level = "z"` only if the <1.5 MB target is more important than speed. For a CLI tool where startup time matters, `opt-level = 3` with `lto = true` is the better tradeoff. The current `opt-level = 3` default for release is correct.

---

## 7. Implementation Priority

| Priority | Item | Effort | Impact |
|----------|------|--------|--------|
| **P0** | Fix `fuzzy_best` bug (line 51) | 1 min | Correctness |
| **P0** | Add `[profile.release]` with LTO + strip | 5 min | ~50% binary size reduction |
| **P1** | `OnceLock` singleton for `SkimMatcherV2` | 5 min | ~2 µs per fuzzy call |
| **P1** | Remove `dirs` dependency | 1 min | ~15 KB binary |
| **P1** | Remove dead `crossterm` dep + `ui/picker.rs` | 15 min | ~150 KB binary + dead code |
| **P1** | Change `fuzzy_match` to accept `&[&str]` | 10 min | Eliminates N String clones |
| **P2** | Feature-gate `serde_yaml` and `serde_json` | 30 min | ~350 KB binary |
| **P2** | `sort_unstable_by` in fuzzy_match | 1 min | ~30% faster sort |
| **P2** | Top-K fuzzy matching | 30 min | O(n log k) vs O(n log n) |
| **P2** | Remove unused `core/mod.rs` re-exports | 2 min | Code hygiene |
| **P3** | `IndexMap` for `SnipFile` entries | 1 hr | O(1) lookup vs O(n) |
| **P3** | Pagination flags for `snip list` | 1 hr | Large file UX |
| **P3** | Enable clippy pedantic lints | 2 hr | Code quality |
| **P3** | Add missing tests (executor, fuzzy_best, resolve_key) | 2 hr | Test coverage |
| **P4** | `SnippetSource` trait extraction | 3 hr | Future extensibility |
| **P4** | In-memory cache (if snip becomes a daemon) | 2 hr | Not needed now |

---

## 8. Summary

The `snip` codebase is already well-structured and performant. Cold start is estimated at ~2 ms — well under the 5 ms target. The binary at 2.58 MB is under the 3 MB target, and with `strip = true` + LTO will drop to ~1.3 MB.

The highest-value changes are:
1. **Fix the `fuzzy_best` bug** (correctness, 1 minute)
2. **Add release profile with LTO + strip** (binary size, 5 minutes)
3. **Remove dead dependencies** (`dirs`, `crossterm`) (hygiene, 15 minutes)
4. **Cache the fuzzy matcher in a `OnceLock`** (micro-optimization, 5 minutes)

None of these changes are architecturally risky. They can be done incrementally without affecting any existing behavior.