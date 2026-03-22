---
title: "Add `crosslink kickoff graph` to show branch topology of in-progress kickoffs"
tags: [design-doc]
sources: []
contributors: [maxine--basel]
created: 2026-03-21
updated: 2026-03-22
---

# Feature: Add `crosslink kickoff graph` to show branch topology of in-progress kickoffs

## Summary

Add a `crosslink kickoff graph` subcommand that renders an ASCII branch topology of all kickoff feature branches relative to `develop` and `main`. The output is a clean, annotated topology map — not a full git log — focused on showing where each agent sits in the branch tree and how to interact with it (tmux session name, docker container, or status).

## Requirements

- REQ-1: `crosslink kickoff graph` renders an ASCII graph showing the branching topology of all active kickoff feature branches relative to the base branches (`develop`, `main`).
- REQ-2: Each branch tip is annotated with the most actionable metadata: tmux session name if active, docker container name if docker-only, or status (`done`, `stopped`, `timed-out`) if no live session.
- REQ-3: Orphaned feature branches (branches matching the compact naming pattern that have no worktree) are included when `--all` is passed, annotated with `[orphan]`.
- REQ-4: The graph shows topology only — no commit hashes, no commit messages. Intermediate commits between a fork point and a branch tip are shown as unlabeled `*` nodes to convey "work happened here."
- REQ-5: Base branches (`develop`, `main`, `HEAD`) are always included as anchor points in the graph.
- REQ-6: The graph is built from raw git data (`git rev-list`, `git merge-base`, `git for-each-ref`) rather than post-processing `git log --graph` output.
- REQ-7: With zero active kickoffs, the command shows only the base branches and exits cleanly.
- REQ-8: The graph renderer detects terminal width via `crossterm::terminal::size()` and truncates branch names and annotations to fit without wrapping. Falls back to 80 columns if detection fails.

## Acceptance Criteria

- [ ] AC-1: `crosslink kickoff graph` prints an ASCII branch graph to stdout showing active kickoff branches forking from base branches.
- [ ] AC-2: Branch tips are labeled with the branch name and an annotation bracket: `[tmux: <session>]`, `[docker: <container>]`, `[done]`, `[stopped]`, `[timed-out]`, or `[orphan]`.
- [ ] AC-3: `crosslink kickoff graph --all` includes completed/stopped agents whose worktrees still exist AND orphaned `feature/*` branches matching the compact naming pattern (`\w{4}-\w{4}-.+`).
- [ ] AC-4: Intermediate commits between a fork point and branch tip are rendered as unlabeled `*` nodes (no hash, no subject).
- [ ] AC-5: `develop` and `main` (when they exist) always appear as anchor nodes in the graph.
- [ ] AC-6: When no kickoff branches exist, the command prints the base branches and exits with code 0.
- [ ] AC-7: The `--json` flag outputs the topology as structured JSON (branch name, parent, fork point, agent metadata) for machine consumption.
- [ ] AC-8: The `--no-pager` flag is accepted (no-op in V1, reserved for future use).
- [ ] AC-9: A new `Graph` variant is added to `KickoffCommands` in `main.rs` and dispatched through `kickoff::dispatch()`.
- [ ] AC-10: Branch names and annotations are truncated to fit the detected terminal width (or 80 columns as fallback) without line wrapping.

## Architecture

### Command registration

A new `Graph` variant is added to the `KickoffCommands` enum in `main.rs:1302-1449`:

```rust
/// Show branch topology of kickoff feature branches
Graph {
    /// Include completed, stopped, and orphaned branches
    #[arg(long)]
    all: bool,
    /// Reserved for future pager support (no-op in V1)
    #[arg(long)]
    no_pager: bool,
},
```

Dispatch in `kickoff/mod.rs:48-183` routes to a new `graph()` function.

### New file: `crosslink/src/commands/kickoff/graph.rs`

This module contains the graph command implementation. It is structured in three phases:

