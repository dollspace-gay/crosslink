---
title: v0.5.0 Release Gap Analysis
tags: [release, planning]
sources: []
contributors: [maxine--plan-050]
created: 2026-03-10
updated: 2026-03-10
---

## Overview

Gap analysis of v0.5.0 design specification against the codebase, produced 2026-03-10. The design covers 10 requirements (REQ-1 through REQ-10) with 18 acceptance criteria.

## Status Summary

### Resolved (merged on origin/develop)

- **REQ-4** (TUI view state restoration) — PR #296. Saves prev_view_mode when entering detail from tree. Partial: does not save sort order or scroll position per the full ViewState stack in the design. May need follow-up for AC-10 completeness.
- **REQ-5** (TUI scroll clamping) — PR #296. Adds Cell<u16> max_scroll to IssuesTab and KnowledgeTab, computed during render. Uses lines.len() (pre-wrap approximation). G key jumps to max_scroll instead of u16::MAX.
- **REQ-6** (Kickoff agent init/sync) — PR #295. Adds agent status verification step, periodic sync instruction, and final sync before session end in KICKOFF.md template. Better than design spec (avoids prompt-ordering problem with agent_id).
- **REQ-9** (CLI subcommand restructure) — PR #294. New IssueCommands, TimerCommands, MigrateCommands sub-enums. Hidden top-level shortcuts. 186 integration tests pass.

### Remaining (blocking)

- **REQ-1** (Knowledge section edits) — Issue #167. No section parser exists. Must build from scratch: heading-level splitting, replace/append within sections, clap argument groups for --replace-section and --append-to-section flags. ~360 lines estimated.
- **REQ-2** (Kickoff list) — Issue #168. No List variant in KickoffCommands. Needs unified discovery across worktrees, tmux sessions, and Docker containers. Highest risk item (~380 lines). Docker label assumption needs verification.
- **REQ-3** (TUI sync) — Issue #169. No sync integration in TUI. Needs startup sync, periodic 30s background sync (reuse agents_tab mpsc pattern), global r keybinding. ~130 lines estimated.

### Remaining (advisory)

- **REQ-7** (Branch protection) — Issue #170. GitHub rulesets config + docs/RELEASING.md.
- **REQ-8** (Repo cleanup) — Issue #171. Migrate 5 design docs to knowledge, move ARCHITECTURE.md + ELI5.md to docs/, remove test scripts + .plan/, commit .crosslink/rules/ (28 files), update LICENSE copyright.
- **REQ-10** (Version bump) — Issue #172. Cargo.toml 0.4.0 -> 0.5.0, CHANGELOG.md update. Final step before release branch.

## Implementation Phases

Phase 1 (parallel): #167 knowledge edits, #168 kickoff list, #170 branch protection
Phase 2 (parallel): #169 TUI sync
Phase 3 (sequential, after all features merged): #171 repo cleanup, then #172 version bump
Phase 4: Cut release/v0.5.0 from develop, PR to main

## Key Architectural Notes

- Knowledge section parser: line-by-line scan splitting on ^#{1,6} regex, tracking heading levels, exact string match for heading names
- Kickoff list: 3 discovery sources — git worktree list --porcelain, tmux list-sessions, docker ps with label filter
- TUI sync: SyncManager on background thread with mpsc channel, same pattern as agents_tab.rs. Bare r key safe (no tab uses it)
- The :r command palette refresh (local disk reload) and r keybinding (remote sync) are distinct operations — both should remain

## Conflicts and Risks

1. kickoff status() fallback overlaps with new List command — keep both, List becomes canonical
2. .crosslink/rules/ untracked + .crosslink/.gitignore modified — coordinate during cleanup
3. TUI scroll clamping uses pre-wrap line count — edge cases with very long paragraphs may truncate
4. AC-10 may be partially satisfied — view mode saved but not sort/scroll position
