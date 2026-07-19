# Pet Source Code Analysis (knqyf263/pet)

> Cloned from: `https://github.com/knqyf263/pet.git` (depth 1)
> Date: 2025-01
> Stars: ~7.5K | Language: Go | License: MIT

---

## A. Snippet Storage Format

### File Format: TOML

Pet stores all snippets in **TOML** files. The primary file is `snippet.toml`, and optionally additional `.toml` files from directories.

**Data model** (`snippet/snippet.go` lines 15-25):

```go
type Snippets struct {
    Snippets []SnippetInfo
}

type SnippetInfo struct {
    Filename    string `toml:"-"`              // NOT serialized — only used in-memory to track which file a snippet came from
    Description string                       // Human-readable description
    Command     string `toml:"command,multiline"`  // The actual command, supports multiline via TOML triple quotes
    Tag         []string                     // Array of tag strings
    Output      string                       // Cached output (not auto-populated, manually set)
}
```

**Example TOML on disk** (from `snippet/snippet_test.go` lines 202-209):

```toml
[[Snippets]]
  Description = "Test snippet"
  Output = "Hello, World!"
  Tag = ["test"]
  command = "echo 'Hello, World!'"
```

Key observations:
- `Filename` has `toml:"-"` — it is purely runtime metadata, never persisted to disk
- `Command` uses `toml:"command,multiline"` — TOML's `[[Snippets]]` array-of-tables syntax, with multiline support
- `Output` is a field that stores expected command output but is **never auto-populated** — it's purely manual/reference metadata
- Tags are `[]string` — space-delimited in the CLI, stored as a TOML array

### Storage Location

**Config path** (`config/config.go` lines 154-178):
- Default: `$HOME/.config/pet/config.toml` (XDG-compliant)
- Override: `$PET_CONFIG_DIR` environment variable
- Windows: `%APPDATA%/pet/config.toml`

**Snippet file path** (`config/config.go` line 124):
- Default: `$HOME/.config/pet/snippet.toml`
- Configurable via `[General] snippetfile` in config.toml

### Global vs. Project-Scoped

Pet is **entirely global**. There is no project-scoped snippet concept. The only "scoping" is the `SnippetDirs` feature:

```go
// config/config.go line 29
SnippetDirs []string
```

This lets you specify additional directories full of `.toml` files (`snippet/util.go` lines 12-36 uses `filepath.Walk` to recursively find all `.toml` files). But these are configured globally in `config.toml` — not discovered per-project. The README explicitly states:

> "Snippet files in `snippetdirs` will not be added to Gist or GitLab. You've to do version control manually."

So `SnippetDirs` is a half-measure for project-level snippets — you point them at repo directories manually.

### How Metadata is Handled

- **Description**: Required field, used as primary display/identity field. Duplication check in `cmd/new.go` line 261: `if s.Description == description { return fmt.Errorf("snippet [%s] already exists", description) }`
- **Tags**: Optional, only prompted with `pet new -t` flag. Stored as `[]string`. Displayed as `#tag1 #tag2` in search results.
- **Command**: The actual snippet content. Can be multiline (entered via `pet new --multiline` or `pet new --editor`).
- **Output**: Manual reference field, never auto-captured. The `pet exec` command does NOT populate this field.

---

## B. Fuzzy Selection Integration

### The Exact Mechanism: Stdin Piping to External Selector

This is the **critical piece** — pet does NOT embed any fuzzy finder. It delegates entirely to an external command.

**Default selector** (`config/config.go` line 140):
```go
cfg.General.SelectCmd = "fzf --ansi --layout=reverse --border --height=90% --pointer=* --cycle --prompt=Snippets:"
```

**The `filter()` function** (`cmd/util.go` lines 17-98) — the core selection mechanism:

