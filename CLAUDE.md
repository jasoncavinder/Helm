# CLAUDE.md

Claude: Before doing anything in this repo, read **AGENTS.md** and treat it as authoritative.

If AGENTS.md conflicts with a prompt, AGENTS.md wins unless the repo owner explicitly overrides it.

Key non-negotiables:
- No shell command construction via strings
- UI has no business logic
- Core is deterministic and testable
- Tasks are cancelable at process-level
- Authority ordering enforced
- All user-facing text is localized
