---
title: Refactor Subcommand Structure — Consolidate Semantics and Add Agent-Friendly Aliases
tags: [design-doc]
sources: []
contributors: [maxine--basel]
created: 2026-03-10
updated: 2026-03-10
---


## Design Specification

### Summary

Restructure crosslink's CLI to reduce the 44 top-level subcommands by introducing an `issue` namespace for all issue-lifecycle commands, consolidating timer commands under `timer`, adding silent aliases for common agent mistakes, and normalizing the `--dry_run` flag to `--dry-run`. This directly addresses agent friction observed in the ferrolearn swarm session (#231) where agents repeatedly guessed wrong subcommands, wasting tool-call round-trips.

### Requirements

- REQ-1: All issue-lifecycle commands must be accessible under `crosslink issue <verb>`, including create, quick, show, list, search, update, close, close-all, reopen, delete, comment, intervene, label, unlabel, block, unblock, relate, unrelate, blocked, ready, related, next, tree, and tested.
- REQ-2: The most common issue commands (create, quick, list, show, close) must also remain accessible as top-level shortcuts that silently delegate to `crosslink issue <verb>`.
- REQ-3: `crosslink subissue <parent> "title"` must be replaced by `crosslink issue create --parent <id>` (and `crosslink issue quick --parent <id>`), with `subissue` retained as a silent alias.
- REQ-4: `start`, `stop`, and `timer` must be consolidated under `crosslink timer {start|stop|show}`.
- REQ-5: Common agent-mistaken commands must be registered as silent aliases that execute the canonical command and emit a `hint:` line to stderr suggesting the canonical form.
- REQ-6: `--dry_run` must be renamed to `--dry-run` (kebab-case) across all four commands that use it: `cpitd scan`, `style sync`, `knowledge import`, and `kickoff run`.
- REQ-7: The top-level subcommand count must be reduced from 44 to approximately 25 or fewer.
- REQ-8: `migrate-to-shared`, `migrate-from-shared`, and `migrate-rename-branch` must be consolidated under `crosslink migrate {to-shared|from-shared|rename-branch}`.

### Acceptance Criteria

- [ ] AC-1: `crosslink issue create "title" -p high` creates an issue (verifiable via `crosslink issue show`).
- [ ] AC-2: `crosslink issue create "title" --parent 5` creates a subissue of issue #5 with correct parent relationship.
- [ ] AC-3: `crosslink create "title"` silently delegates to `crosslink issue create` and succeeds identically.
- [ ] AC-4: `crosslink subissue 5 "title"` silently delegates to `crosslink issue create --parent 5 "title"` and succeeds.
- [ ] AC-5: `crosslink timer start <id>` starts a timer; `crosslink timer stop <id>` stops it; `crosslink timer show <id>` displays it.
- [ ] AC-6: `crosslink start <id>` (old form) produces a `hint:` on stderr and delegates to `crosslink timer start <id>`.
- [ ] AC-7: `crosslink new "title"` produces `hint: did you mean 'crosslink issue create'? Using that.` on stderr and creates the issue.
- [ ] AC-8: `crosslink issues` produces a hint and delegates to `crosslink issue list`.
- [ ] AC-9: `crosslink issues list` produces a hint and delegates to `crosslink issue list`.
- [ ] AC-10: All alias hints are written to stderr only (not stdout), so they don't pollute `--json` or `--quiet` output.
- [ ] AC-11: `--dry-run` (kebab-case) works on `cpitd scan`, `style sync`, `knowledge import`, and `kickoff run`.
- [ ] AC-12: `--dry_run` (snake_case) continues to work as a silent alias for `--dry-run` (clap handles this natively).
- [ ] AC-13: `crosslink --help` shows approximately 25 or fewer top-level subcommands.
- [ ] AC-14: All existing integration tests pass (commands still reachable via old or new paths).
- [ ] AC-15: `crosslink issue --help` lists all issue-lifecycle subcommands.
- [ ] AC-16: `crosslink migrate to-shared`, `crosslink migrate from-shared`, and `crosslink migrate rename-branch` work correctly.
- [ ] AC-17: The three old `migrate-*` top-level forms are retained as hidden silent aliases.

