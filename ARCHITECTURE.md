# Crosslink Architecture Map

## High-Level ASCII Map

```
┌─────────────────────────────────────────────────────────────────────┐
│                          CROSSLINK CLI                              │
│                         (main.rs / clap)                            │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────┐  │
│  │   COMMANDS    │  │  DATA LAYER  │  │  COORDINATION SYSTEM     │  │
│  │  (35 modules) │  │              │  │                          │  │
│  │              │  │  models.rs   │  │  events.rs  (append-only) │  │
│  │  create      │  │  db.rs       │  │  sync.rs    (hub branch)  │  │
│  │  show/list   │──│  issue_file  │──│  compaction (reduce)      │  │
│  │  session     │  │  hydration   │  │  checkpoint (snapshot)    │  │
│  │  comment     │  │              │  │  shared_writer (writes)   │  │
│  │  ...         │  │  SQLite ◄────│──│──── JSON on git           │  │
│  └──────────────┘  │  (cache)     │  │    (source of truth)     │  │
│                    └──────────────┘  └──────────────────────────┘  │
│                                                                     │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────┐  │
│  │   IDENTITY   │  │    LOCKS     │  │    KNOWLEDGE             │  │
│  │              │  │              │  │                          │  │
│  │  identity.rs │  │  locks.rs    │  │  knowledge.rs            │  │
│  │  signing.rs  │  │  lock_check  │  │  (orphan branch)         │  │
│  │  trust.rs    │  │              │  │  YAML frontmatter + MD   │  │
│  │  (SSH keys)  │  │  (V1 file /  │  │  conflict resolution     │  │
│  │              │  │   V2 event)  │  │                          │  │
│  └──────────────┘  └──────────────┘  └──────────────────────────┘  │
│                                                                     │
│  ┌──────────────┐  ┌──────────────┐                                │
│  │  CONTAINER   │  │    DAEMON    │                                │
│  │              │  │              │                                │
│  │  container.rs│  │  daemon.rs   │                                │
│  │  Dockerfile  │  │  (bg sync)   │                                │
│  │  entrypoint  │  │              │                                │
│  └──────────────┘  └──────────────┘                                │
└─────────────────────────────────────────────────────────────────────┘
        │
        │ deployed by `crosslink init`
        ▼
┌─────────────────────────────────────────────────────────────────────┐
│                     CLAUDE INTEGRATION LAYER                        │
│                                                                     │
│  ┌──────────── HOOKS (.claude/hooks/) ──────────────────────────┐  │
│  │                                                               │  │
│  │  session-start.py    SessionStart   auto-end stale sessions  │  │
│  │  prompt-guard.py     PromptSubmit   inject rules + adaptive  │  │
│  │  work-check.py       PreToolUse     enforce issue tracking   │  │
│  │  post-edit-check.py  PostToolUse    stub/drift detection     │  │
│  │  pre-web-check.py    PreToolUse     web request safety       │  │
│  │  crosslink_config.py (shared)       config loading + drift   │  │
│  │                                                               │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                     │
│  ┌──────────── SKILLS (.claude/commands/) ───────────────────────┐  │
│  │                                                               │  │
│  │  /preflight   load rules + grounding before implementation   │  │
│  │  /review      pre-commit quality gate (stubs, lint, tests)   │  │
│  │  /audit       full context dump when stuck                   │  │
│  │  /commit      commit + auto-document on crosslink issue      │  │
│  │  /feature     create feature branch                          │  │
│  │  /featree     feature branch in worktree                     │  │
│  │  /kickoff     launch background agent (container or tmux)    │  │
│  │  /check       monitor background agent status                │  │
│  │  /workflow    manage crosslink configuration                 │  │
│  │                                                               │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                     │
│  ┌──────────── RULES (.crosslink/rules/) ────────────────────────┐  │
│  │                                                               │  │
│  │  global.md          core rules (no stubs, security, etc.)    │  │
│  │  project.md         project-specific customizations          │  │
│  │  tracking-*.md      strict / normal / relaxed enforcement    │  │
│  │  rust.md            ┐                                        │  │
│  │  python.md          │                                        │  │
│  │  javascript.md      ├── 20+ language-specific rule files     │  │
│  │  typescript.md      │                                        │  │
│  │  go.md, java.md ... ┘                                        │  │
│  │  knowledge.md       knowledge contribution guidelines        │  │
│  │  web.md             web/frontend rules                       │  │
│  │                                                               │  │
│  └───────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

## Data Flow

```
              SINGLE AGENT                          MULTI-AGENT
              ───────────                          ───────────

  User ──► crosslink create           Agent A ──► create (UUID)
               │                                     │
               ▼                                     ▼
           db.rs (SQLite)              shared_writer ──► JSON on hub branch
               │                                     │ commit + push
               ▼                                     ▼
           Issue #1 ready              compaction ──► assign display_id
                                                     │
                                                     ▼
                                       hydration ──► SQLite (read cache)
                                                     │
                                                     ▼
                                                 Issue #1 ready
