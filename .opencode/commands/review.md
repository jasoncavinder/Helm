---
description: Review current Helm changes without modifying files
agent: reviewer
subtask: true
---

Perform a read-only review of the current Helm repository changes.

Requested scope:

$ARGUMENTS

If no scope was supplied, review all staged and unstaged changes and identify
relevant untracked source or test files from `git status --short`.

Begin by inspecting:

- `git status --short`
- `git diff --stat`
- `git diff`
- `git diff --cached`

Then read enough surrounding source, tests, contracts, and documentation to
validate the changes in context.

Do not modify files. Do not commit anything. Return the structured review
specified by the reviewer agent.
