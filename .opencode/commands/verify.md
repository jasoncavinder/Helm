---
description: Verify Helm changes without fixing or modifying them
agent: verifier
subtask: true
---

Verify the following scope:

$ARGUMENTS

If no scope is supplied, inspect the current working-tree changes and run the
applicable non-release verification.

Do not modify files. Do not run signing, notarization, packaging, publishing,
deployment, or release operations.
