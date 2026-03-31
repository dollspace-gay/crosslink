---
title: "v070-release"
tags: ["release, postmortem"]
sources: []
contributors: ["maxine--basel"]
created: 2026-03-31
updated: 2026-03-31
---

# v0.7.0 Release — QA Audit, Init Upgrades, and Smoke Test Fixes

Released 2026-03-31. 187 files changed, +12,933 / -8,919 lines across 9 PRs.

## What Changed

### QA Audit (PR #527) — the bulk of this release

A full-codebase quality audit touching 127 files with 180+ fixes:

**Security (12 fixes)**: shell injection in hook commands, fail-open hooks that silently allowed blocked operations, allow-list bypass via git flag injection, MD5→SHA256 for integrity checks, server bound to localhost only, bearer auth on all API endpoints, restrictive temp file permissions, YAML injection in config, path traversal in knowledge pages, CORS tightened.

**Correctness (50+)**: resolve_id edge cases, signing oracle, timer corruption on concurrent stop, transaction safety gaps, hydration data loss on interrupted sync, non-atomic writes to hub files, TOCTOU races in lock acquisition, V1/V2 dispatch errors, lock release on panic, hub write lock contention, DAG state machine invalid transitions, clock skew detection, conflict detection in compaction, enum type safety.

**Architecture (60+)**: tokio Mutex replaced with std Mutex where async not needed, N+1 query patterns in list commands, shared error helpers for server handlers, config registry extraction from monolithic config.rs, init.rs decomposition (was ~2000 lines, now split into mod.rs + merge.rs + python.rs + signing.rs + walkthrough.rs), DRY extractions across TUI tabs, typed API enums (ApiPriority vs Priority), LockMode enum, hook function splits.

### New Features

- **`crosslink init --update`** (PR #534) — Manifest-tracked safe upgrades. Tracks which version of hooks, skills, and rules were installed. On re-run, applies incremental updates without overwriting user customizations.
- **Shell/Bash language support** (PR #531) — First-class rules, auto-detection, and hook configuration for shell scripts.
- **QA skill** (PR #524) — `/qa` architectural review skill ships with `crosslink init`.

### Bug Fixes

- **Hub write-lock recovery loop** (PR #537, GH #528) — `.hub-write-lock` is a runtime PID file but was tracked in git. Every sync cycle: acquire lock (write PID) → drop (delete file) → dirty state detected → recovery commit → cannot push → repeat. Produced 274 recovery commits after a crash. Fixed by gitignoring the lock file on the hub branch.
- **Swarm merge --base** (PR #539) — Repos without a `develop` branch could not use swarm merge.
- **`gh` in allowed bash prefixes** (PR #538) — GitHub CLI was blocked by the work-check hook.
- **Signing bypass** (PR #536) — Hub cache commits were inconsistently bypassing GPG signing.

## Smoke Test Regression Patterns

The QA audit introduced 42 smoke test failures that were fixed during the release. These patterns are worth knowing for future maintenance:

1. **Bearer auth on server API** — The QA audit added `Authorization: Bearer <token>` middleware to all API routes (except `/health` and `/ws`). Smoke tests were sending raw HTTP without auth → 401. Fix: capture auth token from server stdout during startup, pass in test requests.

2. **Agent identity auto-creation** — `crosslink init --defaults` now auto-creates an agent identity (agent.json + SSH key). Tests that subsequently called `agent init <id>` got "Agent already configured." Fix: add `--force` flag to test agent init calls.

3. **Milestone hub dependency** — Milestone operations now go through SharedWriter and require the hub cache to be initialized. Tests creating milestones without prior `crosslink sync` got "Sync cache not initialized." Fix: add `sync` call before milestone operations in tests.

4. **Priority enum mismatch** — The server API uses `ApiPriority` (low/medium/high only) while the CLI uses `Priority` (low/medium/high/critical). A test sending `"critical"` via the API got 422 Unprocessable Entity. Fix: use `"high"` in API tests.

## Architectural Changes Worth Knowing

- **`status.rs` → `lifecycle.rs`** — The issue lifecycle command was renamed for clarity.
- **`server/errors.rs`** — New shared error response helpers, replacing per-handler error construction.
- **`config_registry.rs`** — Config validation and registry logic extracted from the monolithic config.rs.
- **`init/` module** — init.rs was the largest file in the codebase. Now decomposed into focused submodules.

## Known Issues Carried Forward

- Dashboard frontend not included in `cargo install crosslink` (GH #429)
- API `critical` priority gap (CLI supports it, API does not) — tracked but intentional simplification in the typed API layer
