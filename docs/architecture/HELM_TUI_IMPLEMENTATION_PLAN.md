# Helm TUI Implementation Plan

Status: Draft 0.1  
Last Updated: 2026-02-23  
Owner: Helm Core

---

## 1. Purpose

Define an implementation-ready plan for the Helm terminal UI (TUI), launched when users run `helm` with no arguments in a TTY.

This plan targets:

- modern, keyboard-first UX
- low-friction operation for daily terminal workflows
- functional parity with current GUI capabilities (non-onboarding surfaces)
- strict adherence to Helm architectural and branding constraints

Implementation status snapshot (2026-02-23):

- delivered:
  - no-arg TTY entry now launches a ratatui/crossterm TUI
  - branded ASCII splash contract implemented
  - tabbed section shell + keyboard navigation + command palette/help/confirm overlays implemented
  - read-only panes for updates/packages/tasks/managers/settings/diagnostics implemented
  - common mutation hooks implemented (refresh/detect, selected package actions, task cancel, manager lifecycle toggles, upgrade-all workflow submit)
  - manager parity controls now implemented in-pane (selected-manager detect, executable-path selection cycling, install-method selection cycling, priority reordering)
  - updates workflow options now implemented in-pane (`include_pinned` and `allow_os_updates` toggles before upgrade-all submit)
  - diagnostics pane now supports in-TUI export snapshot writes for support workflows
  - settings now include integrated self-update status/check/apply controls aligned with provenance/channel policy constraints
- remaining for full parity hardening:
  - richer per-section action coverage to match every GUI affordance
  - deeper diagnostics/task output UX parity and accessibility polish
  - expanded integration/e2e TUI workflow validation matrix

---

## 2. Non-Negotiable Constraints

1. Preserve Helm architecture invariants:
- TUI is presentation-only.
- Business logic remains in Rust core / coordinator flows.
- All mutations execute as tasks.
- No shell command string construction.

2. Keep command/contract stability:
- Existing CLI command names and flags remain backward-compatible.
- Non-TTY `helm` with no args still prints help and exits `0`.

3. Preserve channel/update boundaries:
- TUI must reuse existing `self`/provenance policy behavior.
- No new update authority bypass paths.

4. Styling must align with documented branding:
- `docs/brand/TYPOGRAPHY_COLOR_SYSTEM.md`
- `docs/brand/WEBSITE_TYPOGRAPHY_COLOR_SPEC.md` (where applicable to color semantics)

---

## 3. Product Scope

### 3.1 Entry Behavior

- `helm` (no args) + TTY: launch TUI.
- `helm` (no args) + non-TTY: print help, exit `0`.
- `helm <command>` behavior unchanged.

### 3.2 TUI Parity Target (v0.17.4 Track)

TUI sections must match GUI operational surfaces:

- Updates
- Packages
- Tasks
- Managers
- Settings
- Diagnostics
- Self Update (integrated panel and action dialogs)

Out of scope for TUI parity in this cycle:

- GUI onboarding/walkthrough overlays
- GUI-only window chrome interactions

---

## 4. UX Structure

### 4.1 Screen Regions

1. Header Bar:
- app name, version, channel, coordinator health indicator
- global key hints (compact)

2. Section Tabs:
- `Updates | Packages | Tasks | Managers | Settings | Diagnostics`
- active tab highlighted with Helm blue accent

3. Main Workspace:
- split panes based on section:
  - list pane
  - detail pane
  - optional secondary pane (task output/logs)

4. Action/Footer Bar:
- context-aware actions and status
- mutation confirmation prompts and task dispatch status

5. Overlay Layer:
- splash screen
- help modal (`?`)
- command palette (`Ctrl+K`)
- confirmation dialog
- transient notifications/toasts

### 4.2 Interaction Principles

- Never trap the user: every modal has explicit cancel key.
- Keep primary flows one-keystroke discoverable.
- Maintain deterministic focus model; no hidden focus jumps.
- All critical actions support both direct hotkeys and discoverable menu/action buttons.

---

## 5. Keyboard Navigation Contract

### 5.1 Global Keys

- `1..6`: jump to section tab
- `Tab` / `Shift+Tab`: cycle focus regions
- `Ctrl+K`: command palette
- `/`: focus search/filter input in current section
- `r`: refresh (scope-aware)
- `?`: open keymap/help overlay
- `Esc`: close overlay or clear focused input
- `q`: go back / close modal / quit (from top-level with no modal/input focus)

