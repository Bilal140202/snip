# Implementation Status — Phase 2-4 Build

> Last updated: 2025-07-11
> Tracking parallel development across 3 phases, 15 work items

## Currently Building (Parallel)

| ID | Feature | Phase | Status | File(s) | Agent |
|----|---------|-------|--------|---------|-------|
| 1 | fzf integration | 2-P0 | 🔨 | `src/ui/picker.rs`, `src/cli/run.rs` | Main |
| 2 | Auto-init on bare `snip` | 2-P0 | 🔨 | `src/cli/list.rs` | Main |
| 3 | Levenshtein error messages | 2-P0 | 🔨 | `src/cli/run.rs`, `src/cli/rm.rs` | Main |
| 4 | Dynamic completions | 2-P1 | 🔨 | `src/cli/completions.rs` (rewrite) | Main |
| 5 | `eval "$(snip hook)"` | 2-P1 | 🔨 | `src/cli/hook.rs` (new) | Main |
| 6 | JSON output mode | 2-P2 | 🔨 | `src/cli/list.rs` | Main |
| 7 | Version lock | 2-P2 | 🔨 | `src/core/snipfile.rs` | Main |
| 8 | `snip setup` wizard | 2-P2 | 🔨 | `src/cli/setup.rs` (new) | Agent 8 |
| 9 | CI/CD workflow | 2-P2 | 🔨 | `.github/workflows/ci.yml` | Agent 9 |
| 10 | `.snips.d/` directory | 3 | 🔨 | `src/core/snipfile.rs` | Main |
| 11 | `snip suggest` | 3 | 🔨 | `src/cli/suggest.rs`, `src/core/history.rs` | Agent 11 |
| 12 | `snip explain` | 3 | 🔨 | `src/cli/explain.rs`, `src/core/explainer.rs` | Agent 12 |
| 13 | `snip stale` | 4 | 🔨 | `src/cli/stale.rs`, `src/core/stale.rs` | Agent 13 |
| 14 | `doctor --fix` | 4 | 🔨 | `src/cli/doctor.rs` | Main |
| 15 | Nushell completions | 4 | 🔨 | `src/cli/completions.rs` | Main |

## Dependencies

```
ID 1 (fzf)        ← no deps, can start immediately
ID 2 (auto-init)  ← no deps, can start immediately
ID 3 (levenshtein)← no deps, can start immediately
ID 4 (completions)← depends on ID 10 (.snips.d/) for full merge-chain support
ID 5 (hook)       ← depends on ID 4 (completions) — hook wraps completion setup
ID 6 (JSON)       ← no deps, can start immediately
ID 7 (version)    ← should land before ID 10 (.snips.d/ adds new format)
ID 8 (setup)      ← depends on ID 14 (doctor --fix) for validation
ID 9 (CI/CD)      ← no deps, can start immediately
ID 10 (.snips.d/) ← depends on ID 7 (version lock) for format header
ID 11 (suggest)   ← depends on ID 10 (.snips.d/) to know all available snippets
ID 12 (explain)   ← depends on ID 10 (.snips.d/) to find snippet definitions
ID 13 (stale)     ← depends on ID 10 (.snips.d/) + analytics readiness
ID 14 (doctor --fix) ← depends on ID 10 (.snips.d/) for multi-file validation
ID 15 (nushell)   ← depends on ID 4 (completions) — extends completion framework
```

## Critical Path

```
ID 7 (version lock)
  └→ ID 10 (.snips.d/)
       ├→ ID 4 (dynamic completions)
       │    ├→ ID 5 (hook)
       │    └→ ID 15 (nushell)
       ├→ ID 11 (suggest)
       ├→ ID 12 (explain)
       ├→ ID 13 (stale)
       └→ ID 14 (doctor --fix)
            └→ ID 8 (setup wizard)
```

## Parallel Tracks

Items with no cross-dependencies can be built simultaneously:

**Track A — Core UX (Main agent)**
- ID 1 (fzf), ID 2 (auto-init), ID 3 (levenshtein), ID 6 (JSON), ID 7 (version)

**Track B — Shell Integration (Main agent, after Track A)**
- ID 4 (completions), ID 5 (hook), ID 15 (nushell)

**Track C — Directory System (Main agent, after ID 7)**
- ID 10 (.snips.d/)

**Track D — Intelligence (Agent 11, 12 — after ID 10)**
- ID 11 (suggest), ID 12 (explain)

**Track E — Maintenance (Agent 8, 9, 13, Main)**
- ID 8 (setup), ID 9 (CI/CD), ID 13 (stale), ID 14 (doctor --fix)

## Progress Log

| Date | ID | Milestone |
|------|-----|-----------|
| 2025-07-11 | all | Build started — all 15 items marked 🔨 Building |