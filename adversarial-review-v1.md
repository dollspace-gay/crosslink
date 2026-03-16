---
title: "Adversarial Review v1 — Correctness, Structure, and Test Hardening"
tags: [design-doc]
sources: []
contributors: [maxine--basel]
created: 2026-03-16
updated: 2026-03-16
---


## Design Specification

### Summary

A comprehensive adversarial review of the crosslink codebase covering correctness hardening (silent error swallowing, transaction safety, migration robustness), structural decomposition of 7 god files into ~19 focused modules, test coverage gap closure (multi-agent adversarial scenarios, daemon testing, dashboard), and operational observability (structured logging, diagnostics). This is the prioritized punch list for the refining stage.

### Requirements

- REQ-1: Eliminate all high-severity silent error swallowing (`let _ =` on fallible operations) in production code paths, replacing with explicit error propagation or documented `// INTENTIONAL:` comments for best-effort patterns
- REQ-2: Wrap multi-step database operations in explicit transactions where partial state would cause hub-local desync (shared_writer mutations, create+label, session end+comment)
- REQ-3: Decompose 7 god files (shared_writer.rs, kickoff.rs, db.rs, sync.rs, knowledge.rs, commands/knowledge.rs, commands/swarm.rs) into ~19 focused modules of 300-1000 lines each without changing public API signatures
- REQ-4: Add adversarial multi-agent coordination tests covering conflict scenarios, clock skew recovery, and hub branch corruption recovery
- REQ-5: Replace all `eprintln!("warning/Warning")` calls with a structured logging facade that supports log levels and machine-parseable output
- REQ-6: Make the schema migration system idempotent with version-gated execution and rollback support, replacing the current "ignore duplicate column" heuristic
- REQ-7: Add smoke tests for under-tested commands: kickoff lifecycle, swarm phase orchestration, daemon start/stop, intervene, timer, design_doc
- REQ-8: Add concurrency and load tests: concurrent server API requests, multi-threaded database access, lock contention under parallel agent writes
- REQ-9: Fix the `.unwrap_or(0)` schema version query (db.rs:166) that silently falls back on read failure, and the `unwrap_or("origin")` remote fallback (sync.rs:45) that could sync to the wrong remote
- REQ-10: Add network partition simulation tests: sync behavior during offline periods, lock handling during split-brain, event log divergence recovery

### Acceptance Criteria

- [ ] AC-1: `grep -rn 'let _ =' crosslink/src/ --include='*.rs' | grep -v '#\[cfg(test)\]' | grep -v '// INTENTIONAL:'` returns zero results outside test modules and documented best-effort paths (validates REQ-1)
- [ ] AC-2: `cargo test` passes with no regressions after transaction wrapping changes; new integration test `test_create_with_label_atomic` verifies that a label failure rolls back the parent issue creation (validates REQ-2)
- [ ] AC-3: No source file in `crosslink/src/` exceeds 1200 lines (excluding test modules); `cargo test` and `cargo clippy` pass after decomposition (validates REQ-3)
- [ ] AC-4: New smoke test file `tests/smoke/adversarial_coordination.rs` contains tests for: two-agent same-issue write conflict, clock-skewed agent sync, corrupted hub branch recovery, stale lock steal under contention; all pass in CI (validates REQ-4)
- [ ] AC-5: Zero `eprintln!` calls remain in `crosslink/src/` outside of `#[cfg(test)]` blocks; all diagnostic output routes through a `log` or `tracing` facade; `crosslink --log-level debug serve` produces structured JSON log lines (validates REQ-5)
- [ ] AC-6: Migration system uses `PRAGMA user_version` correctly via `pragma_query_value`; v1-v15 migrations unchanged; new `run_migration()` method exists for v16+; `test_migration_idempotent` runs all migrations twice with identical schema; `test_fresh_schema_matches_migrated` confirms v1→v15 upgrade matches fresh v15 (validates REQ-6)
- [ ] AC-7: Smoke tests exist for: `kickoff run` → status → logs → stop → cleanup lifecycle, `swarm init` → launch → gate → merge pipeline, `daemon start` → status → stop, `intervene` with all trigger types, `timer start` → stop → show; all pass in CI (validates REQ-7)
- [ ] AC-8: New test file `tests/smoke/concurrency.rs` contains: 10-concurrent-API-request test, 5-thread database write contention test, parallel lock claim race test; all pass without deadlock or data corruption (validates REQ-8)
- [ ] AC-9: Schema version query uses `PRAGMA user_version` directly (not via `query_row`); remote resolution logs a warning when falling back to "origin"; both paths have unit tests (validates REQ-9)
- [ ] AC-10: New test `test_offline_sync_recovery` simulates network partition (bare remote unavailable), verifies local operations succeed, then reconnects and verifies convergence; `test_split_brain_locks` verifies two agents claiming same lock with partitioned remote (validates REQ-10)

### Architecture

### Priority 1: Correctness Hardening (REQ-1, REQ-2, REQ-6, REQ-9)

**Silent error elimination (REQ-1):**

17 `let _ =` patterns identified across production code. Each falls into one of three categories:

