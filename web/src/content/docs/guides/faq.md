---
title: FAQ & Troubleshooting
description: Frequently asked questions and troubleshooting tips for Helm.
---

## General

### What is Helm?

Helm is a native macOS menu bar app that provides a unified control plane for package managers. Instead of switching between terminals to manage Homebrew, npm, pip, Cargo, and others, Helm lets you monitor, search, install, upgrade, and pin packages from one interface.

### What package managers does Helm support?

Helm supports fifteen managers across four categories:

| Category | Managers |
|---------|----------|
| **Toolchain / Runtime** | mise, rustup |
| **System / OS / App Store** | Homebrew, softwareupdate, mas |
| **Core Language** | npm (global), pipx, pip (global), Cargo, cargo-binstall |
| **Extended Language** | pnpm (global), yarn (global), Poetry (self/plugins), RubyGems, Bundler |

### Is Helm free?

Helm is currently in pre-1.0 beta with all features available. After version 1.0, a tiered licensing model (Free, Pro, Business) is planned. See the [licensing page](/licensing/) for details.

### What macOS versions are supported?

Helm requires macOS 12 (Monterey) or later and runs natively on both Apple Silicon and Intel Macs.

### Does Helm require administrator privileges?

No. Helm runs in user space. Some underlying package managers (like `softwareupdate`) may require privileges for certain operations, but Helm itself does not.

---

## Usage

### Why doesn't a manager show up?

Helm auto-detects managers on your system during refresh. If a manager doesn't appear:

1. Make sure the manager is installed and accessible from your shell (e.g., `brew --version` works in Terminal)
2. Click **Refresh** to re-run detection
3. Check the Managers tab in the Control Center — undetected managers show a gray "Not Installed" badge

### How does refresh ordering work?

Helm refreshes managers in a **3-phase authority model**:

1. **Authoritative** (mise, rustup) — toolchain managers that define the runtime; refreshed first
2. **Standard** (npm, pnpm, yarn, pip, pipx, cargo, cargo-binstall, RubyGems, Poetry, Bundler, mas) — language and app store managers
3. **Guarded** (Homebrew, softwareupdate) — system-level managers; refreshed last

Within each phase, managers refresh in parallel. A failure in one manager does not block others.

### What is Safe Mode?

Safe Mode prevents guarded managers (Homebrew and softwareupdate) from executing upgrades during "Upgrade All" operations. When enabled, OS-level updates require explicit confirmation. You can toggle Safe Mode in Settings.

### How does search work?

Search is progressive and non-blocking:

1. **Instant local filter** — installed and cached packages are filtered as you type
2. **Debounced remote search** — after 300ms of inactivity, Helm queries remote sources in the background
3. **Cache enrichment** — remote results are cached locally for faster future searches
4. **Cancellation** — typing again cancels any in-flight remote search

### What does pinning do?

Pinning prevents a package from being included in "Upgrade All" operations. Pinned packages still appear in the updates list but are skipped during bulk upgrades. For Homebrew, Helm uses native `brew pin/unpin`; for other managers, a virtual pin is tracked locally.

### Can I undo an upgrade?

Helm does not currently support rollback. If an upgrade causes issues, use the underlying package manager directly (e.g., `brew switch`, `pip install <package>==<version>`).

---

## Installation

### The app won't open — macOS says it's from an unidentified developer

Helm beta builds are signed and notarized with a Developer ID certificate, so Gatekeeper should allow them. If you still see a warning:

1. Right-click (or Control-click) the app in Finder
2. Select **Open** from the context menu
3. Click **Open** in the dialog

This only needs to be done once. After that, the app opens normally.

### How do I build from source?

See the [Installation guide](/guides/installation/) for prerequisites and build steps. You'll need macOS 12+, Xcode 14+, and a Rust stable toolchain.

---

## Troubleshooting

### A manager is detected but shows no packages

This can happen when:

- The manager is installed but has no packages (e.g., a fresh `pipx` install)
- The manager's data directory is in a non-standard location
- The shell environment differs between Terminal and the XPC service context

Try running the manager's list command directly in Terminal (e.g., `npm list -g --depth=0`) to verify it works.

### Refresh seems stuck

If the refresh indicator spins indefinitely:

1. Check if a specific manager is unresponsive — the Tasks tab shows per-manager task status
2. Helm enforces timeouts (30s for data fetches, 300s for mutations), so stuck tasks will eventually time out
3. Try quitting and reopening Helm — the XPC service reconnects automatically with exponential backoff

### Tasks show "Failed" status

Task failures usually indicate an issue with the underlying package manager. Check the task details in the inspector sidebar for error context. Common causes:

- Network connectivity issues (for remote searches or installs)
- Permission errors (especially for system-level managers)
- Package name conflicts or invalid package identifiers

### How do I report a bug?

You can report bugs in several ways:

1. **In-app:** Open Control Center > Settings > Support & Feedback > Report a Bug
2. **GitHub:** File an issue at [github.com/jasoncavinder/Helm/issues](https://github.com/jasoncavinder/Helm/issues/new?template=bug_report.yml)
3. **Email:** Use the Send Feedback button in Settings

When reporting, enable the "Include Diagnostics" toggle to copy system info to your clipboard — then paste it into the issue.

### How do I copy diagnostics?

Open Control Center > Settings > Support & Feedback > Copy Diagnostics. This copies your Helm version, macOS version, architecture, locale, detected managers, and recent task status to the clipboard.
