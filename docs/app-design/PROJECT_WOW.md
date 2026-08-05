# Project WOW — First-Run Value Initiative

Status: approved internal direction; v0.18 contract/prototype artifact closure complete; runtime implementation deferred
Scope: Internal first-run design initiative with features allocated across existing product plans
Last updated: 2026-08-04

## 1. Objective

Treat Helm's first-run experience as the product's primary marketing engine.

The experience must demonstrate useful, trustworthy, personalized value before asking a user to configure Helm manually. A successful first session should leave the user able to explain:

- what Helm discovered
- what Helm did and did not change
- what Helm recommends next
- why Helm is safer or faster than managing each tool independently

Project WOW is an internal initiative name for creating Helm's initial first-use value moment. It is not a product, edition, entitlement, pricing tier, or user-facing brand. User-facing language should follow Helm's existing brand voice: calm, technical, deterministic, and professional.

Planning artifacts:

- Architecture/state semantics: `docs/architecture/FIRST_RUN_EXPERIENCE_CONTRACTS.md`
- Environment Brief, setup-session, reviewed-plan, Action Receipt, redaction, metrics, and managed-bootstrap schemas: `docs/contracts/first-run/`
- Current executable-route audit: `docs/architecture/BOOTSTRAP_FEASIBILITY_MATRIX.md`
- Native first-run prototype: `docs/app-design/NATIVE_MACOS_PROTOTYPES.md`
- Cross-surface state presentation: `docs/app-design/NATIVE_MACOS_STATE_MATRIX.md`
- Owner-run research and accessibility protocol: `docs/app-design/NATIVE_MACOS_RESEARCH_VALIDATION.md`

## 2. North-Star Experience

Project WOW follows one shared loop across GUI, CLI, and future managed deployments:

1. **Discover** — map the environment using staged, local-first observation.
2. **Reveal value** — present a personalized Environment Brief as soon as useful findings exist.
3. **Recommend** — propose a small number of explainable improvements.
4. **Preview** — show actions, authority, network use, privilege needs, and rollback limits.
5. **Improve** — execute only approved typed actions through Helm orchestration.
6. **Verify** — re-detect affected state and compare before/after results.
7. **Record** — preserve an auditable Action Receipt and recovery state.

The core promise is:

> Helm maps the user's environment, explains what matters, and proposes a safe improvement before asking them to configure anything.

Delight must come from competence rather than gamification. Project WOW does not use fabricated health scores, fake time-saved claims, confetti, forced social sharing, or hidden automation.

## 3. Product/Tier Feature Allocation

Project WOW does not define a new edition or product. Features conceived through the initiative are allocated among Helm's existing product plans:

- **Helm** — the fully usable base consumer product.
- **Helm Pro** — advanced individual intelligence, history, and reusable personal workflows.
- **Helm Fleet** — the business fleet product currently described elsewhere as Helm Business/Fleet.

The final public name of the business product can be resolved separately. This document uses **Helm Fleet** as its capability label while preserving the existing separate-product and separate-lifecycle architecture.

Feature allocation follows these rules:

1. The first-run value reveal belongs to base Helm. It cannot function as Helm's adoption engine if it is paywalled.
2. Safety, transparency, accessibility, offline operation, failure recovery, and honest rollback are never Pro gates.
3. Pro may deepen analysis or portability, but must not withhold information required to make a safe decision.
4. Fleet adds organization authority, deployment, policy, drift, rollout, and audit. It does not replace a business's MDM or software-distribution system.
5. Pre-1.0 builds may label future Pro capabilities without gating them, consistent with the monetization strategy.

## 4. Complete First-Launch Journey

### 4.1 Launch and legal gate

- Render the real Helm shell immediately; do not delay on a timed splash.
- Load cached state when available.
- If the distribution channel requires license acceptance, present a single concise gate with a bundled offline-readable copy.
- Begin automatic local discovery after any required acceptance.
- Keep persistent trust copy visible during the scan: `Local scan · No changes · No network`.

### 4.2 Staged discovery

