# fzf Source Code Deep Analysis

> Cloned from `https://github.com/junegunn/fzf.git` (depth=1, latest commit)
> Analysis date: 2025-01
> Purpose: Extract fuzzy matching & terminal UI techniques for `snip`

---

## Table of Contents

1. [Architecture Overview](#1-architecture-overview)
2. [Fuzzy Matching Algorithm](#2-fuzzy-matching-algorithm)
3. [Scoring & Ranking System](#3-scoring--ranking-system)
4. [Terminal UI](#4-terminal-ui)
5. [Performance Techniques](#5-performance-techniques)
6. [Integration Patterns](#6-integration-patterns)
7. [What to STEAL for snip](#7-what-to-steal-for-snip)
8. [Rust Implementation Guide](#8-rust-implementation-guide)

---

## 1. Architecture Overview

### Core Files

| File | Purpose |
|------|---------|
| `src/algo/algo.go` | **THE CORE** — FuzzyMatchV1, FuzzyMatchV2, scoring constants, character classes |
| `src/algo/normalize.go` | Unicode normalization table (accented → ASCII) |
| `src/algo/indexbyte2_*.go` | SIMD-accelerated byte search (AVX2/SSE2/NEON assembly) |
| `src/matcher.go` | Parallel matching engine — dispatches chunks to worker goroutines |
| `src/pattern.go` | Pattern parsing (extended mode: `^prefix`, `suffix$`, `'exact`, `!inverse`) |
| `src/result.go` | Result type, multi-criteria sort (LSD radix sort), color offset computation |
| `src/chunklist.go` | Chunked item storage (1024 items per chunk) for lock-free snapshotting |
| `src/cache.go` | Bitmap cache per chunk — avoids re-matching on incremental queries |
| `src/merger.go` | K-way merge of sorted partial results from parallel workers |
| `src/item.go` | Item struct (56 bytes) — text, transformed, origText, colors |
| `src/reader.go` | Streaming input reader (stdin, command output, file walker) |
| `src/terminal.go` | Main loop, key handling, rendering orchestration (~8700 lines) |
| `src/tui/light.go` | Terminal renderer — raw escape sequences, no TUI library dependency |
| `src/tui/light_unix.go` | Unix terminal setup (raw mode via `golang.org/x/term`) |
| `src/constants.go` | All magic numbers: chunk sizes, slab sizes, timing, events |

### Event-Driven Architecture

```
Reader goroutine          Matcher goroutine           Terminal goroutine (main)
     |                          |                          |
     | EvtReadNew ----+         |                          |
     |                |         |                          |
     +--push items--> ChunkList |                          |
     |                |         |                          |
     | EvtReadFin     |         |                          |
                      |   Reset(patternRunes)             |
                      |   reqBox ----+                    |
                      |              |                    |
                      |   scan() ->  |                    |
                      |   [worker1, worker2, ...]         |
                      |   resultChan <-+                  |
                      |              |                    |
                      |   EvtSearchFin ----+              |
                      |                     |            |
                      |                t.reqBox          |
                      |                printAll()         |
                      |                     |            |
                      |            [render to terminal]   |
```

Three goroutines communicate via `EventBox` (thread-safe event channels):
- **Reader**: reads stdin/command output, pushes items to `ChunkList`
- **Matcher**: receives pattern changes, matches in parallel, sends results
- **Terminal** (main loop): handles keyboard, renders UI, coordinates everything

---

## 2. Fuzzy Matching Algorithm

### 2.1 Two Algorithms: V1 (fast) and V2 (optimal)

fzf implements **two** fuzzy matching algorithms, selected by `--algo=v1` or `--algo=v2` (default is V2).

#### FuzzyMatchV1 — O(n) Greedy

```go
// src/algo/algo.go:710-790
func FuzzyMatchV1(caseSensitive bool, normalize bool, forward bool,
    text *util.Chars, pattern []rune, withPos bool, slab *util.Slab) (Result, *[]int) {
```

**Strategy:**
1. **Forward scan**: Walk through text left-to-right, matching each pattern character greedily (first occurrence)
2. **Backward scan**: Walk backward from the last match position to find if a shorter match exists
3. **Score**: Apply the `calculateScore()` function on the found match range

**Visual:**
```
Text:    a_____b___abc__   To find "abc"
         *-----*-----*>    1. Forward scan (greedy)
                  <***     2. Backward scan (tighten)
```

**Limitation**: Only finds the first occurrence, not necessarily the highest-scoring one.

#### FuzzyMatchV2 — O(nm) Modified Smith-Waterman (DEFAULT)

```go
// src/algo/algo.go:428-648
func FuzzyMatchV2(caseSensitive bool, normalize bool, forward bool,
    input *util.Chars, pattern []rune, withPos bool, slab *util.Slab) (Result, *[]int) {
```

**This is the algorithm that makes fzf feel "magical".** It's a modified Smith-Waterman dynamic programming algorithm that finds the **optimal** (highest-scoring) alignment of the pattern within the text.

**Key difference from standard Smith-Waterman**: Pattern character omission/mismatch is **not allowed**. Every pattern character must match, but text characters can be skipped (gaps).

**4-Phase Implementation:**

**Phase 1: ASCII Fast-Path Search** — `asciiFuzzyIndex()`
```go
// src/algo/algo.go:348-390
func asciiFuzzyIndex(input *util.Chars, pattern []rune, caseSensitive bool) (int, int) {
```
- For ASCII-only patterns, does a byte-level scan using `bytes.IndexByte` or SIMD `IndexByteTwo`
- Returns `(minIdx, maxIdx)` — the narrowest window where the pattern *could* match
- Reduces the N×M matrix to only the relevant portion
- **This is why fzf is fast even with V2**: it shrinks the search space before DP

**Phase 2: Pre-compute Character Class Bonuses**
```go
// src/algo/algo.go:473-528
for off, char := range T {
    class := asciiCharClasses[char]  // or charClassOfNonAscii
    bonus := bonusMatrix[prevClass][class]
    B[off] = bonus
    // Also find first occurrence of each pattern char -> F[]
    // And compute row 0 of score matrix (H0) for single-char pattern optimization
}
```
- Iterates the window once, computing the bonus at each position
- Also records the first occurrence index `F[pidx]` of each pattern character
- If only 1 pattern character, returns immediately (O(n))

**Phase 3: Fill Score Matrix (Smith-Waterman DP)**
```go
// src/algo/algo.go:554-607
for off, f := range Fsub {
    f := int(f)  // Start from the first occurrence of this pattern char
    pchar := Psub[off]
    pidx := off + 1
    row := pidx * width
    inGap := false
    Tsub := T[f : lastIdx+1]
    // ... DP fill
    for off, char := range Tsub {
        // s1 = match score (diagonal + match bonus)
        if pchar == char {
            s1 = Hdiag[off] + scoreMatch
            // Apply bonus with consecutive chunk logic
            consecutive = Cdiag[off] + 1
            if consecutive > 1 {
                fb := B[col-int(consecutive)+1]
                if b >= bonusBoundary && b > fb {
                    consecutive = 1  // Break consecutive chunk at word boundary
                } else {
                    b = max(b, bonusConsecutive, fb)
                }
            }
            if s1+b < s2 {
                s1 += Bsub[off]
                consecutive = 0
            } else {
                s1 += b
            }
        }
        // s2 = gap score (left + gap penalty)
        if inGap {
            s2 = Hleft[off] + scoreGapExtension
        } else {
            s2 = Hleft[off] + scoreGapStart
        }
        inGap = s1 < s2
        score := max(s1, s2, 0)
        Hsub[off] = score
    }
}
```

**Phase 4: Backtrace** (only if `withPos` is true)
```go
// src/algo/algo.go:616-643
// Standard Smith-Waterman backtrace from maxScorePos
// Tracks diagonal moves (match) vs left moves (gap)
```

**Fallback**: If the matrix would be too large (`N*M > slab capacity`) or pattern > 1000 chars, falls back to V1.

### 2.2 Exact Match Variants

```go
ExactMatchNaive()    // Standard substring search with bonus optimization
ExactMatchBoundary() // Same, but requires word boundary at both ends
PrefixMatch()        // Match at start of text (after leading whitespace)
SuffixMatch()        // Match at end of text (before trailing whitespace)
EqualMatch()         // Exact full-string match (highest score)
```

All use the same `calculateScore()` function for consistency.

---

## 3. Scoring & Ranking System

### 3.1 Score Constants

```go
// src/algo/algo.go:112-146
const (
    scoreMatch        = 16       // Base score for each matched character
    scoreGapStart     = -3       // Penalty for starting a new gap
    scoreGapExtension = -1       // Penalty for extending an existing gap

    // Bonus for matching at word boundaries (start of word, after delimiter)
    // Chosen so bonus is cancelled when gap > ~8 chars
    bonusBoundary = scoreMatch / 2  // = 8

    // Bonus for matching non-word characters
    bonusNonWord = scoreMatch / 2   // = 8

    // Bonus for camelCase transitions (e.g., fooBar matching 'b' or 'B')
    // Reduced because no gap accompanies camelCase transitions
    bonusCamel123 = bonusBoundary + scoreGapExtension  // = 7

    // Minimum bonus for consecutive matching characters
    bonusConsecutive = -(scoreGapStart + scoreGapExtension)  // = 4

    // First character multiplier — first typed char has more significance
    bonusFirstCharMultiplier = 2
)

// Scheme-dependent bonuses
var (
    bonusBoundaryWhite    int16 = bonusBoundary + 2    // = 10 (after whitespace/start)
    bonusBoundaryDelimiter int16 = bonusBoundary + 1   // = 9 (after /,:;|)
)
```

### 3.2 Character Classification

```go
// src/algo/algo.go:164-174
type charClass int
const (
    charWhite    charClass = iota  // space, tab, etc.
    charNonWord                     // punctuation (not delimiters)
    charDelimiter                   // / , : ; |
    charLower                       // a-z
    charUpper                       // A-Z
    charLetter                      // non-ASCII letters
    charNumber                      // 0-9
)
```

**Pre-computed lookup table** for ASCII (gives ~15% speedup):
```go
var asciiCharClasses [unicode.MaxASCII + 1]charClass
var bonusMatrix [charNumber + 1][charNumber + 1]int16
```

### 3.3 Bonus Function

```go
// src/algo/algo.go:268-296
func bonusFor(prevClass charClass, class charClass) int16 {
    if class >= charNonWord {
        switch prevClass {
        case charWhite:     return bonusBoundaryWhite     // +10: word start
        case charDelimiter: return bonusBoundaryDelimiter  // +9: after /
        case charNonWord:   return bonusBoundary           // +8: after punctuation
        }
    }
    if prevClass == charLower && class == charUpper ||
       prevClass != charNumber && class == charNumber {
        return bonusCamel123  // +7: camelCase or letter→digit
    }
    switch class {
    case charNonWord, charDelimiter: return bonusNonWord  // +8
    case charWhite:                   return bonusBoundaryWhite
    }
    return 0
}
```

### 3.4 Score Calculation (V1 path)

```go
// src/algo/algo.go:651-708
func calculateScore(caseSensitive bool, normalize bool, text *util.Chars,
    pattern []rune, sidx int, eidx int, withPos bool) (int, *[]int) {

    pidx, score, inGap, consecutive, firstBonus := 0, 0, false, 0, int16(0)
    for idx := sidx; idx < eidx; idx++ {
        char := text.Get(idx)
        // ... case folding and normalization ...
        if char == pattern[pidx] {
            score += scoreMatch                    // +16 per match
            bonus := bonusMatrix[prevClass][class]
            if consecutive == 0 {
                firstBonus = bonus
            } else {
                // Break consecutive chunk at stronger word boundary
                if bonus >= bonusBoundary && bonus > firstBonus {
                    firstBonus = bonus
                }
                bonus = max(bonus, firstBonus, bonusConsecutive)
            }
            if pidx == 0 {
                score += int(bonus * bonusFirstCharMultiplier)  // ×2 for first char
            } else {
                score += int(bonus)
            }
            inGap = false
            consecutive++
            pidx++
        } else {
            if inGap {
                score += scoreGapExtension   // -1
            } else {
                score += scoreGapStart       // -3
            }
            inGap = true
            consecutive = 0
            firstBonus = 0
        }
    }
}
```

### 3.5 Multi-Criteria Sorting

Results are sorted by up to **4 criteria** stored as `uint16` values in a 64-bit key:

```go
// src/result.go:27-28
type Result struct {
    item   *Item
    points [4]uint16   // Packed into uint64 for O(1) comparison
}
```

**Criteria types** (configurable via `--tiebreak`):
```go
// src/options.go:266-274
type criterion int
const (
    byScore    criterion = iota  // Primary: match score (lower = better, inverted)
    byChunk                      // Match extent size (smaller contiguous match wins)
    byLength                     // Total item length (shorter wins)
    byBegin                      // Distance from start of word to match start
    byEnd                        // Distance from match end to end of word
    byPathname                   // Path component length (shorter path component wins)
)
```

**Default tiebreak**: `score` → `length` (for default scheme), `score` → `pathname` → `length` (for path scheme)

**Radix Sort** for O(n) sorting:
```go
// src/result.go:354-424
func radixSortResults(a []Result, tac bool, scratch []Result) []Result {
    // For n < 128, falls back to comparison sort
    // For n >= 128: 8-pass LSD radix sort on the 64-bit sort key
    // Each pass handles 8 bits (one byte)
    // Skips passes where all items have the same byte value
}
```

**Comparison** uses `unsafe.Pointer` cast to `uint64` on x86/arm64:
```go
// src/result_x86.go:7-16
func compareRanks(irank Result, jrank Result, tac bool) bool {
    left := *(*uint64)(unsafe.Pointer(&irank.points[0]))
    right := *(*uint64)(unsafe.Pointer(&jrank.points[0]))
    if left < right { return true }
    if left > right { return false }
    return (irank.item.Index() <= jrank.item.Index()) != tac
}
```

### 3.6 Case Sensitivity

Three modes via `--case`:
- `smart` (default): Case-insensitive **unless** query contains uppercase
- `ignore`: Always case-insensitive
- `respect`: Always case-sensitive

```go
// src/pattern.go:128-129
caseSensitive = caseMode == CaseRespect ||
    caseMode == CaseSmart && lowerString != asString
```

**Optimization**: Case folding is done inline with a simple `char += 32` for ASCII uppercase, avoiding expensive `unicode.ToLower`:
```go
if char >= 'A' && char <= 'Z' {
    char += 32  // Inlined ToLower for ASCII
}
```

### 3.7 Unicode Normalization

Extensive normalization table in `src/algo/normalize.go` — maps accented Latin characters to ASCII:
```go
var normalized = map[rune]rune{
    0x00E1: 'a', // á → a
    0x00E9: 'e', // é → e
    0x00FC: 'u', // ü → u
    // ... hundreds of entries
}
```

Only applied when `--normalize` is set (off by default, auto-enabled when locale requires it).

---

## 4. Terminal UI

### 4.1 Rendering Architecture

fzf uses a **custom lightweight renderer** (`LightRenderer` in `src/tui/light.go`) that writes ANSI escape sequences directly — **no ncurses, no crossterm, no tcell** (though a tcell backend exists).

```go
// src/tui/light.go:134-167
type LightRenderer struct {
    theme         *ColorTheme
    ttyin         *os.File      // /dev/tty for input
    ttyout        *os.File      // stderr for output (stdout is reserved for results)
    buffer        []byte        // Input buffer
    origState     *term.State   // Saved terminal state for restore
    width, height int
    queued        strings.Builder  // Batched output
    sgr           string        // Current SGR state (for dedup)
    y, x          int           // Current cursor position
    fullscreen    bool
    tabstop       int
    escDelay      int           // ESC key delay (ms)
    mutex         sync.Mutex
}
```

**Key insight**: Output goes to **stderr**, not stdout. This allows piping:
```bash
find . | fzf > selected_file   # stdout = result
```

### 4.2 Terminal Raw Mode

```go
// src/tui/light_unix.go:37-38
func (r *LightRenderer) initPlatform() (err error) {
    r.origState, err = term.MakeRaw(r.fd())  // golang.org/x/term
    return err
}
```

Uses `golang.org/x/term.MakeRaw()` which sets:
- No echo
- No canonical mode (no line buffering)
- No signal processing
- 1-byte minimum read

### 4.3 Escape Sequences Used

**Core sequences:**
```go
// Cursor movement
"\x1b[{row};{col}H"    // Move cursor (CUP)
"\x1b[6n"              // Request cursor position (DSR)
"\x1b[{n}A"            // Cursor up
"\x1b[{n}B"            // Cursor down
"\x1b[{n}C"            // Cursor forward
"\x1b[{n}D"            // Cursor backward

// Screen manipulation
"\x1b[2J"              // Clear entire screen
"\x1b[H"               // Cursor home
"\x1b[J"               // Erase from cursor to end of screen
"\x1b[K"               // Erase to end of line

// Text attributes (SGR)
"\x1b[{codes}m"        // Set graphic rendition
"\x1b[0m"              // Reset all attributes
"\x1b[1m"              // Bold
"\x1b[2m"              // Dim
"\x1b[4m"              // Underline
"\x1b[7m"              // Reverse video

// Cursor visibility
"\x1b[?25l"            // Hide cursor
"\x1b[?25h"            // Show cursor

// Misc
"\a"                   // Bell
"\x1b7 ... \x1b8"     // Save/restore cursor (for passthrough)
```

### 4.4 Color Rendering

```go
// src/tui/light.go:1442-1451
func (w *LightWindow) csiColor(fg Color, bg Color, ul Color, attr Attr) (bool, string) {
    codes := append(attrCodes(attr), colorCodes(fg, bg)...)
    if ulCode := ulColorCode(ul); ulCode != "" {
        codes = append(codes, ulCode)
    }
    if len(codes) == 0 {
        return false, "\x1b[0m"
    }
    return true, "\x1b[;" + strings.Join(codes, ";") + "m"
}
```

**SGR state tracking** to minimize output:
```go
// src/tui/light.go:99-106
func (r *LightRenderer) setSGR(code string) {
    if code != r.sgr {    // Only emit if different from current state
        r.sgr = code
        r.queued.WriteString(code)
    }
}
```

### 4.5 Match Highlighting

Matched characters are highlighted by computing `colorOffset` ranges from the match result, then rendering each character segment with the appropriate color:

```go
// src/result.go:132-301
func (result *Result) colorOffsets(matchOffsets []Offset, nthOffsets []Offset,
    theme *tui.ColorTheme, colBase tui.ColorPair, colMatch tui.ColorPair, ...) []colorOffset {
    // Creates per-cell color map
    // Merges match highlights with existing ANSI colors from items
    // Returns sorted list of (start, end, color) ranges
}
```

In `printItem()` (terminal.go:3945), each item is rendered by:
1. Getting color offsets for matched characters
2. Printing the text segment by segment, changing color for matched portions
3. Filling remaining width with spaces (for current/selected item highlighting)

### 4.6 Input Handling

```go
// src/tui/light_unix.go:117-173
func (r *LightRenderer) getch(cancellable bool, nonblock bool) (int, getCharResult) {
    // Uses select() on tty fd + cancel pipe for cancellation
    // Returns single bytes; escape sequences assembled in higher-level loop
    rfds.Set(fd)
    rfds.Set(cancelFd)
    unix.Select(max(fd, cancelFd)+1, &rfds, nil, nil, nil)
}
```

**Key observation**: Input is read byte-by-byte with non-blocking I/O and `select()`. Escape sequence parsing (for arrow keys, etc.) is done by polling with configurable ESC delay (default 100ms, configurable via `--esc-delay`).

### 4.7 Window Resize Handling

```go
// src/tui/light_unix.go:84-94
func (r *LightRenderer) updateTerminalSize() {
    width, height, err := term.GetSize(r.fd())
    // Falls back to COLUMNS/LINES env vars
}

// Also uses SIGWINCH signal:
resizeChan := make(chan os.Signal, 1)
notifyOnResize(resizeChan)
// In main loop: triggers full redraw on resize
```

### 4.8 Incremental Redraw

fzf only redraws **changed lines**. It tracks the previous state of each line in `prevLines[]`:

```go
// terminal.go:3987-4002
if !forceRedraw &&
    prevLine.hidden == newLine.hidden &&
    prevLine.numLines == newLine.numLines &&
    prevLine.current == newLine.current &&
    prevLine.selected == newLine.selected &&
    prevLine.label == newLine.label &&
    prevLine.queryLen == newLine.queryLen &&
    prevLine.result == newLine.result {
    // Skip redraw — nothing changed!
    return line + numLines - 1
}
```

---

## 5. Performance Techniques

### 5.1 Parallel Matching

```go
// src/matcher.go:175-206
numWorkers := min(m.partitions, numChunks)  // partitions = runtime.NumCPU()
var nextChunk atomic.Int32
resultChan := make(chan partialResult, numWorkers)

for idx := range numWorkers {
    go func(idx int, slab *util.Slab) {
        for {
            ci := int(nextChunk.Add(1)) - 1  // Atomic work stealing
            if ci >= numChunks { break }
            chunkMatches := request.pattern.Match(request.chunks[ci], slab)
            matches = append(matches, chunkMatches...)
        }
        if m.sort && request.pattern.sortable {
            m.sortBuf[idx] = radixSortResults(matches, m.tac, m.sortBuf[idx])
        }
        resultChan <- partialResult{idx, matches}
    }(idx, m.slab[idx])
}
```

**Pattern**: Work-stealing with `atomic.Int32`. Chunks are distributed to N goroutines (N = CPU count). Each worker gets its own pre-allocated slab to avoid GC pressure.

### 5.2 Chunked Data Structure

```go
// src/constants.go:40-41
chunkSize     int = 1024
chunkBitWords     = (chunkSize + 63) / 64  // = 16 uint64s for bitmap

// src/chunklist.go:6-9
type Chunk struct {
    items [chunkSize]Item  // Fixed-size array, not slice (cache-friendly)
    count int
}
```

- Items stored in fixed 1024-item chunks
- Allows **lock-free snapshots** via copy-on-write (only first/last chunk copied)
- Bitmap cache: 1 bit per item × 1024 = 16 uint64s per chunk per query

### 5.3 Slab Allocator (GC Avoidance)

```go
// src/util/slab.go
type Slab struct {
    I16 []int16  // For score matrix, bonuses, consecutive counters
    I32 []int32  // For pattern positions, rune arrays
}

// src/constants.go:44-45
slab16Size int = 100 * 1024  // 200KB per worker
slab32Size int = 2048        // 8KB per worker
```

Each worker goroutine has its own slab, pre-allocated once and reused across searches. This eliminates GC pressure from the hot path.

### 5.4 Bitmap Query Cache

```go
// src/cache.go:12-15
type ChunkCache struct {
    mutex sync.Mutex
    cache map[*Chunk]*queryCache  // chunk → (query → bitmap)
}
```

**How it works:**
1. After matching a chunk with query "abc", store a bitmap of which items matched
2. On next query "abcd", search for prefix "abc" in the cache
3. If found, only test the additional 'd' character on the bitmap-marked items
4. **Searches both prefix AND suffix** of the query for cache hits

```go
// src/cache.go:72-98
func (cc *ChunkCache) Search(chunk *Chunk, key string) *ChunkBitmap {
    for idx := 1; idx < len(key); idx++ {
        prefix := key[:len(key)-idx]
        suffix := key[idx:]
        for _, substr := range [2]string{prefix, suffix} {
            if bm, found := (*qc)[substr]; found {
                return &bm
            }
        }
    }
}
```

**Cache eligibility**: Only cached when `matchCount <= chunkSize/2` (high selectivity) and chunk is full. This prevents wasting memory on queries that match everything.

### 5.5 SIMD Byte Search

Custom assembly implementations for `IndexByteTwo(s, b1, b2)` — finds first occurrence of either byte:

| Platform | Implementation |
|----------|---------------|
| AMD64 | AVX2 (32-byte blocks) with SSE2 fallback |
| ARM64 | NEON (32-byte blocks) |
| Other | Two `bytes.IndexByte` calls |

Used in the critical path of `asciiFuzzyIndex()` to quickly skip non-matching regions.

### 5.6 Fast Path for Single Fuzzy Term

```go
// src/pattern.go:340-356
// When pattern is a single fuzzy term (no nth, no denylist), bypass the
// generic MatchItem/extendedMatch path entirely:
if p.directAlgo != nil && len(p.denylist) == 0 {
    for idx := startIdx; idx < chunk.count; idx++ {
        res, _ := p.directAlgo(...)  // Direct algo call, no []Offset allocation
        if res.Start >= 0 {
            // Build result directly from bounds
        }
    }
}
```

This avoids per-match `[]Offset` heap allocation for the common case.

### 5.7 Pattern Cache

```go
// src/pattern.go:93-97
cached, found := patternCache[asString]
if found {
    return cached
}
// ... build pattern ...
patternCache[asString] = ptr
```

Pattern objects are cached by string representation. Since pattern building involves regex compilation and setup, this avoids redundant work when the user types and then backspaces.

### 5.8 Streaming Input with Event Coalescing

```go
// src/reader.go:52-73
func (r *Reader) startEventPoller() {
    go func() {
        pollInterval := readerPollIntervalMin  // 10ms
        for {
            if atomic.CompareAndSwapInt32(ptr, int32(EvtReadNew), int32(EvtReady)) {
                r.eventBox.Set(EvtReadNew, (*string)(nil))
                pollInterval = readerPollIntervalMin
            } else {
                pollInterval += readerPollIntervalStep  // +5ms
                if pollInterval > readerPollIntervalMax { pollInterval = readerPollIntervalMax }
            }
            time.Sleep(pollInterval)
        }
    }()
}
```

Instead of signaling the terminal on every item, events are coalesced with adaptive polling (10ms → 50ms). This prevents the UI from being overwhelmed during rapid input.

### 5.9 Incremental Search with Cancellation

```go
// src/matcher.go:224-226
if m.cancelScan.Get() || m.reqBox.Peek(reqReset) {
    return MatchResult{nil, nil, wait()}  // Cancel and restart
}
```

When the user types a new character while a search is in progress, the old search is cancelled via atomic flag and a new search starts immediately.

### 5.10 Optimizations Summary

| Technique | Impact | Implementation |
|-----------|--------|---------------|
| V2 algorithm with window narrowing | Reduces N×M to small submatrix | `asciiFuzzyIndex()` |
| Parallel chunk matching | Linear speedup with CPU cores | `sync.WaitGroup` + `atomic.Int32` |
| Slab allocator | Eliminates GC in hot path | Pre-allocated `[]int16`/`[]int32` |
| Bitmap cache | Skip re-matching on incremental queries | `ChunkBitmap` per chunk |
| SIMD byte search | ~15-30% faster character scanning | AVX2/NEON assembly |
| LSD radix sort | O(n) vs O(n log n) | 8-pass byte-level sort |
| Inline case folding | Avoid `unicode.ToLower` overhead | `char += 32` for ASCII |
| SGR state tracking | Minimize terminal output | `setSGR()` comparison |
| Incremental redraw | Only redraw changed lines | `prevLines[]` diff |
| Event coalescing | Prevent UI flooding | Adaptive poll interval |

---

## 6. Integration Patterns

### 6.1 Basic Pipe Pattern

```bash
# The fundamental pattern: command | fzf
find . -type f | fzf
# fzf reads stdin, shows picker, prints selection to stdout
```

**Exit codes:**
- `0`: Match found and selected
- `1`: No match (with `--select-1` / `--exit-0`)
- `2`: Error
- `130`: Interrupted (Ctrl+C)

### 6.2 Ctrl+R History Search

```bash
# shell/key-bindings.bash:93-114
__fzf_history__() {
    # 1. Extract history with `fc -lnr`
    # 2. Deduplicate and format with awk/perl
    # 3. Pipe to fzf with --scheme=history
    # 4. Read selection, strip counter prefix
    # 5. Set READLINE_LINE and READLINE_POINT
    output=$(
        builtin fc -lnr -2147483648 |
        command perl -n -l0 -e "$script" |
        FZF_DEFAULT_OPTS="--n2..,.. --scheme=history ..." \
        fzf --query "$READLINE_LINE"
    )
    READLINE_LINE=$(command perl -pe 's/^\d*\t//' <<< "$output")
    READLINE_POINT=0x7fffffff  # Cursor at end
}

# Bound via:
bind -m emacs-standard -x '"\C-r": __fzf_history__'
```

**Key technique**: Uses bash's `-x` binding to call a function that:
1. Runs fzf as a subprocess
2. Captures stdout
3. Sets `READLINE_LINE` / `READLINE_POINT` to inject the result

### 6.3 TAB Completion Integration

```bash
# shell/completion.bash:348-411
__fzf_generic_path_completion() {
    trigger=${FZF_COMPLETION_TRIGGER-'**'}  # Default trigger: **
    if [[ $cur == *"$trigger" ]]; then
        base=${cur:0:${#cur}-${#trigger}}
        matches=$(
            __fzf_comprun "$4" -q "$leftover" --walker "$walker" --walker-root="$dir"
            | while read -r item; do
                printf "%q " "${item%$3}$3"
            done
        )
        COMPREPLY=("$matches")
        builtin printf '\e[5n'  # Trigger redraw
        return 0
    fi
}
```

**Pattern**:
1. User types `**` (configurable trigger)
2. Completion function intercepts
3. Runs fzf with the current partial word as initial query
4. Sets `COMPREPLY` with the escaped result
5. Sends `\e[5n` (Device Status Report) to force shell to redraw

### 6.4 File Widget (Ctrl+T)

```bash
# shell/key-bindings.bash:64-68
fzf-file-widget() {
    local selected="$(__fzf_select__ "$@")"
    READLINE_LINE="${READLINE_LINE:0:READLINE_POINT}$selected${READLINE_LINE:READLINE_POINT}"
    READLINE_POINT=$((READLINE_POINT + ${#selected}))
}
bind -m emacs-standard -x '"\C-t": fzf-file-widget'
```

### 6.5 Tmux Integration

```bash
# shell/key-bindings.bash:59-62
__fzfcmd() {
    [[ -n ${TMUX_PANE-} ]] && { [[ ${FZF_TMUX:-0} != 0 ]] || [[ -n ${FZF_TMUX_OPTS-} ]]; } &&
        echo "fzf-tmux ${FZF_TMUX_OPTS:--d${FZF_TMUX_HEIGHT:-40%}} -- " || echo "fzf"
}
```

When running in tmux, automatically uses `fzf-tmux` which opens fzf in a split pane.

### 6.6 Programmatic API (`--listen`)

fzf supports a server mode for programmatic control:

```bash
echo -e "change-query\tnew query" > /dev/tcp/localhost/$FZF_PORT
echo "select\t0" > /dev/tcp/localhost/$FZF_PORT
echo "close" > /dev/tcp/localhost/$FZF_PORT
```

---

## 7. What to STEAL for snip

### 7.1 RECOMMENDATION: Shell out to fzf (v1)

**For the MVP, shell out to `fzf`.** This is what most tools do and it's the right call.

**Why:**
1. fzf is installed on virtually every developer machine (or trivially installable)
2. The matching quality is unmatched — years of tuning
3. Zero implementation risk
4. All keyboard handling, rendering, accessibility already done
5. Users already know fzf's keybindings

**How to integrate:**

```rust
// The simplest possible integration
use std::process::{Command, Stdio};

fn fuzzy_select(items: &[String], query: &str) -> Option<String> {
    let mut child = Command::new("fzf")
        .arg("--filter")  // Non-interactive filter mode
        .arg(query)
        .arg("--no-sort") // Preserve our ordering
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;

    let stdin = child.stdin.as_mut().unwrap();
    for item in items {
        writeln!(stdin, "{}", item).ok()?;
    }
    drop(stdin);

    let output = child.wait_with_output().ok()?;
    if output.status.success() {
        let result = String::from_utf8_lossy(&output.stdout);
        let first_line = result.lines().next()?;
        Some(first_line.to_string())
    } else {
        None
    }
}
```

**Interactive picker (for snippet insertion):**

```rust
fn fuzzy_pick(items: &[String], prompt: &str) -> Option<String> {
    let mut child = Command::new("fzf")
        .arg("--prompt").arg(prompt)
        .arg("--bind=enter:accept")  // Or use default behavior
        .arg("--height=40%")
        .arg("--reverse")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;

    // Write items to stdin
    let stdin = child.stdin.as_mut().unwrap();
    for item in items {
        writeln!(stdin, "{}", item).ok()?;
    }
    drop(stdin);

    let output = child.wait_with_output().ok()?;
    if output.status.success() {
        let result = String::from_utf8_lossy(&output.stdout);
        Some(result.trim().to_string())
    } else {
        None
    }
}
```

**Advanced integration with preview:**

```rust
fn fuzzy_pick_with_preview(items: &[Snippet], prompt: &str) -> Option<Snippet> {
    // Format items: "description\t{id}" so we can look up the snippet
    let formatted: Vec<String> = items.iter()
        .map(|s| format!("{}\t{}", s.description, s.id))
        .collect();

    let mut child = Command::new("fzf")
        .arg("--prompt").arg(prompt)
        .arg("--with-nth=1")           // Only search description
        .arg("--preview").arg("echo {}") // Show full line in preview
        .arg("--delimiter=\\t")
        .arg("--height=50%")
        .arg("--reverse")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;

    // ... write and read ...
    // Parse the id from the selected line
}
```

### 7.2 When to Reimplement in Rust (v2)

Reimplement **only if**:
1. snip needs to work without fzf installed (e.g., Windows without WSL)
2. You need custom rendering (e.g., embedded in a TUI)
3. You need tighter integration (e.g., inline fuzzy search in a text area)

If you do reimplement, use the **`skim`** crate or **`fuzzy-matcher`** crate as a starting point. They already implement fzf's algorithm in Rust:

```rust
// Using the fuzzy-matcher crate (fzf-compatible algorithm)
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

let matcher = SkimMatcherV2::default();
let result = matcher.fuzzy("foo bar baz", "fbb");
// Returns: Some(FuzzyMatch { score: 150, indices: [0, 4, 8] })
```

**The `skim` crate** (`github.com/skim-rs/skim`) is literally a Rust port of fzf and includes:
- The full V2 Smith-Waterman algorithm
- Character class bonuses (identical to fzf)
- The same scoring constants
- Terminal rendering

### 7.3 Integration Checklist for snip

- [ ] Detect if `fzf` is installed (`which fzf`)
- [ ] Fall back to built-in simple selection if not
- [ ] Use `--filter` mode for non-interactive filtering
- [ ] Use interactive mode for snippet picker
- [ ] Format items with description as column 1, tab-separated ID
- [ ] Use `--with-nth=1 --delimiter='\t'` for search scope
- [ ] Use `--preview` to show snippet content
- [ ] Use `--bind` for custom keybindings (e.g., `Ctrl+e` to edit)
- [ ] Use `--expect` for multi-action (select vs. edit vs. delete)
- [ ] Set `FZF_DEFAULT_OPTS` environment variable for consistent theming
- [ ] Handle SIGWINCH (fzf handles this internally)
- [ ] Handle exit code 130 (user cancelled)

### 7.4 The Exact Pipe Pattern

```bash
# What snip should do internally:
echo "${snippets[@]}" | fzf --prompt="snippet> " --preview="snip show {1}" \
    --delimiter=$'\t' --with-nth=1 \
    --bind='enter:accept' --bind='ctrl-e:accept-non-default' \
    --expect='ctrl-e,ctrl-d'
```

Then check `$?` and the output to determine which action the user chose.

---

## 8. Rust Implementation Guide

### 8.1 If Implementing the Scoring Algorithm

The scoring system is straightforward to port. Here's the core logic:

```rust
// Score constants (from fzf's algo.go)
const SCORE_MATCH: i16 = 16;
const SCORE_GAP_START: i16 = -3;
const SCORE_GAP_EXTENSION: i16 = -1;
const BONUS_BOUNDARY: i16 = 8;      // SCORE_MATCH / 2
const BONUS_NON_WORD: i16 = 8;      // SCORE_MATCH / 2
const BONUS_CAMEL123: i16 = 7;      // BONUS_BOUNDARY + SCORE_GAP_EXTENSION
const BONUS_CONSECUTIVE: i16 = 4;   // -(SCORE_GAP_START + SCORE_GAP_EXTENSION)
const BONUS_FIRST_CHAR_MULTIPLIER: i16 = 2;
const BONUS_BOUNDARY_WHITE: i16 = 10;  // BONUS_BOUNDARY + 2
const BONUS_BOUNDARY_DELIMITER: i16 = 9;  // BONUS_BOUNDARY + 1

#[derive(Clone, Copy, PartialEq, Eq)]
enum CharClass {
    White,
    NonWord,
    Delimiter,
    Lower,
    Upper,
    Letter,
    Number,
}

fn char_class_of(ch: char) -> CharClass {
    match ch {
        'a'..='z' => CharClass::Lower,
        'A'..='Z' => CharClass::Upper,
        '0'..='9' => CharClass::Number,
        c if c.is_whitespace() => CharClass::White,
        '/' | ',' | ':' | ';' | '|' => CharClass::Delimiter,
        c if c.is_alphabetic() => CharClass::Letter,
        _ => CharClass::NonWord,
    }
}

fn bonus_for(prev: CharClass, curr: CharClass) -> i16 {
    match curr {
        CharClass::NonWord | CharClass::Delimiter | CharClass::Upper | CharClass::Lower | CharClass::Letter | CharClass::Number => {
            match prev {
                CharClass::White if curr >= CharClass::NonWord => BONUS_BOUNDARY_WHITE,
                CharClass::Delimiter if curr >= CharClass::NonWord => BONUS_BOUNDARY_DELIMITER,
                CharClass::NonWord if curr >= CharClass::NonWord => BONUS_BOUNDARY,
                CharClass::Lower if curr == CharClass::Upper => BONUS_CAMEL123,
                _ if prev != CharClass::Number && curr == CharClass::Number => BONUS_CAMEL123,
                CharClass::NonWord | CharClass::Delimiter => BONUS_NON_WORD,
                CharClass::White => BONUS_BOUNDARY_WHITE,
                _ => 0,
            }
        }
    }
}

/// Calculate score for a given match range (V1 style)
fn calculate_score(text: &str, pattern: &str, sidx: usize, eidx: usize) -> (i32, Vec<usize>) {
    let mut score: i32 = 0;
    let mut in_gap = false;
    let mut consecutive = 0;
    let mut first_bonus: i16 = 0;
    let mut pidx = 0;
    let mut positions = Vec::with_capacity(pattern.len());
    let mut prev_class = CharClass::White;

    let text_chars: Vec<char> = text.chars().collect();
    let pattern_chars: Vec<char> = pattern.chars().collect();

    for idx in sidx..eidx {
        let ch = text_chars[idx];
        let class = char_class_of(ch);
        let ch_lower = ch.to_ascii_lowercase();

        if ch_lower == pattern_chars[pidx] {
            positions.push(idx);
            score += SCORE_MATCH as i32;

            let mut bonus = bonus_for(prev_class, class);

            if consecutive == 0 {
                first_bonus = bonus;
            } else {
                if bonus >= BONUS_BOUNDARY && bonus > first_bonus {
                    first_bonus = bonus;
                }
                bonus = bonus.max(first_bonus).max(BONUS_CONSECUTIVE);
            }

            score += if pidx == 0 {
                bonus as i32 * BONUS_FIRST_CHAR_MULTIPLIER as i32
            } else {
                bonus as i32
            };

            in_gap = false;
            consecutive += 1;
            pidx += 1;
        } else {
            score += if in_gap {
                SCORE_GAP_EXTENSION as i32
            } else {
                SCORE_GAP_START as i32
            };
            in_gap = true;
            consecutive = 0;
            first_bonus = 0;
        }
        prev_class = class;
    }

    (score, positions)
}
```

### 8.2 Recommended Rust Crates

| Crate | Purpose | Notes |
|-------|---------|-------|
| `fuzzy-matcher` | fzf-compatible fuzzy matching | SkimMatcherV2 implements the V2 algorithm |
| `skim` | Full fzf alternative in Rust | Can be used as a library |
| `ratatui` | Terminal UI (if building custom picker) | Not needed if shelling out to fzf |
| `crossterm` | Raw terminal handling | For custom TUI |
| `portable-pty` | PTY management | If spawning fzf in a pty |

### 8.3 Key Takeaways for snip

1. **Shell out to fzf for the MVP** — it's the gold standard, zero risk
2. **Format items as tab-separated** `description\tid` for clean fzf integration
3. **Use `--with-nth=1 --delimiter=$'\t'`** to search only descriptions
4. **Use `--preview`** to show full snippet content
5. **Use `--expect`** for multiple actions (insert, edit, delete)
6. **Use `--scheme=history`** if sorting by recency is important
7. **Handle exit code 1** (no match) gracefully — show all snippets unfiltered
8. **Set `FZF_DEFAULT_OPTS`** for consistent snip-specific theming
9. **If reimplementing**: Port the scoring constants and `bonus_for()` function exactly — they're the result of years of tuning
10. **If reimplementing**: The V1 algorithm (O(n) greedy) is "good enough" for most use cases and much simpler than V2

---

## Appendix: File Map

```
src/
├── algo/
│   ├── algo.go              # CORE: FuzzyMatchV1, FuzzyMatchV2, calculateScore, scoring constants
│   ├── normalize.go         # Unicode normalization table
│   ├── indexbyte2_amd64.go  # SIMD byte search (Go decl)
│   ├── indexbyte2_amd64.s   # SIMD byte search (x86 assembly)
│   ├── indexbyte2_arm64.go  # SIMD byte search (Go decl)
│   ├── indexbyte2_arm64.s   # SIMD byte search (ARM assembly)
│   ├── indexbyte2_other.go  # SIMD fallback (pure Go)
│   └── SIMD.md              # SIMD documentation
├── tui/
│   ├── tui.go               # Color types, event types, Window interface
│   ├── light.go             # LightRenderer — custom ANSI renderer (no deps)
│   ├── light_unix.go        # Unix raw mode, getch(), resize
│   ├── light_windows.go     # Windows console API
│   ├── tcell.go             # Alternative tcell renderer
│   └── dummy.go             # Test dummy renderer
├── terminal.go              # Main loop (~8700 lines), key handling, rendering
├── matcher.go               # Parallel matching engine
├── pattern.go               # Pattern parsing, term types
├── result.go                # Result type, color offsets, radix sort
├── result_x86.go            # Fast rank comparison via unsafe.Pointer
├── chunklist.go             # Chunked item storage
├── cache.go                 # Bitmap query cache
├── merger.go                # K-way merge of sorted results
├── item.go                  # Item struct (56 bytes)
├── reader.go                # Streaming input (stdin, commands, file walker)
├── constants.go             # All magic numbers
├── core.go                  # Entry point, sort criteria setup
├── options.go               # CLI option parsing
├── history.go               # Search history
├── tokenizer.go             # Field tokenization (for --nth)
├── ansi.go                  # ANSI escape code parsing
├── proxy.go                 # TCP server for --listen mode
├── server.go                # Server protocol
└── util/
    ├── slab.go              # Pre-allocated memory slabs
    ├── eventbox.go          # Thread-safe event channel
    ├── chars.go             # Efficient rune array
    └── util.go              # Miscellaneous utilities
```