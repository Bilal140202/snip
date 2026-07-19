# `just` Command Runner — Source Code Analysis

> Repo: `casey/just` v1.56.0 | Cloned to `/tmp/recon-just`
> Analysis date: 2025-07-09

---

## A. Fuzzy Picker (`--choose`)

### How it works

**`just` has NO built-in fuzzy matching.** It completely delegates to an external binary.

The `--choose` flag spawns an external process (defaulting to `fzf`), pipes recipe names into its stdin, reads selections from stdout, then runs them sequentially.

**Source: `src/subcommand.rs`, lines 262–355**

```rust
fn choose<'src>(
  chooser: Option<&Path>,
  config: &Config,
  justfile: &Justfile<'src>,
  overrides: &HashMap<Number, String>,
  search: &Search,
) -> RunResult<'src> {
  // 1. Collect all public recipes with 0 required args (optionally filtered by --group)
  let groups = config.groups.iter().cloned().collect::<BTreeSet<String>>();
  let mut recipes = Vec::<&Recipe>::new();
  let mut stack = vec![justfile];
  while let Some(module) = stack.pop() {
    recipes.extend(module.public_recipes(config).iter().filter(|recipe| {
      recipe.min_arguments() == 0
        && (groups.is_empty() || groups.intersection(&recipe.groups()).next().is_some())
    }));
    stack.extend(module.public_modules(config).into_iter().rev());
  }

  // 2. Build the chooser command — defaults to fzf with preview
  let chooser = if let Some(chooser) = chooser {
    OsString::from(chooser)
  } else {
    let mut chooser = OsString::new();
    chooser.push("fzf --multi --preview 'just --unstable --color always --justfile \"");
    chooser.push(&search.justfile);
    chooser.push("\" --show {}'");
    chooser
  };

  // 3. Spawn chooser, pipe recipe names to stdin
  let mut child = justfile.settings.shell_command(config)
    .shell_arg(&chooser)
    .current_dir(&search.working_directory)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .spawn()?;

  let stdin = child.stdin.as_mut().unwrap();
  for recipe in &recipes {
    writeln!(stdin, "{}", recipe.spaced_recipe_path())?;
  }

  // 4. Read selected recipes from stdout, run each
  let output = child.wait_with_output()?;
  for line in stdout.lines() {
    let arguments = line.split_whitespace().map(str::to_owned).collect();
    justfile.run(config, search, &arguments, overrides)?;
  }
}
```

### Key observations

| Aspect | Detail |
|--------|--------|
| **Library** | None. Delegates entirely to external binary |
| **Default chooser** | `fzf --multi --preview 'just --show {}'` |
| **Override** | `--chooser <path>` or `$JUST_CHOOSER` env var |
| **Cancellation** | Exit code 130 (SIGINT) = silent exit |
| **Multi-select** | `--multi` flag in the fzf command |
| **Preview** | `--preview 'just --show {}'` — runs `just --show` on the hovered recipe |
| **UX flow** | Pipe names in → user picks → run each selected recipe in sequence |
| **What's filtered** | Only public recipes with `min_arguments() == 0` |

### Why this is brilliant (and limited)

- **Brilliant**: Zero dependency. Any fuzzy matcher works (fzf, skim, fzy, pick, dmenu). User choice.
- **Limited**: No descriptions shown in the picker. Only recipe names are piped. If you want descriptions, you'd need to modify the chooser command yourself.

---

## B. Command Discovery

### `--list` formatting

**Source: `src/subcommand.rs`, lines 638–970**

The `list_module()` function is the heart of the listing UX:

```rust
fn list_module(
  config: &Config,
  depth: usize,
  groups: &[String],
  module: &Justfile,
) -> RunResult<'static> {
  const MAX_WIDTH: usize = 50;
  // ...
}
```

**Key design decisions:**