```go
func filter(options []string, tag string, raw bool) (commands []string, err error) {
    var snippets snippet.Snippets
    if err := snippets.Load(true); err != nil {
        return commands, fmt.Errorf("load snippet failed: %v", err)
    }

    // 1. Tag filtering (pre-filter before sending to fzf)
    if 0 < len(tag) {
        // ... filters snippets by tag
    }

    // 2. Format all snippets into display strings
    snippetTexts := map[string]snippet.SnippetInfo{}
    var text string
    for _, s := range snippets.Snippets {
        // ... formats each snippet as: "[description]: command #tag1 #tag2"
        format := "[$description]: $command $tags"
        if config.Conf.General.Format != "" {
            format = config.Conf.General.Format
        }
        t := strings.Replace(format, "$command", command, 1)
        t = strings.Replace(t, "$description", s.Description, 1)
        t = strings.Replace(t, "$tags", tags, 1)
        snippetTexts[t] = s
        // ... optional color support for fzf
        text += t + "\n"
    }

    // 3. Pipe all formatted lines to the selector command
    var buf bytes.Buffer
    selectCmd := fmt.Sprintf("%s %s",
        config.Conf.General.SelectCmd, strings.Join(options, " "))
    err = run(selectCmd, strings.NewReader(text), &buf)
    if err != nil {
        return nil, nil  // <-- SILENT failure on cancel/escape
    }

    // 4. Parse selected lines back to commands
    lines := strings.Split(strings.TrimSuffix(buf.String(), "\n"), "\n")
    for _, line := range lines {
        snippetInfo := snippetTexts[line]
        commands = append(commands, fmt.Sprint(snippetInfo.Command))
    }
    return commands, nil
}
```

**The `run()` function** (`cmd/util_unix.go` lines 15-27) — how commands are executed:

```go
func run(command string, r io.Reader, w io.Writer) error {
    var cmd *exec.Cmd
    if len(config.Conf.General.Cmd) > 0 {
        line := append(config.Conf.General.Cmd, command)
        cmd = exec.Command(line[0], line[1:]...)
    } else {
        cmd = exec.Command("sh", "-c", command)
    }
    cmd.Stderr = os.Stderr
    cmd.Stdout = w
    cmd.Stdin = r
    return cmd.Run()
}
```

### The Complete Selection-to-Execution Flow

1. `pet search` / `pet exec` / `pet clip` all call `filter()`
2. `filter()` loads ALL snippets from ALL files
3. Formats each snippet into a single display line using configurable template: `"[$description]: $command $tags"`
4. Joins all lines with `\n` into a single string
5. **Pipes the entire string via stdin** to the configured `SelectCmd` (e.g., `fzf --ansi --layout=reverse ...`)
6. Reads the selected line(s) back from stdout
7. Uses a **map lookup** (`snippetTexts[line]` → `SnippetInfo`) to recover the original struct from the selected display text
8. Returns the `Command` field from the matched `SnippetInfo`

### Shell Integration Pattern (from README)

The recommended shell integration uses **command substitution** to place the selected command on the shell line:

```bash
# bash
function pet-select() {
  BUFFER=$(pet search --query "$READLINE_LINE")
  READLINE_LINE=$BUFFER
  READLINE_POINT=${#BUFFER}
}
bind -x '"\C-x\C-r": pet-select'
```

This is a key insight: `pet search` outputs the raw command to stdout. The shell function captures it and places it on the current input line, so the user can edit before executing. This is how it gets into shell history.

### The Parameter Dialog System

After selection, if the command contains `<param>` or `<param=default>` patterns, pet launches a **gocui TUI** (`dialog/view.go`) to fill in parameters interactively:

```go
// dialog/params.go lines 20-22
parameterStringRegex = `<([^<>]*[^\s])>`
```

- Parameters: `<name>`, `<name=default>`, `<name=|_val1_||_val2_||_val3_|>` (multiple defaults with arrow key cycling)
- Uses `github.com/awesome-gocui/gocui` for a terminal GUI
- TAB to move between fields, ENTER to execute, Ctrl+C to quit
- The `--raw` flag bypasses this dialog and outputs the command with `<param>` placeholders intact (for shell inline editing)

---

## C. Snippet Management Commands

### `pet new` (`cmd/new.go`)

**Interactive flow:**
1. Prompts for `Command>` using `readline` (or multiline mode with double-empty-line termination, or editor mode)
2. Prompts for `Description>` (required, used as dedup key)
3. Optionally prompts for `Tag>` if `-t` flag is passed
4. Checks for duplicate descriptions
5. Appends to the main `snippet.toml` file
6. If `auto_sync` is enabled, triggers sync

**Key code paths:**
- `scan()` (line 33): Uses `github.com/chzyer/readline` for single-line input with history
- `scanMultiLine()` (line 88): State machine (start → lastLineNotEmpty → lastLineEmpty) to detect double-empty-line as "done"
- `createAndEditSnippet()` (line 151): Creates a blank snippet, saves it, then opens `$EDITOR +<lineNumber> <file>` at the right line

### `pet list` (`cmd/list.go`)

