# Crosslink Policy Review Guide

This document describes how forecast-bio reviews, updates, and evolves crosslink's behavioral policies independently from the upstream chainlink project.

## Policy Architecture

Crosslink enforces policies at three layers:

1. **Rules** (`.crosslink/rules/`) — Markdown files injected into Claude's context on every prompt. These define what Claude should and shouldn't do.
2. **Hooks** (`.claude/hooks/`) — Python scripts that run as Claude Code hooks. These enforce rules mechanically (block forbidden commands, require issue tracking, etc.).
3. **Configuration** (`.crosslink/hook-config.json`) — JSON that controls enforcement levels and command allow/block lists.

All three layers are compiled into the `crosslink` binary at build time and deployed via `crosslink init`.

## Source of Truth

All policy files live in the crosslink source tree:

```
crosslink/resources/crosslink/
├── rules/
│   ├── global.md              # Priority 1-4 rules (security, correctness, workflow, style)
│   ├── project.md             # Project-specific customizations
│   ├── web.md                 # Web security / prompt injection prevention (RFIP)
│   ├── sanitize-patterns.txt  # Regex patterns for sanitizing web content
│   ├── tracking-strict.md     # Strict mode enforcement rules
│   ├── tracking-normal.md     # Normal mode (warn, don't block)
│   ├── tracking-relaxed.md    # Relaxed mode (no issue enforcement)
│   └── <language>.md          # Language-specific rules (rust.md, python.md, etc.)
└── hook-config.json           # Default enforcement configuration
```

Hook scripts and Claude settings templates live in:

```
crosslink/resources/claude/
├── settings.json              # Claude Code settings template
├── hooks/
│   ├── prompt-guard.py        # Injects behavioral rules per-prompt
│   ├── work-check.py          # Enforces issue tracking and git restrictions
│   ├── session-start.py       # Auto-starts sessions, loads handoff context
│   ├── post-edit-check.py     # Validates code quality after edits
│   └── pre-web-check.py       # Enforces safe-fetch MCP usage
└── mcp/
    └── safe-fetch-server.py   # MCP server for sanitized web fetching
```

## Review Checklist

When reviewing policies, walk through each component:

### 1. Global Rules (`rules/global.md`)

| Priority | Section | What to review |
|----------|---------|----------------|
| 1 | Security | Safe-fetch enforcement, parameterized queries, no hardcoded secrets |
| 2 | Correctness | No-stubs rule, read-before-write, error handling, test requirements |
| 3 | Workflow | Issue tracking, session management, changelog conventions |
| 4 | Style | Verbosity preferences, implementation size thresholds |

**Key question**: Are the priority orderings still right for our team?

### 2. Tracking Mode (`hook-config.json` → `tracking_mode`)

| Mode | Behavior |
|------|----------|
| `strict` | Blocks Write/Edit/Bash without an active crosslink issue |
| `normal` | Warns about missing issues but doesn't block |
| `relaxed` | No issue enforcement; only git command restrictions apply |

**Key question**: Is `strict` still the right default for new projects?

### 3. Git Command Restrictions (`hook-config.json`)

Three categories:

- **`blocked_git_commands`**: Always forbidden (push, merge, rebase, reset, etc.). The human performs these.
- **`gated_git_commands`**: Allowed only when there's an active crosslink issue (currently: `git commit`).
- **`allowed_bash_prefixes`**: Read-only commands that always pass through (git status, cargo test, etc.).

**Key question**: Should any commands move between categories?

### 4. Language Rules (`rules/<language>.md`)

Each detected language gets its own rules injected. Review for:
- Are the style conventions current with our team preferences?
- Are the security rules appropriate for our stack?
- Do testing requirements match our CI setup?

### 5. Web Security (`rules/web.md` + `sanitize-patterns.txt`)

- RFIP (Recursive Framing Interdiction Protocol) prevents prompt injection from web content
- `sanitize-patterns.txt` contains regex patterns stripped from fetched content
- The safe-fetch MCP server applies these patterns before content reaches Claude

**Key question**: Are there new injection patterns we should add?

### 6. Hook Behavior

| Hook | Trigger | What it does |
|------|---------|-------------|
| `prompt-guard.py` | Every prompt | Injects rules, detects project languages, loads context |
| `work-check.py` | Write/Edit/Bash | Enforces issue tracking, blocks/gates git commands |
| `session-start.py` | Session start/resume | Loads handoff notes, auto-starts sessions |
| `post-edit-check.py` | After Write/Edit | Detects stub patterns, checks code quality |
| `pre-web-check.py` | Before WebFetch/WebSearch | Enforces safe-fetch MCP usage |

## How to Make Changes

### Updating rules (what Claude should do)

1. Edit the relevant file in `crosslink/resources/crosslink/rules/`
2. Rebuild: `cd crosslink && cargo build --release`
3. Deploy to target projects: `crosslink init --force`

### Updating hooks (how rules are enforced)

1. Edit the hook in `crosslink/resources/claude/hooks/`
2. Rebuild and `crosslink init --force`, OR
3. Edit the deployed hook directly in `.claude/hooks/` for immediate testing (changes won't persist across `crosslink init`)

### Updating configuration (enforcement levels)

1. **Per-project override**: Edit `.crosslink/hook-config.json` directly (takes effect immediately)
2. **Change the default**: Edit `crosslink/resources/crosslink/hook-config.json`, rebuild, and deploy

### Adding a new language

1. Create `crosslink/resources/crosslink/rules/<language>.md`
2. Add the `include_str!()` path in `crosslink/build.rs`
3. Add language detection in `crosslink/src/commands/init.rs`
4. Rebuild and deploy

## Propagation

After editing source-of-truth files:

```bash
cd crosslink && cargo build --release
crosslink init --force   # in each target project
```

The `--force` flag overwrites deployed hooks and rules with the latest from the binary. Without `--force`, existing files are preserved.

## Divergence from Upstream

This project is a fork of [chainlink](https://github.com/dollspace-gay/chainlink). When pulling upstream changes:

1. Review upstream rule changes against our customizations
2. Our `hook-config.json` defaults may differ from upstream
3. Language rules may have upstream improvements worth merging
4. Test after merge: `cargo test` in the `crosslink/` directory

Track customizations by keeping a list of intentional divergences here:

### Current Divergences

- **Project name**: chainlink → crosslink
- **GitHub org**: dollspace-gay → forecast-bio
- **Gated git commits**: `git commit` moved from permanently blocked to gated (allowed with active issue)
- **Hook path resolution**: Uses `git rev-parse --show-toplevel` for project-root-relative paths
- **Hook runner**: Uses `python3` directly instead of `uv run` for hook execution

## Review Cadence

Suggested schedule for policy reviews:

- **Monthly**: Scan `sanitize-patterns.txt` for new injection vectors
- **Quarterly**: Review `global.md` rules and tracking mode defaults
- **Per-release**: Check language rules are current with dependency updates
- **As-needed**: Adjust `hook-config.json` when workflow changes
