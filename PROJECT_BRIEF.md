# **Helm — Unified Package & Update Control Center for macOS**

## **Project Overview**

Helm is a native macOS menu bar utility that provides a centralized control center for discovering, installing, uninstalling, and updating third-party software across multiple package managers and runtime tools. It emphasizes safety, clarity, and correctness, enabling users to manage their software environment without shell fatigue or fragmented UIs.

---

## **Product Goals**

- One central “control plane” for third-party package and tool updates.
- High-speed status overview: installed, upgradable, and available items.
- One-click actions for refresh, install, uninstall, and “upgrade all”.
- Clear separation of authority between managers and tools.
- UX that feels responsive even during long background operations.

---

## **Product Variations (1.0 and Beyond)**

Helm ships as two build artifacts with runtime-gated editions:

- **Debug build** (development and internal validation only)
- **Release build** (production signed artifact)
- **Pro edition** (pay-gated runtime entitlement within release build)
- **Business edition** (subscription-gated runtime entitlement within release build)

Business edition extends Helm with centrally managed policy and compliance controls so package and toolchain environments can be kept consistent across enterprise scopes.

---

## **Initial Manager Scope**

### **1. Tool & Runtime Managers (Authoritative)**

These define toolchain versions and are considered authoritative over downstream package managers:

- **mise** (recommended)
- **asdf** (optional / compatibility mode)
- **rustup** (recommended)

**Authority rules:** These should always execute before downstream package updates.

For 1.0:
- mise and rustup are required implementations.
- Toolchain managers execute before downstream package managers in bulk upgrade flows.
- Toolchain upgrade failures do not cascade silently.

---

### **2. System & OS Package Managers (Guarded)**

These run last and may require elevated privileges or reboots:

- **Homebrew (formulae)**
- **macOS `softwareupdate`**
- **MacPorts** (optional)
- **nix-darwin** (optional)

Guardrails:

- Explicit confirmation for privileged or system-level actions.
- Rate-limiting and scheduling options.

For 1.0:
- Homebrew and macOS softwareupdate are required.
- OS updates require explicit confirmation.
- Reboot-required state must be surfaced clearly in the UI.
- Silent OS updates are prohibited.

---

### **3. Language-Specific Package Managers**

Focus on global packages; project-local dependencies are excluded unless explicitly enabled.

- **pnpm** (global)
- **npm** (global)
- **yarn** (classic + berry)
- **pipx** (recommended)
- **pip** (`python3 -m pip`)
- **poetry**
- **RubyGems**
- **bundler**
- **Cargo**
- **cargo-binstall**

Managers must declare:

- install
- uninstall
- list
- outdated
- search
  capabilities.

---

### **4. App Stores & GUI Application Managers**

Primarily for status visibility:

- **Mac App Store (`mas`)**
- **Sparkle-based updaters** (detection only)
- **Setapp** (detection only)
- **Homebrew casks**

For 1.0, mas is required.

---

### **5. Containers, VM & Platform Managers**

Detection and upgrade prompting where applicable:

- **Docker Desktop**
- **podman**
- **colima**
- **Parallels Desktop** (detection only)

---

### **6. Security, Firmware & Vendor Tools**

System integrity and tooling:

- **Xcode Command Line Tools**
- **Rosetta 2** (Apple Silicon)
- **Firmware updates** (`softwareupdate --history`)

---

## **Platform & UX Choices**

- Menu bar macOS utility (`LSUIElement`, no Dock icon).
- SwiftUI frontend for native look & feel.
- Floating panel UI from the menu bar icon.
- Background execution for long-running tasks.
- Live task list with per-task status/progress.

---

## **Architecture**

### **Hybrid Model**

- **SwiftUI frontend:** UI rendering, immediate responses.
- **Background service (daemon / XPC boundary):** Privileged or long-lived operations.
- **Rust core:** Adapter modules, orchestration logic, persistence API.

The core boundary is **documented and versioned**. Rust core is UI-agnostic and safe.

---

## **Persistence & Logging**

- **SQLite** for caches, preferences, and state (schema versioning & migrations).
- **File logs** in Application Support.
- Structured, per-manager log entries for analysis.

---

## **Design Principles**

- Adapter / plugin-style modules per manager.
- Capability-driven behavior; not all managers support all actions.
- Authority & precedence explicitly defined.
- Offline-first search with progressive remote enrichment.
- Pin-aware orchestration.

---

## **Functional Requirements**

### **1) Manager Detection**

- Detect installation state for managers.
- Enable by default for installed managers.
- Allow user toggle on/off per manager.

---

### **2) Refresh Pipeline**

For every enabled manager:

- List installed packages.
- List outdated packages.
- List available / searchable packages when supported.

Available packages are cached opportunistically as they are discovered through:

- user search interactions,
- remote searches,
- on-demand queries.

---

### **3) Package Actions**

- Install package
- Uninstall package
- Upgrade package
- Upgrade all upgradable packages across managers, respecting authority and pinning.

Packages are updated individually rather than in a single bulk command where possible.

---

### **4) Search**