Simple console output. Two modes:
- **Default**: Multi-line display with colored labels (Description, Command, Tag, Output)
- **`--oneline`**: Single-line format with column-truncated description: `"description                                : command"`

### `pet edit` (`cmd/edit.go`)

- If `SnippetDirs` is configured: launches fzf to select WHICH snippet file to edit, then opens it
- If not: opens the main `snippet.toml` directly
- Compares file content before/after editing to decide if sync is needed
- Opens with: `$EDITOR +0 <filepath>` (the `+0` is line number to jump to)

**Editor invocation** (`cmd/util_unix.go` lines 29-32):
```go
func editFile(command string, filePath path.AbsolutePath, startingLine int) error {
    command += " +" + strconv.Itoa(startingLine) + " " + filePath.Get()
    return run(command, os.Stdin, os.Stdout)
}
```

Note: This is a **string concatenation** approach — it just appends `+<line> <path>` to the editor command. Works for vim/nvim but would be fragile for other editors.

### `pet exec` (`cmd/exec.go`)

1. Calls `filter()` to select snippet(s) via fzf
2. Joins multiple selected commands with `"; "`
3. If params detected and not `--raw`: launches gocui dialog
4. Executes via `run()` (which uses `sh -c` by default)
5. Unless `--silent`: prints `> <command>` before executing

### `pet sync` (`cmd/sync.go` + `sync/`)

**Architecture** — uses a `Client` interface (`sync/sync.go` lines 15-18):
```go
type Client interface {
    GetSnippet() (*Snippet, error)
    UploadSnippet(string) error
}
```

Three backends:
- **Gist** (`sync/gist.go`): GitHub Gist API via `google/go-github`
- **GitLab** (`sync/gitlab.go`): GitLab Snippets API via `xanzy/go-gitlab`
- **GHE** (`sync/ghe.go`): GitHub Enterprise Gist (same as Gist but with custom base URLs)

**AutoSync logic** (`sync/sync.go` lines 27-56):
1. Fetches remote snippet + timestamp
2. Compares local file `ModTime` vs remote `UpdatedAt`
3. If local is newer: **upload** (overwrites remote)
4. If remote is newer: **download** (overwrites local)
5. If same: do nothing

**Critical flaw**: This is a **last-write-wins** strategy with no merge. If you edit on two machines, the newer edit silently overwrites the older one. No conflict detection, no diff.

Also: `upload()` and `download()` only operate on the **main snippet file** (`Load(false)`), completely ignoring `SnippetDirs`.

---

## D. Failure Autopsy: Why Pet Only Has ~7.5K Stars

### 1. The Global-Only Storage Problem

Pet's snippets live in `~/.config/pet/snippet.toml`. There is no per-project snippet discovery. The `SnippetDirs` config is a manual workaround — you have to edit your global config to add paths for each project.

**Why this kills adoption in teams:**
- Developer A can't share project-specific snippets with Developer B via the repo
- No `.gitignore`-friendly, committable snippet file in the project root
- The "sync via Gist" approach requires every team member to configure tokens individually
- `SnippetDirs` files are explicitly excluded from sync

### 2. No Native Fuzzy Finding = External Dependency

Pet requires `fzf` or `peco` to be installed separately. The default config assumes fzf. If neither is installed, every selection command silently fails (returns `nil, nil` in `filter()`).

This creates a confusing UX:
- Install pet via brew
- Run `pet search`
- Nothing happens (silent failure)
- User has to read docs to understand they need to also install fzf

### 3. The TOML Format is Hostile to Manual Editing

While the README says "it's easy to edit" because it's TOML, the actual format has issues:

```toml
[[Snippets]]
  Description = "Show expiration date of SSL certificate"
  command = """echo | openssl s_client -connect example.com:443 2>/dev/null |openssl x509 -dates -noout"""
  output = """
notBefore=Nov  3 00:00:00 2015 GMT
notAfter=Nov 28 12:00:00 2018 GMT"""
```

- Mixed casing (`Description` vs `command`) — confusing
- Triple-quoted multiline commands create visual noise
- No comments allowed within `[[Snippets]]` blocks (TOML limitation with array-of-tables)
- Adding a snippet in the middle of the file is tedious (need to repeat `[[Snippets]]` header)

### 4. Go Binary Distribution Friction