Results stream instead of waiting for all managers.

Stage A — instant local context:

- macOS version and architecture
- active shell
- Helm distribution/update authority
- Helm CLI shim state
- cached manager/package state with freshness labels

Stage B — local environment mapping:

- manager detection and versions
- executable instances and active paths
- provenance, confidence, and decision margin
- eligibility and managed-policy constraints
- manager authority and dependency relationships
- post-install setup findings
- local Doctor findings relevant to manager health

Stage C — deeper read-only analysis:

- installed package inventory
- pins and manager scopes
- local configuration conflicts
- stale selections or metadata/executable mismatches

Stage D — disclosed network analysis:

- outdated-package checks
- remote catalog/search enrichment
- advisory enrichment where available and entitled

Stage D requires an explicit action such as `Check Now`; it is never required to reach the first useful result.

### 4.3 Environment Brief

The Environment Brief is the primary first-run value reveal.

Example:

> **Your environment, mapped.**  
> 9 managers ready · 214 packages organized  
> 2 items need attention  
> No changes made.

The brief prioritizes concrete findings over a synthetic score:

- ready managers
- detected-but-protected managers
- duplicate or ambiguous manager instances
- packages organized by manager and authority
- cached or current update state
- missing shell integration
- stale executable choices
- repairable local findings
- clear partial-scan coverage when some managers fail

### 4.4 Conservative defaults

Project WOW removes manager-selection and generic-settings pages from the mandatory path.

Helm may derive and present conservative defaults automatically without persisting a change. Persisting a Helm preference or changing manager enablement, executable selection, shell files, CLI integration, packages, managers, or system state requires the applicable reviewed plan and consent. The Environment Brief and receipt disclose assumed defaults, persisted choices, and unchanged environment state.

- propose monitoring eligible detected managers
- keep ineligible managers visible but disabled
- use canonical authority ordering for every proposed/executed plan
- use system locale, contrast, and motion preferences
- propose an executable only when the choice is unambiguous under core policy
- retain PATH-default behavior when provenance is ambiguous
- leave destructive cleanup and unattended mutations disabled
- keep guarded OS actions behind explicit confirmation
- keep product analytics disabled unless the user opts in

The user can customize proposed defaults without entering a mandatory generic-settings wizard.

### 4.5 Goal-based personalization

Ask for intent only after Helm understands the machine:

- Maintain this Mac
- Audit only; make no changes
- Set up a new Mac
- Customize my stack

Infer optional ecosystem chips from detected tooling and let the user confirm them:

- JavaScript/TypeScript
- Python
- Rust
- Ruby
- Containers
- macOS applications

Do not scan arbitrary projects, shell history, or the user's home directory to infer interests. Project-level analysis requires explicit folder selection in a later context.

### 4.6 Recommended plan

Each plan item explains:

- the observed problem or opportunity
- why the recommendation applies
- the manager and authority involved
- the typed Helm action to be used
- network and download requirements
- whether authorization will be requested
- files or settings that may change
- verification behavior
- recovery and rollback limitations

The primary action is `Review Plan`, not `Fix Automatically`.

### 4.7 First verified improvement

The first session should offer one small, meaningful improvement when applicable:

- repair a stale executable selection
- enable an eligible detected manager
- install the Helm CLI shim
- complete supported mise/rustup/asdf post-install setup
- resolve a metadata-only Homebrew mismatch
- review an ordered update plan
- install a manager through an already supported, policy-allowed method

Execution remains task-based, cancellable, and observable. Helm re-detects affected state before claiming success.

### 4.8 Completion and Action Receipt

Show a concrete before/after result:

> **Helm is ready.**  
> 9 managers monitored  
> 214 packages mapped  
> 1 configuration issue resolved  
> 12 updates ready for review

Completion actions:

- Open Helm
- Review Updates
- Copy Summary
- View Action Receipt

The copied summary is locally generated and redacted by default. It must not expose usernames, paths, package names, organization identifiers, or other environment details without explicit inclusion.

### 4.9 Progressive education

