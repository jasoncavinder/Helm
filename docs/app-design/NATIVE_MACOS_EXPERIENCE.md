# Native Mac Experience Initiative

Status: approved Original Wayfinder direction; incremental runtime migration staged for v0.19-v0.22
Scope: Pre-1.0 product, interaction, and visual design maturation for the macOS app
Last updated: 2026-08-07

## 1. Objective

Make Helm feel unmistakably like a high-quality macOS application: capable, calm, direct, and at home on the platform.

The current app is clean, functional, and professional, but its design has grown through feature delivery and incremental custom styling. The next stage is broader than a visual refresh. It must reconsider how Helm organizes information, uses windows and menus, presents controls, responds to keyboard and pointer input, explains system state, and earns trust during long-running or risky work.

This is an internal design initiative, not a product, edition, entitlement, or user-facing brand. Its quality requirements apply to base Helm and to every future Helm Pro or Helm Fleet surface built on the macOS app.

Approved v0.18 artifact set:

- `NATIVE_MACOS_CURRENT_EXPERIENCE_AUDIT.md`
- `NATIVE_MACOS_COMPONENT_INVENTORY.md`
- `NATIVE_MACOS_INFORMATION_ARCHITECTURE.md`
- `NATIVE_MACOS_PROTOTYPES.md`
- `NATIVE_MACOS_STATE_MATRIX.md`
- `NATIVE_MACOS_QUALITY_BUDGETS.md`
- `NATIVE_MACOS_RESEARCH_VALIDATION.md`
- `NATIVE_MACOS_MIGRATION_MAP.md`

The approved Original Wayfinder target uses Dashboard, Plan, Library, and Activity as peer workspaces with Environment as persistent contextual infrastructure. The status-item popover and Dashboard are the only operational interfaces. Settings is a separate macOS window/scene, diagnostics is contextual, and search is command-based. The Course Indicator communicates the highest-priority environment state without inventing a composite score. No production UI behavior changes until the incremental migration implements these contracts.

## 2. Product Standard

Helm should feel:

- **Native** — familiar macOS structure and behavior before custom decoration.
- **Purposeful** — every surface helps a user understand state or complete an operational task.
- **Quiet** — healthy state recedes; attention and risk are clear without visual alarm fatigue.
- **Dense but legible** — professional information density with strong hierarchy and room to resize.
- **Direct** — common actions are available through the menu bar, keyboard, toolbar, contextual menus, and the relevant content surface.
- **Trustworthy** — plans, authority, progress, failure, recovery, and unchanged state remain visible.
- **Responsive** — cached or partial value appears immediately; background work never makes the app feel frozen.
- **Accessible** — keyboard, VoiceOver, increased contrast, reduced motion, localization, and text expansion are design inputs rather than final checks.

The goal is not to imitate a specific Apple app or chase an annual visual fashion. Helm should use system conventions and controls wherever they fit, add custom identity only where it improves comprehension, and preserve compatibility with the macOS 13 Ventura deployment baseline through availability-aware enhancement.

## 3. Why a Broader Initiative Is Required

The earlier redesign established the current popover, Control Center, sidebar, inspector, onboarding, and visual token system. That work remains a useful baseline, but it intentionally preserved much of the existing layout and interaction model.

The current implementation also contains many bespoke cards, rounded containers, custom button styles, gradients, hover behaviors, and overlay patterns. Individually these can be polished; collectively they can make Helm feel like a cross-platform dashboard rendered on macOS rather than a Mac application designed from platform primitives.

This initiative therefore has authority to change:

- information architecture and navigation
- window, panel, toolbar, menu, and Settings behavior
- content density and presentation model
- workflow sequencing and progressive disclosure
- selection, focus, keyboard, pointer, and contextual action behavior
- component semantics and visual styling
- empty, loading, partial, failure, recovery, and offline states
- animation and state-transition feedback
- accessibility and localization behavior
- perceived performance and continuity between app surfaces

It does not have authority to move business logic into SwiftUI, weaken core safety gates, or bypass the service/core execution boundary.

## 4. Design Principles

### 4.1 Platform conventions before custom components

Prefer system-provided macOS patterns when they express the intended behavior:

- standard windows, title bars, toolbars, sidebars, split views, tables, lists, sheets, alerts, menus, and Settings scenes
- standard control sizes, focus rings, selection behavior, keyboard equivalents, contextual menus, and accessibility semantics
- system colors, materials, symbols, typography, and vibrancy where appropriate

Custom components require a documented reason such as domain-specific state, unusually dense operational data, or a safety interaction that system controls cannot express clearly.

### 4.2 Content before containers

Do not turn every datum into a card. Use tables, lists, outlines, grouped forms, disclosure groups, inspectors, and separators when those structures improve scanning. Reserve cards for meaningful summaries, bounded decisions, or distinct transient states.

