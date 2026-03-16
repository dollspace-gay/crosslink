---
title: "Error Handling Conventions"
tags: [conventions, architecture]
sources: []
contributors: [maxine--basel]
created: 2026-03-16
updated: 2026-03-16
---

# Error Handling Conventions

Established conventions from adversarial review v1 (2026-03-16, GH issue #364).

## The Three Categories of `let _ =`

Every `let _ =` on a fallible operation in production code must fall into exactly one of:

### 1. Must Propagate
The operation's failure causes data loss or state inconsistency. Use `?` or `bail!()`.

**Examples found**:
- Hub sync failures (`write_commit_push`) — silently lose remote state
- Event signing failures — break audit trail integrity
- Git staging failures in compaction — push proceeds with unstaged files
- Handoff comment writes — session notes silently lost

### 2. Must Log
Best-effort operation where failure is acceptable but must be visible for debugging.

**Examples found**:
- Lock file cleanup in `Drop` impls
- PID file cleanup in daemon shutdown
- Cache directory creation

**Pattern**: `if let Err(e) = operation() { tracing::warn!(...); }`

### 3. Intentional (documented)
Failure is truly harmless. Mark with `// INTENTIONAL: <reason>`.

**Examples found**:
- WebSocket broadcast sends (no subscribers = no problem)
- Best-effort offline sync init (`lock_check.rs`)

**Pattern**: `let _ = ws_tx.send(...); // INTENTIONAL: broadcast failure is harmless when no subscribers`

## Transaction Boundaries

Wrap multi-step sequences in `db.transaction()` when partial completion would leave inconsistent state:
- Issue creation + label additions
- Session end + comment write
- Lock claim + session update

For cross-system sequences (git + SQLite), make the SQLite side atomic and add a compensating action (e.g., "needs-rehydrate" flag) if the git side succeeds but SQLite fails.

## Unsafe Defaults

Never use `.unwrap_or(<default>)` on configuration or identity reads where the default could cause silent misbehavior:
- Schema version -> 0 causes full migration re-run
- Git remote name -> "origin" could sync to wrong remote
- Path resolution -> cache_dir fallback could write to wrong location

Instead: propagate the error with `?`, or log a warning before falling back.
