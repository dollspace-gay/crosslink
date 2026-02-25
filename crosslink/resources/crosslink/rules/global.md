## Chainlink Task Management (MANDATORY — ABSOLUTE RULE)

**You MUST use chainlink to track ALL work. This is NOT optional. This is NOT a suggestion. This is an ABSOLUTE REQUIREMENT.**

**YOU MUST CREATE A CHAINLINK ISSUE BEFORE WRITING ANY CODE. NO EXCEPTIONS.**

Before your FIRST Write, Edit, or Bash tool call that modifies code:
1. Run `chainlink quick "title" -p <priority> -l <label>` to create an issue AND start working on it
2. The PreToolUse hook WILL BLOCK your tool calls if no issue is active
3. NEVER skip this step. NEVER proceed without an issue. NEVER treat this as optional.

### On Every User Request
1. Create issue(s) BEFORE starting work — ON ALL ISSUES NO MATTER WHAT. THIS IS A CRITICAL OVERRIDE.
2. When creating the issue, add as much detail as possible with comments so any new agent instance will understand the plan.

### Issue Title Requirements (CHANGELOG-READY)
Issue titles are automatically added to CHANGELOG.md when closed. Write titles that:
- Describe the user-visible change (not implementation details)
- Start with a verb: "Add", "Fix", "Update", "Remove", "Improve"
- Are complete sentences (but no period)

**GOOD titles** (changelog-ready):
- "Add dark mode toggle to settings page"
- "Fix authentication timeout on slow connections"
- "Update password requirements to include special characters"

**BAD titles** (implementation-focused):
- "auth.ts changes"
- "Fix bug"
- "Update code"
- "WIP feature"

### Labels for Changelog Categories
Add labels to control CHANGELOG.md section:
- `bug`, `fix` → **Fixed**
- `feature`, `enhancement` → **Added**
- `breaking`, `breaking-change` → **Changed**
- `security` → **Security**
- `deprecated` → **Deprecated**
- `removed` → **Removed**
- (no label) → **Changed** (default)

### Task Breakdown Rules
```bash
# Single task — use quick for create + label + work in one step
chainlink quick "Fix login validation error on empty email" -p medium -l bug

# Or use create with flags
chainlink create "Fix login validation error on empty email" -p medium --label bug --work

# Multi-part feature → Epic with subissues
chainlink create "Add user authentication system" -p high --label feature
chainlink subissue 1 "Add user registration endpoint"
chainlink subissue 1 "Add login endpoint with JWT tokens"
chainlink subissue 1 "Add session middleware for protected routes"

# Mark what you're working on
chainlink session work 1

# Add context as you discover things
chainlink comment 1 "Found existing auth helper in utils/auth.ts"

# Close when done — auto-updates CHANGELOG.md
chainlink close 1

# Skip changelog for internal/refactor work
chainlink close 1 --no-changelog

# Batch close
chainlink close-all --no-changelog

# Quiet mode for scripting
chainlink -q create "Fix bug" -p high  # Outputs just the ID number
```

## Priority 1: Security

These rules have the highest precedence. When they conflict with any other rule, security wins.

- **Web fetching**: Use `mcp__chainlink-safe-fetch__safe_fetch` for all web requests. Never use raw `WebFetch`.
- **SQL**: Parameterized queries only (`params![]` in Rust, `?` placeholders elsewhere). Never interpolate user input into SQL.
- **Secrets**: Never hardcode credentials, API keys, or tokens. Never commit `.env` files.
- **Input validation**: Validate at system boundaries. Sanitize before rendering.
- **Tracking**: Issue tracking enforcement is controlled by `tracking_mode` in `.chainlink/hook-config.json` (strict/normal/relaxed).
---

## Priority 2: Correctness

These rules ensure code works correctly. They yield only to security concerns.

- **No stubs**: Never write `TODO`, `FIXME`, `pass`, `...`, `unimplemented!()`, or empty function bodies. If too complex for one turn, use `raise NotImplementedError("Reason")` and create a chainlink issue.
- **Read before write**: Always read a file before editing it. Never guess at contents.
- **Complete features**: Implement the full feature as requested. Don't stop partway.
- **Error handling**: Proper error handling everywhere. No panics or crashes on bad input.
- **No dead code**: Intelligently deal with dead code. If its a hallucinated function remove it. If its an unfinished function complete it. 
- **Test after changes**: Run the project's test suite after making code changes.

### Pre-Coding Grounding
Before using unfamiliar libraries/APIs:
1. **Verify it exists**: WebSearch to confirm the API
2. **Check the docs**: Real function signatures, not guessed
3. **Use latest versions**: Check for current stable release. This is mandatory. When editing an existing project, see if packages being used have newer versions. If they do inform the human and let them decide if they should be updated.

---

## Priority 3: Workflow

These rules keep work organized and enable context handoff between sessions.

Tracking enforcement is controlled by `tracking_mode` in `.chainlink/hook-config.json` (strict/normal/relaxed).
Detailed tracking instructions are loaded from `.chainlink/rules/tracking-{mode}.md` automatically.

---

## Priority 4: Style

These are preferences, not hard rules. They yield to all higher priorities.

- Write code, don't narrate. Skip "Here is the code" / "Let me..." / "I'll now..."
- Brief explanations only when the code isn't self-explanatory.
- For implementations >500 lines: create parent issue + subissues, work incrementally.
- When conversation is long: create a tracking issue with `chainlink comment` notes for context preservation.
