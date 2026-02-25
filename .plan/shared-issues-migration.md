# Shared Issues on Git Coordination Branch

Migration from local SQLite `issues.db` to git-mergeable JSON files on the
`crosslink/locks` orphan branch, enabling multi-agent issue coordination with
conflict-free merges.

## Design Principles

1. **One file per issue** — git merges of changes to different files are always clean
2. **Locks guarantee exclusive writes** — no two agents mutate the same issue file
3. **Local SQLite becomes a read cache** — rebuilt from JSON on fetch, preserves fast queries
4. **Graceful degradation** — single-agent mode (no agent.json) keeps working with local-only SQLite
5. **Sessions stay local** — they're machine-specific state, not shared

## Branch Layout

```
crosslink/locks branch (renamed conceptually to "coordination branch"):
  locks.json                    # existing — issue lock assignments
  heartbeats/{agent_id}.json    # existing — agent liveness
  trust/keyring.json            # existing — GPG trust
  issues/{uuid}.json            # NEW — one file per issue
  meta/
    counters.json               # NEW — next display_id, next comment_id
    milestones.json             # NEW — milestone definitions
    labels.json                 # NEW — label registry (optional, for discovery)
```

## Issue File Format

Each issue is a self-contained JSON file at `issues/{uuid}.json`:

```json
{
  "uuid": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "display_id": 42,
  "title": "Fix auth timeout",
  "description": "Users see 504 errors after 30s",
  "status": "open",
  "priority": "critical",
  "parent_uuid": null,
  "created_by": "worker-1",
  "created_at": "2026-02-25T14:30:00Z",
  "updated_at": "2026-02-25T15:00:00Z",
  "closed_at": null,
  "labels": ["bug", "auth"],
  "comments": [
    {
      "id": 1,
      "author": "worker-1",
      "content": "Reproduced on staging",
      "created_at": "2026-02-25T15:10:00Z"
    }
  ],
  "blockers": ["f1e2d3c4-..."],
  "blocking": ["b5a6c7d8-..."],
  "related": ["e9f0a1b2-..."],
  "milestone_uuid": null,
  "time_entries": [
    {
      "id": 1,
      "started_at": "2026-02-25T15:00:00Z",
      "ended_at": "2026-02-25T16:00:00Z",
      "duration_seconds": 3600
    }
  ]
}
```

Key decisions:
- **UUIDs as identity**, display_ids as human-friendly aliases
- **All relationships use UUIDs**, not display_ids
- **Comments are inline** — they're always read with their issue, and the lock
  holder is the only writer, so no conflict
- **Labels are inline** — no separate join table needed
- **Time entries are inline** — scoped to one issue
- **Dependencies stored bidirectionally** — both `blockers` and `blocking` arrays,
  kept consistent by the writing agent

## Counters File

```json
{
  "next_display_id": 43,
  "next_comment_id": 157
}
```

- Atomically incremented in each commit that creates an issue or comment
- On push conflict (non-fast-forward): pull --rebase, re-read counter, re-assign IDs
- This is the **only shared mutable state** beyond locks.json, and it's a single small file

## Milestones File

```json
{
  "milestones": {
    "m-uuid-1": {
      "uuid": "m-uuid-1",
      "display_id": 1,
      "name": "v1.0",
      "description": "Initial release",
      "status": "open",
      "created_at": "2026-02-25T10:00:00Z",
      "closed_at": null
    }
  }
}
```

- Issue-to-milestone association lives in the issue file (`milestone_uuid` field)
- Milestone creation/modification is infrequent and typically done by a single coordinator

## Conflict-Free Guarantee

The invariant:
> Every mutation to `issues/{uuid}.json` requires holding the lock on that UUID.
> Locks are exclusive. Therefore no two agents ever modify the same file in
> the same push window.

This means:
- **Different issues modified** → different files → git auto-merges on rebase
- **Same issue modified** → impossible, lock prevents it
- **New issues created** → new files with unique UUIDs → no conflict
- **Counter conflicts** → handled by rebase retry (same pattern as heartbeat push)

## Implementation Phases

---

### Phase 1: Issue JSON Store (core read/write layer)