### 4.3 One hierarchy for each window

Each surface must have one obvious primary purpose:

- the menu bar popover answers whether attention is needed and offers a short path to act
- the Dashboard supports inspection, planning, execution, and recovery
- Settings contains infrequently changed preferences, not operational work
- sheets contain bounded decisions; inspectors contain selection context

### 4.4 Commands exist beyond visible buttons

Important commands must be represented in the macOS menu bar and receive conventional keyboard shortcuts where appropriate. Toolbar items must not be the only route to an action. Context menus must expose relevant selection-scoped commands without becoming the only discoverable route.

### 4.5 State is explicit and continuous

Helm must preserve the user's mental model as work moves from preview to execution to verification. Selection, filters, scroll position, window state, task progress, and recovery context should not reset unnecessarily.

### 4.6 Brand supports the platform

Helm Blue, Rope Gold, and the command-bridge character remain useful, but they must not overpower system appearance or invent alternate control semantics. Gold stays limited to genuine Pro context or carefully defined priority—not ordinary upgrade or warning actions.

### 4.7 Design for every state

No workflow is complete until it covers:

- loading and cached-first rendering
- healthy and actionable success
- empty and not-applicable states
- partial coverage and partial failure
- blocked-by-policy and blocked-by-permission states
- offline and deferred work
- cancellation and interruption
- verification, recovery, and rollback limits

## 5. Experience Architecture

### 5.1 Menu bar surface

The menu bar remains Helm's ambient entry point. It should be glanceable, lightweight, and stable.

Required direction:

- show cached health immediately and update progressively
- prioritize one current condition over a grid of competing metrics
- expose the most likely action and active work without reproducing the Dashboard
- use one predictable popover for both status-item click types: either click toggles it while Dashboard is closed, primary click focuses an open Dashboard, and secondary click keeps the popover available alongside Dashboard
- keep utility commands consistent with application-menu commands
- avoid embedding full settings, long diagnostics, or deep package management in the popover

### 5.2 Dashboard

The Dashboard is Helm's primary working window and uses the approved Original Wayfinder composition.

Required direction:

- use a native window/titlebar/toolbar hierarchy with resizable content and sensible minimums
- use Dashboard, Plan, Library, and Activity as labeled sidebar workspaces
- keep Environment persistently discoverable at the sidebar foot as contextual manager/source infrastructure
- support show/hide sidebar behavior and preserve the user's window configuration
- avoid a permanent inspector on Dashboard; reveal contextual trailing detail after selection where comparison requires it
- use a content list/table plus contextual detail when users compare many software identities, managers, plan steps, or tasks
- keep global search, refresh, and plan actions in predictable command locations
- prevent contextual detail from becoming an unstructured second application
- allow large displays to reveal context without forcing modal navigation
- use one shared Course Indicator projection in Dashboard and popover; determinate progress requires backend-owned completed/total values

### 5.3 Settings

Settings should behave as a standard macOS Settings window or scene reachable through the application menu and Command-Comma.

Operational actions such as refresh, repair, inspect diagnostics, or run a Plan should live in the Dashboard. Settings should contain durable preferences such as schedule, manager policy, update behavior, appearance/density choices, CLI integration, and support/diagnostic configuration.

### 5.4 First run

Project WOW owns the value sequence and consent model. This initiative owns the macOS presentation quality of that sequence.

The two initiatives share these requirements:

- one clear window hierarchy rather than a generic multi-page setup wizard
- native focus, keyboard, VoiceOver, sheet, progress, and cancellation behavior
- progressive Environment Brief rendering without layout churn
- no mandatory product tour after the user has already received value
- contextual guidance that behaves like part of the app, not an overlay presentation layered over it

### 5.5 Notifications and background behavior

Notifications are reserved for meaningful state changes when Helm is not frontmost, such as completed work, required approval, or a failure needing attention. They must deep-link to the relevant context and respect system notification settings. In-app banners must not duplicate active-window information unnecessarily.

## 6. Workstreams

### Product and workflow design

- Validate the highest-frequency jobs: determine environment state, understand updates, run a safe Plan, find/manage software through Library, diagnose a failure, and manage Environment sources.
- Remove steps and duplicated entry points that do not improve safety.
- Define progressive disclosure for novice, returning, and expert users.

### Information architecture

- Implement and preserve the approved Dashboard, Plan, Library, Activity, and contextual Environment model while keeping Settings separate.
- Define what belongs in list content, the inspector, a sheet, Settings, or a dedicated detail view.
- Standardize navigation and deep-link behavior across popover, notifications, menus, and Dashboard.

### macOS application model

- Establish application menus and command routing.
- Define window lifecycle, restoration, resizing, full-screen policy, multi-display behavior, and activation behavior for a menu-bar-first app.
- Rework Settings as a first-class macOS settings surface.
- Define toolbar composition, customization policy, overflow, and command parity.