While Go produces static binaries (good), the project:
- Requires external runtime dependencies (fzf/peco) — undermining the "just drop the binary" pitch
- Uses `gocui` for parameter dialogs — a relatively heavy TUI dependency that can have terminal compatibility issues
- The binary is ~8-10MB for a tool that mostly formats text and pipes to fzf

### 5. No Discovery / Onboarding

There is no concept of importing snippets from a community collection. You start with an empty file. The only "sharing" mechanism is Gist/GitLab sync, which is 1:1 (your machine ↔ your Gist), not 1:many.

### 6. Silent Failures Everywhere

- `filter()` returns `nil, nil` on any selector error (line 72-73): `return nil, nil` — this means cancelling fzf is indistinguishable from fzf not being installed
- No validation of snippet file format before save
- Sync errors are wrapped but the tool continues

### 7. Feature Creep Without Core Polish

Pet has 3 sync backends (Gist, GitLab, GHE), a parameter dialog TUI, multiline input, color support, configurable format strings, sort options — but the core experience (create → find → run) has rough edges. The `prev()` shell function for "register previous command" is the most commonly needed feature and it requires users to manually add shell functions to their `.bashrc`/`.zshrc`.

### What Could Have Made Pet Succeed

- **Project-local `.pet.toml` or `.pet/` directory** that's auto-discovered and committable
- **Built-in fuzzy finder** (even a basic one) as fallback
- **Better error messages** instead of silent `nil, nil` returns
- **Snippet import from URL/file** for community sharing
- **Simpler data format** (maybe just comment-prefixed shell commands)

---

## E. Techniques to STEAL for Snip

### 1. The fzf Piping Pattern ⭐⭐⭐⭐⭐

**File: `cmd/util.go` lines 36-73**

This is the #1 thing to steal. The pattern is:

```
Format snippets as lines → Pipe to fzf via stdin → Read selected line from stdout → Map back to snippet struct
```

```go
// Build display lines
snippetTexts := map[string]snippet.SnippetInfo{}
var text string
for _, s := range snippets.Snippets {
    t := formatSnippet(s)  // "[description]: command #tags"
    snippetTexts[t] = s    // map display text → full struct
    text += t + "\n"
}

// Pipe to selector
var buf bytes.Buffer
selectCmd := config.Conf.General.SelectCmd
err = run(selectCmd, strings.NewReader(text), &buf)

// Recover selected snippet
selectedLine := strings.TrimSuffix(buf.String(), "\n")
snippet := snippetTexts[selectedLine]
```

**Why this pattern works:**
- fzf does all the heavy lifting (fuzzy matching, UI, keyboard navigation)
- pet is just a data formatter + mapper
- The `map[string]SnippetInfo` lookup is O(1)
- Supports multi-select naturally (fzf can return multiple lines)

**For snip:** Use this exact pattern. Format `.snips` file entries as fzf-consumable lines, pipe stdin, read stdout. The key improvement: since snip's file is simpler (just `# description\ncommand`), the formatting is trivial.

### 2. Configurable Display Format

**File: `config/config.go` line 143**

```go
cfg.General.Format = "[$description]: $command $tags"
```

**File: `cmd/util.go` lines 49-56**

```go
format := "[$description]: $command $tags"
if config.Conf.General.Format != "" {
    format = config.Conf.General.Format
}
t := strings.Replace(format, "$command", command, 1)
t = strings.Replace(t, "$description", s.Description, 1)
t = strings.Replace(t, "$tags", tags, 1)
```

Simple template variable replacement. Users can customize what fzf shows. For snip, this could be a nice advanced feature.

### 3. Shell Integration via Command Substitution

**File: `README.md` lines 149-156**

```bash
function pet-select() {
  BUFFER=$(pet search --query "$READLINE_LINE")
  READLINE_LINE=$BUFFER
  READLINE_POINT=${#BUFFER}
}
bind -x '"\C-x\C-r": pet-select'
```

The key insight: `pet search` outputs the RAW COMMAND to stdout (not the display-formatted line). This allows shell integration where the selected snippet is placed on the shell's command line for editing before execution — which means it naturally enters shell history.

**For snip:** `snip` should have a mode that outputs just the command to stdout, designed to be wrapped in `$(...)` by shell functions.

### 4. The `--query` Pre-fill Pattern

**File: `cmd/util.go` lines 68-69**

```go
selectCmd := fmt.Sprintf("%s %s",
    config.Conf.General.SelectCmd, strings.Join(options, " "))
```

