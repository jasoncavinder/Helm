# CLI/TUI Interaction Contract

This document specifies the proposed `helm first-run` command behavior and machine-output semantics, ensuring parity with the GUI.

## 1. Commands

### `helm first-run status` (or `brief`)
- **Behavior**: Executes local observation stages.
- **Output**: JSON representation of the `Environment Brief`.
- **Exit Code**: `0` on success, `1` on fatal evaluation error.

### `helm first-run plan`
- **Behavior**: Generates a recommendation plan based on the brief. Does NOT execute it.
- **Output**: JSON representation of the `Recommendation / Reviewed Plan`.
- **Interactive**: TUI presents a preview list.
- **Non-Interactive**: Outputs JSON envelope.

### `helm first-run apply [--plan <plan-id>]`
- **Behavior**: Executes the approved plan.
- **Interactive**: Prompts for required consent (network, mutation, privilege) if not already granted. Displays progress via standard TUI spinners.
- **Non-Interactive**: Fails with exit code `2` (Consent Required) unless `--auto-approve` or strict policy is provided.
- **Output**: Stream of NDJSON events, culminating in `Action Receipt` summaries.
- **Exit Code**: `0` (Success), `3` (Partial Failure), `1` (Fatal Failure).

### `helm first-run resume`
- **Behavior**: Resumes the last pending or interrupted `Setup Session`.
- **Output**: Revalidates the plan and continues `apply`.

### `helm first-run cancel`
- **Behavior**: Safely interrupts an ongoing session.
- **Output**: Emits a `cancelled` state event. Next action is cleanly aborted.
- **Exit Code**: `130` (SIGINT equivalent).

### `helm first-run receipt <receipt-id>`
- **Behavior**: Displays an `Action Receipt`.
- **Flags**: `--redact` outputs a `Redacted Summary`.

### `helm first-run reset`
- **Behavior**: Clears local onboarding state (for testing/development).

## 2. Envelopes and Accessibility

- **Output Format**: All machine output uses standard JSON. Streamed execution updates use NDJSON (Newline Delimited JSON) to allow piping to `jq` or external tools.
- **Accessibility / Plain-Terminal**: If `NO_COLOR=1` or a dumb terminal is detected, the TUI gracefully degrades to linear plaintext logs instead of interactive spinners.

## 3. Timeout and Cancellation
- Network requests have a strict timeout (e.g., 30s for catalog fetch).
- Cancellation (`Ctrl-C`) is trapped. Helm completes the currently running atomic action (if unsafe to interrupt) and then cleanly halts the session, preserving state for a future `resume`.
