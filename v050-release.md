---
title: v0.5.0 Release — Implementation and Coordination
tags: [design-doc]
sources: []
contributors: [maxine--basel]
created: 2026-03-10
updated: 2026-03-10
---


## Design Specification

### Summary

Release v0.5.0 encompasses a CLI subcommand restructure (already in-flight via PR), new knowledge edit operations, kickoff agent listing, TUI sync and scroll fixes, kickoff instruction fixes, branch protection improvements, and a repo cleanup pass. This document specifies the unimplemented features and coordinates the sequencing, dependencies, and release checklist across all 8 sub-issues.

### Requirements

- REQ-1: The `crosslink knowledge edit` command must support `--replace-section <heading>` and `--append-to-section <heading>` flags for section-targeted edits.
- REQ-2: `crosslink kickoff list` must enumerate all active and recently completed kickoff agents across tmux sessions, Docker containers, and worktree sentinel files, with `--status`, `--json`, and `--quiet` flags.
- REQ-3: The TUI must sync from the coordination branch on startup before first render, poll for updates periodically during operation, and support a manual refresh keybinding that triggers a full sync (not just a disk cache reload).
- REQ-4: The TUI issue detail view must restore the parent view's exact state (view mode, sort order, scroll position) when navigating back via Escape.
- REQ-5: The TUI knowledge detail view must clamp scroll position so it stops at the bottom of the page content rather than scrolling infinitely into empty space.
- REQ-6: Kickoff agent instruction injection must include `crosslink agent init <agent-id>` and `crosslink sync` commands so that sub-agent comments propagate to the coordination branch.
- REQ-7: Release branch protection must be relaxed to allow direct pushes to `release/*` branches while still requiring CI-passing PRs to merge into `main`.
- REQ-8: Repo cleanup must migrate design docs and policy review to crosslink knowledge, move `ARCHITECTURE.md` and `ELI5.md` to `docs/`, remove root test scripts, update the infographic, and update the LICENSE copyright.
- REQ-9: The CLI subcommand restructure (#236) must be merged — tracked by its own design doc at `.design/refactor-subcommand-structure.md` with an incoming PR.
- REQ-10: CHANGELOG.md must be updated and Cargo.toml version bumped to 0.5.0 before the release branch is cut.

### Acceptance Criteria

- [ ] AC-1: `crosslink knowledge edit <slug> --replace-section "## Architecture" --content "new content"` replaces only that section's content, leaving other sections intact.
- [ ] AC-2: `crosslink knowledge edit <slug> --append-to-section "## Notes" --content "new paragraph"` appends to that section without affecting other sections.
- [ ] AC-3: Section-based edits fail gracefully with a clear error if the heading is not found in the page.
- [ ] AC-4: `crosslink kickoff list` outputs a table of all agents with columns for ID, issue, status, session name, and worktree path.
- [ ] AC-5: `crosslink kickoff list --status running` filters to only active agents.
- [ ] AC-6: `crosslink kickoff list --json` produces machine-readable output matching the global `--json` pattern.
- [ ] AC-7: `crosslink tui` runs `sync` before first render — data shown on launch reflects the latest coordination branch state.
- [ ] AC-8: While the TUI is open, data refreshes automatically on a periodic interval without user interaction.
- [ ] AC-9: Pressing `r` in the TUI triggers a full sync (fetch from coordination branch + hydrate), not just a database re-read.
- [ ] AC-10: In the TUI, entering issue detail from tree view with a specific sort, then pressing Escape, returns to tree view with the same sort and scroll position preserved.
- [ ] AC-11: In the TUI knowledge reader, scrolling down stops when the last line of content is visible — further down-scroll has no effect.
- [ ] AC-12: A kickoff-launched agent that runs `crosslink comment <id> "text" --kind plan` has that comment visible via `crosslink show <id>` from the root repo after the next sync.
- [ ] AC-13: GitHub rulesets allow direct pushes to `release/*` branches while PRs from `release/*` to `main` still require CI checks to pass.
- [ ] AC-14: No `DESIGN-*.md`, `POLICY-REVIEW.md`, `ADR.md`, or `test_*.sh` files remain in the repo root after cleanup.
- [ ] AC-15: `ARCHITECTURE.md` and `ELI5.md` exist in `docs/` with links from README.md.
- [ ] AC-16: `crosslink knowledge show <slug>` can retrieve the migrated design docs and ADR as knowledge pages.
- [ ] AC-17: `cargo test` passes, `cargo clippy` is clean, and CHANGELOG.md reflects all changes in v0.5.0.
- [ ] AC-18: Cargo.toml version reads `0.5.0` and the release branch PR to `main` is ready.

### Architecture

### 1. Knowledge section-based editing (crosslink/src/commands/knowledge.rs)

The existing `edit` subcommand at `crosslink/src/commands/knowledge.rs` (lines 317-413) already supports `--append`, `--content`, `--tag`, and `--source`. Two new flags are added:

- `--replace-section <heading>`: Parse the page body into sections by splitting on markdown headings (`^#{1,6} `). Find the section whose heading matches the argument. Replace its content (everything between this heading and the next heading of equal or higher level) with the value of `--content`. Error if heading not found.
- `--append-to-section <heading>`: Same section lookup, but append `--content` to the end of the section body (before the next heading) rather than replacing.

The section parser is a simple line-by-line scan: track the current heading level and accumulate lines until the next heading of equal/higher level or EOF. This reuses the same frontmatter-aware parsing already in `parse_frontmatter()`. No external markdown parsing crate needed.

Conflict with existing flags: `--content` becomes required when `--replace-section` or `--append-to-section` is used. `--append` (whole-page append) and section-based flags are mutually exclusive — enforce with a clap argument group.

### 2. Kickoff list (crosslink/src/commands/kickoff.rs)

Add a `List` variant to `KickoffCommands` in `crosslink/src/main.rs`. The implementation in `kickoff.rs` unifies three discovery sources:

1. **Worktree scan**: `git worktree list --porcelain` → filter to `.worktrees/` paths → read `.kickoff-status` sentinel file for completion state.
2. **Tmux sessions**: `tmux list-sessions -F '#{session_name} #{session_path}'` → match `feat-*` sessions to worktree paths.
3. **Docker containers**: `docker ps -a --filter label=crosslink-agent=true --format` → match `crosslink-task` label to worktree directory names.

Status determination follows the existing pattern in the `status` subcommand (line 1988-2025): sentinel file → tmux session check → hub heartbeat. The `list` command runs this for every discovered agent and outputs a formatted table (or JSON).

The `--status` flag filters the output. `--quiet` emits only agent IDs (one per line), matching the pattern used by `crosslink list --quiet` for issues.

### 3. TUI sync and refresh (crosslink/src/tui/mod.rs)

**Startup sync**: Before constructing the `App` and entering the event loop (before line 581 in `mod.rs`), call the sync subsystem to pull from the coordination branch and hydrate the local database. Show a brief "Syncing..." message on the terminal during this operation.

**Periodic sync**: Add a `last_sync: Instant` field to the `App` struct. In the main event loop (line 584-612), check if `last_sync.elapsed() > Duration::from_secs(30)`. If so, run sync in the background. To avoid blocking the UI, spawn sync on a background thread and poll for completion in `poll_updates()`. On completion, trigger a `refresh()` on the active tab.

**Manual refresh**: Bind `r` as a global keybinding (not tab-specific). When pressed, run the same sync + refresh cycle immediately, updating `last_sync` to reset the periodic timer.

### 4. TUI view state restoration (crosslink/src/tui/issues_tab.rs)

The current `prev_view_mode` field (line 119) tracks which view to return to, but scroll position and sort state are not saved. Add a `ViewState` struct:

```rust
struct ViewState {
    view_mode: ViewMode,
    scroll_position: u16,
    sort_column: SortColumn,
    sort_direction: SortDirection,
}
```

Push the current `ViewState` onto a `view_stack: Vec<ViewState>` when entering detail view (line 238). On Escape, pop and restore. This naturally handles multi-level navigation (list → tree → detail → back to tree → back to list).

### 5. TUI knowledge scroll clamping (crosslink/src/tui/knowledge_tab.rs)

The `reader_scroll` field (line 52) is a `u16` that increments without bounds. In the render method, calculate `max_scroll = content_lines.saturating_sub(viewport_height)`. Clamp `reader_scroll` to `max_scroll` before rendering. Apply the same clamp in the key handler for Down/PageDown/End (lines 267-295).

The viewport height is available from the `Rect` passed to the render function. Content line count comes from wrapping the rendered paragraph — use `Paragraph::line_count()` if available in the ratatui version, or count newlines in the rendered text.

### 6. Kickoff instruction injection fix (crosslink/src/commands/kickoff.rs)

In the prompt builder (lines 722-819), add two commands to the injected instruction sequence, placed before `crosslink session start`:

```
crosslink agent init {agent_id} -d "Agent for {issue_title}"
crosslink sync
```

The `agent_id` is already derived at line 1484 as `"{parent_id}--{slug}"`. The sync ensures the local database has the latest state before the agent begins work. These two lines are inserted into the KICKOFF.md template that gets written to the worktree root (line 1804).

### 7. Branch protection (GitHub settings, not code)

Create a separate GitHub ruleset for `release/*` branches:
- Allow direct pushes (no required status checks for pushes to the branch itself)
- Require PR with passing CI to merge into `main` (enforced by the existing `main` branch ruleset)

Update the existing ruleset that currently covers `release/**` to exclude those branches, or narrow its target to only `develop` and `feature/**`.

Document the release flow in a `docs/RELEASING.md` file:
1. Back-merge `main` → `develop` to resolve any conflicts from previous releases
2. Cut `release/vX.Y.Z` from `develop`
3. Version bump, changelog, any final fixes — push directly to the release branch
4. PR `release/vX.Y.Z` → `main` — requires CI to pass
5. Merge, tag, publish

### 8. Repo cleanup (multiple files)

This is the final step before release. Sequence:
1. Migrate `DESIGN-CONTAINER-AGENTS.md`, `DESIGN-EVENT-SOURCED-COORDINATION.md`, `DESIGN-SWARM-INTROSPECTION.md`, `POLICY-REVIEW.md` → `crosslink knowledge add` with historical dates in frontmatter
2. Migrate `ADR.md` → `crosslink knowledge add "adr-adversarial-review" --tag adr` with original creation date
3. Move `ARCHITECTURE.md` → `docs/ARCHITECTURE.md`, update README link
4. Move `ELI5.md` → `docs/ELI5.md`, update README link
5. Remove or relocate `test-intervention.sh`, `test-plan.sh` (move to `scripts/` if still useful, delete if superseded)
6. Remove `.plan/` directory if present
7. Clean up accidentally committed `.claude/` worktree artifacts
8. Audit `.gitignore`: remove `.crosslink/rules/` entry, ensure `rules.local/` is ignored
9. Commit default `.crosslink/rules/` files
10. Update infographic
11. Update LICENSE copyright year/holder

### Implementation sequencing

```
Phase 1 — Independent features (can run in parallel):
  PR for #236 (CLI restructure) ← already in-flight
  #264 (knowledge section edits)
  #270 (kickoff list)
  #289 (kickoff agent init fix)
  #287 (branch protection ruleset change)

Phase 2 — TUI fixes (can run in parallel, depend on nothing):
  #281 (TUI sync)
  #293 (TUI scroll + view state)

Phase 3 — Integration and cleanup (after all code features merged):
  #291 (repo cleanup)
  Merge all feature branches to develop
  Version bump + CHANGELOG

Phase 4 — Release:
  Back-merge main → develop (if needed)
  Cut release/v0.5.0 from develop
  PR to main, tag, publish
```

### Files modified (per feature)

| Feature | Files |
|---------|-------|
| #236 CLI restructure | `crosslink/src/main.rs`, `crosslink/src/commands/create.rs` (see `.design/refactor-subcommand-structure.md`) |
| #264 Knowledge edits | `crosslink/src/commands/knowledge.rs`, `crosslink/src/main.rs` (add flags to KnowledgeCommands::Edit) |
| #270 Kickoff list | `crosslink/src/commands/kickoff.rs`, `crosslink/src/main.rs` (add KickoffCommands::List) |
| #281 TUI sync | `crosslink/src/tui/mod.rs` (startup sync, periodic timer, `r` keybinding) |
| #289 Kickoff init | `crosslink/src/commands/kickoff.rs` (prompt builder, lines 722-819) |
| #293 TUI scroll/view | `crosslink/src/tui/issues_tab.rs` (ViewState stack), `crosslink/src/tui/knowledge_tab.rs` (scroll clamp) |
| #287 Branch protection | `.github/` rulesets (GitHub settings), `docs/RELEASING.md` (new) |
| #291 Repo cleanup | Root `.md` files, `.gitignore`, `.crosslink/rules/`, `docs/`, `README.md`, `LICENSE` |

### Out of Scope

- Line-based or regex-based knowledge editing — rejected in favor of section-based approach
- Knowledge `patch` or `insert` subcommands — covered by section-based `edit` flags instead
- Flag aliases (`--reason` → `--notes`, etc.) — follow-up to v0.5.0
- `--force` short flag standardization — follow-up
- Web dashboard (#290) feature expansion — already merged as Phase 1, future iterations are separate
- Export/import formatting modernization — separate issue
- Kickoff screenshot capture on `kickoff list` — stretch goal, not required for v0.5.0

### resolved questions

### Q1: Scope of this design doc
**Decision**: Hybrid — full architectural spec for unimplemented features, plus coordination plan for sequencing and release. The CLI restructure (#236) defers to its own design doc.

### Q2: Knowledge CLI edit granularity
**Decision**: Section-based operations. Add `--replace-section` and `--append-to-section` flags to the existing `edit` command. Line-based and regex-based approaches rejected due to fragility with shifting content.

### Q3: TUI sync strategy
**Decision**: All three — sync on startup, periodic background sync every 30s, and manual `r` keybinding that triggers a full sync (not just disk re-read).

### Q4: Repo cleanup ordering
**Decision**: Last, after all code features are merged. This avoids merge conflicts with in-flight feature branches.

### Q5: Branch protection approach
**Decision**: Separate GitHub ruleset for `release/*` that allows direct pushes but still requires CI-passing PR to merge into `main`. Combined with the habit of back-merging `main` into `develop` before cutting releases.