When `--query "something"` is passed, it appends `--query 'something'` to the fzf command. This allows the shell integration to pre-fill fzf with whatever the user has already typed on the command line.

### 5. Silent Stdin/stdout for Piping

**File: `cmd/util_unix.go` lines 15-27**

The `run()` function accepts `io.Reader` and `io.Writer` — making it trivially testable and composable. Commands are executed via `sh -c`, with stdin piped from the formatted snippet text and stdout captured to a buffer.

### 6. Tag Filtering Before fzf

**File: `cmd/util.go` lines 24-34**

```go
if 0 < len(tag) {
    var filteredSnippets snippet.Snippets
    for _, snippet := range snippets.Snippets {
        for _, t := range snippet.Tag {
            if tag == t {
                filteredSnippets.Snippets = append(filteredSnippets.Snippets, snippet)
            }
        }
    }
    snippets = filteredSnippets
}
```

Pre-filtering by tag before sending to fzf reduces the dataset. For snip, this could be pre-filtering by a `.snips` file's context (project, global, etc.).

### 7. Edit-at-Line Number

**File: `cmd/util_unix.go` lines 29-32**

```go
func editFile(command string, filePath path.AbsolutePath, startingLine int) error {
    command += " +" + strconv.Itoa(startingLine) + " " + filePath.Get()
    return run(command, os.Stdin, os.Stdout)
}
```

When creating a new snippet, pet saves it, then opens the editor at the exact line of the new snippet. This is a nice UX touch.

### 8. Parameterized Snippets

**File: `dialog/params.go` lines 20-22, 50-85**

```go
parameterStringRegex = `<([^<>]*[^\s])>`
```

The `<param>` and `<param=default>` syntax for template variables in commands. The `--raw` flag bypasses the TUI dialog and outputs the raw template, letting users edit parameters inline in their shell.

**For snip:** Consider a simpler approach — maybe just shell variables or `{placeholder}` syntax instead of the TUI dialog.

---

## F. What Pet Does BADLY That Snip Fixes by Design

### 1. No Committable Snippet Files

**Pet's approach:** All snippets in `~/.config/pet/snippet.toml` (global, hidden). Sync requires Gist/GitLab API tokens. Team sharing is impractical.

**Snip's approach:** `.snips` files live in project directories, committed to git. Every team member gets them automatically on clone. No accounts, no tokens, no sync configuration.

```
# Pet's reality:
~/.config/pet/snippet.toml    ← global, invisible, unshared
my-project/.git/              ← no snippet knowledge

# Snip's reality:
my-project/.snips             ← committable, visible, shared
my-project/.gitignore         ← (doesn't need to ignore .snips)
```

### 2. TOML Format vs. Human-Readable Comments

**Pet's TOML:**
```toml
[[Snippets]]
  Description = "Deploy to staging"
  command = "kubectl apply -f k8s/staging/"
  Tag = ["k8s", "deploy"]
  output = ""
```

**Snip's format (presumed):**
```
# Deploy to staging
kubectl apply -f k8s/staging/
```

The snip format is:
- **Infinitely more readable** — looks like commented shell history
- **Editable in any editor** without TOML syntax knowledge
- **Git-diffable** — each snippet is 2 lines, diffs are clear
- **Copy-pasteable** — grab the command line, it's already a valid shell command
- **No escaping issues** — TOML multiline strings require careful escaping of quotes and special characters

### 3. Global Singleton vs. Scoped Discovery

**Pet:** One config file, one main snippet file, optionally some directories you manually point to. No auto-discovery. No concept of "project snippets."

**Snip:** Walk up from CWD looking for `.snips` files (like `.env` files, `Makefile`, etc.). Project snippets are automatically available when you're in the project. Global snippets from `~/.snips` are always available. This is how every developer tool actually works (git, make, docker-compose).

### 4. Sync via API vs. Sync via Git

**Pet's sync (`sync/sync.go`):**
- Requires personal access tokens for GitHub/GitLab
- Last-write-wins with timestamp comparison (data loss risk)
- Only syncs the main file, not SnippetDirs
- No conflict resolution
- One-way per sync direction (upload OR download, not merge)

**Snip's sync:**
- `git push` / `git pull` — already solved, already secure, already conflict-aware
- Works with any VCS (git, fossil, whatever)
- Branch-based workflows, PR reviews for snippet changes
- No API tokens, no backend configuration

### 5. Silent Failures vs. Clear Errors