Replace the mandatory multi-step walkthrough with dismissible contextual tips:

- authority ordering when the first update plan opens
- provenance when an executable is ambiguous
- guarded action policy before the first guarded action
- diagnostics and recovery after the first failure
- pin behavior when the first pin is created

Tips do not auto-replay, auto-advance, or block normal work. A replayable optional tour may remain in Help or Settings.

## 5. Discovery and Consent Boundary

| Class | Examples | Default |
|---|---|---|
| Local metadata | OS, architecture, shell, Helm channel | Automatic |
| Local tool detection | paths, versions, instances, provenance, eligibility | Automatic |
| Helm configuration | Helm-owned preferences, markers, CLI shim | Automatic observation |
| Package inventory | installed packages through read-only manager operations | Background and cancellable |
| Remote status | outdated checks and remote catalogs | Disclosed one-click consent |
| Project inspection | repositories or selected folders | Explicit scope selection |
| Mutations | installs, upgrades, shell edits, cleanup | Plan and confirmation |
| Privileged/system work | CLT, Rosetta, OS updates | Separate just-in-time confirmation |
| Product analytics | onboarding behavior or environment composition | Explicit opt-in |

Helm never scans shell history, arbitrary home-directory contents, credentials, secrets, or unrelated project files during first run.

## 6. Bootstrap Workflow

`Set up a new Mac` generates a dependency-aware plan. It is not an unattended installer.

```text
System prerequisites
└── Xcode Command Line Tools, if required
    └── Homebrew, if selected or required downstream
        ├── mise
        │   ├── Node ecosystem
        │   ├── Python ecosystem
        │   └── Ruby ecosystem
        ├── mas
        ├── pipx / Poetry
        └── podman / colima

Direct alternatives
├── rustup
├── mise official installer
└── asdf official installer

Finalization
├── Helm CLI shim and shell completion
├── Helm-owned shell integration
├── manager-scoped verification
└── first Environment Brief
```

Recommended ownership model:

- mise for language runtimes
- rustup when the user wants the upstream Rust toolchain model
- Homebrew for native CLI utilities and macOS applications
- pipx for standalone Python applications
- avoid installing one runtime through multiple authorities without an explicit reason

Pre-1.0 bootstrap should use only lifecycle planners and typed actions already hardened in Helm. Installing Homebrew itself and initiating Xcode Command Line Tools require dedicated typed, verified workflows before they can join automatic planning.

## 7. GUI and CLI Surface Contract

### 7.1 GUI

The first-run GUI should use one streaming Environment Brief followed by an optional plan, rather than a five-page configuration wizard.

Required UI elements:

- persistent scan safety label
- streamed manager/finding rows
- clear coverage and freshness status
- `See what Helm checked` details
- `Review Plan`, `Use Helm Now`, and `Customize` paths
- non-modal transition into a populated Control Center

### 7.2 CLI/TUI

Retain `helm onboarding` compatibility and consider `helm setup` as the human-facing namespace:

```text
helm setup
helm setup scan --offline
helm setup plan --profile maintain
helm setup apply --plan <plan-id> --yes
helm setup resume
helm setup receipt <receipt-id> --redacted
```

Contract requirements:

- prompt only when stdin is a TTY
- preserve deterministic noninteractive flags and exit codes
- keep progress off JSON/NDJSON stdout
- support quiet, no-color, cancellation, and plain line-oriented output
- continue the user's original command after any required first-run gate
- provide the same finding, plan, verification, and receipt semantics as GUI

## 8. Recovery, Rollback, and Failure Handling

Every Project WOW mutation belongs to a persisted setup session.

After interruption, offer:

- reconcile persisted and currently observed state
- resume only still-valid unfinished work after explicit review and revalidation
- retry the failed step
- keep successful changes
- roll back reversible changes
- export diagnostics