**New module: `crosslink/src/issue_store.rs`**

Responsibilities:
- Define `IssueFile` struct (the JSON schema above) with serde derives
- `read_issue(cache_dir, uuid)` → deserialize one issue file
- `write_issue(cache_dir, issue_file)` → serialize and write
- `list_issue_files(cache_dir)` → glob `issues/*.json`, return Vec<IssueFile>
- `delete_issue_file(cache_dir, uuid)` → remove file
- `read_counters(cache_dir)` / `increment_counter(cache_dir, field)` → counter management
- `read_milestones(cache_dir)` / `write_milestones(cache_dir)` → milestone CRUD
- UUID generation (uuid crate v4)
- Display ID ↔ UUID index (built in-memory from file scan)

**New module: `crosslink/src/issue_index.rs`**

Responsibilities:
- `IssueIndex` struct: HashMap<i64, Uuid> (display_id → uuid), HashMap<Uuid, IssueFile>
- Build from scanning all issue files
- Query methods: `by_display_id()`, `by_uuid()`, `by_status()`, `by_label()`, `by_priority()`
- Dependency graph traversal: `is_blocked()`, `blockers_of()`, `would_create_cycle()`
- Search: title/description/comment substring matching
- Ready issues: open + no open blockers
- Tree building: parent-child traversal

This replaces all the SQL queries in db.rs with in-memory operations over the
deserialized issue files. The index is rebuilt on every `fetch` — the dataset is
small enough (hundreds to low thousands of issues) that this is instantaneous.

**Tests:**
- Round-trip serialization for every field combination
- Property tests: create N random issues, serialize, rebuild index, verify queries
- Cycle detection on dependency graphs
- Counter increment + conflict simulation

---

### Phase 2: Extend SyncManager for Issue Operations

**Modify: `crosslink/src/sync.rs`**

Add to SyncManager:
- `read_issue(uuid)` → load single issue file from cache
- `write_issue(issue_file)` → write file, stage, commit
- `delete_issue(uuid)` → remove file, stage, commit
- `read_all_issues()` → load all issue files
- `read_counters()` / `write_counters()` → counter operations
- `read_milestones()` / `write_milestones()` → milestone operations
- `push_issues()` → push to remote with rebase-retry on conflict
- `rebuild_index()` → returns `IssueIndex` from current cache state
- `claim_and_write(uuid, agent, issue_file)` → atomic lock-claim + issue-write in one commit

The commit + push flow:
```
1. Stage changed files (issues/{uuid}.json, counters.json if changed)
2. Commit with message: "{agent_id}: {action} #{display_id} {title}"
3. Push to origin/crosslink/locks
4. On rejection: pull --rebase, re-read counters, re-assign if needed, retry push
5. Max 3 retries, then fail with clear error
```

**Extend `init_cache()`:**
- Create `issues/` and `meta/` directories on first init
- Write initial `counters.json` with `{"next_display_id": 1, "next_comment_id": 1}`

**Tests:**
- Write + read roundtrip in tempdir (no git needed)
- Multiple writes to different files don't conflict
- Counter increment simulation

---

### Phase 3: Dual-Mode Database Adapter

**New module: `crosslink/src/store.rs`**

A trait-based adapter that presents a uniform interface regardless of backend:

```rust
pub trait IssueStore {
    fn create_issue(&mut self, title: &str, desc: Option<&str>, priority: &str) -> Result<i64>;
    fn get_issue(&self, display_id: i64) -> Result<Option<Issue>>;
    fn list_issues(&self, status: Option<&str>, label: Option<&str>, priority: Option<&str>) -> Result<Vec<Issue>>;
    fn update_issue(&mut self, display_id: i64, title: Option<&str>, desc: Option<&str>, priority: Option<&str>) -> Result<bool>;
    fn close_issue(&mut self, display_id: i64) -> Result<bool>;
    fn reopen_issue(&mut self, display_id: i64) -> Result<bool>;
    fn delete_issue(&mut self, display_id: i64) -> Result<bool>;
    fn add_label(&mut self, display_id: i64, label: &str) -> Result<bool>;
    fn remove_label(&mut self, display_id: i64, label: &str) -> Result<bool>;
    fn get_labels(&self, display_id: i64) -> Result<Vec<String>>;
    fn add_comment(&mut self, display_id: i64, content: &str) -> Result<i64>;
    fn get_comments(&self, display_id: i64) -> Result<Vec<Comment>>;
    fn add_dependency(&mut self, blocked_id: i64, blocker_id: i64) -> Result<bool>;
    fn remove_dependency(&mut self, blocked_id: i64, blocker_id: i64) -> Result<bool>;
    fn list_ready_issues(&self) -> Result<Vec<Issue>>;
    fn list_blocked_issues(&self) -> Result<Vec<Issue>>;
    fn search_issues(&self, query: &str) -> Result<Vec<Issue>>;
    // ... milestone, relation, time tracking, archive methods
}
```