### Architecture

### Current structure (main.rs, lines 49-491)

The `Commands` enum in `crosslink/src/main.rs` defines 44 top-level variants. Grouped commands like `Session`, `Knowledge`, `Container` etc. use clap's `#[command(subcommand)]` pattern with separate nested enums (lines 493-1176). Dispatch happens via a match block (lines 1252-1792).

### Proposed changes

**1. New `IssueCommands` enum (crosslink/src/main.rs)**

Create a new `IssueCommands` enum containing all issue-lifecycle variants currently in the top-level `Commands` enum:

```
Issue { action: IssueCommands }
```

The `IssueCommands` enum absorbs these from `Commands`: `Create`, `Quick`, `Show`, `List`, `Search`, `Update`, `Close`, `CloseAll`, `Reopen`, `Delete`, `Comment`, `Intervene`, `Label`, `Unlabel`, `Block`, `Unblock`, `Relate`, `Unrelate`, `Blocked`, `Ready`, `Related`, `Next`, `Tree`, `Tested`.

The `Create` and `Quick` variants gain an optional `--parent <id>` flag. The standalone `Subissue` variant is removed.

**2. Timer consolidation**

The top-level `Start`, `Stop`, and `Timer` variants move into a new `TimerCommands` enum:

```
Timer { action: TimerCommands }
```

With variants: `Start { id }`, `Stop { id }`, `Show { id }`.

**3. Top-level shortcuts (crosslink/src/main.rs)**

Retain a small set of top-level variants that parse identically to their `issue` counterparts but delegate internally. These use `#[command(hide = true)]` to keep them out of `--help` while still being parseable:

- `Create` → delegates to `Issue { IssueCommands::Create }`
- `Quick` → delegates to `Issue { IssueCommands::Quick }`
- `List` → delegates to `Issue { IssueCommands::List }`
- `Show` → delegates to `Issue { IssueCommands::Show }`
- `Close` → delegates to `Issue { IssueCommands::Close }`

Implementation: in the dispatch match block, these variants print a hint to stderr and fall through to the same handler functions.

**4. Alias system**

Aliases for common agent mistakes. Each is a hidden top-level variant that maps to a canonical command:

| Alias | Canonical | Hint message |
|-------|-----------|-------------|
| `new "title"` | `issue create "title"` | `hint: did you mean 'crosslink issue create'? Using that.` |
| `issues` | `issue list` | `hint: did you mean 'crosslink issue list'? Using that.` |
| `subissue <p> "t"` | `issue create --parent <p> "t"` | `hint: did you mean 'crosslink issue create --parent'? Using that.` |
| `start <id>` | `timer start <id>` | `hint: did you mean 'crosslink timer start'? Using that.` |
| `stop <id>` | `timer stop <id>` | `hint: did you mean 'crosslink timer stop'? Using that.` |

Aliases use `#[command(hide = true)]` so they don't appear in `--help` but are still recognized by clap's parser.

The hint is emitted via `eprintln!()` before delegating to the canonical handler. In `--quiet` mode, hints are suppressed.

**5. Migrate consolidation**

The top-level `MigrateToShared`, `MigrateFromShared`, and `MigrateRenameBranch` variants move into a new `MigrateCommands` enum:

```
Migrate { action: MigrateCommands }
```

With variants: `ToShared`, `FromShared`, `RenameBranch`. The old top-level forms (`migrate-to-shared`, etc.) are retained as hidden aliases.

**6. `--dry-run` normalization**

