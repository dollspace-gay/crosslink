# Design: Event-Sourced CRDT Coordination

**Epic:** [GH #113](https://github.com/forecast-bio/crosslink/issues/113)
**Status:** Draft v2 — revised for 50+ agent scale
**Last updated:** 2026-03-01

---

## 1. Problem Statement

The current coordination model uses a shared `locks.json` file and monolithic `issues/{uuid}.json` files on the `crosslink/hub` branch. This works for single-machine multi-agent setups but breaks down in two key scenarios:

1. **Worktree isolation**: Parallel agents in git worktrees deadlock because hooks can't resolve shared crosslink state (see #111). The `find_crosslink_dir()` function walks up the directory tree looking for `.crosslink/`, which doesn't exist in worktrees — only in the main repo.

2. **Container isolation**: Agents running in containers have no shared filesystem at all. They need a coordination mechanism that works entirely through git.

The current `SharedWriter` model (read JSON → modify → commit → push with 3-retry rebase) is also fragile under contention. With ~6 agents across two developers, manual edits already appear from failed rebases.

### What works today

- Hub branch as coordination layer (git as transport)
- Per-UUID issue files (no issue-level conflicts between agents)
- Heartbeat-based stale lock detection
- Signed commits and comment signatures for audit trail
- Offline issue creation with negative display IDs and promotion

### What doesn't

- `locks.json` is a single file — all lock operations serialize
- Comments are inline in issue JSON — commenting on the same issue conflicts
- Counter allocation (`meta/counters.json`) is a hot contention point
- `SharedWriter.write_commit_push()` does read-modify-write cycles that race
- No way for an agent to write without potentially conflicting with another agent

---

## 2. Consistency Tier Model

This is the key insight from #113 — not all operations need the same consistency guarantees.

| Tier | Operations | Requirement | Current mechanism |
|------|-----------|-------------|-------------------|
| **T1: Exclusive** | Lock acquire/release, display ID assignment, trust store changes | Exactly-once, atomic. Two agents cannot both win. | `locks.json` + push retry |
| **T2: Causal** | Status transitions, dependency changes, milestone assignment | Must see prior state. Independent entities don't conflict. | `SharedWriter` read-modify-write |
| **T3: Eventually consistent** | Comments, labels, heartbeats, sessions, interventions | Delay is fine. Stale reads cause no damage. | Inline in issue JSON (conflicts!) |

**Key observation:** T3 operations (the majority — probably 80%+ of all writes) can be made **conflict-free by file layout alone**. If each comment is its own file, two agents commenting on the same issue never conflict. No event system needed for these.

T1 and T2 operations are where the event-sourced model earns its keep.

---

## 3. Proposed Hub Branch Layout

```
crosslink/hub:
├── agents/
│   └── {agent-id}/
│       ├── heartbeat.json              # Direct write (T3)
│       ├── session.json                # Direct write (T3)
│       ├── events.log                  # Append-only event log (T1+T2 ops)
│       └── time/
│           └── {issue-uuid}.json       # Direct write (T3, per-agent time tracking)
│
├── checkpoint/
│   ├── state.json                      # Compacted state snapshot
│   └── watermark                       # Ordering key of last compacted event
│
├── issues/
│   └── {uuid}/
│       ├── issue.json                  # Materialized by compaction (T1+T2)
│       └── comments/
│           └── {comment-uuid}.json     # Direct write (T3, new file = no conflict)
│
├── locks/
│   └── {display-id}.json              # Materialized by compaction (T1)
│
├── trust/
│   ├── allowed_signers
│   └── approval_log.json
│
└── meta/
    └── milestones/
        └── {uuid}.json
```

### Key changes from current layout

| Current | New | Why |
|---------|-----|-----|
| `issues/{uuid}.json` (monolithic) | `issues/{uuid}/issue.json` + `comments/` | Comments as separate files = conflict-free |
| `locks.json` (single file) | `locks/{display-id}.json` (materialized) | Per-lock files, but populated by compaction |
| (none) | `agents/{id}/events.log` | Per-agent append-only event stream |
| (none) | `checkpoint/state.json` + `watermark` | Compacted state snapshot for fast reads |
| `meta/counters.json` | Absorbed into `checkpoint/state.json` | Counter allocation via compaction, not CAS |
| `heartbeats/{id}.json` | `agents/{id}/heartbeat.json` | Grouped under agent directory |

### Agent isolation property

Each agent writes **only** to:
- `agents/{own-id}/*` (heartbeat, session, events)
- `issues/{uuid}/comments/{new-uuid}.json` (new files only)

No two agents ever write to the same file. This is the core property that eliminates git merge conflicts.

---

## 4. Event Model

### 4.1 Event envelope

Every event carries:

```rust
struct EventEnvelope {
    agent_id: String,           // Who emitted it
    agent_seq: u64,             // Per-agent monotonic counter (starts at 1)
    timestamp: DateTime<Utc>,   // Wall clock at creation
    event: Event,               // The payload
    signed_by: Option<String>,  // SSH fingerprint of signer
    signature: Option<String>,  // SSH signature over canonical event payload
}
```

**Total ordering key:** `(timestamp, agent_id, agent_seq)` — globally unique, deterministic. Ties broken by agent_id (lexicographic), then sequence number.

**Event signing:** All events carry SSH signatures. The signature covers the canonical JSON serialization of the `event` field concatenated with `agent_id`, `agent_seq`, and `timestamp`. Compaction verifies signatures and flags unsigned events as warnings in `checkpoint/state.json`. At 50+ agents, full attribution on every event is essential — the 5-10ms signing cost is negligible compared to git push latency (~500ms). See [resolved Q7](#resolved-q7-event-signing) for rationale.

### 4.2 Event types

```rust
enum Event {
    // === Tier 1: Exclusive operations ===

    /// Creates an issue. display_id is assigned during compaction, not creation.
    IssueCreated {
        uuid: Uuid,
        title: String,
        description: Option<String>,
        priority: Priority,
        labels: Vec<String>,
        parent_uuid: Option<Uuid>,
        created_by: String,
    },

    /// Acquires a lock. First-claim-wins in event order.
    LockClaimed {
        issue_display_id: u32,
        branch: Option<String>,
    },

    /// Releases a lock. Only valid from the current holder.
    LockReleased {
        issue_display_id: u32,
    },

    /// Assigns a display ID during compaction (internal, not emitted by agents)
    // NOTE: Do we need this? Or is ID assignment purely a compaction side-effect?

    // === Tier 2: Causal operations ===

    IssueUpdated {
        uuid: Uuid,
        title: Option<String>,         // Only set fields are changed
        description: Option<String>,
        priority: Option<Priority>,
    },

    StatusChanged {
        uuid: Uuid,
        new_status: Status,
        closed_at: Option<DateTime<Utc>>,
    },

    DependencyAdded {
        blocked_uuid: Uuid,
        blocker_uuid: Uuid,
    },

    DependencyRemoved {
        blocked_uuid: Uuid,
        blocker_uuid: Uuid,
    },

    RelationAdded {
        uuid_a: Uuid,
        uuid_b: Uuid,
    },

    RelationRemoved {
        uuid_a: Uuid,
        uuid_b: Uuid,
    },

    MilestoneAssigned {
        issue_uuid: Uuid,
        milestone_uuid: Option<Uuid>,  // None = unassign
    },

    LabelAdded {
        issue_uuid: Uuid,
        label: String,
    },

    LabelRemoved {
        issue_uuid: Uuid,
        label: String,
    },

    ParentChanged {
        issue_uuid: Uuid,
        new_parent_uuid: Option<Uuid>,
    },
}
```

### 4.3 What's NOT an event

These are direct writes (T3) — no event needed:

- **Comments** → `issues/{uuid}/comments/{comment-uuid}.json` (new file)
- **Heartbeats** → `agents/{id}/heartbeat.json` (overwrite own file)
- **Sessions** → `agents/{id}/session.json` (overwrite own file)
- **Time entries** → `agents/{id}/time/{issue-uuid}.json` (per-agent, conflict-free)

Time entries are inherently per-agent (agent A's timer doesn't conflict with agent B's). At 50 agents, routing them through events would add ~100 entries/hour of noise to the compaction pipeline. Direct writes to agent directories are conflict-free by construction. Compaction aggregates time entries from `agents/*/time/*.json` into materialized issue state during the materialize step.

---

## 5. Serialization: CBOR vs NDJSON

The #113 proposal specifies CBOR (RFC 8949). Worth discussing tradeoffs:

| Factor | CBOR | NDJSON |
|--------|------|--------|
| **Size** | ~60% of JSON | 100% baseline |
| **Self-delimiting** | Yes (native) | Yes (newline-delimited) |
| **Human readable** | No | Yes (`cat events.log` works) |
| **Debugging** | Needs tooling (`cbor-diag`) | `jq` / text editor |
| **Rust crate** | `ciborium` (well-maintained) | `serde_json` (already a dep) |
| **Cross-language** | Good (`cbor-x`, `cbor2`) | Universal |
| **Git diff** | Binary blob (opaque) | Line-level diffs visible |
| **Append safety** | Inherent (self-delimiting frames) | Inherent (newline as delimiter) |

### Decision: NDJSON with `EventCodec` abstraction

**NDJSON for all phases. CBOR is not planned.**

Rationale:
- Event logs are small. Even at 50 agents × 20 events/hour = 1000 events/hour, that's ~500KB of JSON. Size savings from CBOR are negligible at this scale.
- Debuggability is critical: `cat events.log | jq` on the hub branch, line-level git diffs, no special tooling needed.
- No new dependency — `serde_json` already in Cargo.toml.
- If volume ever becomes an issue (unlikely), zstd-compressed NDJSON gives 80%+ compression while preserving debuggability (just decompress first).

To keep the door open without overengineering, define a serialization abstraction:

```rust
trait EventCodec {
    fn encode(&self, event: &EventEnvelope) -> Result<Vec<u8>>;
    fn decode(&self, bytes: &[u8]) -> Result<EventEnvelope>;
    fn encode_append(&self, events: &[EventEnvelope]) -> Result<Vec<u8>>;
    fn decode_all(&self, bytes: &[u8]) -> Result<Vec<EventEnvelope>>;
}

struct NdjsonCodec;  // The only implementation. ~20 lines.
```

This costs nothing to build and makes the format swappable without touching any call site. But the expectation is that `NdjsonCodec` is the only implementation we ever ship.

---

## 6. Compaction Algorithm

Compaction is the heart of the system. Any agent can run it. Two agents compacting simultaneously produce the same result.

### 6.1 Algorithm

```
COMPACT():
  1. fetch latest hub branch
  2. read checkpoint/state.json + checkpoint/watermark
  3. for each agents/*/events.log:
       read all events with ordering_key > watermark
  4. merge all new events into a single list
  5. sort by (timestamp, agent_id, agent_seq)
  6. for each event in sorted order:
       apply(state, event)  // deterministic reduction
  7. materialize:
       write issues/{uuid}/issue.json for each changed issue
       write locks/{id}.json for each changed lock
  8. write checkpoint/state.json (new state)
  9. write checkpoint/watermark (ordering key of last event)
  10. commit + push
       if push conflicts: YIELD (other compactor produced same or newer result)
```

### 6.2 Deterministic reduction rules

The `apply(state, event)` function must be deterministic. Key rules:

| Event | Reduction rule |
|-------|---------------|
| `IssueCreated` | Assign next `display_id` from counter. If UUID already exists, skip (idempotent). |
| `LockClaimed` | If lock unclaimed, grant to event's agent. If already claimed by different agent, **reject** (first-claim-wins). |
| `LockReleased` | If held by event's agent, release. Otherwise skip. |
| `StatusChanged` | Last-writer-wins (latest timestamp). |
| `IssueUpdated` | Merge fields. Last-writer-wins per field. |
| `LabelAdded` | Add to set (idempotent). |
| `LabelRemoved` | Remove from set (idempotent). |
| `DependencyAdded` | Add to set (idempotent). |
| `DependencyRemoved` | Remove from set (idempotent). |

### 6.3 Display ID stability

Once a display ID is assigned in a checkpoint, it **never changes**. Late-arriving `IssueCreated` events get the next available ID, not a backdated one. The ID counter is part of checkpoint state.

### 6.4 Compaction triggers

When should compaction run?

- **On sync**: `crosslink sync` compacts if the lease is available (see 6.6)
- **On lock claim**: Agent emits `LockClaimed` event → compacts (ignores lease — lock confirmation requires it) → checks if it won
- **On issue create (interactive)**: Agent needs a display ID → emits event → compacts → reads assigned ID
- **On issue create (batch)**: Agent emits event → does NOT compact → returns UUID only. Display ID assigned on next compaction cycle. Use `crosslink create --defer-id` for this path.
- **Periodic**: Daemon heartbeat loop can trigger compaction every N seconds
- **Manual**: `crosslink compact` command for debugging

### 6.5 Checkpoint state schema

```rust
struct CheckpointState {
    /// Next display ID to assign
    next_display_id: u32,

    /// Next comment ID (if we still need global comment IDs)
    next_comment_id: u64,

    /// UUID → display_id mapping (stable, never removed)
    display_id_map: BTreeMap<Uuid, u32>,

    /// Current lock state
    locks: BTreeMap<u32, LockEntry>,

    /// Issue state (compact representation for reduction)
    issues: BTreeMap<Uuid, CompactIssue>,

    /// Clock skew warnings
    skew_warnings: Vec<SkewWarning>,

    /// Compaction lease (prevents thundering herd at scale)
    compaction_lease: Option<CompactionLease>,

    /// Unsigned event warnings (events missing SSH signatures)
    unsigned_event_warnings: Vec<UnsignedEventWarning>,
}
```

### 6.6 Compaction lease (thundering herd prevention)

At 50+ agents, redundant compaction becomes a contention multiplier — 50 agents all self-compacting on every sync means 50 competing pushes where 49 fail. The compaction lease prevents this without introducing a coordinator SPOF.

```rust
struct CompactionLease {
    agent_id: String,
    acquired_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,  // Default: 30 seconds
}
```

**Protocol:**
1. Agent wants to compact → reads `checkpoint/state.json`
2. If `compaction_lease` is `None` or `expires_at < now()` → acquire lease (write own agent_id + new expiry), proceed with compaction
3. If lease is held by another agent and not expired → **skip compaction** (the leaseholder will do it)
4. After compaction completes, clear the lease (or let it expire naturally)

**Exceptions that ignore the lease:**
- Lock confirmation (`CLAIM_LOCK` protocol) — an agent confirming a lock claim must compact regardless, because it needs the result immediately
- `crosslink compact --force` — manual override for debugging

**No daemon, no SPOF:** The lease is advisory, enforced by git CAS (push). If an agent crashes mid-compaction, the lease expires in 30 seconds and another agent picks up. Any agent *can* still compact — the lease just reduces redundant work.

**Preferred compactor role (optional, convention-based):** At high agent counts, designate one agent (or the driver's CLI) as the "preferred compactor" via config:
```json
// .crosslink/hook-config.json
{ "preferred_compactor": "driver-cli" }
```
The preferred compactor always compacts on sync (ignores lease held by others). Other agents only compact when the lease is expired. This is a soft optimization, not a hard requirement — removing the preferred compactor just means all agents compete equally for the lease.

---

## 7. Event Flushing (Self-Pruning)

After compaction advances the watermark, events below the watermark are settled. Each agent prunes its own log:

```
PRUNE(agent_id):
  1. read checkpoint/watermark
  2. read own agents/{agent_id}/events.log
  3. filter: keep only events where ordering_key > watermark
  4. rewrite events.log with remaining events
  5. commit + push (own directory only)
```

Pruned events remain in git history for audit. No separate archive needed.

**When to prune:** During the agent's next event emission or sync cycle. Not critical-path.

---

## 8. Lock Confirmation Protocol

Current model: write to `locks.json` → push with retry.
New model: event-based with confirmation.

```
CLAIM_LOCK(issue_id):
  1. emit LockClaimed { issue_id } to own events.log
  2. push events.log
  3. compact (or wait for compaction)
  4. read materialized locks/{issue_id}.json
  5. if lock.agent_id == self → SUCCESS
     else → FAILED (another agent claimed first)
  6. on failure: emit LockReleased (cleanup), notify agent
```

**Timeout/backoff:** If compaction hasn't run within 30s, agent compacts itself. If push conflicts on compaction, yield (another compactor produced the result).

**Stale lock detection:** Same as today — compare heartbeat timestamps. But now heartbeats are in `agents/{id}/heartbeat.json`, which is direct-write (no conflicts).

---

## 9. Comment Direct-Write Protocol

Comments are T3 (eventually consistent). No event needed.

```
ADD_COMMENT(issue_uuid, content):
  1. generate comment_uuid
  2. write issues/{issue_uuid}/comments/{comment_uuid}.json:
     {
       "uuid": comment_uuid,
       "author": agent_id,
       "content": content,
       "created_at": now(),
       "kind": "note",
       "trigger_type": null,
       "signed_by": fingerprint,
       "signature": sign(content)
     }
  3. commit + push
     (no conflict possible — new file with unique name)
```

**Hydration change:** `hydrate_to_sqlite` reads `issues/{uuid}/comments/*.json` and sorts by `created_at` to produce a deterministic comment order.

---

## 10. Migration Path

### Phase 0: Prerequisite — fix worktree path resolution

Before the hub layout change, fix the immediate worktree problem:
- Update `find_crosslink_dir()` in `main.rs` to use `resolve_main_repo_root()` (already exists in `sync.rs`)
- Update Python hooks (`crosslink_config.py`) to resolve to main repo
- This unblocks worktree agents immediately with the current architecture

### Phase 1: Hub layout migration (non-breaking reads, breaking writes)

```
crosslink migrate-hub
```

1. Read all `issues/{uuid}.json` files
2. For each issue:
   - Extract comments → write to `issues/{uuid}/comments/{comment-uuid}.json`
   - Strip comments from issue → write to `issues/{uuid}/issue.json`
   - Delete old `issues/{uuid}.json`
3. Read `locks.json` → write individual `locks/{display-id}.json` → delete `locks.json`
4. Create `checkpoint/` directory with initial `state.json` (from current `meta/counters.json`)
5. Move `heartbeats/{id}.json` → `agents/{id}/heartbeat.json`
6. Update `hydrate_to_sqlite` to read new layout
7. Keep backward-compat read path for old layout during transition

**Milestone:** v0.3.0 (per GH milestone)

### Phase 2: Event system + compaction

1. Define `Event` enum + `EventEnvelope` with signing fields
2. Implement `EventCodec` trait + `NdjsonCodec`
3. Implement append-only log writer/reader for `agents/{id}/events.log`
4. Implement `SharedWriter` methods as event emitters (facade stays the same)
5. Implement deterministic compaction (reduction function + materialization)
6. Implement compaction lease (section 6.6) — required for scale, not deferred
7. Implement event flushing (self-pruning)
8. Add `crosslink compact [--force]` command
9. Add `crosslink create --defer-id` for batch creation without self-compaction
10. Implement clock skew detection (commit timestamp witness)
11. Implement event signature verification during compaction (warn on unsigned)

### Phase 3: Lock confirmation protocol

1. Replace push-retry lock acquisition with event-based protocol (section 8)
2. Add timeout/backoff for contention
3. Update stale lock detection for new heartbeat location (`agents/{id}/heartbeat.json`)
4. Test: two agents racing for same lock, deterministic winner
5. Test: 10+ agents contending, compaction lease prevents thundering herd

### Phase 4: Container bootstrap

1. `crosslink agent bootstrap --repo <url> --branch <branch> --identity <id>`
2. Shallow clone + agent identity setup + SSH key generation
3. Test: container agent can emit events, compact, and coordinate with host agent

---

## 11. Impact on Existing Code

### Files that change significantly

| File | Change |
|------|--------|
| `shared_writer.rs` | Becomes event emitter instead of read-modify-write. Core rewrite. |
| `sync.rs` | Add compaction cycle, event reading, checkpoint management |
| `hydration.rs` | Read new layout (`issues/{uuid}/issue.json` + `comments/`). Simpler — no inline comment parsing. |
| `issue_file.rs` | `IssueFile` struct drops `comments` field. New `CommentFile` struct. |
| `locks.rs` | Read from per-file `locks/{id}.json` instead of monolithic `locks.json` |
| `main.rs` | `find_crosslink_dir()` fix. New `compact` and `migrate-hub` commands. |

### Files that change minimally

| File | Change |
|------|--------|
| `db.rs` | Schema unchanged — SQLite is the local read view |
| `identity.rs` | Unchanged — agent identity model stays |
| `models.rs` | Unchanged — local models stay |
| `commands/*.rs` | Mostly unchanged — they call SharedWriter which changes underneath |

### API surface for agents

The CLI command interface stays the same. `crosslink create`, `crosslink comment`, `crosslink close` etc. all work as before. The underlying write path changes from "modify JSON file" to "emit event + compact", but this is transparent to callers.

---

## 12. Design Decisions (Resolved)

These questions were raised during design iteration and have been resolved. Rationale preserved for future reference.

### Resolved Q1: NDJSON, not CBOR {#resolved-q1-serialization}

**Decision:** NDJSON with an `EventCodec` trait abstraction (~20 lines). No CBOR dependency.

**Rationale:** At 50 agents × 20 events/hour, event logs are ~500KB/hour of JSON — trivial. Debuggability (`cat | jq`, line-level git diffs) outweighs the ~40% size savings. If compression is ever needed, zstd-compressed NDJSON gives 80%+ savings without sacrificing readability. The `EventCodec` trait keeps the format swappable at zero cost.

**Alternatives rejected:**
- CBOR: opaque in git diffs, requires `ciborium` dep and `cbor-diag` tooling, size savings don't matter at this scale
- Raw JSON (non-delimited): no append-only semantics without a wrapper format

### Resolved Q2: Labels are events (T2), not direct writes {#resolved-q2-labels}

**Decision:** Labels use `LabelAdded`/`LabelRemoved` events, processed during compaction.

**Rationale:** Labels are rare operations (creation-time and close-time mostly). Keeping all mutable issue state in a single mutation path (events) avoids a split-brain mental model where developers must remember which fields go through events vs. direct writes. The set semantics (idempotent add/remove) make them naturally conflict-free in the reduction function.

**Alternative rejected:**
- Direct writes to `issues/{uuid}/labels.json`: splits the issue state across two sources (event-materialized `issue.json` + direct-write `labels.json`), complicates hydration, adds merge semantics for a file that two agents could write to simultaneously

### Resolved Q3: Time entries are direct writes, not events {#resolved-q3-time-entries}

**Decision:** Time entries are written directly to `agents/{id}/time/{issue-uuid}.json`. Compaction aggregates them into materialized issue state.

**Rationale:** Time entries are inherently per-agent (agent A's timer never conflicts with agent B's) and relatively high-frequency (start + stop per work session). At 50 agents, routing them through events would add ~100 entries/hour of noise to compaction. Direct writes to agent directories are conflict-free by construction. Hydration globs `agents/*/time/*.json` and groups by issue UUID.

**Alternative rejected:**
- Events: adds volume without coordination value, since time entries don't need ordering relative to other agents' operations

### Resolved Q4: Hybrid display ID assignment (interactive self-compact + batch defer) {#resolved-q4-display-id}

**Decision:** Option (c) — hybrid. Interactive use (`crosslink create`, `crosslink quick`) self-compacts immediately to return `#N`. Batch/scripted creation (`crosslink create --defer-id`) skips compaction and returns UUID only.

**Rationale:** At 50 agents, every `crosslink create` triggering a full compaction (read 50 agent logs, sort, reduce, push) creates a thundering herd when 10 agents create issues simultaneously — 10 competing compactions where 9 fail and retry. The `--defer-id` flag avoids this for batch scenarios while preserving the current UX for interactive use (which is the 95% case today).

**Implementation:** `SharedWriter` internal API accepts a `compact: bool` parameter. Interactive commands pass `true`, kickoff/batch scripts pass `false`. Deferred IDs are assigned on the next compaction cycle.

**Alternatives rejected:**
- Always self-compact (option a): thundering herd at scale
- Always defer (option b): breaks interactive UX, hooks/scripts depend on `#N` output

### Resolved Q5: Inline compaction with lease, no daemon {#resolved-q5-compaction-process}

**Decision:** Peer-to-peer compaction with a compaction lease (see section 6.6). No dedicated daemon. Compaction lease is a Phase 2 deliverable, not a future optimization.

**Rationale:** The CRDT model's value is that compaction is peer-to-peer and idempotent — any agent can compact, racing compactors produce identical results. A coordinator daemon would be a SPOF that undermines this property. The compaction lease (30s advisory lock in `checkpoint/state.json`) prevents the thundering herd at 50+ agents without adding infrastructure. An agent that crashes mid-compaction just lets the lease expire.

**Scaling path:** At very high agent counts, designate a "preferred compactor" via config (soft convention, not hard requirement). The preferred compactor always compacts on sync; other agents only compact when the lease is expired.

**Alternative rejected:**
- Dedicated daemon: SPOF, requires process management, doesn't work in containers, against the "no coordinator" design principle

### Resolved Q6: Version flag for hub layout migration {#resolved-q6-transition}

**Decision:** Option (c) — `meta/version.json` with clear error messages for old agents.

**Rationale:** Hard cutover is operationally impossible with long-running worktree agents. Dual-write doubles the write surface area and creates months of maintenance burden. The version flag approach is ~3 lines of version-check code:

1. `crosslink migrate-hub` writes `meta/version.json: { "layout_version": 2 }`
2. Old agents see `layout_version: 2`, print "Hub format upgraded. Run `crosslink upgrade` to update your agent." and exit non-destructively
3. New agents check for version ≥ 2, fall back to v1 read path if absent (for un-migrated hubs)

**Alternatives rejected:**
- Hard cutover (a): impossible with long-running agents in worktrees
- Dual-write (b): doubles write logic, doubles bugs, unclear when to remove it

### Resolved Q7: Sign all events {#resolved-q7-event-signing}

**Decision:** All events carry SSH signatures in the `EventEnvelope`, not just T1 operations.

**Rationale:** At 50 agents, the attack surface grows. A compromised agent could emit `StatusChanged` events to close other agents' issues or `DependencyAdded` events to create false blockers that stall work — these are T2 operations. Full signing provides:
- Complete attribution on every event
- Tamper-evident audit trail that survives git history compaction
- Simpler code (one path: always sign) vs. conditional (sign if T1, skip if T2)

The 5-10ms per signature is negligible compared to git push latency (~500ms).

Compaction **warns** on unsigned events rather than rejecting them (graceful degradation during rollout), but the expectation is all events are signed in steady state.

**Alternative rejected:**
- T1-only signing: leaves T2 operations unverifiable, two code paths for marginal performance gain

### Resolved Q8: Comment ordering uses triple sort key {#resolved-q8-comment-ordering}

**Decision:** Sort comments by `(created_at, agent_id, comment_uuid)` for deterministic ordering.

**Rationale:** Comments are T3 direct writes, so they arrive in arbitrary order. Two agents commenting at the "same" millisecond (within clock skew) need a stable tiebreaker. The triple matches the event ordering key convention. Sequential IDs would require counter coordination, defeating the purpose of T3 direct writes. The ordering between near-simultaneous comments from different agents doesn't matter semantically — we just need it to be stable across hydrations.

---

## 13. Testing Strategy

### Unit tests
- Deterministic reduction: given N events, verify the reduced state is identical regardless of processing order (for commutative operations) or that sorting produces the correct order
- Display ID stability: create events, compact, add more events, compact again — IDs don't change
- Lock contention: two `LockClaimed` events for the same lock — first in order wins
- Event serialization round-trip (NDJSON via `EventCodec` trait)
- Event signature verification: signed events pass, tampered events fail, unsigned events warn
- Clock skew detection with synthetic timestamps
- Compaction lease: acquire/release/expiry semantics
- `--defer-id` path: issue created without display ID, assigned on next compaction

### Integration tests
- Two agents in separate worktrees, both emit events, compaction produces consistent state
- Lock race: two agents claim simultaneously, exactly one wins
- Offline agent creates issues, comes back online, events merge correctly
- Migration: `crosslink migrate-hub` on existing hub data, hydration still works
- Comment ordering: two agents comment on same issue simultaneously, hydration produces deterministic order
- Time entry aggregation: multiple agents track time on same issue, compaction aggregates correctly

### Scale tests
- 50-agent simulation: spawn 50 event logs with synthetic events, measure compaction time and verify determinism
- Compaction lease under contention: 10 simulated agents attempting compaction, verify only one proceeds
- Thundering herd: 20 agents creating issues simultaneously with `--defer-id`, verify no push conflicts
- Event log growth: 10,000 events across 50 agents, verify flushing keeps log sizes bounded

### Fuzz targets (extend existing)
- `fuzz_create_issue.rs` — event-based creation
- New: `fuzz_compaction.rs` — random event sequences from N agents, verify deterministic output regardless of read order
- New: `fuzz_event_serialization.rs` — round-trip fuzzing via `EventCodec`
- New: `fuzz_event_reduction.rs` — adversarial event sequences (duplicate UUIDs, conflicting locks, clock skew)

---

## 14. Phase 1 Detailed Spec (v0.3.0 scope)

Since Phase 1 (hub layout migration) is the v0.3.0 milestone, here's a more detailed breakdown:

### 14.1 New file structures

**`issues/{uuid}/issue.json`** — same as current `IssueFile` but without `comments` field:
```rust
struct IssueFileV2 {
    uuid: Uuid,
    display_id: Option<i64>,
    title: String,
    description: Option<String>,
    status: String,
    priority: String,
    parent_uuid: Option<Uuid>,
    created_by: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    closed_at: Option<DateTime<Utc>>,
    labels: Vec<String>,
    blockers: Vec<Uuid>,
    related: Vec<Uuid>,
    milestone_uuid: Option<Uuid>,
    time_entries: Vec<TimeEntry>,
    // comments: REMOVED — now separate files
}
```

**`issues/{uuid}/comments/{comment-uuid}.json`**:
```rust
struct CommentFile {
    uuid: Uuid,
    author: String,
    content: String,
    created_at: DateTime<Utc>,
    kind: String,
    trigger_type: Option<String>,
    intervention_context: Option<String>,
    driver_key_fingerprint: Option<String>,
    signed_by: Option<String>,
    signature: Option<String>,
}
```

**`locks/{display-id}.json`**:
```rust
struct LockFile {
    issue_id: u32,
    agent_id: String,
    branch: Option<String>,
    claimed_at: DateTime<Utc>,
    signed_by: Option<String>,
}
```

### 14.2 Migration command

```bash
crosslink migrate-hub [--dry-run]
```

Steps:
1. Check current hub layout version (default: v1 if no version file)
2. If already v2, print message and exit
3. For each `issues/{uuid}.json`:
   - Create `issues/{uuid}/` directory
   - Write `issues/{uuid}/issue.json` (without comments)
   - For each comment: write `issues/{uuid}/comments/{generated-uuid}.json`
   - Delete `issues/{uuid}.json`
4. Read `locks.json` → write `locks/{id}.json` for each entry → delete `locks.json`
5. Move `heartbeats/*` → `agents/{id}/heartbeat.json`
6. Create `checkpoint/state.json` from `meta/counters.json`
7. Write `meta/version.json`: `{ "layout_version": 2 }`
8. Commit all changes
9. Push

### 14.3 Hydration v2

Update `hydrate_to_sqlite()`:
1. Check layout version
2. If v2: read `issues/{uuid}/issue.json` + `issues/{uuid}/comments/*.json`
3. If v1: read `issues/{uuid}.json` (backward compat)
4. Rest of hydration unchanged (SQLite schema stays at v13)

### 14.4 SharedWriter v2

Update write methods to use new file paths:
- `create_issue()` → write to `issues/{uuid}/issue.json`
- `add_comment()` → write new file to `issues/{uuid}/comments/{uuid}.json`
- `close_issue()` → modify `issues/{uuid}/issue.json`
- `add_label()` → modify `issues/{uuid}/issue.json`

Key improvement: `add_comment()` no longer modifies the issue file, so it can't conflict with other operations on the same issue.

---

## 15. Dependency Graph

```
Phase 0 ─── fix worktree path resolution (bugfix, independent)
   │
   ▼
Phase 1 ─── hub layout migration (v0.3.0)
   │
   ▼
Phase 2 ─── event system + compaction + lease (core CRDT machinery)
   │
   ├──► Phase 3 ─── lock confirmation protocol
   │
   └──► Phase 4 ─── container bootstrap
```

- **Phase 0** can be done immediately and independently — it's a bugfix, not an architecture change.
- **Phase 1** is the v0.3.0 milestone. Breaking change to hub layout, but version-flagged for safe transition.
- **Phase 2** is the core architecture change. Includes compaction lease from day one (not deferred) to handle 50+ agent scenarios.
- **Phase 3 and 4** are independent of each other but both depend on Phase 2.

---

## 16. Scale Considerations

Summary of how the design handles growth from current (~6 agents) to target (50+):

| Concern | Mechanism | Threshold |
|---------|-----------|-----------|
| Event log read amplification | Compaction watermark — only uncompacted events are read | Linear in uncompacted events, not total history |
| Compaction contention | Compaction lease (30s advisory) | Prevents thundering herd at any agent count |
| Push conflicts | Per-agent directories — agents only write to own files | Zero conflicts for T3 ops (80%+ of writes) |
| Display ID allocation latency | Self-compact for interactive, `--defer-id` for batch | Batch path avoids compaction entirely |
| Lock acquisition latency | Event + self-compact + verify (bypasses lease) | Bounded by git round-trip, not agent count |
| Audit trail integrity | SSH signatures on all events | Full attribution scales linearly |
| Git object count growth | Event flushing (self-pruning after watermark advances) | Bounded by compaction frequency, not event volume |

**When to reconsider architecture:**
- \>100 agents: consider dedicated compactor process (still not a daemon — a periodic job)
- \>1000 events/minute: consider sharding agent directories by hash prefix
- Cross-datacenter: consider replacing git transport with a proper distributed log

None of these scenarios are likely in the near term. The current design handles the 50-agent target with headroom.