1. **Column alignment**: Calculates `max_signature_width` across all recipes (up to `MAX_WIDTH = 50` chars). Uses `unicode-width` crate for proper CJK/emoji width handling.

   ```rust
   // src/subcommand.rs, lines 729–779
   let signature_widths = {
     let mut signature_widths: BTreeMap<&str, usize> = BTreeMap::new();
     for (name, recipe) in &module.recipes {
       signature_widths.insert(name, UnicodeWidthStr::width(
         RecipeSignature { name, recipe }.color_display(Color::never()).to_string().as_str(),
       ));
     }
     signature_widths
   };
   let max_signature_width = signature_widths.values().copied()
     .filter(|width| *width <= MAX_WIDTH).max().unwrap_or(0);
   ```

2. **Inline vs block comments**: If the signature is ≤ 50 chars and the doc is ≤ 1 line, print inline. Otherwise, print the doc as `#`-prefixed lines above the recipe.

   ```rust
   // src/subcommand.rs, lines 906–923
   let inline_comment = signature_widths[entry.name] <= MAX_WIDTH
     && entry.comment.as_ref().is_none_or(|doc| doc.lines().count() <= 1);
   ```

3. **Backtick highlighting in docs**: Uses regex to find backticks in doc comments and renders them differently.

   ```rust
   // src/subcommand.rs, lines 685–698 (within print_doc_and_aliases)
   for backtick in BACKTICK_RE.find_iter(doc) {
     let prefix = &doc[end..backtick.start()];
     print!("{}", color.doc().paint(prefix));
     print!("{}", color.doc_backtick().paint(backtick.as_str()));
     end = backtick.end();
   }
   ```

4. **Group support**: Recipes can be tagged with `[group("name")]`. Groups are printed as `[group-name]` section headers.

5. **Submodule display**: Either recurses into submodules (`--list-submodules`) or shows `submodule-name ...`

6. **Alias display**: Three styles via `--alias-style`:
   - `right` (default): `[aliases: x, y]` after the recipe
   - `left`: `[aliases: x, y]` before the recipe
   - `separate`: Each alias gets its own line marked "alias for `recipe`"

### Description extraction (`##` comment syntax)

**Source: `src/parser.rs`, lines 420–455**

```rust
fn take_doc_comment(&mut self, attributes: &AttributeSet<'src>) -> Option<String> {
  // If [doc] attribute is present, skip ## comment extraction
  if attributes.contains(AttributeKind::Doc) {
    return None;
  }

  let mut items = self.items.iter().rev();

  // Must be: Comment, Newline, Recipe
  if !matches!(items.next()?, Item::Newline) {
    return None;
  }

  let Item::Comment(contents) = items.next()? else {
    return None;
  };

  // Must be at top of file or preceded by newline
  let first = match items.next() {
    None => true,
    Some(Item::Newline) => false,
    Some(_) => return None,
  };

  // Skip shebangs
  if first && contents.starts_with("#!") {
    return None;
  }

  // Strip leading "# " and trim
  let doc = contents[1..].trim().to_owned();
  if doc.is_empty() { return None; }

  // Remove the comment and newline from the item stream
  self.items.pop().unwrap();
  self.items.pop().unwrap();

  Some(doc)
}
```

**The rule**: A `# comment` on the line immediately before a recipe (separated by exactly one blank line from the previous item) becomes that recipe's doc comment. The `#` prefix is stripped.

Example:
```
# Build the project
build:
    cargo build
```

The parser also supports `[doc("description")]` attributes as an alternative.

### Suggestion system (typo correction)

**Source: `src/justfile.rs`, lines 55–64**

```rust
fn find_suggestion(
  input: &str,
  candidates: impl Iterator<Item = Suggestion<'src>>,
) -> Option<Suggestion<'src>> {
  candidates
    .map(|suggestion| (strsim::levenshtein(input, suggestion.name), suggestion))
    .filter(|(distance, _suggestion)| *distance < 3)
    .min_by_key(|(distance, _suggestion)| *distance)
    .map(|(_distance, suggestion)| suggestion)
}
```

Uses Levenshtein distance from the `strsim` crate, threshold of < 3. Covers recipes, aliases, submodules, and variables.

### What makes the listing UX good

1. **Column-aligned signatures** with unicode-width awareness
2. **Inline comments** when they fit, block comments when they don't
3. **Color** via `nu-ansi-term` — recipe names, params, docs, aliases, groups all styled differently
4. **Groups** as organizational sections
5. **`--summary`** for compact single-line output (just space-separated names)
6. **`--list-prefix`** and `--list-heading` for customization