```

## Git Branch Layout

```
main                     ← user's code
  └─ feature/*           ← work branches (worktrees)

crosslink/hub            ← coordination (orphan branch)
  ├─ agents/
  │   ├─ agent-1/
  │   │   ├─ events.log  ← append-only NDJSON event stream
  │   │   └─ heartbeat.json
  │   └─ agent-2/
  │       └─ ...
  ├─ issues/
  │   └─ {uuid}.json     ← materialized issue snapshots
  ├─ checkpoint/
  │   └─ state.json      ← compaction result
  ├─ meta/
  │   └─ counters.json   ← display_id allocator
  └─ trust/
      ├─ keys/           ← agent public keys
      └─ allowed_signers ← SSH trust store

crosslink/knowledge      ← shared research (orphan branch)
  └─ pages/
      └─ {slug}.md       ← knowledge pages with YAML frontmatter
```

## Command Modules (src/commands/)

### Issue Management
| Module | Commands | Purpose |
|--------|----------|---------|
| `create.rs` | `create`, `quick`, `subissue` | Create issues with templates, labels, auto-work |
| `show.rs` | `show` | Display full issue details + relationships |
| `list.rs` | `list` | Filtered table or JSON output |
| `search.rs` | `search` | Text search across titles/descriptions/comments |
| `update.rs` | `update` | Modify title, description, priority |
| `delete.rs` | `delete` | Remove issue (cascades children) |
| `status.rs` | `close`, `close-all`, `reopen` | Status transitions, auto-changelog |

### Relationships & Organization
| Module | Commands | Purpose |
|--------|----------|---------|
| `deps.rs` | `block`, `unblock`, `blocked`, `ready` | Dependency graph |
| `relate.rs` | `relate`, `unrelate`, `related` | Bidirectional links |
| `tree.rs` | `tree` | Hierarchy visualization |
| `next.rs` | `next` | Smart priority scoring for next task |
| `label.rs` | `label`, `unlabel` | Changelog categorization |
| `milestone.rs` | `milestone *` | Release grouping |
| `timer.rs` | `start`, `stop`, `timer` | Time tracking |

### Workflow & Session
| Module | Commands | Purpose |
|--------|----------|---------|
| `session.rs` | `session start/end/status/work/action` | Session lifecycle + handoff |
| `comment.rs` | `comment` | Typed comments (plan/decision/observation/...) |
| `intervene.rs` | `intervene` | Log driver interventions for audit |
| `tested.rs` | `tested` | Mark tests run (resets reminder) |
| `workflow.rs` | `workflow diff/trail` | Policy drift detection, comment trails |

### Multi-Agent & Infrastructure
| Module | Commands | Purpose |
|--------|----------|---------|
| `agent.rs` | `agent init/status/bootstrap` | Agent identity + SSH keys |
| `trust.rs` | `trust approve/revoke/list/pending/check` | SSH trust management |
| `locks_cmd.rs` | `locks list/check/claim/release/steal`, `sync` | Lock management |
| `container.rs` | `container build/start/ps/logs/stop/rm/kill/shell/snapshot` | Docker agent execution |
| `compact.rs` | `compact` | Manual event compaction |
| `knowledge.rs` | `knowledge add/show/list/edit/remove/sync/search` | Shared research pages |
| `context.rs` | `context measure/check` | Context injection measurement |
| `config.rs` | `config show/get/set/list/reset/diff` | Hook configuration |

### Data Management
| Module | Commands | Purpose |
|--------|----------|---------|
| `init.rs` | `init` | Project setup (hooks, rules, db, signing) |
| `export.rs` | `export` | JSON/markdown export |
| `import.rs` | `import` | JSON import |
| `archive.rs` | `archive add/remove/list/older` | Issue archival |
| `migrate.rs` | `migrate-to-shared/from-shared/rename-branch` | Schema migration |
| `integrity_cmd.rs` | `integrity counters/hydration/locks/schema` | Data integrity checks |
| `style.rs` | `style set/sync/diff/show/unset` | House style syncing |
| `cpitd.rs` | `cpitd scan/status/clear` | Code clone detection |

## Hook Execution Flow

```
User types prompt
        │
        ▼
  ┌─────────────────┐
  │  session-start   │  (SessionStart — once per session)
  │  auto-end stale  │
  │  show handoff    │
  └────────┬────────┘
           ▼
  ┌─────────────────┐
  │  prompt-guard    │  (UserPromptSubmit — every prompt)
  │                  │
  │  1st prompt:     │──► full rules + tree + deps (15-30KB)
  │  subsequent:     │──► adaptive drift check
  │    drift < N:    │──► (silent)
  │    drift >= N:   │──► condensed reminder (~500B)
  └────────┬────────┘
           ▼
     Agent works...
           │
           ▼
  ┌─────────────────┐
  │  work-check      │  (PreToolUse — before Write/Edit/Bash)
  │                  │
  │  strict:  BLOCK  │──► must have active issue
  │  normal:  WARN   │──► reminder but allow
  │  relaxed: PASS   │──► no enforcement
  │                  │
  │  always:  block  │──► git push/merge/reset/etc.
  │  gated:   check  │──► git commit needs active issue
  │                  │
  │  crosslink cmd?  │──► reset drift counter
  └────────┬────────┘
           ▼
  ┌─────────────────┐
  │  pre-web-check   │  (PreToolUse — before WebFetch/WebSearch)
  │  URL safety      │
  └────────┬────────┘
           ▼
  ┌─────────────────┐
  │  post-edit-check │  (PostToolUse — after Write/Edit)
  │  stub detection  │
  │  drift warnings  │
  └─────────────────┘
```

## Key Architecture Decisions

| Decision | Implementation | Why |
|----------|---------------|-----|
| Event sourcing | Append-only NDJSON logs per agent | Audit trail, conflict-free merge, offline-safe |
| Git as coordination DB | `crosslink/hub` orphan branch | Distributed, no external service needed |
| Dual storage | JSON on git (truth) + SQLite (cache) | Fast local reads, durable distributed state |
| UUID-first IDs | Create with UUID, display_id assigned on push | Offline creation, eventual consistency |
| SSH signing (not GPG) | Ed25519 keys, AllowedSigners format | Modern, fast, offline verification |
| Hook-based enforcement | Python scripts on Claude tool calls | Non-intrusive, configurable strictness |
| Adaptive context injection | Drift counter, threshold-based reminders | Saves ~33% context tokens over 50 prompts |
| On-demand skills | /preflight, /review, /audit | Rules loaded only when needed, not every prompt |