### Interaction and input

- Complete logical Tab and Shift-Tab traversal using AppKit bridging where SwiftUI alone is insufficient.
- Support arrow-key navigation, default and cancel actions, focus restoration, selection semantics, contextual menus, drag and drop where valuable, and conventional shortcuts.
- Remove pointer-only and hover-only dependencies.

### Visual and component system

- Audit every custom control against an equivalent native component.
- Reduce container, radius, shadow, gradient, and material variation.
- Define semantic typography, spacing, selection, focus, state, destructive-action, and Pro tokens.
- Validate light, dark, increased-contrast, inactive-window, and reduced-transparency appearances.

### Content design

- Use concise, action-oriented labels and platform terminology.
- Separate status, explanation, consequence, and action.
- Standardize manager, provenance, authority, policy, and failure language.
- Ensure localization expansion does not destroy hierarchy or hide safety information.

### Motion and feedback

- Use motion to explain causality, continuity, and changing state.
- Prefer system transitions and progress indicators.
- Avoid scale, bounce, blur, or ambient animation that exists only for style.
- Provide equivalent reduced-motion behavior and announcements for nonvisual state changes.

### Accessibility and inclusion

- Treat keyboard-only and VoiceOver completion of core workflows as release gates.
- Validate Full Keyboard Access, focus order, labels, values, actions, headings, groups, announcements, contrast, increased contrast, reduced motion, and reduced transparency.
- Test color-independent status, text expansion, truncation, localization, and zoomed display configurations.

### Perceived performance

- Render cached structure and last-known state immediately.
- Avoid whole-window invalidation and layout jumps during polling.
- Keep selection and controls responsive while background tasks run.
- Define budgets for window open, navigation, search response, scrolling, progress updates, and idle resource use.

### Design quality operations

- Maintain state-complete prototypes and a component inventory.
- Review representative screens at common and minimum window sizes in all supported appearances.
- Add deterministic screenshot/state fixtures where stable enough to provide signal.
- Run moderated usability sessions at milestone boundaries rather than waiting for release candidates.

## 7. Pre-1.0 Milestones

### 0.18.x — Experience definition and prototypes (post-v0.18.1 planning closure)

This planning and validation artifact closure completed after the corrective `v0.18.1` release and is included as planning-only content in the final `v0.18.2` containment release. It does not claim implementation of the target experience. Production `0.19.x` redesign implementation begins from these artifacts.

Deliverables:

- current-experience audit with screenshots and a custom-versus-native component inventory
- task-based human-research protocol and expert cognitive-walkthrough baseline; owner-run participant sessions remain a later checkpoint
- approved experience principles, information-architecture options, and platform-pattern decisions
- low- and high-fidelity prototypes for the menu bar surface, Dashboard, Settings, and Project WOW first run
- state matrix covering loading, success, empty, partial, failure, offline, blocked, and recovery states
- accessibility and performance budgets
- migration map that permits incremental delivery without duplicating business logic

Exit gate:

- prototypes cover the target workflows and pass expert review before production UI refactoring begins
- open design decisions and intentional departures from macOS conventions are recorded
- Project WOW and whole-app navigation use one compatible experience model
- the owner-run moderated study remains open and is required before v0.20 workflow sign-off and again before v0.22 UI lock

### 0.19.x — Native foundation and first-run value

Deliverables:

- native window, titlebar, toolbar, menu-command, sidebar/split-view, selection, and focus foundations
- approved Original Wayfinder shell and shared Course Indicator state projection
- standard semantic component layer and retirement plan for redundant custom controls
- Settings window/scene architecture with Command-Comma behavior
- window restoration, resizing, activation, and multi-display behavior
- Project WOW Environment Brief and reviewed-plan foundation using the new experience model
- keyboard traversal infrastructure, including targeted AppKit bridges

Exit gate:

- the app shell and first-run foundation feel coherent in light/dark appearance at minimum and expanded window sizes
- toolbar actions have menu-command equivalents
- no new feature introduces an ad hoc button, card, sheet, or status semantic outside the design system

### 0.20.x — Core workflow redesign

Deliverables:

- redesigned Dashboard, Plan, Library, Activity, Environment, search-command, contextual-detail, and diagnostics workflows
- native list/table/outline patterns where comparison and density matter
- simplified action hierarchy and contextual commands
- continuous plan -> execution -> verification -> recovery presentation
- complete content-design pass for labels, empty states, errors, policy blocks, provenance, and authority
- revised unified status-item popover connected cleanly to Dashboard context

Exit gate:

- moderated users complete the six primary jobs without assistance or avoidable navigation reversals
- destructive, privileged, and policy-blocked actions remain explicit and attributable
- each domain has complete state coverage rather than only ideal-state polish

