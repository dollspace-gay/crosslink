---
title: "Allow crosslink knowledge and issue commands to query external sources"
tags: [design-doc]
sources: []
contributors: [maxine--basel]
created: 2026-03-17
updated: 2026-03-17
---


## Design Specification

### Summary

Enable `crosslink knowledge` and `crosslink issue` read commands to query data from other repositories — either by fetching a remote repo's `crosslink/knowledge` and `crosslink/hub` branches, or by reading from another local repo's `.crosslink` data. This enables agent-to-agent knowledge transfer: an agent in repo A can query the crosslink trail from repo B to understand how and why code was built.

### Requirements

- REQ-1: `crosslink knowledge search/show/list` must accept a flag to specify an external repository, resolving it to a fetchable `crosslink/knowledge` branch and reading pages from the fetched tree.
- REQ-2: `crosslink issue search/show/list` must accept the same flag, resolving to a fetchable `crosslink/hub` branch and reading `IssueFile` JSON from the `issues/` directory.
- REQ-3: External queries must be strictly read-only — no writes, no pushes, no modifications to the external repo's data.
- REQ-4: The CLI flag must not collide with the existing `--source` flag on `knowledge search` (which filters by source URL domain via `KnowledgeManager::search_sources()`). A new flag name is required.
- REQ-5: Remote data must be cached locally with a configurable TTL to avoid repeated fetches. Cache location must be under `.crosslink/` and isolated per external source.
- REQ-6: External data must be visually distinguished from local data in CLI output — via a labeled header/footer for human output, a prefix for individual results, and a `source` field in `--json` output.
- REQ-7: Named aliases for frequently-used external sources must be configurable, so users can write `--repo @upstream` instead of a full URL.
- REQ-8: Authentication must leverage existing git credentials — no new credential management.
- REQ-9: The MCP knowledge server (`.claude/mcp/knowledge-server.py`) must gain a `source` parameter on its `search_knowledge` tool, passing through to `crosslink knowledge search --repo <value>`.
- REQ-10: The REST API (`server/handlers/`) must NOT gain external source support — it serves only local repository data.

### Acceptance Criteria

- [ ] AC-1: `crosslink knowledge search "auth" --repo github.com/org/other-repo` fetches the remote's `crosslink/knowledge` branch, caches it locally, and returns matching pages with a `--- Results from github.com/org/other-repo ---` banner (validates REQ-1, REQ-6).
- [ ] AC-2: `crosslink knowledge show page-name --repo /path/to/local/repo` reads from the local repo's knowledge cache and displays the page (validates REQ-1).
- [ ] AC-3: `crosslink issue search "migration" --repo github.com/org/other-repo` fetches the remote's `crosslink/hub` branch, deserializes `issues/*.json` using `read_all_issue_files()`, and returns matching issues (validates REQ-2).
- [ ] AC-4: `crosslink issue show 42 --repo github.com/org/other-repo` displays the issue with display_id 42 from the external hub data (validates REQ-2).
- [ ] AC-5: `crosslink issue list -s closed --repo /path/to/local/repo` lists closed issues from the external source (validates REQ-2).
- [ ] AC-6: Running `crosslink knowledge add "page" --repo github.com/org/other-repo` produces a clear error: "External sources are read-only" (validates REQ-3).
- [ ] AC-7: `crosslink knowledge search "auth" --source rust-lang.org --repo github.com/org/other-repo` correctly combines both flags: searches external pages filtered by source URL domain (validates REQ-4).
- [ ] AC-8: A second invocation of `--repo github.com/org/other-repo` within the TTL window does not trigger a `git fetch` (validates REQ-5).
- [ ] AC-9: `crosslink config set repo-alias.upstream github.com/org/other-repo` followed by `crosslink knowledge search "auth" --repo @upstream` works identically to using the full URL (validates REQ-7).
- [ ] AC-10: `--json` output includes `"source": "github.com/org/other-repo"` on each result object; `--quiet` output omits the banner but preserves the data (validates REQ-6).
- [ ] AC-11: The MCP tool `search_knowledge` accepts an optional `source` parameter and returns results from the specified external repo (validates REQ-9).
- [ ] AC-12: REST API endpoints (`GET /api/v1/knowledge`, `GET /api/v1/issues`) do not accept a source/repo parameter and continue to serve only local data (validates REQ-10).

### Architecture

### Flag naming: `--repo`

The existing `--source` flag on `knowledge search` (defined at `main.rs:1167`, dispatched to `KnowledgeManager::search_sources()` at `knowledge.rs:612`) means "filter by source URL domain." To avoid collision, the external source flag is named `--repo`. This is:
- Unambiguous: it clearly refers to a repository, not a metadata filter
- Composable: `--source` and `--repo` can be used together (AC-7)
- Consistent: the flag refers to the same concept across both `knowledge` and `issue` commands

The `--repo` flag is added to: `KnowledgeCommands::Search`, `KnowledgeCommands::Show`, `KnowledgeCommands::List`, `IssueCommands::Search`, `IssueCommands::Show`, `IssueCommands::List` in `main.rs`.

### Source resolution

A `--repo` value is resolved in this order:

1. **Named alias**: If the value starts with `@`, look up `repo-alias.<name>` in `crosslink config` (stored in `.crosslink/config.toml`). Example: `@upstream` → `github.com/org/other-repo`.
2. **Local path**: If the value is a path that exists on disk and contains `.crosslink/` or `.git/`, treat it as a local repository.
3. **Git URL**: Otherwise, treat it as a git remote URL (supports `github.com/org/repo` shorthand, `https://`, `git@` formats).

