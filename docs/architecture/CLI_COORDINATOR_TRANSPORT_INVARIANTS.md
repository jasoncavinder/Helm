# CLI Coordinator Transport Invariants

This document records the invariants for Helm CLI coordinator transport behavior.

## Boundary Ownership

- `core/rust/crates/helm-cli/src/main.rs` owns command orchestration and coordinator workflow decisions.
- `core/rust/crates/helm-cli/src/coordinator_transport.rs` owns transport-level helpers (state-dir parsing, file IPC paths, polling intervals, process ownership checks, stale-lock and launch-on-demand policy helpers).
- Command handlers should call transport helpers instead of re-implementing transport logic inline.

## Fallback Policy

- Default CLI submit request/response transport is local in-process for `--wait` execution mode.
- macOS coordinator bridge policy is local/XPC-preferred by default.
- Legacy external file-IPC compatibility remains opt-in and must be explicitly enabled (`HELM_LEGACY_FILE_COORDINATOR_IPC`).
- CLI `--detach` and cancellation paths continue to use external coordinator file-IPC transport for cross-process lifecycle control.
- Timeout errors must not trigger blind launch-on-demand resets.

## State and Ownership Safety

- Coordinator state, request, and response directories are private (`0700` on Unix).
- Coordinator request/response/ready files are private (`0600` on Unix).
- Process liveness/ownership checks use fixed absolute binaries (`/bin/ps`) to avoid PATH-based command spoofing.
- Coordinator termination/reset only applies to coordinator processes that match expected ownership markers.

## Bootstrap and Recovery

- Bootstrap lock acquisition is serialized with stale-lock cleanup.
- Stale state detection and recovery is bounded and deterministic.
- Request-response waiting remains bounded by configured timeout policy.

## Test References

- CLI coordinator transport tests: `core/rust/crates/helm-cli/src/main.rs` (`coordinator_*` tests).
- CLI transport helper tests: `core/rust/crates/helm-cli/src/coordinator_transport.rs`.
- FFI coordinator mode/permission tests: `core/rust/crates/helm-ffi/src/lib.rs`.