### 0.21.x — Accessibility, system integration, and resilience

Deliverables:

- keyboard-only and VoiceOver parity for all release-critical workflows
- Full Keyboard Access, focus restoration, standard shortcuts, contextual menus, and command validation
- increased contrast, reduced motion, reduced transparency, localization, and text-expansion hardening
- notification deep links and background/foreground continuity
- offline, partial-data, interrupted-task, and recovery-state UX validation
- Project WOW verified improvement, Action Receipt, and recovery presentation integrated with the redesigned surfaces

Exit gate:

- no known keyboard trap or pointer-only release-critical action
- Accessibility Inspector and manual VoiceOver runs pass the defined matrix
- all seven current locales pass representative narrow/expanded window and critical-sheet validation

### 0.22.x — Fit, finish, and design validation

Deliverables:

- perceived-performance and scrolling optimization against documented budgets
- motion, progress, inactive-window, menu, focus, selection, and transition polish
- final visual consistency and custom-component exception audit
- state-fixture and screenshot-regression coverage for stable representative surfaces
- multi-display, resolution, appearance, localization, long-running-task, and failure-injection validation
- final moderated usability and first-run studies
- updated product screenshots and documentation only after UI lock

Exit gate:

- no high-severity design-system, accessibility, workflow, or perceived-performance defects
- users can identify current state, next action, action consequence, and recovery path on release-critical surfaces
- design sign-off covers behavior and all state variants, not only static screenshots

### 1.0.0 — Native experience release gate

Helm 1.0 does not ship merely because the redesign is visually complete.

Required evidence:

- the Project WOW first-run gate passes on the production macOS experience
- core workflows pass moderated usability, keyboard, VoiceOver, localization, failure, and performance validation
- important commands are available through appropriate menus and shortcuts
- window, selection, focus, inspector, and task state survive expected lifecycle transitions
- custom components have documented product value and complete native interaction/accessibility behavior
- no known high-severity mismatch between visible UI state and core/service authority

## 8. Success Measures

Quantitative measures:

- time to identify health and required action
- completion time and error rate for the six primary jobs
- percentage of release-critical workflows completable by keyboard and VoiceOver
- navigation reversals and abandoned sheets per task
- first useful render, window-open, section-switch, search-response, and scroll-performance budgets
- accessibility, localization, state-matrix, and visual-regression pass rates
- support reports attributable to unclear state, hidden actions, or lost context

Qualitative measures:

- users describe Helm as a Mac app rather than a web dashboard or command wrapper
- users can predict where commands and settings live
- users understand why an action is available, blocked, risky, or incomplete
- expert users find the app efficient without making first-time users feel lost
- brand character is recognizable without custom styling interfering with platform behavior

Metrics must not justify invasive telemetry. Prefer moderated studies, opt-in beta feedback, local timing instrumentation, test fixtures, and aggregate privacy-preserving signals when telemetry is later introduced under an approved policy.

## 9. Relationship to Existing Design Documents

This document is the canonical pre-1.0 whole-app design direction.

- `docs/app-design/PROJECT_WOW.md` remains canonical for first-run value, consent, recovery, and word-of-mouth goals.
- The eight `docs/app-design/NATIVE_MACOS_*.md` definition artifacts listed above are the approved target audit, IA, prototypes, state/budget contracts, research protocol, and migration plan.
- `docs/app-design/APP_REDESIGN_PROPOSAL.md` records the earlier visual-refinement proposal and is superseded where it limits work to visual changes or forbids layout/interaction changes.
- `docs/ui/REDESIGN_CONCEPT.md`, `INFORMATION_ARCHITECTURE.md`, `USER_FLOWS.md`, `VISUAL_SYSTEM.md`, and `MOCKUPS.md` describe the current integrated baseline and are research inputs, not immutable constraints.
- `docs/ui/SWIFTUI_ARCHITECTURE.md` remains the implementation-state reference.
- `docs/brand/` remains the brand source, subordinate to native behavior and accessibility inside the app.

## 10. Platform References

Design and review should use current Apple guidance, especially:

- [Designing for macOS](https://developer.apple.com/design/human-interface-guidelines/designing-for-macos/)
- [Toolbars](https://developer.apple.com/design/human-interface-guidelines/toolbars)
- [Sidebars](https://developer.apple.com/design/human-interface-guidelines/sidebars)
- [Menus](https://developer.apple.com/design/human-interface-guidelines/menus)
- [Settings](https://developer.apple.com/design/human-interface-guidelines/settings)
- [Accessibility](https://developer.apple.com/design/human-interface-guidelines/accessibility)

Apple guidance is a baseline rather than a substitute for user research. Any intentional departure must explain the Helm-specific benefit and preserve expected macOS input, accessibility, and command behavior.
