# Crosslink Issue Tracker

Track tasks across AI sessions. Data in `.crosslink/issues.db`.

## Commands

```bash
# Issues
crosslink create "title" [-p high] [-d "desc"]
crosslink list [-s all|closed] [-l label] [-p priority]
crosslink show|update|close|reopen|delete <id>
crosslink subissue <parent> "title"

# Organization
crosslink comment <id> "text"
crosslink label|unlabel <id> <label>
crosslink block|unblock <id> <blocker>
crosslink blocked|ready

# Sessions
crosslink session start|end|status|work <id>
crosslink session end --notes "handoff context"
```

## Workflow

1. `session start` → see previous handoff
2. `session work <id>` → mark focus
3. Work, add comments
4. `session end --notes "..."` → save context

## Best Practices

- Start sessions when beginning work
- Use `ready` to find unblocked issues
- Use subissues for tasks >500 lines
- End with handoff notes before context compresses

---

*Language rules, security requirements, and testing guidelines are in `.crosslink/rules/` and auto-injected based on detected project languages.*