Two implementations:
- `SqliteStore` — wraps the existing `Database`, delegates all calls. Zero behavior change.
- `SharedStore` — wraps `SyncManager` + `IssueIndex`. Each write operation:
  1. Checks/acquires lock
  2. Modifies the in-memory index + writes JSON file
  3. Commits to the coordination branch
  4. Pushes (with retry)

**Mode selection** (in main.rs):
```rust
let store: Box<dyn IssueStore> = if AgentConfig::load(&crosslink_dir)?.is_some() {
    // Multi-agent mode: use shared store on coordination branch
    Box::new(SharedStore::new(&crosslink_dir)?)
} else {
    // Single-agent mode: use local SQLite (existing behavior)
    Box::new(SqliteStore::new(db))
};
```

This preserves full backward compatibility. If there's no `agent.json`, behavior
is identical to today.

**Sessions stay in SQLite regardless** — they're machine-local state (which agent
started when, what they're working on). The `Session` model gets no changes.

**Tests:**
- Run the full existing test suite against `SqliteStore` → must pass unchanged
- Mirror every test against `SharedStore` in a tempdir with a git repo
- Property tests: random operation sequences produce same results on both backends

---

### Phase 4: Wire Commands to Store Trait

**Modify all command files** to accept `&dyn IssueStore` instead of `&Database`:

The mechanical change is:
1. Every command function signature changes from `db: &Database` to `store: &dyn IssueStore`
2. All `db.foo()` calls become `store.foo()` calls
3. `main.rs` constructs the appropriate store and passes it through

Commands affected:
- `create.rs` — `store.create_issue()`, `store.add_label()`, lock enforcement stays
- `show.rs` — `store.get_issue()`, `store.get_labels()`, `store.get_comments()`, etc.
- `list.rs` — `store.list_issues()`
- `update.rs` — `store.update_issue()`
- `delete.rs` — `store.delete_issue()`
- `comment.rs` — `store.add_comment()`
- `label.rs` — `store.add_label()`, `store.remove_label()`
- `deps.rs` — `store.add_dependency()`, `store.remove_dependency()`, etc.
- `search.rs` — `store.search_issues()`
- `next.rs` — `store.list_ready_issues()`
- `tree.rs` — `store.list_issues()`, `store.get_subissues()`
- `milestone.rs` — all milestone methods
- `relate.rs` — all relation methods
- `timer.rs` — all time tracking methods
- `archive.rs` — archive/unarchive methods
- `export.rs` / `import.rs` — bulk operations
- `session.rs` — stays on `Database` directly for session ops; uses `store` for issue lookups
- `tested.rs` — `store.add_label()`

**main.rs changes:**
- Construct store based on agent config presence
- Pass `&dyn IssueStore` (or `&mut dyn IssueStore` for writes) to each command
- Keep `Database` for session-only operations

**Tests:**
- All existing command tests must pass (they use `setup_test_db()` → SqliteStore)
- Add parallel test suite using SharedStore

---

### Phase 5: Lock Claim/Release Commands

**New commands** (the missing write side from the original commit):

`crosslink locks claim <display_id> [--branch <name>]`:
1. Resolve display_id → uuid
2. Fetch latest locks
3. Check if already locked (fail if locked by other, succeed if locked by self)
4. Write lock entry to `locks.json`
5. Commit and push

