# Helm Architecture

This document defines the architectural model of Helm.

It is the canonical reference for how the system is structured, how components interact, and what guarantees the system provides.

This document should be considered **stable** approaching 1.0 and only changed deliberately.

---

## 1. Architectural Goals

Helm is designed as a **local control plane** for system and package management.

Primary goals:

- Safety-first execution
- Deterministic orchestration
- Clear authority boundaries
- UI responsiveness under long-running tasks
- Complete transparency of operations
- Extensibility via adapters

Minimum platform baseline:

- macOS 11+ (Big Sur)
- Rationale: aligns with modern platform security primitives required for planned App Attest-based install authentication in the Shared Brain stage (`1.4.x`)

Helm is not a shell wrapper. It is a **structured execution engine**.

---

## 2. High-Level Architecture

Helm uses a **three-layer architecture**:

```

┌─────────────────────────────┐
│        SwiftUI UI           │
│ (apps/macos-ui/)           │
└────────────┬────────────────┘
│ XPC (typed API)
┌────────────▼────────────────┐
│        XPC Service          │
│ (unsandboxed execution)     │
└────────────┬────────────────┘
│ FFI (C ABI)
┌────────────▼────────────────┐
│         Rust Core           │
│ (core/rust/)               │
└─────────────────────────────┘

```

---

## 3. Layer Responsibilities

### 3.1 UI Layer (SwiftUI)

Location:
```

apps/macos-ui/

```

Responsibilities:

- Render application state
- Dispatch user intents
- Display tasks, logs, and status via menu bar popover and control center window
- Handle user interaction (search, install, upgrade)
- Inspector sidebar for task/package/manager detail
- Guided onboarding walkthrough (SpotlightOverlay system)
- Perform no business logic

Constraints:

- Must not perform system execution
- Must not parse command output
- Must remain responsive at all times

---

### 3.2 Service Layer (XPC)

Location:
```

apps/macos-ui/HelmService/

```

Responsibilities:

- Bridge SwiftUI to Rust core
- Host Rust FFI in an unsandboxed process
- Execute system commands via Rust core
- Enforce process isolation

Key properties:

- Separate process from UI
- Resilient to crashes (auto-reconnect)
- Validates calling client (code signing)

---

### 3.3 Core Layer (Rust)

Location:
```

core/rust/

```

Responsibilities:

- All business logic
- Adapter system
- Orchestration engine
- Persistence (SQLite)
- Task management
- Policy enforcement

This layer is:

- UI-agnostic
- Fully testable
- Deterministic

---

## 4. Core Subsystems

### 4.1 Adapter System

Each package manager is implemented as an adapter.

Helm currently implements 28 adapters:

- **Toolchain / Runtime:** mise, asdf (optional), rustup
- **System / OS:** Homebrew (formulae), softwareupdate, MacPorts (optional), nix-darwin (optional)
- **Language:** npm, pnpm, yarn, pip, pipx, cargo, cargo-binstall, RubyGems, Poetry, Bundler
- **App / GUI:** mas, Homebrew casks, Sparkle (detection-only), Setapp (detection-only)
- **Container / VM:** Docker Desktop, podman, colima, Parallels Desktop (detection-only)
- **Security / Firmware:** Xcode Command Line Tools, Rosetta 2, Firmware Updates

Adapters declare capabilities:

- list_installed
- list_outdated
- search
- install
- uninstall
- upgrade
- pin / unpin

Adapters must:

- Use structured process invocation (no shell)
- Return structured results
- Handle parsing defensively

---

### 4.2 Capability Model

Not all managers support all operations.

Helm uses a **capability-driven model**:

```

if adapter.supports(Upgrade):
allow upgrade
else:
disable UI action

```

This avoids false assumptions about manager behavior.

---

### 4.3 Authority Model

Managers are grouped by authority level:

1. **Authoritative**
   - mise, asdf, rustup

