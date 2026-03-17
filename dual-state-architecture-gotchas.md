---
title: "Dual-State Architecture Gotchas (CRDT + SQLite)"
tags: [architecture, coordination]
sources: []
contributors: [maxine--basel]
created: 2026-03-17
updated: 2026-03-17
---

# Dual-State Architecture Gotchas

Crosslink maintains two sources of truth: event-sourced CRDT files on a git hub branch, and a materialized SQLite view for fast local queries. This creates a specific class of consistency bugs to watch for.

## The Two Truth Sources

1. **Hub branch** (git): JSON event files, shared across agents via git sync. Authoritative for multi-agent state.
2. **SQLite** (.crosslink/issues.db): Materialized view, local to each agent. Authoritative for local queries and the dashboard API.

**Hydration** (hub -> SQLite) is one-way: `hydration.rs` reads hub files and upserts into SQLite. The CLI writes directly to SQLite for local-only state (sessions, time entries) but writes through SharedWriter for shared state (issues, comments, labels).

## Known Gotcha Categories

### 1. Hub-Write + Hydrate Desync
When SharedWriter pushes to hub but hydration into SQLite fails, the hub has state that local queries don't reflect. 
**Mitigation**: Make SQLite writes atomic; add a "needs-rehydrate" flag checked on next sync.

### 2. Offline Divergence
`shared_writer.rs` saves locally on push failure but has no active reconciliation — relies on "try again later" at next sync.
**Mitigation**: Network partition tests verify convergence after reconnect.

### 3. Concurrent Agent Writes
Two agents can modify the same issue via hub simultaneously. The CRDT event model handles this via total ordering, but the SQLite materialization must correctly replay the merged event log.
**Mitigation**: Adversarial coordination tests with `SmokeHarness::fork_agent()`.

### 4. Clock Skew
Event ordering depends on timestamps. Agents with skewed clocks can produce events that sort incorrectly.
**Mitigation**: `clock_skew.rs` detects and warns; adversarial tests verify behavior under 5-minute skew.

## Rules of Thumb

- **Shared state** (issues, comments, labels, milestones, dependencies): Always go through SharedWriter -> hub -> hydrate. Never write directly to SQLite.
- **Local state** (sessions, time entries, config): Write directly to SQLite. These are not synced.
- **Dashboard reads**: Always from SQLite. Dashboard never reads hub directly.
- **Sync operations**: Always hub -> SQLite direction. Never reverse-hydrate from SQLite to hub.
