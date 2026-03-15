# Crosslink Issue Tracker

Track tasks across AI sessions. Data in `.crosslink/issues.db`.

## Issue Commands

```bash
# Create and manage issues (canonical: crosslink issue <verb>)
crosslink issue create "title" [-p high] [-d "desc"]
crosslink issue quick "title" -p <priority> -l <label>   # create + label + session work
crosslink issue list [-s all|closed] [-l label] [-p priority]
crosslink issue search "query"
crosslink issue show <id>
crosslink issue update <id> [-t "new title"] [-p priority]
crosslink issue close <id>
crosslink issue close-all [--no-changelog]
crosslink issue reopen <id>
crosslink issue delete <id>
crosslink issue next                                      # suggest next issue to work on

# Subissues and hierarchy
crosslink subissue <parent> "title"
crosslink issue tree

# Comments and documentation trail
crosslink issue comment <id> "text" --kind <plan|decision|observation|blocker|resolution|result>
crosslink issue intervene <id> "description" --trigger <type> --context "what you were doing"

# Labels, relations, and blocking
crosslink issue label <id> <label>
crosslink issue unlabel <id> <label>
crosslink issue block <id> <blocker-id>
crosslink issue unblock <id> <blocker-id>
crosslink issue blocked
crosslink issue ready
crosslink issue relate <id1> <id2>
crosslink issue tested <id>
```

Top-level shortcuts still work: `crosslink create`, `crosslink list`, `crosslink quick`, etc.

**Global flags**: `--quiet` / `-q` (minimal output, scripts), `--json` (machine-readable output).

## Session Commands

```bash
crosslink session start                    # begin session, see previous handoff
crosslink session end --notes "context"    # save handoff notes
crosslink session status                   # show current session info
crosslink session work <id>                # set active work item
crosslink session last-handoff             # show previous session's handoff notes
crosslink session action "description"     # record action breadcrumb for context compression
```

## Other Command Groups

```bash
# Time tracking
crosslink timer start|stop|show <id>

# Knowledge base (shared markdown pages on crosslink/knowledge branch)
crosslink knowledge add|show|list|edit|remove|sync|import|search

# Agent management
crosslink agent init|status|bootstrap
crosslink trust approve|revoke|list|pending|check
crosslink locks list|check|claim|release|steal

# Kickoff (launch agents to implement features)
crosslink kickoff run|status|logs|stop|plan|show-plan|report|list|cleanup

# Swarm (multi-agent coordination)
crosslink swarm init|status|resume|launch|gate|checkpoint|config|estimate|harvest|plan|plan-show

# Container execution
crosslink container build|start|ps|logs|stop|rm|kill|shell|snapshot

# Infrastructure
crosslink daemon start|stop|status
crosslink config show|get|set|list|reset|diff
crosslink sync                             # sync state from remote
crosslink compact                          # run event compaction
crosslink prune                            # prune hub/knowledge history
crosslink integrity counters|hydration|locks|schema

# Organization
crosslink milestone create|list|show|add|remove|close|delete
crosslink archive add|remove|list|older
crosslink export|import

# UI
crosslink tui                              # interactive terminal dashboard
crosslink mc                               # tmux mission control
crosslink serve                            # web dashboard server

# Tooling
crosslink context measure|check
crosslink workflow diff|trail
crosslink style set|sync|diff|show|unset
crosslink cpitd scan|status|clear
crosslink migrate to-shared|from-shared|rename-branch
```

## Workflow

1. `session start` -> see previous handoff
2. `issue quick "what I'm doing" -p medium -l bug` -> create + track
3. Work, add typed comments (`--kind plan`, `--kind decision`, etc.)
4. `session end --notes "..."` -> save context

## Best Practices

- Start sessions when beginning work
- Use `issue ready` to find unblocked issues
- Use subissues for tasks >500 lines
- End with handoff notes before context compresses

---

*Language rules, security requirements, and testing guidelines are in `.crosslink/rules/` and auto-injected based on detected project languages.*