2. **Standard**
   - npm, pnpm, yarn, pip, pipx, cargo, cargo-binstall, RubyGems, Poetry, Bundler, mas
   - homebrew_cask, docker_desktop, podman, colima
   - sparkle (detection-only), setapp (detection-only), parallels_desktop (detection-only)

3. **Guarded**
   - Homebrew, softwareupdate, macports, nix_darwin
   - xcode_command_line_tools, rosetta2, firmware_updates

Execution order is enforced:

```

Authoritative → Standard → Guarded

```

Guarantees:

- Toolchains updated before dependent packages
- System updates run last
- Failures do not cascade silently

---

### 4.4 Orchestration Engine

Core of Helm.

Responsibilities:

- Task queue
- Execution scheduling
- Cancellation
- Failure isolation

Properties:

- Cross-manager parallelism
- Per-manager serialization
- True process cancellation
- Deterministic execution ordering

---

### 4.5 Task System

All operations are tasks:

- refresh
- install
- uninstall
- upgrade
- search

Each task:

- Has unique ID
- Has lifecycle (Queued → Running → Completed/Failed)
- Emits logs
- Can be canceled

Tasks persist across restarts.

---

### 4.6 Persistence (SQLite)

Helm uses SQLite for:

- Installed packages cache
- Outdated packages
- Search cache
- Pin state
- Task history
- App settings

Requirements:

- Schema versioned
- Migrations tested
- Corruption-safe behavior

---

### 4.7 Pinning System

Two modes:

1. Native pinning (manager-supported)
2. Virtual pinning (Helm-enforced)

Guarantees:

- Pinned packages are excluded from upgrade-all
- Pin state is durable
- UI reflects pin state consistently

---

### 4.8 Safe Mode & Policy

Safe mode enforces execution constraints:

- Blocks OS updates
- Requires explicit confirmation
- Prevents unsafe operations

Policy layer ensures:

- Guardrails for system-level actions
- Explicit user intent required

---

### 4.9 Localization System

All UI text is key-based.

Rules:

- No hardcoded strings
- ICU message format required
- Fallback to English

Localization is handled at UI layer.

Service returns keys, not strings.

See:
```

docs/I18N_STRATEGY.md

```

---

## 5. Execution Model

### 5.1 Process Execution

All commands:

- Use structured arguments
- No shell invocation
- Timeouts enforced
- Output captured

---

### 5.2 Cancellation

Cancellation is:

- Process-level (SIGTERM / kill)
- Not UI-only
- Guaranteed by orchestration layer

---

### 5.3 Failure Isolation

Failures:

- Are localized to a single manager
- Do not cascade
- Are surfaced clearly in UI

---

## 6. Security Model

### 6.1 Core Execution Security

Helm enforces:

- No shell injection vectors
- Explicit command arguments
- XPC boundary validation
- Code signing checks
- Guardrails for privileged operations

### 6.2 Staged Security Rollout

Security capabilities are staged and intentionally separated:

Stage 0 (`<=0.16.x`):
- Documentation and planning only
- No implemented security advisory logic

Stage 1 (`0.18.x`):
- Internal local-only groundwork for vulnerability data handling
- No public feature exposure
- No Pro gating
- No centralized backend

Stage 2 (`1.3.x`) - Security Advisory System (Helm Pro):
- Local-first CVE/advisory scanning and recommendation engine
- Optional queries to public advisory APIs (for example OSV / GitHub Advisory Database)
- Local SQLite cache with TTL-based refresh
- No Helm-operated central database required
- No fingerprint sharing
- No App Attest

Stage 3 (`1.4.x`) - Shared Brain:
- Centralized fingerprint and known-fix lookup services
- Managed Postgres backend with edge/serverless API entry points
- Anonymous per-install auth via Apple App Attest
- Signed requests, nonce/replay protection, per-install rate limiting, and abuse controls

### 6.3 System Boundary

