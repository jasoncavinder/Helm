---
name: agent-worktree-isolation
description: Create an isolated linked worktree and task branch for each Helm agent task without sharing mutable Git state.
---

# agent-worktree-isolation

## Purpose

Create and use one isolated linked worktree and task branch for every Helm agent task, keeping the primary checkout coordination-only and preventing concurrent agents from sharing mutable Git state.

## When to Use

- before starting any Helm agent task from the primary checkout
- when assigning separate implementation, test, review, or exploration lanes
- when an agent must verify or resume an existing assigned in-repository worktree

## Inputs

- task ID in kebab-case
- unique task branch name
- intended base ref
- primary checkout path, discovered from Git when not supplied
- optional existing assigned worktree path

## Outputs

- dedicated worktree at `<primary-checkout>/.worktrees/<task-id>`
- unique task branch attached to that worktree
- verified worktree path, branch, and status
- concise handoff containing path, branch, changes, and verification
- safe cleanup guidance without automatic removal

## Safety Rules

- never edit, build, test, or commit from the primary coordination checkout
- never share a worktree or task branch between agents
- never create nested worktrees
- never use long-lived branches `main`, `dev`, `docs`, or `web` as task branches
- never modify or remove another agent's worktree
- never use forced worktree creation or removal
- stop and ask if the destination or branch already exists
- do not clean up a worktree containing uncommitted or untracked changes

## Workflow Steps

1. Detect whether the agent is already inside an assigned worktree under the primary checkout's `.worktrees/` directory; use it without nesting when present.
2. Identify the primary checkout and intended base ref; ask when the base is ambiguous.
3. Confirm the task ID, branch, and destination are unused and do not belong to another agent.
4. Ensure the primary .worktrees directory exists and is ignored.
5. From the primary checkout, run `scripts/create-agent-worktree.sh <task-id> <task-branch> <base-ref>` to create the unique branch and linked worktree with structured Git arguments.
6. Move all subsequent edits, builds, tests, and commits into the assigned worktree.
7. Verify `git rev-parse --show-toplevel`, `git branch --show-current`, and `git status --short` before editing.
8. Perform the task and targeted verification only in the assigned worktree.
9. Report the worktree path, branch, changed files, and verification results.
10. Never remove the worktree automatically; always explicitly clean up your assigned task branch and worktree once they are no longer needed (e.g., after the work is merged, discarded, or safely handed off).