**Pet (`cmd/util.go` line 72-73):**
```go
if err != nil {
    return nil, nil  // User presses ESC in fzf? Silent no-op.
}
```

**Snip should:** Clearly communicate what happened — "No snippet selected", "No .snips file found in /current/path or parents", etc.

### 6. Monolithic Binary with External Dependencies vs. Simple Script

**Pet:** Go binary (~8MB) + requires fzf/peco installed separately + requires gocui for parameter dialogs + requires readline for interactive input. That's 4 runtime dependencies for a "simple" snippet manager.

**Snip:** Can be a single shell script or a minimal binary. The `.snips` format is so simple it can be parsed with `grep`, `awk`, or any language. fzf is optional (can fall back to line-number selection or `select` builtin).

### 7. No Delete Command

Pet has no `pet delete` command. You have to either:
- Open the TOML file in an editor and manually delete the `[[Snippets]]` block
- Use `pet edit` which opens the whole file

This is a UX gap. The TOML format makes it impossible to do a simple `pet delete <index>` because there's no stable identifier — snippets are identified only by their description string (which must be unique) or their array position (which changes on every edit).

**Snip:** With a line-oriented format, deleting a snippet is `sed -i 'N,d' .snips` or similar. Trivial.

### 8. The Configuration Tax

Pet requires:
1. Running `pet configure` to create config
2. Setting up fzf separately
3. Adding shell functions to `.bashrc`/`.zshrc` for the `prev()` and `pet-select()` bindings
4. Configuring Gist/GitLab tokens for sync
5. Setting `selectcmd`, `editor`, `column`, etc.

That's 5 configuration steps before you can use the tool effectively. Most users will bounce at step 2.

**Snip:** Zero config. Put a `.snips` file in your project. Run `snip`. It works. The fzf integration should be auto-detected (check if `fzf` is in PATH) and used if available, with a graceful fallback if not.

---

## Summary: Pet's Architecture at a Glance

```
pet/
├── cmd/
│   ├── root.go          # Cobra root, config init
│   ├── new.go           # Interactive snippet creation (readline-based)
│   ├── list.go          # Console output, formatted
│   ├── edit.go          # Opens TOML in $EDITOR
│   ├── exec.go          # Select + execute (via sh -c)
│   ├── search.go        # Select + output to stdout (for shell integration)
│   ├── clip.go          # Select + copy to clipboard
│   ├── sync.go          # Trigger sync
│   ├── configure.go     # Edit config.toml
│   ├── util.go          # ⭐ filter() — the fzf piping core
│   ├── util_unix.go     # ⭐ run() — sh -c execution + editFile()
│   └── util_windows.go  # Windows equivalent
├── config/
│   └── config.go        # TOML config loading, defaults
├── snippet/
│   ├── snippet.go       # ⭐ Data model + Load/Save/Order/Filter
│   └── util.go          # Directory walker for .toml files
├── sync/
│   ├── sync.go          # ⭐ AutoSync (last-write-wins) + Client interface
│   ├── gist.go          # GitHub Gist backend
│   ├── gitlab.go        # GitLab Snippets backend
│   └── ghe.go           # GitHub Enterprise Gist backend
├── dialog/
│   ├── view.go          # gocui TUI for parameter filling
│   ├── params.go        # ⭐ <param=default> parsing + regex
│   └── util.go          # StringInSlice helper
└── path/
    └── path.go          # AbsolutePath type (expand ~, validate)
```

**Total LOC:** ~1,800 lines of Go (excluding tests)
**Dependencies:** 15 direct Go dependencies + runtime fzf/peco
**Commands:** `new`, `list`, `edit`, `exec`, `search`, `clip`, `sync`, `configure`, `version`

---

## Key Takeaways for Snip

1. **Steal the fzf pipe pattern** — it's proven, simple, and effective
2. **Steal the `--query` pre-fill** for shell integration
3. **Steal the display-format template** idea (configurable `$description`, `$command`, `$tags`)
4. **Steal the `--raw` flag concept** — output template with placeholders for inline shell editing
5. **Fix the storage** — committable `.snips` over hidden TOML
6. **Fix the scoping** — CWD-walk discovery over global-only
7. **Fix the sync** — git over API tokens
8. **Fix the format** — `# comment\ncommand` over `[[Snippets]]` TOML blocks
9. **Fix the errors** — informative messages over silent `nil, nil`
10. **Fix the delete gap** — line-oriented format makes deletion trivial