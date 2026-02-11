# **Helm — Unified Package & Update Control Center for macOS**

## **Project Overview**

Helm is a native macOS menu bar utility that provides a centralized control center for discovering, installing, uninstalling, and updating third-party software across multiple package managers and runtime tools. It emphasizes safety, clarity, and correctness, enabling users to manage their software environment without shell fatigue or fragmented UIs.

---

## **Product Goals**

* One central “control plane” for third-party package and tool updates.
* High-speed status overview: installed, upgradable, and available items.
* One-click actions for refresh, install, uninstall, and “upgrade all”.
* Clear separation of authority between managers and tools.
* UX that feels responsive even during long background operations.

---

## **Initial Manager Scope**

### **1. Tool & Runtime Managers (Authoritative)**

These define toolchain versions and are considered authoritative over downstream package managers:

* **mise** (recommended)
* **asdf** (optional / compatibility mode)
* **rustup** (recommended)

**Authority rules:** These should always execute before downstream package updates.

---

### **2. System & OS Package Managers (Guarded)**

These run last and may require elevated privileges or reboots:

* **Homebrew (formulae)**
* **macOS `softwareupdate`**
* **MacPorts** (optional)
* **nix-darwin** (optional)

Guardrails:

* Explicit confirmation for privileged or system-level actions.
* Rate-limiting and scheduling options.

---

### **3. Language-Specific Package Managers**

Focus on global packages; project-local dependencies are excluded unless explicitly enabled.

* **pnpm** (global)
* **npm** (global)
* **yarn** (classic + berry)
* **pipx** (recommended)
* **pip** (`python3 -m pip`)
* **poetry**
* **RubyGems**
* **bundler**
* **Cargo**
* **cargo-binstall**

Managers must declare:

* install
* uninstall
* list
* outdated
* search
  capabilities.

---

### **4. App Stores & GUI Application Managers**

Primarily for status visibility:

* **Mac App Store (`mas`)**
* **Sparkle-based updaters** (detection only)
* **Setapp** (detection only)
* **Homebrew casks**

---

### **5. Containers, VM & Platform Managers**

Detection and upgrade prompting where applicable:

* **Docker Desktop**
* **podman**
* **colima**
* **Parallels Desktop** (detection only)

---

### **6. Security, Firmware & Vendor Tools**

System integrity and tooling:

* **Xcode Command Line Tools**
* **Rosetta 2** (Apple Silicon)
* **Firmware updates** (`softwareupdate --history`)

---

## **Platform & UX Choices**

* Menu bar macOS utility (`LSUIElement`, no Dock icon).
* SwiftUI frontend for native look & feel.
* Floating panel UI from the menu bar icon.
* Background execution for long-running tasks.
* Live task list with per-task status/progress.

---

## **Architecture**

### **Hybrid Model**

* **SwiftUI frontend:** UI rendering, immediate responses.
* **Background service (daemon / XPC boundary):** Privileged or long-lived operations.
* **Rust core:** Adapter modules, orchestration logic, persistence API.

The core boundary is **documented and versioned**. Rust core is UI-agnostic and safe.

---

## **Persistence & Logging**

* **SQLite** for caches, preferences, and state (schema versioning & migrations).
* **File logs** in Application Support.
* Structured, per-manager log entries for analysis.

---

## **Design Principles**

* Adapter / plugin-style modules per manager.
* Capability-driven behavior; not all managers support all actions.
* Authority & precedence explicitly defined.
* Offline-first search with progressive remote enrichment.
* Pin-aware orchestration.

---

## **Functional Requirements**

### **1) Manager Detection**

* Detect installation state for managers.
* Enable by default for installed managers.
* Allow user toggle on/off per manager.

---

### **2) Refresh Pipeline**

For every enabled manager:

* List installed packages.
* List outdated packages.
* List available / searchable packages when supported.

Available packages are cached opportunistically as they are discovered through:

* user search interactions,
* remote searches,
* on-demand queries.

---

### **3) Package Actions**

* Install package
* Uninstall package
* Upgrade package
* Upgrade all upgradable packages across managers, respecting authority and pinning.

Packages are updated individually rather than in a single bulk command where possible.

---

### **4) Search**

**Automatic, progressive, cancelable search behavior**:

* Local cache fuzzy search returns instantly.
* After a short idle debounce, remote searches spawn in background for managers that support remote search; results update cache and UI incrementally.
* Ongoing remote searches are cancellable when user resumes typing, with a *grace period* allowing near-complete tasks to finish and avoid thrashing.

Remote search enriches local cache rather than replacing it, improving responsiveness and relevance.

---

### **5) Package Pinning**

* **Native pinning** where supported (e.g., Homebrew `brew pin`, manager exact version installs).
* **Virtual pinning** fallback: Helm records pinned versions and enforces exclusion during upgrades.
* Pins are visible in UI; pinned packages are excluded from:

  * bulk upgrades
  * automatic update modes (unless overridden)

---

### **6) Settings**

Settings include:

* Auto-check toggle
* Frequency (daily/weekly/monthly)
* Time of day
* Auto-apply toggle (off by default)
* Check on launch toggle
* Fully automatic mode toggle with rate limits
* Quiet hours
* Major upgrade policies

---

### **7) Tasks**

* Background task queue with observable statuses.
* Task types: detection, install, uninstall, refresh, search, upgrade.
* Per-manager exclusivity locks; same manager tasks run serially.
* True process cancellation, not just UI dismissal.

---

### **8) Onboarding Wizard**

* Detect available managers
* Present safety policy defaults
* Choose auto-update preferences

---

## **Future Enhancements**

* Notification & history timeline
* Dependency / conflict resolution
* CLI companion tool
* Export/import configuration
* API for 3rd-party integrations
* Interactive upgrade previews

---

## **Quality Constraints**

* No shell injection vectors — structured process args only.
* Defensive output parsing.
* Clear per-manager error reporting.
* Thread-safe shared state.
* Reasonable timeouts / retries.
* Unit tests for adapters and parsers.
* Integration tests for orchestration and UI flows.

---

## **Implementation Phases**

1. Scaffold app + background service + Rust core API.
2. SQLite store & manager registry.
3. Core adapters: brew, npm, pip.
4. Task & orchestration pipeline (refresh/search).
5. SwiftUI dashboard, packages, tasks, settings.
6. Logging, tests, and hardening.

---

## **Deliverables**

* Working macOS app bundle.
* README with architecture and setup steps.
* Documented core UI ↔ service ↔ Rust interfaces.
* Clear limits and known shortcomings.

---

## **Notes**

* Managers run in parallel across categories; within one manager they run serially.
* Respect manager interdependencies (e.g., `mas` depends on Homebrew).
* Prefer per-package ops over bulk where feasible.