### 5.2 List Navigation

- `↑/↓` and `j/k`: move selection
- `PgUp/PgDn`: page movement
- `Home/End` and `g/G`: list bounds
- `Enter`: open detail / activate primary action

### 5.3 Task and Mutation Keys

- `u`: update selected package (where applicable)
- `i`: install selected package
- `x`: uninstall selected package
- `p`: pin/unpin selected package
- `c`: cancel selected task
- `a`: section-specific bulk action (for example `Upgrade All` in Updates)

### 5.4 Confirmation Rules

- mutating actions require explicit confirmation dialog
- default focus is non-destructive option
- `Enter` confirms only when confirm button is selected

---

## 6. Visual System for TUI

Terminal constraints require semantic mapping from brand tokens.

### 6.1 Color Mapping

Use 256-color palette with truecolor support when available:

- Helm Blue 900 `#1B3A66`: primary headers / active structural accents
- Helm Blue 700 `#2A5DA8`: selected tabs / focused borders
- Helm Blue 500 `#3C7DD9`: interactive highlights
- Gold 500 `#C89C3D`: premium/accent markers only
- Neutrals:
  - dark background target: `#0E1624`
  - panel target: `#141E2F`
  - secondary text target: `#9FB0C7`

Rules:

- Gold stays accent-only (<10% visual weight).
- No aggressive blinking/inversion effects.
- `--no-color` renders fully readable monochrome output.

### 6.2 Typography in Terminal Context

Terminal uses monospace, but hierarchy is still required:

- strong structural separators
- consistent title casing for section headers
- compact but readable density

---

## 7. Splash Screen Contract

The splash screen appears on TUI launch and includes:

1. Helm logo (ASCII)
2. Program name: `Helm` (ASCII art)
3. Tagline: `Take the helm.` (ASCII art)

### 7.1 Splash Timing

- show for `900ms` maximum
- dismiss immediately on any keypress
- skip animation path when terminal is very small (render compact static variant)

### 7.2 ASCII Assets

Store deterministic splash assets in-source:

- `core/rust/crates/helm-cli/src/tui/assets/splash_large.txt`
- `core/rust/crates/helm-cli/src/tui/assets/splash_compact.txt`

First draft content target:

```text
             _.-.
          .-'   '-.
        .'  .- -.  '.
       /   (  |  )   \
      ;     '-'      ;
      |  .---------. |
      ;  |  HELM   | ;
       \  '-------' /
        '.         .'
          '-.___.-'

 _   _      _
| | | | ___| |_ __ ___
| |_| |/ _ \ | '_ ` _ \
|  _  |  __/ | | | | | |
|_| |_|\___|_|_| |_| |_|

 _____     _          _   _            _          _
|_   _|_ _| | _____   | |_| |__   ___  | |__   ___| |_
  | |/ _` | |/ / _ \  | __| '_ \ / _ \ | '_ \ / _ \ __|
  | | (_| |   <  __/  | |_| | | |  __/ | | | |  __/ |_
  |_|\__,_|_|\_\___|   \__|_| |_|\___| |_| |_|\___|\__|
```

Final art can be tuned during implementation, but must preserve all three required elements.

---

## 8. Technical Architecture (TUI Layer)

Use `ratatui` + `crossterm`.

### 8.1 Module Layout

Planned module extraction from current `main.rs` monolith:

- `src/tui/mod.rs` (entry and lifecycle)
- `src/tui/app.rs` (application state model)
- `src/tui/event.rs` (input and tick events)
- `src/tui/ui.rs` (render functions/widgets)
- `src/tui/action.rs` (intent/action enums and dispatch mapping)
- `src/tui/theme.rs` (brand token mapping + no-color fallback)
- `src/tui/splash.rs` (splash rendering/timing/assets)

### 8.2 State Model

Primary TUI state buckets:

- global app state (active section, focus region, overlay stack)
- section states (filters, selected row, paging)
- task state (running/queued list, selected task, follow mode)
- async operation state (loading/error/toast)

### 8.3 Data Access

- read-only data uses existing CLI read paths (`status`, `packages`, `updates`, `tasks`, `managers`, `settings`, `diagnostics`)
- mutations dispatch through existing task/coordinator command paths
- no duplicated business logic in TUI rendering layer