### What makes the listing UX bad

1. No description in `--choose` mode (only recipe names are piped)
2. No search/filter in `--list` itself (only via `--group`)
3. No tree view of namespaces (modules are either recursed or collapsed with `...`)
4. 50-char max width for inline comments is hardcoded

---

## C. File Format & Parsing

### Parser architecture

**`just` uses a hand-written recursive descent parser.** No parser generator.

**Lexer: `src/lexer.rs`**
- Character-by-character lexing (the author explicitly notes the previous regex-based lexer was "slower and generally godawful")
- Tracks indentation stack for recipe bodies
- Handles string interpolation `{{ ... }}` with nesting
- Distinguishes `recipe_body` mode from top-level mode

**Parser: `src/parser.rs`** (3,439 lines)
- 2 tokens of lookahead for disambiguation
- Tracks "expected tokens" set for error messages: when parsing fails, reports which tokens would have been valid
- `expect_*` methods return user-facing errors; `presume_*` methods return internal bugs
- Key method: `parse_ast()` → `parse_item()` → `parse_recipe()` / `parse_assignment()` / etc.

### Variable system

```rust
// src/evaluator.rs
pub(crate) fn evaluate_const_assignments(
  assignments: &'run Table<'src, Assignment<'src>>,
  evaluation_order: &[Name<'src>],
  overrides: &'run HashMap<Number, String>,
  scope: &'run Scope<'src, 'run>,
  variable_references: &HashSet<Number>,
  lists: bool,
) -> CompileResult<'src, Self>
```

- Variables are lazily evaluated by default (`set lazy`)
- Evaluation order is topologically sorted during compilation
- Variables can reference other variables, functions, and backticks
- Constants (`const foo = "bar"`) are evaluated at compile time

### Dependencies between recipes

**Source: `src/recipe_resolver.rs`**

```rust
pub(crate) struct RecipeResolver<'src: 'run, 'run> {
  unresolved_recipes: Table<'src, UnresolvedRecipe<'src>>,
  resolved_recipes: Table<'src, Arc<Recipe<'src>>>,
  // ...
}
```

- Recipes declare dependencies with `: dep1 dep2`
- Dependencies can pass arguments: `: dep1(arg1) dep2(arg2)`
- `priors` vs `subsequents` separated by `&&` — priors run in parallel, subsequents after
- Circular dependency detection via a stack during resolution
- The `*` operator forwards extra arguments to a dependency

### Edge cases handled

