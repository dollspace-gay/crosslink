---
allowed-tools: Bash(tmux *), Bash(cat *), Bash(test *), Bash(ls *), Bash(git *), Bash(tail *)
description: Check on a background feature agent running in a tmux session
---

## Context

- Active tmux sessions: !`tmux list-sessions 2>/dev/null || echo "no tmux server running"`
- Current worktrees: !`git worktree list`

## Your task

The user optionally provides a tmux session name (e.g. `feat-add-batch-retry`). If no session name is given, check **all** active `feat-*` tmux sessions and report a summary for each.

### 1. Identify sessions to check

- If the user provided a session name, use that single session.
- If no session name was provided, find `feat-*` tmux sessions that belong to **this repo only**:
  1. Get the list of worktree paths for this repo: `git worktree list --porcelain | grep '^worktree ' | sed 's/^worktree //'`
  2. Get all `feat-*` tmux sessions with their working directories: `tmux list-sessions -F '#{session_name} #{session_path}' 2>/dev/null | grep '^feat-'`
  3. Only include sessions whose `session_path` matches one of this repo's worktree paths
- If no matching sessions exist, report "No active feature agent sessions for this repo."

### 2. For each session, perform these checks:

#### a. Check the sentinel file

Get the worktree path for this session from tmux: `tmux display-message -t <session-name> -p '#{session_path}'`. Alternatively, match the session name to a feature branch in `git worktree list`.

- Check if `.kickoff-status` exists in the worktree: `cat <worktree-path>/.kickoff-status 2>/dev/null`
- If it contains `DONE`, mark this session as finished.

#### b. Capture the terminal state

```bash
tmux capture-pane -t <session-name> -p -S -80
```

This captures the last ~80 lines of visible output.

#### c. Analyze state

Read the captured output and determine the agent's current state:

- **Working**: Tool calls in progress, code being written/read
- **Waiting for input**: A question or prompt is displayed (look for `?`, option lists, or input prompts)
- **Error/stuck**: Error messages, repeated failures, or no recent activity
- **Completed**: The sentinel file says DONE, or the claude process has exited

### 3. Report

When checking **multiple sessions**, use a compact table format:

```
Feature Agents:

  feat-add-retry       Working    Implementing retry logic in _sources.py
  feat-fix-lens-bug    Done       All changes committed and reviewed
  feat-new-cli-cmd     Waiting    Asking about CLI argument format
```

When checking a **single session**, use the detailed format:

```
Session: <name>
Status:  <Working | Waiting | Done | Error>

<2-3 sentence summary of what the agent is currently doing or has accomplished>
```

### 4. Offer actions

Based on the status of each session, suggest relevant next steps:

- **If working**: "Check back later, or attach directly: `tmux attach -t <name>`"
- **If waiting for input**: Read the question, and ask the user what to answer. If the user provides an answer, send it: `tmux send-keys -t <session-name> "<response>" Enter`
- **If done**: "Agent finished. Review the changes: `cd <worktree-path> && git log --oneline <base-branch>..HEAD`"
- **If error**: Show the relevant error output and suggest the user attach to debug: `tmux attach -t <name>`

## Constraints

- Do not modify any files in the worktree — this is a read-only check.
- Do not kill the tmux session unless the user explicitly asks.
- When relaying a user's answer to a waiting prompt, send exactly what the user provides — do not embellish or modify.