| Change | Recovery policy |
|---|---|
| Helm preferences | Restore prior snapshot |
| Manager enablement or priority | Restore prior values |
| Helm-owned shell block | Remove exact marked block; use a validated backup when needed |
| Helm CLI shim | Remove only when Helm-created ownership still matches |
| Manager installation | Offer uninstall only when Helm installed it and dependency policy permits |
| Package upgrade | Do not promise rollback without a verified manager downgrade path |
| macOS/system update | Never claim rollback |

Manager failures remain isolated. Partial results state exact coverage, for example: `7 of 9 managers mapped; Homebrew and npm require retry.`

Special cases:

- no managers: offer audit details, supported bootstrap, or normal app entry
- offline: show local/cached results and mark network work deferred
- service unavailable: retry with bounded backoff, retain cached value, and expose diagnostics
- privilege denied: stop only the dependent plan branch
- insufficient disk or power: fail preflight before downloading or mutating
- ambiguous provenance: preserve read-only behavior until the user or managed policy resolves authority

## 9. Security and Privacy Requirements

1. No administrator request at launch.
2. Authorization occurs immediately before the requiring action.
3. Plans reference allowlisted typed actions, never arbitrary commands or remote scripts.
4. Installer downloads use HTTPS, approved hosts, bounded size/time, and integrity verification.
5. State is revalidated immediately before apply and again after execution.
6. Installer source and install-instance provenance are recorded.
7. Ambiguous ownership is never resolved silently.
8. Eligibility, dependency, managed-policy, timeout, and cancellation gates remain core-owned.
9. Share/export payloads are redacted and schema-versioned.
10. Analytics are optional and never a prerequisite for first-run value.

## 10. Accessibility, Performance, and Offline Gates

Accessibility requirements:

- full keyboard completion with predictable focus
- VoiceOver summaries that avoid announcement storms as results stream
- textual state in addition to color and animation
- Reduce Motion and Increased Contrast behavior
- resizable/localized copy without clipped findings
- no forced countdowns or timed decisions
- plain CLI mode where alternate-screen TUI behavior is unsuitable

The existing SwiftUI Tab-traversal limitation is a Project WOW release blocker, not an accepted limitation for the revised first-run flow.

Performance targets:

- interactive shell visible: p95 under 1 second
- first personalized result: p50 under 5 seconds and p95 under 15 seconds
- complete local brief: p95 under 45 seconds
- no mandatory network operation
- bounded detection concurrency and continued shared-backend serialization
- every stage cancellable

Offline behavior:

- manager detection, versions, provenance, and eligibility remain available
- cached package/update data includes freshness
- local Doctor findings remain available
- plan preview remains available
- bundled terms and help remain available
- network actions are `Deferred`, not presented as local failures
- cached installers run only when integrity and policy can be verified

## 11. Memorable Feature and Product/Tier Matrix

Complexity uses `S` (localized surface), `M` (shared contract and surfaces), `L` (persisted cross-layer workflow), and `XL` (new hardened subsystem). Impact is scored from 1 to 5.