### 8.4 Refresh and Polling

- section-aware polling cadence:
  - active section: fast interval
  - inactive sections: slower interval
- throttle updates when no active tasks
- explicit manual refresh always available

---

## 9. Feature Parity Matrix (TUI vs GUI)

### 9.1 Read Surfaces

- Updates list + summary + manager scope
- Packages list + status filters + search
- Tasks list + task detail + logs/output follow
- Managers list + manager detail + detection/executable/method views
- Settings read/write for CLI-exposed settings
- Diagnostics summary/task/manager/provenance/export

### 9.2 Mutation Surfaces

- package install/uninstall/upgrade/pin/unpin
- updates preview/run (with confirmations)
- manager enable/disable/install/update/uninstall
- manager executable and install-method selection
- manager priority list/set/reset
- settings set/reset
- self update/check/status/auto-check controls

### 9.3 Remaining Known Cross-Surface Parity Gaps (Track During TUI Work)

- progressive remote search orchestration parity: delivered
- per-package Homebrew keg-policy parity: delivered
- manager-scoped bulk-upgrade parity: delivered
- package-install action parity in TUI packages pane: delivered
- launch-at-login remains intentionally GUI-scoped (no CLI/TUI parity target)

---

## 10. Implementation Phases

### Phase 0 — Foundation Refactor (No UX Change)

- extract CLI core into modules reusable by TUI entry
- isolate output/render concerns from business operations
- keep existing command behavior unchanged

Acceptance:

- `cargo test -p helm-cli` passes
- existing read/mutation command contract unchanged

### Phase 1 — TUI Shell + Splash + Navigation Skeleton

- no-arg TTY entry launches TUI
- render header/tabs/footer skeleton
- implement splash screen contract
- implement global keyboard/focus navigation

Acceptance:

- keyboard-only navigation functional
- non-TTY no-arg behavior unchanged
- splash renders logo + Helm + tagline ASCII art

### Phase 2 — Read-Only Section Parity

- implement Updates/Packages/Tasks/Managers/Settings/Diagnostics read panes
- add section search/filter inputs and pagination controls
- add help overlay

Acceptance:

- all read-only sections accessible and stable
- data values match existing CLI JSON/human commands

### Phase 3 — Mutation Flows

- wire mutation hotkeys + action dialogs
- task submission with wait/detach choices
- task cancellation and live output follow

Acceptance:

- all currently implemented CLI mutation commands reachable in TUI
- clear success/failure/cancel feedback

### Phase 4 — Self-Update and Policy UX

- self status/check/update UI panel
- provenance channel display + recommended action
- managed/channel policy messaging

Acceptance:

- behavior matches `helm self ...` command semantics exactly

### Phase 5 — Polish, Performance, and Hardening

- command palette (`Ctrl+K`)
- keybinding discoverability and conflict resolution
- resize handling and compact layout variants
- no-color and low-color rendering validation

Acceptance:

- no stuck focus states
- smooth rendering on low-end hardware
- deterministic exit/cleanup on panic and SIGINT

---

## 11. Testing Strategy

### 11.1 Unit Tests

- keymap reducer behavior
- focus transitions
- overlay stack state transitions
- splash selection logic (large vs compact)

### 11.2 Integration Tests

- no-arg TTY entry launches TUI shell
- non-TTY no-arg path prints help
- mutation dispatch from TUI maps to correct command/task intents

### 11.3 Manual Validation Matrix

- terminal sizes: narrow, standard, wide
- color modes: truecolor, 256-color, `--no-color`
- shells: zsh, bash, fish
- hardware: Intel + Apple Silicon, slower machines included

---

## 12. Delivery Tracking and Documentation Updates

When each phase lands, update:

- `docs/CURRENT_STATE.md` (implementation reality)
- `docs/NEXT_STEPS.md` (priority and completion status)
- `docs/architecture/HELM_CLI_SPEC.ms` (contract updates if behavior changes)

---

## 13. Immediate Next Slice (Execution Order)

1. Add dependencies (`ratatui`, `crossterm`) and create TUI module scaffold.
2. Implement no-arg TTY routing to `tui::run()`.
3. Implement splash screen assets + rendering + skip behavior.
4. Implement section tabs and global key handling.
5. Deliver read-only Updates/Packages/Tasks panes first (highest daily value).