This resolution logic lives in a new module `src/external.rs`.

### Named aliases

Aliases are managed via `crosslink config`:

```bash
crosslink config set repo-alias.upstream github.com/forecast-bio/other-repo
crosslink config set repo-alias.ml-core /Users/maxine/code/forecast/ml-core
crosslink config list repo-alias    # show all aliases
crosslink config unset repo-alias.upstream
```

This uses the existing `crosslink config` infrastructure (`commands/config.rs`) which stores key-value pairs in `.crosslink/config.toml`. The `repo-alias` namespace is a convention — no schema changes needed. This is preferable to `hook-config.json` because:
- It's discoverable via `crosslink config list`
- It uses the same set/get/unset verbs users already know
- `hook-config.json` is for hook behavior, not user preferences

### External data access: knowledge

For knowledge queries against an external source:

1. **Resolve source** → git URL or local path
2. **Fetch/update cache**: For remote URLs, `git fetch <url> crosslink/knowledge` into a bare ref under `.crosslink/.external-cache/<hash>/knowledge/`. For local repos, read directly from their `.crosslink/.knowledge-cache/` (or set up a worktree from their branch).
3. **Construct a `KnowledgeManager`-like reader**: Rather than modifying `KnowledgeManager::new()` (which is tightly coupled to the local repo root at `knowledge.rs:136-155`), introduce an `ExternalKnowledgeReader` that takes a cache directory path and exposes `list_pages()`, `search_content()`, `search_sources()`, and `show_page()`. These can largely reuse the existing parsing functions (`parse_frontmatter()` at `knowledge.rs:708`, `search_content()` at `knowledge.rs:523`) by extracting them into standalone functions that operate on a directory path.
4. **Return results** with source annotation.

### External data access: issues

For issue queries against an external source:

1. **Resolve source** → git URL or local path
2. **Fetch/update cache**: `git fetch <url> crosslink/hub` into `.crosslink/.external-cache/<hash>/hub/`.
3. **Read issue files directly**: Use `read_all_issue_files()` from `issue_file.rs` against the cached hub tree's `issues/` directory. This returns `Vec<IssueFile>` — the same struct used for local hydration (defined at `issue_file.rs:13-45`).
4. **Search/filter in memory**: Since we're not hydrating into SQLite, implement search and filtering directly over `Vec<IssueFile>`:
   - `search`: case-insensitive substring match on title, description, and comment content (mirroring `db.rs:1087-1108`)
   - `list`: filter by status, label, priority (mirroring `db.rs:489-539`)
   - `show`: find by display_id
5. **Return results** with source annotation.

This avoids creating a temporary SQLite database, keeping the external read path simple and stateless.

### Cache management

External source data is cached under `.crosslink/.external-cache/`:

```
.crosslink/.external-cache/
  <sha256-of-repo-url>/
    knowledge/     # bare checkout of crosslink/knowledge branch
    hub/           # bare checkout of crosslink/hub branch
    meta.json      # { "url": "...", "last_fetched": "...", "ttl_seconds": 300 }
```

- Default TTL: 5 minutes (configurable via `crosslink config set external-cache-ttl 600`)
- `meta.json` tracks when the last fetch occurred; if within TTL, skip fetch
- For local paths, no caching — read directly from the source repo's existing worktree/cache
- Cache can be cleared with `crosslink config unset` or by deleting the directory

### Output formatting

**Human output (default)**:
```
--- Results from github.com/org/other-repo ---

  auth-middleware (line 12):
    11 | The auth middleware validates JWT tokens...
    12 | ...using the RS256 algorithm with key rotation.
    13 |

--- End external results ---
```

**JSON output (`--json`)**:
Each result object gains a `"source"` field:
```json
{
  "slug": "auth-middleware",
  "line_number": 12,
  "context_lines": ["..."],
  "source": "github.com/org/other-repo"
}
```

**Quiet output (`--quiet`)**: No banner, just data. Source info only in `--json`.

### New module: `src/external.rs`

This module contains:
- `resolve_repo(value: &str, config: &Config) -> Result<RepoSource>` — alias/path/URL resolution
- `enum RepoSource { Local(PathBuf), Remote(String) }`
- `ExternalKnowledgeReader` — reads knowledge pages from an arbitrary directory
- `ExternalIssueReader` — reads and filters `IssueFile` structs from an arbitrary `issues/` directory
- `ExternalCache` — manages fetch, TTL, and cache directory lifecycle
- Cache hashing: `sha256(canonical_url)` truncated to 16 hex chars for directory name

### MCP integration

Update `.claude/mcp/knowledge-server.py`:
- Add `source` parameter to the `search_knowledge` tool definition
- When `source` is provided, pass `--repo <value>` to the `crosslink knowledge search` CLI call
- Add a new tool `search_external_issues` with parameters `query` (required) and `source` (required)

### Commands that reject `--repo`

Write commands (`knowledge add/edit/remove/sync/import`, all `issue` mutation commands) reject `--repo` with error: `"External sources are read-only. The --repo flag is only supported on read commands."` This is enforced at the clap argument level using `conflicts_with` on the write-specific args, or at dispatch time in the command handler.

### Out of Scope

- Writing to external sources (creating issues, adding knowledge pages remotely)
- REST API support for external sources (the web dashboard serves local data only)
- Bidirectional sync or replication between repositories
- External source support for non-read commands (timer, session, milestone, etc.)
- Automatic discovery of related repositories (must be explicitly specified)
- External source support for hub branch mutations (locks, agent state, events)