**Automatic, progressive, cancelable search behavior**:

- Local cache fuzzy search returns instantly.
- After a short idle debounce, remote searches spawn in background for managers that support remote search; results update cache and UI incrementally.
- Ongoing remote searches are cancellable when user resumes typing, with a *grace period* allowing near-complete tasks to finish and avoid thrashing.

Remote search enriches local cache rather than replacing it, improving responsiveness and relevance.

---

### **5) Package Pinning**

- **Native pinning** where supported (e.g., Homebrew `brew pin`, manager exact version installs).
- **Virtual pinning** fallback: Helm records pinned versions and enforces exclusion during upgrades.
- Pins are visible in UI; pinned packages are excluded from:

  - bulk upgrades
  - automatic update modes (unless overridden)

---

### **6) Settings**

Settings include:

- Auto-check toggle
- Frequency (daily/weekly/monthly)
- Time of day
- Auto-apply toggle (off by default)
- Check on launch toggle
- Fully automatic mode toggle with rate limits
- Quiet hours
- Major upgrade policies

---

### **7) Tasks**

- Background task queue with observable statuses.
- Task types: detection, install, uninstall, refresh, search, upgrade.
- Per-manager exclusivity locks; same manager tasks run serially.
- True process cancellation, not just UI dismissal.

---

### **8) Onboarding Wizard**

- Detect available managers
- Present safety policy defaults
- Choose auto-update preferences

---

### 9) Upgrade Preview & Dry-Run

Before executing bulk upgrades, Helm must provide:

- Ordered execution plan
- Manager grouping
- Pin exclusions
- Potential reboot warnings
- Estimated impact

Users may cancel before execution begins.

Dry-run mode must be supported for CLI and UI.

---

### 10) Helm Self-Update

Helm must support self-updating via a signed update mechanism.

Requirements:
- Code-signed updates
- Version integrity verification
- Delta updates preferred
- No shell-based update mechanisms
- Manual approval required (auto-update optional)

Self-update must not depend on Homebrew.

---

### 11) Diagnostics & Transparency

Helm must provide:

- Per-task structured logs
- Manager detection diagnostics
- Service health visibility
- Copyable logs for support

Control planes must be transparent.

---

### 12) Enterprise Managed Mode (Post-1.0 Expansion)

Business edition adds centrally managed operation without collapsing Helm's core architecture:

- MDM deploys Helm and provides bootstrap managed configuration.
- Helm consumes scoped policies from a central control plane.
- Rust core evaluates policy before mutating actions (install, uninstall, upgrade, pin changes).
- Drift and compliance states are computed locally and reported with clear attribution.
- Offline behavior uses last known valid policy snapshot and degrades safely when policy is unavailable.

This scope is planned for 1.x and is not a 1.0 release gate.

---

## **Future Enhancements**

- Notification & history timeline
- Dependency / conflict resolution
- CLI companion tool
- Export/import configuration
- API for 3rd-party integrations
- Interactive upgrade previews
- Enterprise central policy and rollout management (Business edition)

---

## **Quality Constraints**

- No shell injection vectors — structured process args only.
- Defensive output parsing.
- Clear per-manager error reporting.
- Thread-safe shared state.
- Reasonable timeouts / retries.
- Unit tests for adapters and parsers.
- Integration tests for orchestration and UI flows.

---

## **Implementation Phases**

1. `0.1.x-0.3.x` foundation (completed): Rust core contracts, SQLite persistence, orchestration queue, cancellation semantics.
2. `0.4.x-0.5.x` shell + search (completed): SwiftUI menu bar shell, XPC/FFI bridge, progressive local-first search with remote enrichment.
3. `0.6.x-0.7.x` manager expansion (completed): mise/rustup/softwareupdate/mas adapters, authority ordering, restart-required surfacing, manager controls.
4. `0.8.x` pinning and policy enforcement (next): native/virtual pin model, pin-aware upgrade-all, safe mode guardrails.
5. `0.9.x-0.10.x` package manager coverage: core and extended language manager adapters with capability-complete operations.
6. `0.11.x-0.15.x` hardening and operator workflows: reliability, automation rules, cross-machine state, and scale.
7. `1.0.0` stabilization: release criteria closure, documentation lock, and production readiness sign-off.

---

## **Deliverables**

- Working macOS app bundle.
- README with architecture and setup steps.
- Documented core UI ↔ service ↔ Rust interfaces.
- Clear limits and known shortcomings.

---

## Licensing Strategy (Pre-1.0)

Helm is currently distributed under a source-available, non-commercial license.

Goals:

- Allow evaluation and feedback during development
- Prevent unauthorized commercial use before 1.0
- Prevent reuse of Helm source code in other projects
- Preserve flexibility for future licensing models (proprietary, open core, or open source)

All contributions are subject to a Contributor License Agreement (CLA), allowing relicensing in future versions.

---

## **Notes**

- Managers run in parallel across categories; within one manager they run serially.
- Respect manager interdependencies (e.g., `mas` depends on Homebrew).
- Prefer per-package ops over bulk where feasible.
