---
title: "Module Size and Decomposition Conventions"
tags: [conventions, architecture]
sources: []
contributors: [maxine--basel]
created: 2026-03-16
updated: 2026-03-16
---

# Module Size and Decomposition Conventions

Established from adversarial review v1 (2026-03-16, GH issue #364).

## Size Limits

- **Hard limit**: No source file should exceed 1200 lines (excluding inline test modules)
- **Target range**: 300-1000 lines per module
- **Rationale**: Both human and AI reviewers lose coherence beyond ~1200 lines; decomposition improves parallel work and test isolation

## When to Decompose

A file needs decomposition when it has:
- Multiple independent concerns (e.g., CRUD + sync + offline promotion in shared_writer.rs)
- Types/functions that are only used by one logical subsystem
- Test modules that are hard to reason about because they test unrelated behaviors

## Decomposition Strategy

Convert `foo.rs` to a directory module:
1. Create `foo/mod.rs` — re-exports all public items
2. Move logical groups into submodules (`foo/core.rs`, `foo/mutations.rs`, etc.)
3. Use `pub(crate)` for internal cross-submodule access
4. Callers don't change — `mod.rs` re-exports maintain the same public API

## God Files Identified (March 2026)

| File | Lines | Proposed Split |
|------|-------|---------------|
| shared_writer.rs | ~2300 | core, mutations, milestones, offline |
| commands/kickoff.rs | ~3800 | types, launch, run, monitor |
| db.rs | ~1750 | core, issues, relations, sessions |
| sync.rs | ~1200 | core, operations, integration |
| knowledge.rs | ~1600 | core, pages, sync |
| commands/knowledge.rs | ~1100 | core, edit |
| commands/swarm.rs | ~3400 | types, phase, budget, review_merge |

Batch by coupling: (1) db+sync, (2) shared_writer+knowledge, (3) kickoff+swarm.