| Edge case | How handled |
|-----------|-------------|
| Comments | Full-line comments in recipe bodies (`settings.ignore_comments`), doc comments above recipes |
| Multi-line strings | Via indentation-aware lexing; continuation with `\` at end of line |
| Backticks | `` `command` `` executed via shell, output captured, trailing newline stripped |
| Shebangs | First line starting with `#!` → script mode (write to temp file, execute) |
| String interpolation | `{{ expression }}` in recipe bodies and strings, `{{{{` as escape |
| Windows paths | `ShellKind` enum, `CommandExt::resolve()` for PATHEXT lookup, cygpath support |
| Markdown justfiles | `src/tangle.rs` — extracts ` ```just ` code blocks from Markdown |
| CRLF line endings | Handled throughout (e.g., `unindent.rs` checks for `\r\n`) |

### The `unindent` function

**Source: `src/unindent.rs`** (entire file, 126 lines)

Clean algorithm: finds common indentation across non-blank lines, strips it. Handles mixed tabs/spaces, `\r\n`, preserves blank lines.

---

## D. Shell Integration

### Shell support

**Source: `src/settings.rs`**

```rust
pub(crate) const DEFAULT_SHELL: &str = "sh";
pub(crate) const DEFAULT_SHELL_ARGS: &[&str] = &["-cu"];
pub(crate) const WINDOWS_POWERSHELL_SHELL: &str = "powershell.exe";
pub(crate) const WINDOWS_POWERSHELL_ARGS: &[&str] = &["-NoLogo", "-Command"];
```

Default shell is `sh -cu` (read from stdin, unset env, exit on error).

**Shell resolution order** (`src/settings.rs`, lines 67–93):
1. CLI `--shell` + `--shell-arg` (highest priority)
2. Justfile `set shell := [...]`
3. Windows: `set windows_powershell := true` → `powershell.exe -NoLogo -Command`
4. Default: `sh -cu`

### Shell kind detection

**Source: `src/shell_kind.rs`**

```rust
pub(crate) enum ShellKind {
  Cmd,       // → .bat extension, no shell name arg
  Powershell, // → .ps1 extension, no shell name arg, BOM needed
  Other,      // → no extension, passes shell name as arg
}
```

Detected from the command name. Used for script file extensions and argument passing.

### Argument passing to recipes

**Two modes:**

1. **Shell mode** (no shebang): Each line of recipe body is passed to `sh -c "line"` one at a time. Interpolation happens in Rust before passing to shell.

   ```rust
   // src/recipe.rs, lines 349-355
   let mut cmd = settings.shell_command(config);
   cmd.shell_arg(command);
   ```

2. **Script mode** (shebang or `[script]`): Entire body written to a temp file and executed. Shebang line determines the interpreter.

   ```rust
   // src/recipe.rs, lines 473-506
   let executor = if self.attributes.contains(AttributeKind::Script) {
     Executor::Command(interpreter)
   } else if self.body.first().is_some_and(Line::is_shebang) {
     Executor::Shebang(Shebang::new(&evaluated_lines[0]))
   } else {
     Executor::Command(context.module.settings.script_interpreter.clone())
   };
   ```

### Environment variable handling

**Source: `src/environment.rs`**

```rust
impl Environment {
  fn scope(&mut self, scope: &Scope, settings: &Settings, unexports: &BTreeSet<String>) {
    for unexport in unexports {
      self.variables.insert(unexport.clone(), None);  // None = env_remove
    }
    for binding in scope.bindings() {
      if (binding.export || settings.export) && !binding.value.is_empty() {
        self.variables.insert(binding.name.lexeme().to_string(), Some(binding.value.join()));
      }
    }
  }
}
```

- Variables marked `export` or when `set export := true` are passed as env vars
- `unexport` removes variables from the environment
- `.env` file loaded via `dotenvy` crate
- Scope chain: parent modules → current module → dotenv overrides

### Tab completions

**Source: `src/completer.rs`, `src/arguments.rs`**

Uses `clap_complete` with `unstable-dynamic` feature. Completions work by:

1. Shell sets `JUST_COMPLETE=bash|zsh|fish|...` env var
2. `just` re-parses, loads the justfile, and prints completion candidates
3. Dynamic completers: `Completer::complete_recipe`, `Completer::complete_variable`, `Completer::complete_group`
4. Uses prefix matching (`path.starts_with(current)`)

Shell scripts are trivial one-liners:
```bash
# just.bash
eval "$(JUST_COMPLETE=bash just)"

# just.fish
JUST_COMPLETE=fish just | source

# just.zsh
source <(JUST_COMPLETE=zsh just)
```

---

## E. What We Should STEAL for `snip`

### 1. Levenshtein suggestion algorithm
- **Steal from**: `src/justfile.rs`, lines 55–64
- **What**: `find_suggestion()` — Levenshtein distance with threshold < 3, applied to all candidate names
- **Why**: Simple, effective typo correction for "Did you mean X?"
- **Library**: `strsim` crate

### 2. Column-aligned listing with unicode width
- **Steal from**: `src/subcommand.rs`, lines 729–779 (width calculation) and 906–941 (printing)
- **What**: `signature_widths` calculation using `unicode-width`, `MAX_WIDTH` cap, inline vs block comment layout
- **Why**: Produces clean, aligned output. The inline/block threshold is a nice UX touch.
- **Library**: `unicode-width` crate

### 3. Doc comment extraction from preceding comments
- **Steal from**: `src/parser.rs`, lines 420–455 (`take_doc_comment()`)
- **What**: The pattern of looking backwards through parsed items to find a `Comment` + `Newline` before a recipe
- **Why**: The `# comment above recipe` → description pattern is intuitive and well-tested
- **Adaptation**: For `snip`, we'd adapt this to extract from `##` comments in Markdown

