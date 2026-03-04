# Helm Security Review (Pre-1.0)

Date: 2026-02-25  
Branch audited: `chore/pre1-quality-audit`  
Scope: command execution, privileged operations, update integrity, networking, secrets/privacy, dependency/supply chain, unsafe file handling, logging/telemetry.

## Threat Model Lite

### Assets
- Local system integrity on macOS hosts (user data, package manager state, toolchains).
- Privileged execution path (`sudo -A`) and any launchd/login helper behavior.
- Update trust chain (Sparkle feed + EdDSA signatures, CLI manifest/checksum paths).
- Helm persistence data (SQLite, task output logs, settings, install markers).
- Release pipeline outputs (DMG/appcast/CLI metadata and checksums).

### Likely attackers
- Local unprivileged attacker on same host (PATH/env poisoning, symlink tricks, race attempts).
- Network/on-path attacker when endpoints or redirects are weakly constrained.
- CI/release process attacker through malformed inputs (tag/env/path traversal).
- Insider or compromised maintainer environment during release/publish.

### Entry points
- CLI invocation and environment (`PATH`, `HELM_*` variables).
- GUI/FFI/XPC boundary between app and background service.
- Update endpoints (CLI metadata, assets, Sparkle feed/appcast URLs).
- Adapter process output captured into logs/task output storage.
- Release scripts/workflows consuming environment variables.

### Trust boundaries
- SwiftUI app <-> Rust core/FFI <-> XPC service.
- User context <-> elevated `sudo` task context.
- Local machine <-> remote update/metadata endpoints.
- Trusted repository/workflow definitions <-> runtime-provided env/inputs.

## Findings

## Command Execution

### 1) Coordinator helper commands use PATH-resolved `ps`
- Severity: Medium
- Exploit scenario: A local attacker (or polluted runtime env) places a malicious `ps` earlier in `PATH`; Helm CLI executes attacker code when probing coordinator liveness/ownership.
- Affected code:
  - `core/rust/crates/helm-cli/src/main.rs`
  - Functions: `process_is_alive`, `coordinator_process_looks_owned` (uses `Command::new("ps")`)
- Recommended fix:
  - Use absolute path (`/bin/ps`) or sanitized minimal `PATH` for these helper invocations.
  - Add regression tests for command path hardening.

### 2) Unsanitized release tag input can escape artifact output root
- Severity: Medium
- Exploit scenario: A crafted `TAG_NAME` with traversal segments influences ZIP/PKG output paths in release helper script, enabling writes outside expected `build/variants` directories.
- Affected code:
  - `scripts/release/build_unsigned_variant.sh`
  - Variables/flow: `TAG_NAME` -> `ZIP_PATH`/`PKG_PATH`
- Recommended fix:
  - Validate `TAG_NAME` against strict release regex (`^v[0-9]+\.[0-9]+\.[0-9]+(-rc\.[0-9]+)?$`).
  - Canonicalize output paths and reject any path outside intended root.

## Privileged Operations

### 3) `HELM_SUDO_ASKPASS` override accepts arbitrary executable path
- Severity: Medium
- Exploit scenario: If Helm runs in an attacker-influenced environment, setting `HELM_SUDO_ASKPASS` to malicious script can hijack privileged flow and capture/administer sudo prompts during elevated tasks.
- Affected code:
  - `core/rust/crates/helm-core/src/execution/tokio_process.rs`
  - Functions: `prepare_sudo_askpass_script`, `prepare_command_for_spawn`
- Recommended fix:
  - Disallow arbitrary external askpass by default.
  - Only allow Helm-managed helper path, or allow override only behind explicit debug/test flag with strict ownership/permissions checks.

### 4) XPC client trust is team-ID based only
- Severity: Low
- Exploit scenario: If signing material is compromised, any binary signed with the same Team ID can connect to service APIs.
- Affected code:
  - `apps/macos-ui/HelmService/Sources/HelmServiceDelegate.swift`
  - Function: `isClientTrusted`
- Recommended fix:
  - Keep signing keys tightly controlled; optionally add designated requirement checks (bundle ID/anchor) or connection nonce/handshake.

### 5) CLI shim install path has residual symlink-safety edge case
- Severity: Low
- Exploit scenario: Pre-existing tampered managed shim content can influence overwrite behavior when install path handling allows symlinked targets.
- Affected code:
  - `apps/macos-ui/Helm/Core/HelmCore+Settings.swift`
  - Functions around `writeTextAtomically` usage for CLI shim/marker writes