In the four commands that define `--dry_run` (Cpitd::Scan at line 677, Style::Sync at line 837, Knowledge::Import at line 925, Kickoff::Run at line 1003), rename the field to `dry_run` with explicit `#[arg(long = "dry-run")]`. Clap automatically accepts both `--dry-run` and `--dry_run`, so this is backwards-compatible.

**7. Dispatch refactor (crosslink/src/main.rs, lines 1252-1792)**

The main match block gains:
- A `Commands::Issue { action }` arm with a nested match on `IssueCommands`
- A `Commands::Timer { action }` arm with a nested match on `TimerCommands`
- A `Commands::Migrate { action }` arm with a nested match on `MigrateCommands`
- Arms for each shortcut/alias that emit hints and call the same handler functions

Handler functions themselves (`commands/create.rs`, `commands/list.rs`, etc.) are unchanged — only the dispatch routing in `main.rs` changes.

### Resulting top-level command surface

After restructure, `crosslink --help` shows:

```
Commands:
  init          Initialize crosslink in a project
  issue         Issue lifecycle commands (create, show, list, close, ...)
  timer         Time tracking (start, stop, show)
  session       Session lifecycle (start, end, status, work, action)
  knowledge     Shared research pages
  agent         Agent identity management
  trust         SSH trust management
  locks         Distributed lock management
  container     Docker agent execution
  kickoff       Launch background agents
  swarm         Multi-agent swarm coordination
  milestone     Release grouping
  archive       Issue archival
  config        Hook configuration
  workflow      Policy drift detection
  style         House style syncing
  context       Context injection measurement
  integrity     Data integrity checks
  cpitd         Code clone detection
  daemon        Background sync daemon
  export        Export issues
  import        Import issues
  migrate       Schema migration (to-shared, from-shared, rename-branch)
  sync          Manual sync
  compact       Manual event compaction
  tui           Terminal dashboard
  mc            Mission control
```

**~26 visible top-level commands** (down from 44). Hidden shortcuts and aliases add ~13 more that work but don't clutter help.

### Files modified

| File | Change |
|------|--------|
| `crosslink/src/main.rs` | New `IssueCommands`, `TimerCommands`, and `MigrateCommands` enums, alias variants, dispatch refactor |
| `crosslink/src/commands/create.rs` | Add `--parent` flag handling (absorb `subissue` logic from `commands/create.rs` where subissue is already implemented) |
| No other command modules change | Handler functions keep their existing signatures; only the dispatch in `main.rs` routes differently |

### Migration surface (out of scope for this design, noted for implementation)

These files reference crosslink commands and will need updating to use canonical forms over time. They continue to work via aliases/shortcuts in the interim:

- `CLAUDE.md` — command quick reference
- `README.md` — usage examples
- `.claude/commands/` — skill files
- `.claude/hooks/session-start.py` — hardcoded `run_crosslink()` calls
- Documentation site content

### Out of Scope

- Flag aliases (`--reason` → `--notes`, `--msg` → `--notes`) — follow-up work
- `--force` short flag standardization — requires resolving `-f` conflict with `--follow` and `--format`
- `--notes` short flag standardization on `swarm checkpoint` — minor follow-up
- Migration of hooks, skills, and documentation to canonical command forms — these continue to work via aliases
- Changes to `crosslink knowledge` subcommand names (e.g., `knowledge update` alias for `knowledge edit`) — separate issue
- Any changes to command handler implementations beyond dispatch routing
- Export/import output formatting modernization — file a separate GH issue tagged `enhancement`

### resolved questions

### Q1: Should `migrate-*` commands be grouped?
**Decision**: Yes. Consolidated under `crosslink migrate {to-shared|from-shared|rename-branch}`. Old top-level forms retained as hidden aliases.

### Q2: Should `export` and `import` move under `issue`?
**Decision**: No. They remain top-level — they're project-wide operations, not single-issue. A separate GitHub issue (tagged `enhancement`) should be filed to audit and modernize the export/import output formatting.