1. **Must propagate** (7 instances) — errors that cause data loss or state inconsistency:
   - `shared_writer.rs:275` — event signing failure breaks audit trail; must `bail!()` or log + continue with unsigned marker
   - `shared_writer.rs:525,558,582` — `write_commit_push` failures in update/close/reopen silently lose hub sync; must propagate to caller
   - `compaction.rs:293,298-300` — git staging failures in commit-push loop; must check return codes and abort push if staging fails
   - `commands/session.rs:117,122` — handoff comment write failures lose notes; must warn user

2. **Must log** (6 instances) — best-effort operations where failure is acceptable but must be visible:
   - `compaction.rs:67,71,115` — lock file cleanup in Drop; log warning on failure
   - `daemon.rs:27,71,80` — PID file cleanup; log warning
   - `shared_writer.rs:275` (signing, if we decide audit-trail is best-effort)

3. **Intentional and documented** (4 instances) — broadcast sends, daemon cleanup where failure is truly harmless:
   - `server/handlers/*.rs` — `ws_tx.send()` broadcast failures (no subscribers is fine)
   - Add `// INTENTIONAL: broadcast failure is harmless when no WebSocket subscribers` comments

**Transaction wrapping (REQ-2):**

Four multi-step sequences need transaction guards:

1. `commands/create.rs:137-159` — issue creation + label additions: wrap in `db.transaction()`
2. `commands/session.rs:102-125` — session end + comment write: wrap in `db.transaction()`
3. `shared_writer.rs` mutations — the hub-write + hydrate-to-sqlite sequence can't be fully transactional (spans git + SQLite), but the SQLite side should be atomic. Add compensating action: if hydration fails after hub push, log error and set a "needs-rehydrate" flag checked on next sync.
4. `commands/session.rs:170-240` — lock claim + session update: if DB update fails after lock claim, release the lock in a cleanup block.

**Migration robustness (REQ-6):**

Hybrid approach — preserve v1-v15, introduce proper runner for v16+:

- Fix version read: replace `.unwrap_or(0)` with `self.conn.pragma_query_value(None, "user_version", |row| row.get(0))?` — errors propagate instead of silently re-running all migrations
- Leave v1-v15 migrations and `migrate()`/`migrate_batch()` helpers untouched (battle-tested, "ignore duplicate column" stays as defense-in-depth for early migrations)
- Add new `run_migration(version, fn)` method for v16+: wraps migration in its own transaction, propagates errors on failure (no silent ignore), bumps `user_version` per-step instead of one big jump at the end
- New test: `test_migration_idempotent` applies all 15 migrations to fresh DB, asserts schema matches; applies all 15 again, asserts no errors and no schema changes
- New test: `test_fresh_schema_matches_migrated` compares a fresh v15 DB schema to one that migrated from v1 through all 15 versions

**Schema version fix (REQ-9):**

`db.rs:166` currently does `self.conn.query_row("SELECT user_version FROM pragma_user_version", ...)` which may fail on some SQLite builds. Replace with `self.conn.pragma_query_value(None, "user_version", |row| row.get(0))` which is the canonical rusqlite pattern.

### Priority 2: Structural Decomposition (REQ-3)

Seven files decompose into 19 focused modules. All decompositions preserve existing `pub` API by re-exporting from the parent module.

**shared_writer.rs (2300 lines) → 4 modules:**
- `shared_writer/core.rs` (~550 lines) — SharedWriter struct, event infrastructure, emit_compact_push, write_commit_push retry loop, git helpers
- `shared_writer/mutations.rs` (~650 lines) — issue/comment/label/dependency/relation CRUD via hub
- `shared_writer/milestones.rs` (~350 lines) — milestone operations via hub
- `shared_writer/offline.rs` (~400 lines) — offline issue promotion, local reference rewriting

**commands/kickoff.rs (3800 lines) → 4 modules:**
- `commands/kickoff/types.rs` (~300 lines) — ContainerMode, VerifyLevel, Criterion, KickoffMetadata, report types, parse helpers
- `commands/kickoff/launch.rs` (~900 lines) — platform detection, worktree setup, agent command building, watchdog, execution
- `commands/kickoff/run.rs` (~1000 lines) — main `run()` orchestration, design doc parsing, criteria extraction
- `commands/kickoff/monitor.rs` (~800 lines) — status, list, logs, stop, cleanup, report generation

**db.rs (1750 lines) → 4 modules:**
- `db/core.rs` (~200 lines) — Database struct, init_schema, transaction(), migrate(), constants, validators
- `db/issues.rs` (~450 lines) — issue CRUD, hierarchy, export metadata
- `db/relations.rs` (~350 lines) — labels, comments, dependencies, relations, milestone-issue binding
- `db/sessions.rs` (~400 lines) — sessions, time tracking, milestones, archives, token usage, search, hydration

**sync.rs (1200 lines) → 3 modules:**
- `sync/core.rs` (~350 lines) — SyncManager struct, cache init, migration from old branch, constants
- `sync/operations.rs` (~400 lines) — fetch/pull/push, rebase, divergence detection, dirty state
- `sync/integration.rs` (~300 lines) — SSH signing setup, worktree management, git command wrappers

