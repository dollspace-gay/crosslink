---
allowed-tools: Bash(git *), Bash(uuidgen), Bash(ls *), Bash(ln *), Bash(rm *), Bash(test *), Bash(mkdir *), Bash(grep *), Skill
description: Create a feature branch and move it to a new git worktree
---

## Context

- Current repo root: !`git rev-parse --show-toplevel`
- Current branch: !`git branch --show-current`
- Existing worktrees: !`git worktree list`
- Working tree status: !`git status --short`

## Your task

The user provides a human-readable feature description (e.g. "add batch retry logic"). You will first create a feature branch using the `/feature` skill, then move it into a new git worktree.

### 1. Create the feature branch

- Invoke the `/feature` skill with the user's description as the argument.
- This creates the `feature/<slug>` branch.
- Note the branch name that was created.

### 2. Generate worktree path

- The worktree directory is `<repo-root>/.worktrees/<slug>` (inside the repo, gitignored).
- Extract the slug from the branch name by stripping the `feature/` prefix.
- Create the `.worktrees` directory if it doesn't exist: `mkdir -p <repo-root>/.worktrees`
- Ensure `.worktrees/` is gitignored: check if it's already in `.gitignore`, and if not, append it.

### 3. Create the worktree

- Switch back to the previous branch (the one we were on before `/feature` created the new branch): `git checkout -`
- Create the worktree pointing at the feature branch: `git worktree add <worktree-path> feature/<slug>`

### 4. Symlink shared databases (if applicable)

Check if the project uses any local databases or state files that should be shared across worktrees (e.g. `.crosslink/issues.db`). For each one found in the base repo:

```bash
# Only if the file exists in the base repo
ln -s <repo-root>/<db-file> <worktree-path>/<db-file>
```

Known database files to check: `.crosslink/issues.db`

If none of these files exist in the base repo, skip this step.

### 5. Report to user

Print a summary:
```
Worktree: <path>
Branch:   feature/<slug>

To start working:
  cd <worktree-path>
```

## Constraints

- Never force-push or delete branches.
- Do not push the branch to a remote — the user will do that when ready.
- Worktrees MUST be placed inside `<repo-root>/.worktrees/` to inherit the project's Claude Code trust scope and settings hierarchy.
