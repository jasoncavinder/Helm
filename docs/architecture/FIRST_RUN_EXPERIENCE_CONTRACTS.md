# First-Run Experience Contracts

This document establishes the normative architecture, semantics, and boundaries for Helm's Project WOW first-run setup experience. It applies equally across GUI, CLI, and TUI presentation surfaces.

## 1. Discovery Stages and Consent Boundary

The first-run discovery process is split into stages based on user privacy and consent.

### 1.1 Local Observation (No Prompt)
Helm may perform local, non-mutating observations to construct an `Environment Brief` without prompting the user. This includes:
- Checking OS version and architecture (`uname`, `sw_vers`).
- Checking for the existence of known package manager binaries in standard `PATH` locations (e.g., `/opt/homebrew/bin/brew`, `~/.local/share/mise/bin/mise`).
- Checking local `sudo` timeout/passwordless status without triggering a prompt.
- Reading local configuration files strictly associated with supported package managers.

### 1.2 Network Disclosure
Any action that requires network access (e.g., fetching upstream catalogs, updating package indices, checking for Helm updates) MUST be explicitly disclosed.
- The `Environment Brief` indicates whether the system is online.
- Network operations are gated by the `consent.networkAllowed` flag in the `Setup Session`.

### 1.3 Explicit Confirmation (Mutation & Privilege)
All mutating actions and any action requiring privilege escalation (e.g., `sudo`) require explicit user confirmation.
- Users must review and approve a `Recommendation / Reviewed Plan` before any changes are applied.
- The `consent.mutationAllowed` and `consent.privilegeAllowed` flags govern execution limits.

## 2. Setup Session State Machine

A `Setup Session` tracks the user's progress through first-run onboarding.

### 2.1 States
- **Queued**: Session initialized, waiting for environment observation.
- **Running**: Active evaluation, planning, or execution in progress.
- **Completed**: Plan successfully verified and executed.
- **Failed**: Execution halted due to an unrecoverable error.
- **Cancelled**: Explicitly aborted by the user.
- **Deferred**: User opted to delay setup until a later time.

### 2.2 Idempotency and Retry Behavior
- All actions proposed in a `Plan` MUST be strictly idempotent. 
- A retry of a failed or interrupted action must safely resume or do nothing if already satisfied.
- The planner uses `preVerificationStatus` to skip actions that are already satisfied.

### 2.3 Interruption and Resume
- If the application crashes, the terminal is closed, or the user quits, the `Setup Session` persists to the local SQLite store.
- On relaunch, Helm resumes from the last known state. Unfinished plans trigger revalidation.

## 3. Plan Invalidation and Revalidation

A generated plan is ephemeral and dependent on the `Environment Brief`.
- If the system state changes out-of-band (e.g., the user installs Homebrew manually while the prompt is open), the plan becomes `invalidated`.
- Helm MUST perform a revalidation check before transitioning a plan to `approved` and executing it.

## 4. Coverage and Partial Execution

- **Partial Manager Coverage**: Users can explicitly exclude discovered managers (e.g., "Manage Homebrew, but ignore MacPorts"). These are tracked in `partialManagerCoverage`.
- **Failures**: If one independent action fails, Helm attempts to continue with unrelated actions unless the failure breaks a dependency chain (Failure Taxonomy: `isolated` vs `cascading`).

## 5. Verification Rules

Actions are governed by a strict Before/After contract:
1. **Pre-Verification**: Check if the action is already satisfied.
2. **Execution**: Perform the typed action.
3. **Post-Verification**: Validate the result. If post-verification fails, the action status is `failed` and recovery limits are evaluated.

## 6. Recovery vs Rollback

- **Recovery**: Forward-moving correction (e.g., deleting a corrupted cache to allow a clean retry).
- **Rollback**: Returning to the exact pre-execution state.
- *Distinction*: True rollback is often impossible for third-party package managers. Helm favors idempotent recovery. If an action is `rollbackEligible`, Helm guarantees state restoration; otherwise, it logs the `recoveryLimits`.

## 7. Receipts, Retention, and Redaction

- Every completed or failed action yields an immutable `Action Receipt`.
- **Retention**: Receipts are stored locally in SQLite indefinitely unless explicitly cleared.
- **Redaction**: When users share logs (e.g., `Redacted Summary`), Helm applies strict redaction, stripping:
  - Absolute paths (e.g., `/Users/jason/...`)
  - Usernames and hostnames
  - Hardware serials
  - Any PII not strictly required for telemetry metrics.

## 8. Offline Behavior
- Governed by `offlineBehavior` in the `Setup Session` (`strict`, `opportunistic`, `abort`).
- If network access drops during a network-required action, the action fails cleanly. 
- Local evaluation and caching mechanisms remain fully functional offline.

## 9. GUI / CLI / TUI Semantic Parity
- No business logic or state mutation is permitted in the presentation layer.
- The CLI (`helm first-run`) and GUI trigger the exact same XPC/FFI endpoints, resulting in identical JSON envelopes and state transitions.

## 10. Localization Boundary
- Machine-readable JSON contracts use strictly English, enum-based keys.
- User-facing text is driven by `localizationKey` and `localizationArgs`. The presentation layer resolves these against `locales/` (e.g., `receipt.homebrew.update_tap.success`).
