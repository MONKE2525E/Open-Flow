# Multi Agent Worktrees Skill

Use this skill when you want to run multiple agents at the same time without file collisions.

## Goal

Create isolated working directories per agent using `git worktree`, run each agent in its own branch, then cleanly push and tear down.

## Preconditions

1. Run from the repo root (`Open Flow`).
2. `git status --porcelain` should be clean in the main repo before spawning worktrees.
3. Keep the main repo on your stable branch (`main` unless explicitly told otherwise).

## Standard Layout

From your repo parent directory, create a sibling area for active worktrees:

- `<project-parent>/agent-worktrees/agent-<task-or-id>/`

Example:

- `/workspace/agent-worktrees/agent-settings-redesign/`
- `/workspace/agent-worktrees/agent-hotkey-fix/`
- `C:\dev\agent-worktrees\agent-settings-redesign\`

Do not nest worktrees inside the main repo.

## Setup Sequence (Per Agent)

1. Define names:
   - `BRANCH=agent/<task-or-id>`
   - `WT_PATH=<project-parent>/agent-worktrees/agent-<task-or-id>`
2. Create the worktree from repo root:
   - `git worktree add "<WT_PATH>" -b "<BRANCH>"`
3. Move into the worktree:
   - `cd "<WT_PATH>"` (bash/zsh)
   - `Set-Location "<WT_PATH>"` (PowerShell)
4. Install deps only if needed:
   - `npm install`
5. Run the agent workflow in that path only.

## Agent Rules

1. Never edit files in the main repo path while agent worktrees are active.
2. All commits must be made from inside the assigned worktree.
3. Keep branch scope narrow to one task.
4. Never force push unless explicitly approved.
5. Never edit `tests/smoke/` files.

## Verification Before Push

Run inside the agent worktree:

1. `npm run check`
2. `npm run lint`
3. Relevant smoke test flow from `Agent-Skills/SmokeTest.md`
4. Review diff:
   - `git status`
   - `git diff --stat`

## Push and Teardown

Run these after agent work is committed and pushed:

1. From worktree:
   - `git push -u origin "<BRANCH>"`
2. Return to your main repo path.
3. Remove worktree:
   - `git worktree remove "<WT_PATH>"`
4. Optionally delete local branch if merged:
   - `git branch -d "<BRANCH>"`

If Git says the worktree is dirty, either commit/stash inside that worktree or inspect and intentionally discard there. Do not run destructive cleanup commands from the main repo.

## Recovery Commands

1. List active worktrees:
   - `git worktree list`
2. Repair stale metadata:
   - `git worktree prune`
3. If a folder was manually deleted:
   - `git worktree prune`
   - then verify with `git worktree list`

## Orchestrator Prompt Snippet

Use this exact instruction when launching an agent:

`Set your AI area up with Agent-Skills/Multi_Agent_Worktrees.md, create a new worktree + branch for this task, do all edits and commits there, run required checks, then show me diff/test results for approval before push.`

## Why This Setup Wins

1. No file collisions across agents.
2. No duplicate `.git` or full reclones.
3. Fast create/remove cycle for temporary isolated workspaces.