### 4. Unindent algorithm
- **Steal from**: `src/unindent.rs`, entire file (lines 1–63)
- **What**: Find common indentation across non-blank lines, strip it. Handles `\r\n`, mixed tabs/spaces.
- **Why**: Battle-tested, handles all edge cases, only 63 lines

### 5. Backtick highlighting in descriptions
- **Steal from**: `src/subcommand.rs`, lines 10, 685–698
- **What**: Regex to find backtick-quoted text in doc comments and render with different color
- **Why**: `code in docs` should look different — this is a nice polish detail
- **Regex**: `` static BACKTICK_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new("(`.*?`)|(`[^`]*$)").unwrap()); ``

### 6. External chooser delegation pattern
- **Steal from**: `src/subcommand.rs`, lines 262–355
- **What**: The pattern of piping candidates to an external binary and reading selections back
- **Why**: Zero-dependency fuzzy selection. Works with any picker (fzf, skim, pick, etc.)
- **Improvement for snip**: Pipe `name<TAB>description` instead of just names, so the chooser can display both

### 7. Search-upwards-for-config-file pattern
- **Steal from**: `src/search.rs`, lines 246–290 (`justfile()`)
- **What**: Walk `directory.ancestors()`, look for config file, respect ceiling
- **Why**: Standard pattern for tools that live in project roots
- **Improvement for snip**: We might want to search for `snip.md` or `.snip.md`

### 8. Markdown-as-config (tangle)
- **Steal from**: `src/tangle.rs`, entire file (lines 1–44)
- **What**: Extract ```` ```just ` code blocks from Markdown files using `pulldown-cmark`
- **Why**: `snip.md` is Markdown-native — this exact pattern applies
- **Library**: `pulldown-cmark`

### 9. Shell argument passing pattern
- **Steal from**: `src/settings.rs`, lines 55–93 and `src/command_ext.rs`, lines 93–101
- **What**: `shell_arg()` method that handles Windows `cmd.exe` (uses `raw_arg`) vs Unix (uses `arg`)
- **Why**: Correct cross-platform shell invocation is tricky

### 10. Dynamic tab completion via env var
- **Steal from**: `src/completer.rs` + `src/arguments.rs` (ArgValueCompleter pattern)
- **What**: On `SNIP_COMPLETE=bash`, re-parse config, print candidates, exit
- **Why**: Works with any shell, no complex completion scripts needed

### 11. Recipe caching with content-addressed keys
- **Steal from**: `src/cache.rs` and `src/cache_key.rs`
- **What**: Blake3 hash of (body + env + interpreter + inputs) → cache hit/miss with file locking
- **Why**: For expensive snippets, caching is valuable
- **Library**: `blake3`

### 12. The `Color` system
- **Steal from**: `src/color.rs`, entire file
- **What**: Wraps `nu-ansi-term` with semantic methods: `.recipe()`, `.parameter()`, `.doc()`, `.alias()`, `.group()`, `.banner()`, etc. Auto-detects terminal via `IsTerminal`.
- **Why**: Clean abstraction. Each semantic element has its own style.

### 13. Signal handling in child processes
- **Steal from**: `src/command_ext.rs`, lines 16–18, 103–105 and `src/signal_handler.rs`
- **What**: `status_guard()` and `output_guard()` spawn a thread to wait for child processes, allowing signal interception
- **Why**: Proper Ctrl-C handling while child processes run

### 14. Usage/help per-recipe
- **Steal from**: `src/usage.rs` + `src/recipe_signature.rs`
- **What**: `Usage` struct renders `just recipe [ARGUMENTS]` with parameter details, option short/long flags, defaults, and patterns
- **Why**: Per-command help is essential for a good CLI

---

## F. What `just` Does BADLY (That `snip` Can Do Better)

### 1. No fuzzy matching built in
`just --choose` just shells out to `fzf`. There's no built-in substring matching, no scoring, no ranked results. `snip` should have built-in fuzzy matching so it works out of the box without requiring external tools.