`crosslink locks release <display_id>`:
1. Fetch latest locks
2. Verify this agent holds the lock (fail otherwise)
3. Remove lock entry from `locks.json`
4. Commit and push

`crosslink locks steal <display_id>` (for stale lock recovery):
1. Fetch latest locks
2. Verify lock is stale
3. Replace lock entry with this agent
4. Commit and push

**Auto-claim integration:**
- `session work <id>` → auto-claims lock if in multi-agent mode
- `session end` / `close` → auto-releases lock
- `create --work` → auto-claims after creation

---

### Phase 6: Migration Tool

`crosslink migrate-to-shared`:
1. Verify agent config exists
2. Init coordination branch cache
3. Read all issues from local SQLite
4. For each issue: generate UUID, write `issues/{uuid}.json`
5. Write `counters.json` with next IDs
6. Write `milestones.json`
7. Commit all files
8. Push to remote
9. Print summary

`crosslink migrate-from-shared` (reverse):
1. Fetch coordination branch
2. Read all issue files
3. Insert into local SQLite (creating fresh DB if needed)
4. Print summary

---

### Phase 7: Daemon & Hook Updates

**Daemon:**
- Add periodic `fetch` cycle (every N heartbeat cycles) to keep local cache fresh
- After fetch, rebuild index for faster command execution

**Hooks:**
- `session-start.py`: Already runs `crosslink sync` — now also shows shared issue count
- `work-check.py`: Lock warnings already in place — now locks are actually enforceable

---

## Files Changed Summary

### New files:
- `crosslink/src/issue_store.rs` — JSON issue file read/write
- `crosslink/src/issue_index.rs` — in-memory query index
- `crosslink/src/store.rs` — `IssueStore` trait + `SqliteStore` + `SharedStore`
- `crosslink/src/commands/migrate.rs` — migration commands

### Modified files:
- `crosslink/src/sync.rs` — issue/counter/milestone operations on coordination branch
- `crosslink/src/main.rs` — store construction, new commands
- `crosslink/src/commands/*.rs` — all commands: `&Database` → `&dyn IssueStore`
- `crosslink/src/commands/locks_cmd.rs` — claim/release/steal commands
- `crosslink/src/daemon.rs` — periodic fetch cycle
- `crosslink/src/lock_check.rs` — auto-claim on `session work`
- `crosslink/Cargo.toml` — add `uuid` crate

### Unchanged:
- `crosslink/src/db.rs` — kept as-is, wrapped by `SqliteStore`
- `crosslink/src/models.rs` — kept as-is, used by both backends
- `crosslink/src/locks.rs` — kept as-is
- `crosslink/src/identity.rs` — kept as-is

## Risk Mitigations

1. **Data loss during migration** — migration tool is additive (writes JSON from SQLite),
   never deletes the SQLite file. Both can coexist.

2. **Performance regression** — the index rebuild on fetch is O(n) where n is issue count.
   For <10,000 issues this is <100ms. If it becomes a problem, add a local SQLite cache
   that's rebuilt from JSON (Phase 3 already supports this via the trait).

3. **Network dependency** — SharedStore falls back to last-fetched cache state when offline.
   All reads work. Writes are committed locally and pushed when connectivity returns.

4. **Counter conflicts under high concurrency** — bounded retries (3 attempts) with
   exponential backoff. In practice, issue creation is infrequent enough that this
   almost never happens.

5. **Backward compatibility** — no `agent.json` = SqliteStore = identical to today.
   The migration is opt-in per-machine.

## Open Questions

1. **Should the coordination branch be renamed?** `crosslink/locks` is historical.
   `crosslink/coordination` or `crosslink/shared` better reflects the expanded scope.

2. **Should sessions be shared?** Currently local-only. Some teams might want to see
   what other agents are working on. Could add an optional `sessions/` directory on
   the coordination branch.

3. **Should there be a "leader" agent concept?** A designated agent that handles
   milestone management and other low-frequency shared mutations to avoid even the
   small conflict surface on `milestones.json`.

4. **Import/export format** — should `crosslink export` emit the new JSON format
   or keep the current format? Both?