| Priority | Feature | Product/tier | Why it is remarkable | First-five-minute value | Recommendation potential | Complexity / impact |
|---:|---|---|---|---|---|---|
| 1 | Environment Brief | Helm | Makes a fragmented toolchain understandable immediately | Very high | Very high | M / 5 |
| 2 | Action Receipt and redacted summary | Helm | Proves transparency and creates something safe to share | Very high | Very high | S-M / 5 |
| 3 | One Safe Improvement | Helm | Improves the machine during the first session and verifies the result | Very high | Very high | L / 5 |
| 4 | Contextual tips replacing forced tours | Helm | Users learn while accomplishing real work | High | Medium-high | S-M / 4 |
| 5 | Safety Conflict Detection | Helm | Protects users from ambiguous ownership, duplicate instances, and system-tool hazards | High | High | M / 5 |
| 6 | GUI/CLI/TUI setup parity | Helm | Makes terminal-first setup feel as deliberate as the native app | High | High | M / 4 |
| 7 | Resumable setup and honest rollback | Helm | Makes exploration unusually safe | High | High | L / 5 |
| 8 | Complete offline first run | Helm | Demonstrates that Helm remains useful and trustworthy without a service dependency | Medium-high | Medium | M / 4 |
| 9 | Basic Bootstrap Plan | Helm | Turns supported manager setup into one reviewed dependency-aware workflow | Very high on a new Mac | High | L / 5 |
| 10 | Advanced Conflict Radar | Helm Pro | Adds optimization, what-if analysis, and historical context beyond safety-critical findings | High | Very high among power users | L / 5 |
| 11 | Personal Helm Blueprint | Helm Pro | Lets an individual export, diff, and reuse a declarative toolchain recipe | Medium-high | Very high | XL / 5 |
| 12 | Advanced personal insights | Helm Pro | Adds history, recurring maintenance patterns, and richer recommendation ranking | Medium | Medium-high | L / 4 |
| 13 | Advisory-enriched recommendations | Helm Pro | Connects local state to actionable vulnerability intelligence | Medium | High | L / 5 |
| 14 | Managed zero-touch first run | Helm Fleet | Arrives preconfigured while still showing employees what is managed and why | Very high | High for business adoption | L / 5 |
| 15 | Fleet Environment and Compliance Brief | Helm Fleet | Converts local package/tool state into bounded, explainable compliance evidence | Very high for administrators | Very high | XL / 5 |
| 16 | Organization Baselines | Helm Fleet | Applies team/device-group desired state without hiding local evidence | High | Very high | XL / 5 |
| 17 | Ring rollout, approvals, and audit | Helm Fleet | Brings Helm safety semantics into enterprise change control | Medium on first device; high operationally | High | XL / 5 |
| 18 | Management-tool integration kits | Helm Fleet | Fits Helm into existing deployment and inventory workflows instead of demanding replacement | High during evaluation | Very high | XL / 5 |

Safety Conflict Detection remains in Helm. Helm Pro can add deeper optimization, history, and simulation, but it cannot gate the information needed to avoid an unsafe action.

## 12. Helm Fleet Integration Contract

### 12.1 Product boundary

Helm Fleet is a developer-tool and package-control layer that works beneath existing business management authority.

It does not replace:

- Apple device enrollment or MDM
- application/package distribution points
- organization device/user grouping
- identity, certificate, or secrets management
- OS update enforcement
- enterprise self-service portals
- SIEM, ticketing, or compliance systems of record

MDM/software distribution owns enrollment, assignment, delivery, and organization-wide enforcement. Helm owns deterministic local observation, package/toolchain policy evaluation, provenance-aware planning, typed execution, verification, and explainability.

### 12.2 Authority order

Managed state uses the following precedence:

1. Apple/MDM-enforced configuration and OS update declarations
2. Signed Helm Fleet organization policy snapshot
3. Distribution-tool assignment and rollout scope
4. Helm core safety minimums
5. Local Helm defaults
6. User preferences where organization policy permits

No Helm policy may weaken Apple/MDM enforcement or Helm core safety minimums. Conflicts fail closed and identify the controlling authority.

### 12.3 Vendor-neutral integration surfaces

Build standards-based surfaces before bespoke vendor APIs:

- signed/notarized universal PKG
- managed preferences and managed app configuration schema
- offline bootstrap policy file with signature and expiry handling
- stable CLI JSON/NDJSON schemas and documented exit codes
- idempotent scan/plan/apply/reconcile operations
- bounded single-value inventory attributes plus a separate detailed export
- compliance discovery output with explicit schema version
- local audit/event spool with redaction and retention policy
- optional HTTPS/webhook/SIEM connectors after local contracts stabilize
- user-context versus root-context behavior documented for every action

Vendor agents or private command surfaces must not become correctness dependencies when standard PKG, profile, script, inventory, or API mechanisms are available.

### 12.4 Initial compatibility targets