- Recommended fix:
  - Enforce no-symlink target for shim writes and verify ownership/permissions on parent directories before overwrite.

## Update Integrity & Unsafe File Handling

### 6) Redirect destination host is not re-validated after HTTP 3xx
- Severity: Medium
- Exploit scenario: Initial URL passes allowlist, then redirects to non-allowlisted host; integrity checks still protect bytes via checksum, but host allowlist policy can be bypassed and network trust assumptions weakened.
- Affected code:
  - `core/rust/crates/helm-cli/src/main.rs`
  - Flow: `validate_update_url` + `fetch_cli_update_manifest`/`download_update_bytes` with `ureq` GET
- Recommended fix:
  - Disable automatic redirects and follow manually with per-hop `validate_update_url`, or validate final effective URL host post-redirect.

### 7) Binary replacement has TOCTOU window between metadata check and rename
- Severity: Low
- Exploit scenario: Local attacker races target path state after initial symlink/non-file check but before final rename.
- Affected code:
  - `core/rust/crates/helm-cli/src/main.rs`
  - Function: `apply_update_bytes`
- Recommended fix:
  - Re-check destination with `O_NOFOLLOW`-style guard just before final replacement; reject if inode/type changed.

## Networking

### 8) No explicit certificate pinning for update endpoints
- Severity: Low
- Exploit scenario: System trust store compromise or mis-issued cert could still satisfy standard TLS validation.
- Affected code:
  - `core/rust/crates/helm-cli/src/main.rs` (`ureq::AgentBuilder`)
  - `core/rust/crates/helm-ffi/src/lib.rs` (auto-check HTTP agent)
- Recommended fix:
  - Keep current HTTPS allowlists and signatures/checksums; evaluate optional pinning or TUF-style metadata later if operationally acceptable.

## Secrets, Logging, Telemetry Privacy

### 9) Raw stdout/stderr persisted without secret redaction
- Severity: Medium
- Exploit scenario: Package-manager commands can emit tokens/credentials into stdout/stderr; Helm stores and surfaces these in task outputs, inspector, and CLI diagnostics.
- Affected code:
  - `core/rust/crates/helm-core/src/execution/task_output_store.rs`
  - `core/rust/crates/helm-core/src/execution/tokio_process.rs`
  - `core/rust/crates/helm-ffi/src/lib.rs` (task output exposure)
- Recommended fix:
  - Apply redaction pipeline before persistence/display (token patterns, auth headers, known key formats).
  - Preserve raw output only in short-lived memory when needed for active task streaming.

## Dependency Risk & Supply Chain Controls

### 10) Dependency vulnerability scanning coverage is incomplete
- Severity: Low
- Exploit scenario: Known vulnerable Rust/web transitive deps may not be detected early without dedicated dependency-audit jobs.
- Affected configuration:
  - `.github/workflows/` (no `cargo-audit`, `cargo-deny`, or dependency-review workflow found)
  - Existing controls include `CodeQL` (Swift-focused), Semgrep, pinned release actions, lockfiles.
- Recommended fix:
  - Add lightweight scheduled + PR dependency checks (`cargo audit` or `cargo-deny`, plus GitHub dependency review where applicable).

## Must-Fix Before 1.0

1. Harden privileged askpass handling (`HELM_SUDO_ASKPASS`) to prevent arbitrary helper execution in elevated paths.
2. Eliminate PATH hijack risk in coordinator helper command invocations (`ps` absolute path/sanitized env).
3. Redact sensitive stdout/stderr before task-output persistence and exposure surfaces.
4. Enforce strict tag/path validation in release artifact helper script to block path traversal writes.
5. Re-validate update redirect destinations against allowlist on every hop/final URL.

## Existing Strengths

- Structured command invocation in core adapters (no broad shell-string execution model).
- CLI update path already enforces HTTPS allowlists, checksum verification, bounded download size, and atomic replacement workflow.
- Sparkle update flow includes channel policy checks, signed appcast artifacts, and direct-channel restrictions.
- Release workflows use pinned actions and explicit publication verification/drift guard jobs.
- Support export path already includes redaction controls for user paths/emails/tokens.

## Critical-Issue Patch Status

No clearly critical, straightforward issue was identified in this pass; therefore no code patch was applied in this review-only step.
