---
title: Web Dashboard for Crosslink Orchestration and Monitoring
tags: [design, architecture]
sources:
  - url: DESIGN-WEB-DASHBOARD.md
    title: 
    accessed_at: 2026-03-11
contributors: [maxine--basel--gh-287-and-gh-291]
created: 2026-03-11
updated: 2026-03-11
---

# Design: React Web Dashboard for Crosslink Orchestration & Monitoring

**GH Issue:** [#257](https://github.com/forecast-bio/crosslink/issues/257)
**Status:** Draft v1
**Last updated:** 2026-03-09
**Depends on:** Heartbeat hooks (#275), Mission Control (#266), Watchdog (#282)

---

## 1. Problem Statement

Crosslink's power is locked behind a CLI. Monitoring agents means `tmux attach`, checking status means `crosslink kickoff status`, understanding the dependency graph means reading `crosslink tree` output in your head. For orchestrating multi-phase builds from design documents, you're juggling `crosslink swarm`, `kickoff`, `mission-control`, and manual merges.

A web dashboard unlocks:
- **At-a-glance monitoring** — see all agents, heartbeats, locks, and progress without attaching to terminals
- **Full CRUD** — every crosslink command available through forms, not memorized CLI syntax
- **Design doc orchestration** — upload a doc, review the decomposed plan, hit "Go", watch the DAG execute
- **Real-time streaming** — heartbeats and events push to the browser, no polling

### What exists today

| Capability | Status | Location |
|-----------|--------|----------|
| Agent heartbeats on hub branch | Working | `sync.rs:701` (`push_heartbeat`) |
| Heartbeat reading + staleness | Working | `sync.rs:763` (`read_heartbeats`) |
| Lock management (claim/release/stale) | Working | `sync.rs:1043` (`claim_lock`) |
| Issue CRUD + full organization | Working | `db.rs` (50+ public methods) |
| Session management | Working | `db.rs` sessions API |
| Milestone management | Working | `db.rs` milestones API |
| Knowledge pages | Working | `knowledge.rs` |
| Hub sync (push/pull/fetch) | Working | `sync.rs` |
| Export/import (JSON) | Working | `commands/export.rs`, `commands/import.rs` |
| Swarm plan/execute/resume | Working | `commands/swarm.rs` |
| Kickoff run/plan/status/report | Working | `commands/kickoff.rs` |
| TUI (ratatui terminal UI) | Working | `tui/` |
| Mission control (tmux dashboard) | Working | `commands/mission_control.rs` |
| Watchdog (idle agent nudging) | Working | `commands/kickoff.rs` watchdog sidecar |

### Design goals

1. **Full CLI parity** — every crosslink command has a GUI equivalent
2. **Real-time** — WebSocket push for heartbeats, events, agent status
3. **Orchestration** — LLM-assisted design doc decomposition → DAG execution
4. **Localhost-first** — no auth, no cloud, single-operator dashboard
5. **Parallel buildable** — each phase decomposes into 3-5 independent agent tasks

---

## 2. Architecture

```
┌─────────────────────────────────────────────┐
│               Browser (React)                │
│  ┌─────────┐ ┌──────────┐ ┌──────────────┐  │
│  │ Agent   │ │ Issues / │ │ Design Doc   │  │
│  │ Monitor │ │ Sessions │ │ Orchestrator │  │
│  └────┬────┘ └────┬─────┘ └──────┬───────┘  │
│       │           │              │           │
│       └───────────┼──────────────┘           │
│               WebSocket + REST               │
└───────────────────┬─────────────────────────┘
                    │
┌───────────────────┴─────────────────────────┐
│          crosslink serve (axum)              │
│  ┌──────────┐ ┌──────────┐ ┌─────────────┐  │
│  │ REST API │ │ WS Hub   │ │ Static File │  │
│  │ /api/*   │ │ /ws      │ │ Serving     │  │
│  └────┬─────┘ └────┬─────┘ └─────────────┘  │
│       │            │                         │
│  ┌────┴────────────┴──────────────────────┐  │
│  │         Crosslink Core (lib.rs)        │  │
│  │  Database · SyncManager · Knowledge    │  │
│  │  Identity · Kickoff · Swarm · Events   │  │
│  └────────────────────────────────────────┘  │
└──────────────────────────────────────────────┘
        │              │
   ┌────┴────┐    ┌────┴────┐
   │ SQLite  │    │ Hub Git │
   │issues.db│    │ Branch  │
   └─────────┘    └─────────┘
```

### 2.1 Backend: `crosslink serve`

New subcommand added to the existing `crosslink` binary.

```
crosslink serve [--port 3100] [--dashboard-dir ./dashboard/dist]
```

**Framework:** axum (already in the Rust ecosystem, async, lightweight)

**Key design decisions:**
- Direct Rust function calls into `db.rs`, `sync.rs`, `knowledge.rs` etc. — no shelling out
- `Database` and `SyncManager` wrapped in `Arc<Mutex<>>` for shared access across handlers
- WebSocket hub uses `tokio::sync::broadcast` — file watcher on `issues.db` and hub cache triggers events
- Static file serving from `dashboard/dist/` on disk (not embedded — dashboard is optional)
- All API responses are JSON, all mutations accept JSON bodies

**New Cargo dependencies:**
- `axum` — HTTP framework
- `tower-http` — CORS, static file serving, compression
- `tokio` — async runtime (may already be transitive)
- `tokio-tungstenite` or axum's built-in WS — WebSocket support
- `notify` — filesystem watcher for real-time event push

### 2.2 Frontend: `dashboard/`

Lives at repo root as a sibling to `crosslink/`.

```
dashboard/
├── package.json
├── vite.config.ts
├── tsconfig.json
├── src/
│   ├── main.tsx
│   ├── App.tsx
│   ├── api/              # REST client + WebSocket hook
│   │   ├── client.ts     # fetch wrapper, typed endpoints
│   │   └── ws.ts         # WebSocket connection + reconnect
│   ├── stores/           # zustand stores
│   │   ├── agents.ts
│   │   ├── issues.ts
│   │   └── orchestrator.ts
│   ├── pages/
│   │   ├── Dashboard.tsx       # Overview / home
│   │   ├── Agents.tsx          # Agent monitoring
│   │   ├── AgentDetail.tsx     # Single agent drilldown
│   │   ├── Issues.tsx          # Issue list
│   │   ├── IssueDetail.tsx     # Single issue view
│   │   ├── Sessions.tsx        # Session management
│   │   ├── Milestones.tsx      # Milestone views
│   │   ├── Knowledge.tsx       # Knowledge browser
│   │   ├── Sync.tsx            # Hub sync status
│   │   ├── Config.tsx          # Config editor
│   │   ├── Orchestrator.tsx    # Design doc import
│   │   └── Execution.tsx       # DAG execution view
│   ├── components/
│   │   ├── ui/           # shadcn/ui components
│   │   ├── AgentCard.tsx
│   │   ├── IssueTable.tsx
│   │   ├── DagGraph.tsx
│   │   ├── GanttChart.tsx
│   │   ├── CommandPalette.tsx
│   │   └── ...
│   └── lib/
│       ├── types.ts      # Shared TypeScript types matching Rust models
│       └── utils.ts
├── public/
└── dist/                 # Built output, served by crosslink serve
```

**Stack:**
- React 19 + TypeScript
- Vite (build + dev server with proxy to `crosslink serve`)
- shadcn/ui + Tailwind CSS 4
- zustand (state management)
- React Router v7 (navigation)
- @xyflow/react (DAG visualization — formerly reactflow)
- recharts (token usage graphs, timeline charts)

### 2.3 REST API Surface

All endpoints prefixed with `/api/v1/`.

#### Issues
| Method | Path | Maps to |
|--------|------|---------|
| GET | `/issues` | `db.list_issues()` with query params for filters |
| POST | `/issues` | `db.create_issue()` |
| GET | `/issues/:id` | `db.get_issue()` + labels + comments + deps |
| PATCH | `/issues/:id` | `db.update_issue()` |
| DELETE | `/issues/:id` | `db.delete_issue()` |
| POST | `/issues/:id/close` | `db.close_issue()` |
| POST | `/issues/:id/reopen` | `db.reopen_issue()` |
| POST | `/issues/:id/subissue` | `db.create_subissue()` |
| GET | `/issues/:id/comments` | `db.get_comments()` |
| POST | `/issues/:id/comments` | `db.add_comment()` |
| POST | `/issues/:id/labels` | `db.add_label()` |
| DELETE | `/issues/:id/labels/:label` | `db.remove_label()` |
| POST | `/issues/:id/block` | `db.add_dependency()` |
| DELETE | `/issues/:id/block/:blocker` | `db.remove_dependency()` |
| GET | `/issues/:id/tree` | `db.get_subissues()` recursive |
| GET | `/issues/blocked` | `db.get_blocked_issues()` |
| GET | `/issues/ready` | `db.get_ready_issues()` |

#### Sessions
| Method | Path | Maps to |
|--------|------|---------|
| GET | `/sessions/current` | `db.get_current_session_for_agent()` |
| POST | `/sessions/start` | `db.start_session()` |
| POST | `/sessions/end` | `db.end_session()` |
| POST | `/sessions/work/:id` | `db.set_active_work()` |

#### Milestones
| Method | Path | Maps to |
|--------|------|---------|
| GET | `/milestones` | `db.list_milestones()` |
| POST | `/milestones` | `db.create_milestone()` |
| GET | `/milestones/:id` | `db.get_milestone()` |
| POST | `/milestones/:id/assign` | `db.assign_milestone()` |
| POST | `/milestones/:id/close` | `db.close_milestone()` |

#### Knowledge
| Method | Path | Maps to |
|--------|------|---------|
| GET | `/knowledge` | `knowledge::list_pages()` |
| GET | `/knowledge/:slug` | `knowledge::read_page()` |
| POST | `/knowledge` | `knowledge::create_page()` |
| GET | `/knowledge/search?q=` | `knowledge::search_content()` |

#### Agents & Monitoring
| Method | Path | Maps to |
|--------|------|---------|
| GET | `/agents` | `sync.read_heartbeats()` + worktree probe |
| GET | `/agents/:id` | Agent detail (heartbeat + locks + events) |
| GET | `/agents/:id/status` | `kickoff::status()` equivalent |
| GET | `/locks` | `sync.read_locks_auto()` |
| GET | `/locks/stale` | `sync.find_stale_locks_with_age()` |

#### Sync
| Method | Path | Maps to |
|--------|------|---------|
| GET | `/sync/status` | Hub init state, last fetch time |
| POST | `/sync/fetch` | `sync.fetch()` |
| POST | `/sync/push` | `sync.push()` |

#### Config
| Method | Path | Maps to |
|--------|------|---------|
| GET | `/config` | Read `hook-config.json` |
| PATCH | `/config` | Merge-update `hook-config.json` |

#### Orchestrator
| Method | Path | Maps to |
|--------|------|---------|
| POST | `/orchestrator/decompose` | LLM-assisted doc → phase/stage/task breakdown |
| GET | `/orchestrator/plan` | Current execution plan |
| POST | `/orchestrator/execute` | Start DAG execution |
| POST | `/orchestrator/pause` | Pause execution |
| GET | `/orchestrator/status` | Execution progress |

### 2.4 WebSocket Protocol

Single WebSocket endpoint: `/ws`

Messages are JSON with a `type` field:

```typescript
// Server → Client
{ type: "heartbeat", agent_id: string, timestamp: string, issue_id?: number }
{ type: "agent_status", agent_id: string, status: "running" | "idle" | "done" | "failed" }
{ type: "issue_updated", issue_id: number, field: string }
{ type: "lock_changed", issue_id: number, action: "claimed" | "released" }
{ type: "execution_progress", phase: string, stage: string, status: string }

// Client → Server
{ type: "subscribe", channels: ["agents", "issues", "execution"] }
```

Implementation: `notify` crate watches `issues.db` mtime and hub cache directory. On change, diff the state and broadcast relevant events through `tokio::sync::broadcast`.

---

## 3. Phase Breakdown

### Phase 1: Skeleton (3 agents, ~2 hours each)

**Merge gate:** `crosslink serve` boots, serves the React app at `http://localhost:3100`, health endpoint returns OK, frontend shows a layout shell with sidebar navigation.

#### Agent 1A: Rust axum server

**Files to create/modify:**
- `crosslink/Cargo.toml` — add axum, tower-http, tokio, serde_json deps
- `crosslink/src/server/mod.rs` — server module
- `crosslink/src/server/state.rs` — `AppState` struct wrapping `Arc<Database>`, `Arc<SyncManager>`, config
- `crosslink/src/server/routes.rs` — route definitions
- `crosslink/src/server/handlers/health.rs` — `GET /api/v1/health`
- `crosslink/src/main.rs` — add `Commands::Serve { port, dashboard_dir }` variant

**Deliverables:**
- `crosslink serve --port 3100 --dashboard-dir ./dashboard/dist` starts an axum server
- `GET /api/v1/health` returns `{"status": "ok", "version": "0.4.0"}`
- Static files served from the dashboard directory at `/`
- CORS configured for development (vite dev server on :5173)

#### Agent 1B: React + Vite scaffold

**Files to create:**
- `dashboard/package.json` — deps: react, react-dom, react-router, zustand, tailwindcss, shadcn/ui
- `dashboard/vite.config.ts` — proxy `/api` and `/ws` to `localhost:3100`
- `dashboard/tsconfig.json`
- `dashboard/tailwind.config.ts`
- `dashboard/src/main.tsx` — React entry point
- `dashboard/src/App.tsx` — Router with sidebar layout
- `dashboard/src/pages/Dashboard.tsx` — placeholder home page
- `dashboard/src/api/client.ts` — typed fetch wrapper
- `dashboard/src/api/ws.ts` — WebSocket connection manager with auto-reconnect
- `dashboard/src/components/ui/` — shadcn/ui init (button, card, table, dialog, input, badge)
- `dashboard/src/lib/types.ts` — TypeScript types matching Rust models (Issue, Session, Agent, etc.)

**Deliverables:**
- `cd dashboard && npm install && npm run dev` starts dev server on :5173
- Sidebar navigation with placeholder links for all sections
- Dark theme (matches terminal aesthetic)
- API client with typed methods (stubs that call the health endpoint)
- WebSocket hook that connects and logs messages

#### Agent 1C: API contract + shared types

**Files to create:**
- `dashboard/src/lib/types.ts` — complete TypeScript types for all API entities
- `crosslink/src/server/types.rs` — serde-serializable response/request types
- `docs/api.md` — API reference documenting every endpoint, request/response shapes

**Deliverables:**
- TypeScript types for: Issue, Comment, Label, Session, Milestone, Agent, Heartbeat, Lock, KnowledgePage, OrchestratorPlan, ExecutionStatus
- Rust response structs with `#[derive(Serialize)]` matching the TS types
- Request structs with `#[derive(Deserialize)]` for mutations
- API reference document that agents in later phases can use as their spec

---

### Phase 2: Agent Dashboard (4 agents, ~2 hours each)

**Merge gate:** Dashboard shows live agent cards that update in real-time via WebSocket. Clicking an agent shows detail view with heartbeat timeline.

**Depends on:** Phase 1

#### Agent 2A: Backend — agent REST endpoints

**Files to create/modify:**
- `crosslink/src/server/handlers/agents.rs`

**Endpoints:**
- `GET /api/v1/agents` — list all agents with latest heartbeat, status, worktree info
  - Combines `sync.read_heartbeats_auto()` with worktree probing (reuse `mission_control::discover_agents` logic)
- `GET /api/v1/agents/:id` — single agent detail: heartbeat history, locks held, active issue
- `GET /api/v1/agents/:id/status` — kickoff status (`.kickoff-status` file content + tmux session state)
- `GET /api/v1/locks` — all current locks
- `GET /api/v1/locks/stale` — stale locks with age

#### Agent 2B: Backend — WebSocket hub

**Files to create/modify:**
- `crosslink/src/server/ws.rs` — WebSocket handler, broadcast hub
- `crosslink/src/server/watcher.rs` — filesystem watcher using `notify` crate

**Deliverables:**
- WebSocket upgrade at `/ws`
- `notify` watcher on hub cache directory — detects heartbeat file changes
- Broadcasts `heartbeat` and `agent_status` events to all connected clients
- Client can send `subscribe` message to filter channels
- Heartbeat polling fallback: if no fs events in 30s, re-read heartbeats and diff

#### Agent 2C: Frontend — agent list view

**Files to create/modify:**
- `dashboard/src/pages/Agents.tsx` — agent list page
- `dashboard/src/components/AgentCard.tsx` — status card per agent
- `dashboard/src/stores/agents.ts` — zustand store, populated via REST + WS updates

**Deliverables:**
- Grid of agent cards showing: name, status (running/idle/done/failed), last heartbeat age, active issue
- Color-coded status indicators (green=active, yellow=idle, red=stale, grey=done)
- Auto-updates via WebSocket — cards animate on heartbeat receipt
- Click card → navigates to detail page
- Empty state: "No active agents. Launch one with `crosslink kickoff run`"

#### Agent 2D: Frontend — agent detail view

**Files to create/modify:**
- `dashboard/src/pages/AgentDetail.tsx`
- `dashboard/src/components/HeartbeatTimeline.tsx`
- `dashboard/src/components/LockList.tsx`

**Deliverables:**
- Agent metadata: ID, worktree path, branch, session name
- Heartbeat timeline (last 24h, shows active/idle periods)
- Locks currently held
- Active issue link
- Kickoff status + report summary if available
- "Nudge" button — calls backend to send `tmux send-keys` continue (stretch goal)

---

### Phase 3: Issues & Sessions (4 agents, ~3 hours each)

**Merge gate:** Full issue CRUD through the web UI. Create, edit, close, reopen, comment, label, manage dependencies. Session start/end/work.

**Depends on:** Phase 1

#### Agent 3A: Backend — issues CRUD endpoints

**Files to create/modify:**
- `crosslink/src/server/handlers/issues.rs`

**Endpoints:** All issue endpoints from the API surface table in section 2.3.

Key implementation notes:
- `GET /issues` supports query params: `?status=open&label=bug&priority=high&search=text`
- `GET /issues/:id` returns a hydrated object: issue + labels + comments + blockers + blocking + subissues
- All mutations broadcast `issue_updated` over WebSocket

#### Agent 3B: Backend — sessions + organization endpoints

**Files to create/modify:**
- `crosslink/src/server/handlers/sessions.rs`
- `crosslink/src/server/handlers/milestones.rs`

**Endpoints:** Sessions and milestones from section 2.3.

#### Agent 3C: Frontend — issue list + detail

**Files to create/modify:**
- `dashboard/src/pages/Issues.tsx`
- `dashboard/src/pages/IssueDetail.tsx`
- `dashboard/src/components/IssueTable.tsx`
- `dashboard/src/components/IssueForm.tsx`
- `dashboard/src/components/CommentThread.tsx`
- `dashboard/src/stores/issues.ts`

**Deliverables:**
- Sortable, filterable issue table (status, priority, label, search)
- Inline status toggle (open/closed)
- Create issue dialog
- Issue detail page: title, description, priority, labels (as chips), status
- Comment thread with add comment form
- Dependency visualization: "Blocked by" / "Blocking" lists with links
- Subissue tree view

#### Agent 3D: Frontend — session + organization UI

**Files to create/modify:**
- `dashboard/src/pages/Sessions.tsx`
- `dashboard/src/components/SessionPanel.tsx`
- `dashboard/src/components/LabelManager.tsx`
- `dashboard/src/components/DependencyEditor.tsx`

**Deliverables:**
- Current session status panel (sidebar or header widget)
- Start/end session buttons
- "Work on" issue selector
- Label management (add/remove with autocomplete)
- Dependency editor: add blocker with issue picker, remove blocker
- Bulk operations: multi-select issues → close/label/assign milestone

---

### Phase 4: Remaining CLI Parity (4 agents, ~2 hours each)

**Merge gate:** Every crosslink CLI command has a web equivalent. Command palette works.

**Depends on:** Phase 3

#### Agent 4A: Backend — knowledge + search endpoints

**Files to create/modify:**
- `crosslink/src/server/handlers/knowledge.rs`
- `crosslink/src/server/handlers/search.rs`

**Endpoints:** Knowledge and search from section 2.3. Search endpoint performs full-text search across issues, comments, and knowledge pages.

#### Agent 4B: Backend — sync + config endpoints

**Files to create/modify:**
- `crosslink/src/server/handlers/sync.rs`
- `crosslink/src/server/handlers/config.rs`

**Endpoints:** Sync and config from section 2.3.

#### Agent 4C: Frontend — knowledge, milestones, search

**Files to create/modify:**
- `dashboard/src/pages/Knowledge.tsx`
- `dashboard/src/pages/KnowledgeDetail.tsx`
- `dashboard/src/pages/Milestones.tsx`
- `dashboard/src/components/CommandPalette.tsx`

**Deliverables:**
- Knowledge page browser with markdown rendering
- Create/edit knowledge pages
- Milestone list with progress bars (% of assigned issues closed)
- Command palette (Cmd+K): fuzzy search across issues, pages, agents, commands
- Search results page

#### Agent 4D: Frontend — sync dashboard + config editor

**Files to create/modify:**
- `dashboard/src/pages/Sync.tsx`
- `dashboard/src/pages/Config.tsx`
- `dashboard/src/components/LockVisualization.tsx`

**Deliverables:**
- Sync status: last fetch time, hub branch state, push/pull buttons
- Lock table: who holds what, staleness indicators
- Config editor: form-based editor for `hook-config.json` fields
- Trust store viewer: list of allowed signers

---

### Phase 5: Token Tracking (2 agents, ~3 hours each)

**Merge gate:** Per-agent token usage displayed, session cost estimates, usage graphs.

**Depends on:** Phase 2 (agent data model)

#### Agent 5A: Backend — token usage collection + storage

**Files to create/modify:**
- `crosslink/src/token_usage.rs` — token tracking module
- `crosslink/src/db.rs` — new table `token_usage(agent_id, session_id, timestamp, input_tokens, output_tokens, model, cost_estimate)`
- `crosslink/src/server/handlers/usage.rs`

**Implementation:**
- Parse token usage from agent event logs (kickoff reports already have timing data)
- Store per-interaction token counts in SQLite
- REST endpoints: `GET /api/v1/usage?agent_id=&from=&to=`, `GET /api/v1/usage/summary`
- Aggregate by agent, session, time window

#### Agent 5B: Frontend — usage graphs + budget alerts

**Files to create/modify:**
- `dashboard/src/pages/Usage.tsx`
- `dashboard/src/components/UsageChart.tsx`
- `dashboard/src/components/CostBreakdown.tsx`

**Deliverables:**
- Per-agent token usage bar chart
- Session timeline showing input/output token consumption
- Cost estimate display (based on model pricing)
- Cumulative usage over time (line chart)
- Budget threshold configuration + visual alert when approaching limit

---

### Phase 6: Design Document Orchestration (5 agents, ~4 hours each)

**Merge gate:** Upload a design doc, review LLM-decomposed plan, edit stages, execute as a managed DAG, monitor progress in real-time.

**Depends on:** Phases 1-4

#### Agent 6A: Backend — LLM-assisted document decomposition

**Files to create/modify:**
- `crosslink/src/orchestrator/mod.rs`
- `crosslink/src/orchestrator/decompose.rs`
- `crosslink/src/orchestrator/models.rs`
- `crosslink/src/server/handlers/orchestrator.rs`

**Implementation:**
- Accept markdown document via `POST /api/v1/orchestrator/decompose`
- Call Claude API (via `claude` CLI or direct API) with a structured prompt:
  - "Decompose this build document into phases, stages, and tasks"
  - Output format: JSON with phases → stages → tasks, dependencies, complexity estimates
- Parse response into `OrchestratorPlan` struct
- Store plan in `.crosslink/orchestrator/` as JSON
- Return plan to frontend for review

**Decomposition prompt structure:**
```
Given this design document, produce a JSON execution plan:
- Phases: major milestones (sequential)
- Stages: work units within a phase (parallelizable where independent)
- Tasks: atomic work items within a stage
- Dependencies: which stages block which
- Complexity: estimated agent-hours per stage
- Agent count: suggested parallel agents per phase
```

#### Agent 6B: Backend — DAG execution engine

**Files to create/modify:**
- `crosslink/src/orchestrator/executor.rs`
- `crosslink/src/orchestrator/dag.rs`

**Implementation:**
- `OrchestratorExecutor` manages the execution lifecycle:
  1. Create crosslink issues for each stage
  2. Set up parent/child relationships (phase → stages)
  3. Set up blocking dependencies between stages
  4. Create milestones for each phase
  5. For each stage with no unmet dependencies: `kickoff run` with the stage description
  6. Monitor agent heartbeats + `.kickoff-status` for completion
  7. When a stage completes: check what it unblocks, launch newly-unblocked stages
  8. When all stages in a phase complete: run phase gate (tests, merge), advance to next phase
- Execution state stored in `.crosslink/orchestrator/execution.json`
- Exposes status via REST + WebSocket events
- Supports pause/resume — stops launching new stages but lets running ones finish

#### Agent 6C: Frontend — document import + stage editor

**Files to create/modify:**
- `dashboard/src/pages/Orchestrator.tsx`
- `dashboard/src/components/DocumentImport.tsx`
- `dashboard/src/components/StageEditor.tsx`
- `dashboard/src/stores/orchestrator.ts`

**Deliverables:**
- Document import: paste markdown or upload file
- "Decompose" button → shows loading state → displays parsed plan
- Interactive stage editor:
  - Phase accordion with nested stages
  - Drag-and-drop stage reordering within phases
  - Edit stage title, description, priority, estimated complexity
  - Add/remove dependency edges between stages
  - Assign agent pool size per phase
- "Execute" button with confirmation dialog

#### Agent 6D: Frontend — DAG/Gantt visualization

**Files to create/modify:**
- `dashboard/src/components/DagGraph.tsx`
- `dashboard/src/components/GanttChart.tsx`
- `dashboard/src/pages/Execution.tsx`

**Deliverables:**
- DAG view using @xyflow/react:
  - Nodes = stages, edges = dependencies
  - Color by status: grey=pending, blue=running, green=done, red=failed, yellow=blocked
  - Click node → shows stage detail panel
  - Animated edges showing data flow direction
- Gantt view using recharts:
  - Timeline with phase rows
  - Stage bars showing estimated vs actual duration
  - Agent assignment labels on bars
- Toggle between DAG and Gantt views
- Auto-updates via WebSocket execution_progress events

#### Agent 6E: Frontend — execution control + live monitoring

**Files to create/modify:**
- `dashboard/src/components/ExecutionControls.tsx`
- `dashboard/src/components/StageDetail.tsx`
- `dashboard/src/components/AgentLogStream.tsx`

**Deliverables:**
- Control bar: Play/Pause/Resume buttons, overall progress percentage
- Phase progress indicators in sidebar
- Stage detail panel (click from DAG or Gantt):
  - Assigned agent info
  - Live heartbeat indicator
  - Kickoff report summary when available
  - Link to issue
  - "View agent" button → navigates to agent detail page
- Execution event log: chronological list of stage starts, completions, failures
- Failure handling: retry button per stage, option to skip and continue

---

## 4. Dependency Graph Between Phases

```
Phase 1 (Skeleton)
    ├──→ Phase 2 (Agent Dashboard)
    │        └──→ Phase 5 (Token Tracking)
    ├──→ Phase 3 (Issues & Sessions)
    │        └──→ Phase 4 (CLI Parity)
    └──────────────→ Phase 6 (Orchestrator) [depends on 1-4]
```

Phases 2 and 3 can run **in parallel** after Phase 1.
Phase 4 depends on Phase 3 (needs issue UI to build on).
Phase 5 depends on Phase 2 (needs agent data model).
Phase 6 depends on Phases 1-4 (needs full backend + frontend foundation).

**Optimized execution order:**
1. Phase 1 (skeleton) — 3 agents
2. Phase 2 + Phase 3 **in parallel** — 8 agents simultaneously
3. Phase 4 + Phase 5 **in parallel** — 6 agents simultaneously
4. Phase 6 (orchestrator) — 5 agents

**Total: 22 agent sessions across 4 sequential rounds.**

---

## 5. File Structure Summary

### New Rust modules
```
crosslink/src/
├── server/
│   ├── mod.rs
│   ├── state.rs          # AppState, shared DB/Sync handles
│   ├── routes.rs         # All route definitions
│   ├── ws.rs             # WebSocket hub + broadcast
│   ├── watcher.rs        # Filesystem watcher for real-time events
│   ├── types.rs          # Request/response serialization types
│   └── handlers/
│       ├── mod.rs
│       ├── health.rs
│       ├── agents.rs
│       ├── issues.rs
│       ├── sessions.rs
│       ├── milestones.rs
│       ├── knowledge.rs
│       ├── search.rs
│       ├── sync.rs
│       ├── config.rs
│       ├── usage.rs
│       └── orchestrator.rs
├── orchestrator/
│   ├── mod.rs
│   ├── models.rs         # Plan, Phase, Stage, Task types
│   ├── decompose.rs      # LLM-assisted document parsing
│   ├── dag.rs            # DAG operations (topo sort, ready nodes)
│   └── executor.rs       # Execution engine
└── token_usage.rs        # Token tracking + storage
```

### New frontend directory
```
dashboard/
├── package.json
├── vite.config.ts
├── tsconfig.json
├── tailwind.config.ts
├── components.json        # shadcn/ui config
├── src/
│   ├── main.tsx
│   ├── App.tsx
│   ├── api/
│   ├── stores/
│   ├── pages/             # 12 page components
│   ├── components/        # 15+ shared components
│   └── lib/
└── dist/                  # Build output
```

---

## 6. Open Risks & Mitigations

| Risk | Mitigation |
|------|-----------|
| Agent merge conflicts (8 agents in phases 2+3) | Clear file ownership per agent. Backend agents never touch frontend, vice versa. |
| WebSocket complexity | Start with polling fallback, upgrade to WS. axum has solid WS support. |
| LLM decomposition quality (phase 6) | Human review step before execution. Iterative refinement prompt. |
| SQLite concurrent access | Single writer via `Arc<Mutex<Database>>`. Reads can use separate connections. |
| Large design docs overwhelming LLM context | Chunk by section, decompose phases independently, merge plans. |
| Dashboard build adding to CI time | Separate CI job. `crosslink serve` works without dashboard present. |