| Tool | Deployment/configuration seam | Inventory/compliance seam | Authority rule |
|---|---|---|---|
| Jamf Pro | PKG policies, configuration profiles, scripts, Self Service | Extension Attributes, policy logs, export connector | Jamf scope, policy frequency, and Self Service assignment remain authoritative |
| Microsoft Intune | macOS PKG/LOB deployment, settings/custom profiles, shell scripts | Custom attributes and custom compliance discovery | Intune assignments and compliance policy remain authoritative |
| Kandji | Custom App, Custom Profile, Custom Script, Blueprint/Assignment Map, Self Service | Audit/remediation scripts and Library Item status | Kandji Assignment Map and execution frequency remain authoritative |
| Munki | PKG/pkginfo, manifests, catalogs, Managed Software Center | install checks, version checks, ManagedInstall reports | Munki manifest/catalog desired state remains authoritative |

Later compatibility validation can cover Workspace ONE, Mosyle, Addigy, SimpleMDM, and other macOS management systems through the same vendor-neutral artifacts before adding product-specific connectors.

### 12.5 Coexistence behavior

- Detect the presence of managed configuration and display `Managed by your organization`.
- Skip consumer first-run questions already answered by managed configuration.
- Show employees a readable receipt of organization-managed versus user-controllable choices.
- Clamp local automation to the most restrictive effective policy.
- Do not race a distribution tool that is actively installing or updating the same asset.
- Reconcile and report drift; do not automatically seize ownership of software managed elsewhere.
- Attribute every result to Helm, the native manager, or the external management authority.
- Keep offline operation on the last valid signed policy snapshot and surface its age/expiry.

### 12.6 Current ecosystem references

- Apple ManagedApp and declarative configuration: <https://developer.apple.com/documentation/managedapp>
- Apple declarative device management: <https://support.apple.com/guide/deployment/depc30268577/web>
- Jamf Pro package deployment: <https://learn.jamf.com/r/en-US/jamf-pro-documentation-current/Package_Deployment>
- Jamf Pro scripts and policies: <https://learn.jamf.com/en-US/bundle/jamf-pro-documentation-current/page/Scripts.html>
- Microsoft Intune macOS scripts and custom attributes: <https://learn.microsoft.com/en-us/intune/device-management/tools/run-shell-scripts-macos>
- Microsoft Intune macOS PKG deployment: <https://learn.microsoft.com/en-us/intune/app-management/deployment/add-unmanaged-pkg-macos>
- Kandji Custom Apps: <https://support.kandji.io/kb/custom-apps-overview>
- Kandji Custom Scripts: <https://support.kandji.io/kb/custom-scripts-overview>
- Munki managed software behavior: <https://github.com/munki/munki/wiki/managedsoftwareupdate>

These references identify supported integration patterns, not committed vendor-specific implementation APIs. Revalidate vendor contracts when the corresponding Fleet milestone begins.

## 13. Success Metrics

The primary metric is **time to verified value**, not wizard completion.

Local-by-default measurements:

- time to first personalized result
- time to first successful verified action
- Environment Brief reach rate
- plan-view and plan-apply rate
- verification success rate
- partial-failure rate by manager
- privilege-request cancellation rate
- resume and rollback success
- offline completion rate
- contextual-tip engagement/dismissal
- copy-summary and Blueprint export usage

Any upload of onboarding behavior or environment composition requires explicit consent and a documented retention/redaction policy.

Pre-1.0 qualitative gates:

- at least 8 of 10 moderated users can explain Helm's value after two minutes
- at least 8 of 10 can identify what Helm did and did not change
- at least 8 of 10 can recover from an injected manager failure
- no participant mistakes a recommendation for an already-applied action
- no action is labeled reversible unless rollback passes validation

Fleet-specific indicators:

- successful deployment/configuration across each certified management tool
- policy ingestion and validation success
- inventory/compliance output acceptance rate
- managed-policy conflict rate with controlling-authority attribution
- ring rollout and rollback completion
- offline policy continuity and expiry handling
- audit export completeness and redaction correctness

## 14. Prioritized Implementation Roadmap

### Internal initiative phase 0 — Definition closure immediately after v0.18.1