**Phase 1 — Collect refs.** Query active kickoff branches via `discover_agents()` (`monitor.rs:122-258`), which scans `.worktrees/` for sentinel files and reconciles with tmux/docker. When `--all` is set, also run `git for-each-ref refs/heads/feature/` and match branch names against the compact naming pattern (`<repo_id>-<agent_id>-<slug>`) to find orphaned branches with no worktree.

**Phase 2 — Build topology.** For each collected branch, compute:
- The fork point relative to base branches using `git merge-base --fork-point <base> <branch>` (trying `develop` then `main`, mirroring the base-ref cascade in `swarm/merge.rs:36`).
- The number of intermediate commits between fork point and tip using `git rev-list --count <fork-point>..<tip>`.
- The commit at the branch tip using `git rev-parse <branch>`.

Assemble these into a `BranchNode` struct:

```rust
struct BranchNode {
    branch_name: String,
    fork_point: String,     // commit hash where it diverges from base
    base_branch: String,    // which base it forked from
    tip_commit: String,     // commit hash at tip
    intermediate_count: usize,
    annotation: Annotation,
}

enum Annotation {
    Tmux(String),           // session name
    Docker(String),         // container name
    Status(String),         // done, stopped, timed-out, failed
    Orphan,
}
```

**Phase 3 — Render ASCII.** Walk the topology and render a text graph. Branches sharing the same fork point are grouped. The renderer:
- Draws base branches as the trunk (`develop`, `main`).
- Draws each feature branch forking off at the appropriate point.
- Shows `intermediate_count` unlabeled `*` nodes between fork and tip.
- Labels the tip with `<branch_name>  [<annotation>]`.

The renderer does not need to handle complex merge topologies — kickoff branches are linear forks off a base branch. This keeps the ASCII rendering straightforward: each branch is a vertical column that merges back to the trunk at its fork point.

### Integration points

- **`discover_agents()`** (`monitor.rs:122-258`): Reused directly for agent metadata. The `AgentInfo` struct (`types.rs:257-264`) provides `id`, `issue`, `status`, `session` (tmux), `docker`, and `worktree` fields.
- **`tmux_session_name()`** (`helpers.rs:299`): Already computes the tmux session name from a slug — used to display the actionable session name.
- **`truncate_str()`** (`helpers.rs:586-592`): Reused for branch name truncation in narrow terminals.
- **Compact naming pattern** (`utils.rs:182-202`): `compose_compact_name()` and `validate_compact_name()` define the naming format used to identify kickoff branches among all `feature/*` branches.
- **`crossterm::terminal::size()`**: Already a dependency (`Cargo.toml:41`, used in `wizard.rs:4`). Returns `(cols, rows)` for terminal width detection. Falls back to 80 columns on failure (piped output, non-TTY).

### Output format

Default (ASCII):
```
  * feature/XZ3j-81jF-add-auth-fe2d      [tmux: agent-add-auth-fe2d]
  *
  |
  | * feature/XZ3j-92kG-fix-sync-a1b2    [done]
  |/
  | * feature/XZ3j-73hK-batch-retry-c3d4 [orphan]
  |/
  * develop
  |
  * main
```

JSON (`--json`):
```json
{
  "base_branches": ["develop", "main"],
  "kickoff_branches": [
    {
      "branch": "feature/XZ3j-81jF-add-auth-fe2d",
      "base": "develop",
      "intermediate_commits": 2,
      "annotation": { "tmux": "agent-add-auth-fe2d" }
    }
  ]
}
```

### Error handling

- If `git` is not available, bail with a clear error (consistent with `preflight_check()` in `launch.rs`).
- If a branch's fork point cannot be determined (detached from all base branches), skip it with a warning on stderr.
- If `.worktrees/` does not exist, treat as zero agents (REQ-7).

## Open Questions

No open questions remain.

## Out of Scope

- SVG rendering (`--svg` flag) — deferred to follow-up.
- Pager integration — V1 prints directly to stdout; `--no-pager` is accepted but is a no-op.
- Color output by kickoff status (green/red/blue) — potential follow-up enhancement.
- Integration with `crosslink tui` Agents panel — separate feature.
- Ahead/behind commit counts relative to base branches.
- Commit hashes or commit messages in the graph output.
