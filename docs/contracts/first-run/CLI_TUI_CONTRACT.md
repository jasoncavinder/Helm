# CLI/TUI Interaction Contract

Status: proposed v0.19 interface; no command is implemented by this planning closure

The human-facing target namespace is `helm setup`. Existing `helm onboarding` behavior remains compatible until a reviewed implementation introduces an alias or migration. GUI, CLI, and TUI consume the same Environment Brief, Setup Session, Reviewed Plan, action, verification, and Action Receipt semantics.

## Commands

### `helm setup status`

- Reads the latest setup session and brief without starting mutation.
- Human output summarizes state, coverage, freshness, consent, and valid next actions.
- `--json` returns the versioned session/brief envelope.

### `helm setup scan [--offline]`

- Starts or resumes bounded local observation.
- `--offline` sets strict local-only behavior and marks network work Deferred.
- TTY progress goes to stderr; JSON/NDJSON stdout remains machine-only.

### `helm setup plan [--profile maintain|audit|new-mac|customize]`

- Generates a proposed plan from a brief revision and does not execute it.
- Interactive mode presents review and explicit inclusion choices.
- Machine mode returns the plan envelope and plan ID.

### `helm setup apply --plan <plan-id> --yes`

- Revalidates the plan before submission.
- `--yes` confirms only the reviewed plan. Network, mutation, and privilege must also be allowed by explicit flags or managed policy defined by the implementation; none can weaken core safety.
- Missing consent in noninteractive mode returns a deterministic machine error and performs no mutation.
- NDJSON events carry state transitions and conclude with receipt references.

### `helm setup resume [--session <session-id>]`

- Reconciles interrupted/unverified work and revalidates before any new submission.
- Never resumes mutation merely because the process relaunched.

### `helm setup cancel [--session <session-id>]`

- Requests real cancellation through orchestration.
- Emits `cancelling` until authoritative terminal state, prevents future dependent submission, then records `cancelled` or actual terminal outcome.
- Exit `130` is reserved for an invocation interrupted by SIGINT; command-requested cancellation uses the normal documented result envelope.

### `helm setup receipt <receipt-id> [--redacted]`

- Displays one Action Receipt.
- `--redacted` emits the strict Redacted Summary. Unredacted local fields require explicit supported inclusion flags if added later.

## Machine-Mode Rules

- Prompt only when stdin is a TTY.
- Preserve deterministic `--json`/`--ndjson`, `--quiet`, `--no-color`, cancellation, and exit codes.
- Keep progress and diagnostics off JSON/NDJSON stdout.
- Plain output is line-oriented when alternate-screen TUI behavior is unsuitable.
- Machine errors identify the required state/consent and a deterministic next command.
- No `--auto-approve` bypass exists.

Suggested exit classes for implementation review:

| Code | Meaning |
|---:|---|
| 0 | Requested observation/review/apply completed with declared successful or no-change result. |
| 1 | Usage, validation, prerequisite, policy, or fatal command/session error; no successful requested result. |
| 2 | One submitted action/task failed, including a sole failed-verification result. |
| 3 | Mixed/partial result, including verified work plus failed or failed-verification scope. |
| 4 | Command-requested or authoritative task/session cancellation. |
| 5 | Existing Helm onboarding completion is required. |
| 6 | Existing Helm license acceptance is required. |
| 130 | Process was terminated by SIGINT before Helm returned its normal result envelope; query authoritative session state. |

These classes preserve the existing task-oriented `0`-`6` CLI contract. v0.19 implementation must add setup cases without changing existing command meanings; normal setup cancellation returns `4`, not `130`.