- [x] approve this charter and product/tier feature allocation
- [x] define Environment Brief, setup-session, Action Receipt, and redaction schemas (`docs/contracts/first-run/`)
- [x] define discovery/privacy classification (`docs/architecture/FIRST_RUN_EXPERIENCE_CONTRACTS.md`)
- [x] define Fleet managed-configuration contract (`docs/contracts/first-run/FLEET_BOUNDARY.md`)
- [x] prototype GUI and CLI flows (`docs/app-design/NATIVE_MACOS_PROTOTYPES.md` and `docs/contracts/first-run/CLI_TUI_CONTRACT.md`)
- [x] create first-run usability and accessibility protocols (`docs/app-design/NATIVE_MACOS_RESEARCH_VALIDATION.md`)
- [x] define local metrics events without adding telemetry transport (`docs/contracts/first-run/metrics-event.schema.json`)
- [x] bootstrap feasibility and typed-action matrix (`docs/architecture/BOOTSTRAP_FEASIBILITY_MATRIX.md`)

This completed planning/prototype closure does not reopen the released `v0.18.1` runtime scope or require another `0.18.x` publication. Production `0.19.x` first-run implementation begins from these contracts.

Artifact closure is complete. The human moderated-study checkpoint remains explicitly open: no participant session or result is claimed by these documents, and v0.20 workflow sign-off/v0.22 UI lock still require owner-run evidence.

### Internal initiative phase 1 — Base value reveal foundation in 0.19.x

- staged local scan and streaming Environment Brief
- conservative automatic defaults with disclosure
- remove mandatory manager/settings setup pages
- replace forced walkthroughs with contextual tips
- offline and partial-coverage behavior
- build the GUI flow on the native window, navigation, focus, progress, and sheet foundation defined in `docs/app-design/NATIVE_MACOS_EXPERIENCE.md`

### Internal initiative phase 2 — Verified improvement in 0.20.x

- personalized deterministic recommendations
- plan preview and one supported safe improvement
- maintain continuous plan -> execution -> verification -> recovery context in the redesigned core workflows

### Internal initiative phase 3 — Recovery and accessibility in 0.21.x

- persisted setup session
- resume/retry and bounded rollback
- before/after verification and Action Receipt
- redacted Copy Summary
- safety conflict detection
- first supported bootstrap plans
- GUI/CLI/TUI finding, plan, verification, and receipt parity where first-run interaction is supported
- full keyboard and VoiceOver completion
- locale/text-expansion validation

### Internal initiative phase 4 — Pre-1.0 validation in 0.22.x

- performance budgets and cancellation tests
- moderated first-run validation
- offline, partial-failure, interruption, and recovery-state validation
- native macOS design-quality sign-off for the production first-run flow

### 1.0 release gate

- no unprompted mutations
- no mandatory network dependency
- full keyboard and VoiceOver completion
- useful partial results under manager failure
- verified rollback for every action advertised as reversible
- first personalized result within the agreed budget
- users can accurately explain Helm's value and actions

### Post-1.0 Helm Pro

- Advanced Conflict Radar
- Personal Helm Blueprint export/import and diff
- advanced personal insights/history
- advisory-enriched recommendations aligned with the Security Advisory milestone
- optional Shared Brain enrichment without weakening local-first behavior

### Post-1.0 Helm Fleet

- managed zero-touch first run and readable managed-state receipt
- Fleet Environment/Compliance Brief
- organization baselines and desired-state drift
- signed offline policy snapshots
- PKG, managed configuration, CLI/inventory, and compliance integration kits
- certified Jamf Pro, Intune, Kandji, and Munki workflows
- ring rollout, approvals, rollback, audit, and SIEM/ticketing export

## 15. Smallest Exceptional Pre-1.0 Set

The smallest set capable of making Helm feel exceptional is:

1. Start local discovery immediately after any required legal gate.
2. Replace configuration pages with a personalized Environment Brief.
3. Apply conservative defaults and disclose them.
4. Offer one reviewed, safe, verified improvement.
5. Finish with an auditable and shareable Action Receipt.
6. Teach everything else contextually.

These capabilities belong to base Helm and form part of the 1.0 product-quality bar.