- The Security Advisory System (`1.3.x`) is independent of Shared Brain and remains functional without Helm-hosted services.
- Shared Brain (`1.4.x`) is additive infrastructure that can enrich advisory outcomes but is not a prerequisite for local advisory evaluation.

---

## 7. Data Flow

### Example: Upgrade Package

```

UI → XPC → FFI → Core
↓
Task created
↓
Adapter executes command
↓
Output parsed
↓
Post-upgrade validation (re-check list_outdated)
↓
SQLite updated
↓
UI refreshed (inspector shows task detail)

```

---

## 8. Concurrency Model

- Managers run in parallel
- Tasks within same manager are serialized
- Authority phases executed sequentially

---

## 9. Extensibility

Helm is designed to support:

- New adapters
- New capabilities
- Enterprise policy layer
- CLI interface (future)

Adapters are the primary extension point.

---

## 10. Future Architecture (Post-1.0)

Planned extensions:

- Enterprise control plane (policy + compliance)
- Remote management
- Plugin ecosystem
- CLI companion

These must not violate:

- Local-first execution
- Safety guarantees
- Deterministic behavior

### 10.1 Planned Distribution and Licensing Architecture (Future State)

This is architectural direction only. It is not implemented yet.

#### Build Matrix

| Artifact | Product | Channel | Licensing Authority | Update Authority | Sparkle |
|---|---|---|---|---|---|
| Helm (MAS build) | Helm (Consumer) | Mac App Store | App Store commerce/receipt model | Mac App Store | No |
| Helm (Developer ID build) | Helm (Consumer) | Direct DMG, Homebrew, MacPorts | Helm consumer entitlement model | Sparkle (direct channel only) | Yes |
| Helm (Setapp build) | Helm (Consumer) | Setapp | Setapp subscription/license model | Setapp | No |
| Helm Business (Fleet build) | Helm Business | Enterprise PKG deployment | Offline organizational license files | Admin-controlled PKG/MDM rollout | No |

Channel-to-licensing-to-update mapping must remain explicit:

- Update system and licensing system are decoupled.
- Homebrew and MacPorts distribute the same Developer ID consumer binary.
- Helm Business release lifecycle is separate from consumer channel lifecycle.
- Sparkle is excluded from MAS, Setapp, and Helm Business fleet builds.

#### Build Configuration and CI/CD Implications (High-Level)

- Shared core codebase remains single-source.
- Build configurations must differentiate MAS, Developer ID, Setapp, and Fleet packaging paths.
- CI/CD must keep shared test gates common, then run channel-specific packaging/signing/notarization/publishing steps.
- Business PKG release flow is managed separately from consumer DMG/App Store/Setapp release flow.

### 10.2 Implementation Phasing Strategy

1. Document and freeze channel boundaries, licensing authority, and update authority.
2. Add channel-aware build configurations without changing core architecture boundaries.
3. Deliver Sparkle only for the direct Developer ID consumer channel.
4. Deliver MAS and Setapp channels as separate distribution tracks with channel-owned update/licensing behavior.
5. Deliver Helm Business as a separate fleet binary with PKG + MDM deployment workflows.
6. Add offline organizational license-file handling for Helm Business and keep admin-controlled update workflows independent.

---

## 11. Architectural Invariants

These must never be violated:

- No shell execution
- UI contains no business logic
- Core is deterministic and testable
- Authority ordering is respected
- Tasks are cancelable
- All user-facing text is localized

---

## 12. Relation to Other Docs

- Product definition:
  - docs/PROJECT_BRIEF.md
- Roadmap:
  - docs/ROADMAP.md
- Definition of done:
  - docs/DEFINITION_OF_DONE.md
- i18n:
  - docs/I18N_STRATEGY.md

---

## 13. Summary

Helm is a **local control plane for package management** built on:

- Rust core (logic)
- XPC service (execution boundary)
- SwiftUI UI (presentation)

It prioritizes:

- Safety
- Transparency
- Determinism
- Extensibility

This architecture is the foundation for Helm 1.0.