**knowledge.rs (1600 lines) → 3 modules:**
- `knowledge/core.rs` (~300 lines) — KnowledgeManager struct, types, init
- `knowledge/pages.rs` (~500 lines) — page CRUD, listing, metadata, bulk import
- `knowledge/sync.rs` (~400 lines) — sync, conflict resolution, git operations, search

**commands/knowledge.rs (1100 lines) → 2 modules:**
- `commands/knowledge/core.rs` (~400 lines) — dispatch, add/show/list/remove, JSON formatting
- `commands/knowledge/edit.rs` (~500 lines) — section editing, import, markdown parsing, path utilities

**commands/swarm.rs (3400 lines) → 4 modules:**
- `commands/swarm/types.rs` (~300 lines) — all data model definitions, hub I/O helpers
- `commands/swarm/phase.rs` (~700 lines) — init, status, resume, launch, gate, agent management
- `commands/swarm/budget.rs` (~600 lines) — budget config, cost analysis, window planning
- `commands/swarm/review_merge.rs` (~800 lines) — review pipeline, merge orchestration, trust/pipeline

**Decomposition strategy:** Convert each file to a directory module (`foo.rs` → `foo/mod.rs` + submodules). The `mod.rs` re-exports all public items so callers don't change. Use `pub(crate)` for internal cross-submodule access.

### Priority 3: Test Coverage (REQ-4, REQ-7, REQ-8, REQ-10)

**Adversarial coordination tests (REQ-4):**

New file `tests/smoke/adversarial_coordination.rs` using the existing `SmokeHarness::fork_agent()` pattern:

- `test_concurrent_same_issue_update` — Two agents update the same issue's title simultaneously via `crosslink sync`; verify convergence (last-writer-wins or merge)
- `test_clock_skewed_agent` — Agent B's system clock is 5 minutes ahead; create issues on both agents, sync, verify total ordering key resolves correctly
- `test_hub_branch_corruption_recovery` — Corrupt a file in the hub branch worktree, then run `crosslink sync`; verify error is reported and recovery is possible via re-init
- `test_stale_lock_steal_contention` — Agent A claims lock, Agent B waits, Agent A's lock goes stale (modify timestamp), Agent B steals; verify A detects stolen lock on next operation
- `test_event_log_divergence` — Both agents append events offline, then sync; verify compaction produces consistent state

**Under-tested command smoke tests (REQ-7):**

Extend existing smoke test files:
- `cli_tooling.rs`: kickoff lifecycle (run → status → logs → stop → cleanup with a mock agent), design_doc generation, intervene with trigger types
- `cli_data.rs`: timer start → stop → show roundtrip
- `cli_infra.rs`: daemon start → status → stop (with PID file verification)
- `coordination.rs`: swarm init → status pipeline

**Concurrency tests (REQ-8):**

New file `tests/smoke/concurrency.rs`:
- `test_concurrent_api_creates` — 10 threads POST to `/api/v1/issues` simultaneously; verify all 10 issues created with unique IDs
- `test_concurrent_db_writes` — 5 threads write to same database via CLI binary; verify no SQLITE_BUSY errors or data loss
- `test_parallel_lock_claims` — 3 agents attempt to claim same issue lock; verify exactly 1 succeeds, 2 fail with contention error

**Network partition tests (REQ-10):**

New tests in `adversarial_coordination.rs`:
- `test_offline_local_operations` — Remove bare remote directory, run create/close/comment operations, verify all succeed locally
- `test_offline_then_sync` — Create issues offline on two agents, restore remote, sync both, verify merged state
- `test_split_brain_lock` — Agent A claims lock, partition remote, Agent B claims same lock; restore remote, both agents sync; verify conflict is detected

### Priority 4: Observability (REQ-5)

**Structured logging migration:**

Add `tracing` crate (standard Rust ecosystem choice, already compatible with tokio/axum):
- Replace all `eprintln!("warning: ...")` with `tracing::warn!(...)`
- Replace all `eprintln!("error: ...")` with `tracing::error!(...)`
- Add `--log-level` global CLI flag (default: `warn` for CLI, `info` for `serve`)
- For `crosslink serve`, use `tracing-subscriber` with JSON formatter
- For CLI commands, use compact single-line formatter to stderr
- Add span context for multi-agent operations: `tracing::info_span!("sync", agent_id = %agent_id)`

18+ `eprintln!` call sites across: `db.rs` (3), `daemon.rs` (9+), `commands/create.rs` (4), `commands/session.rs` (3), `shared_writer.rs` (2), `server/watcher.rs` (5+).

### Out of Scope

- Dashboard extraction (covered in separate design doc `.design/dashboard-extraction.md`)
- VS Code extension testing (separate workstream, different language/toolchain)
- Performance benchmarking suite (useful but distinct from correctness/structural work)
- New feature development (this review is purely about hardening existing code)
- Changing the dual-state architecture (event-sourced CRDT + SQLite) — that's a fundamental design decision, not a bug
- Rewriting the CLI argument structure (already covered by `.design/refactor-subcommand-structure.md`)