### 2. No descriptions in the chooser
`just --choose` only pipes recipe names to the fuzzy picker. Descriptions are completely lost. `snip` should pipe `name\tdescription` pairs so the picker can show context.

### 3. The Justfile format is a custom language
`just` invented its own language with its own parser (3,400+ lines), its own variable system, its own function library. This means:
- No syntax highlighting in editors without custom plugins
- No LSP support
- Steep learning curve ("what does `{{ }}` do? what's `set shell`?")
- **`snip` wins here**: Using Markdown as the config format means every editor already supports it, and users already know the syntax.

### 4. No search/filter in `--list`
`just --list` dumps everything. The only filtering is `--group`. You can't do `just --list build` to find recipes matching "build". `snip` should support `snip list build` with fuzzy/substring filtering.

### 5. Module/namespacing system is overly complex
The `mod foo ` import system with `modpath`, `submodules`, `cross_module_aliases`, `absent_modules`, etc. is a lot of machinery for what amounts to splitting a justfile across files. `snip` can use a simpler approach: just support multiple files or sections within a single Markdown file.

### 6. No tagging or categorization beyond "groups"
Groups are the only organizational mechanism. You can't tag a recipe with `#security`, `#devops`, `#local-only` etc. `snip` could support tags as a first-class concept derived from Markdown headings or frontmatter.

### 7. Error messages, while good, could be better
`just` has good error messages but they're text-only. `snip` could provide:
- Inline Markdown rendering of errors (show the exact line from the source file)
- Links to documentation
- Suggestions that include descriptions ("Did you mean `build`? — Build the project")

### 8. No caching of the parsed config
Every invocation re-reads and re-parses the justfile from scratch. While `just` has recipe result caching (via `src/cache.rs`), the config parsing itself is not cached. `snip` could cache parsed Markdown ASTs.

### 9. The `##` doc comment syntax is fragile
The rule "exactly one blank line between comment and recipe" is brittle. Users frequently get confused when their doc comments aren't picked up. `snip` using Markdown headings (`###`) as command boundaries is more natural and less error-prone.

### 10. No interactive mode
`just` is purely batch: you run it, it does one thing, it exits. There's no REPL, no persistent process, no way to chain commands interactively. `snip` could explore a TUI or interactive mode.

### 11. Aliases are second-class
Aliases are limited to simple name→recipe mappings. You can't alias with partial arguments, can't compose aliases, can't have conditional aliases. `snip` could treat aliases as first-class commands with their own descriptions.

### 12. No template/variable system in descriptions
Recipe descriptions are static strings. You can't say `"Build {{project_name}}"` in a doc comment. `snip` could support template variables in descriptions for dynamic help text.

### 13. The completion system is limited
Completions are prefix-only (no fuzzy matching). The completion scripts are thin wrappers around `clap_complete`'s dynamic mode. `snip` could provide richer completions with descriptions, grouped by heading, with fuzzy matching.

---

## Summary Table

| Feature | `just` | Steal? | `snip` Opportunity |
|---------|--------|--------|--------------------|
| Fuzzy picker | External (fzf) only | Pattern yes, implementation improve | Built-in fuzzy matching |
| Listing | Column-aligned, colored, grouped | Yes (alignment, color) | Add search/filter |
| Doc comments | `# comment` above recipe | Pattern yes | Markdown headings (native) |
| Parser | 3,400-line hand-written | No (too complex) | Markdown parser (existing) |
| Shell integration | Excellent cross-platform | Yes (shell_arg, ShellKind) | Same approach |
| Suggestions | Levenshtein < 3 | Yes (exact algorithm) | Add descriptions to suggestions |
| Completions | clap dynamic via env var | Yes (pattern) | Add fuzzy + descriptions |
| Caching | Blake3 content-addressed | Yes (for expensive snippets) | Same approach |
| Unindent | 63 lines, battle-tested | Yes (exact code) | Same |
| Color | Semantic method names | Yes (Color trait pattern) | Same |
| Config format | Custom language | No | Markdown (huge advantage) |