# Whole-Workflow Research Fixtures

These fixtures provide deterministic, synthetic records for Helm's moderated native-macOS research protocol. They are documentation assets, not application resources, and are not copied into Release bundles.

## Current Dataset

`v0.20-whole-workflow-v1.json` covers all seven tasks in `docs/app-design/NATIVE_MACOS_RESEARCH_VALIDATION.md`:

1. partial ambient health with one failed verification;
2. 12 updates across all three authority stages, including pinned, authorization, restart, and excluded-OS consequences;
3. immediate cached and delayed remote `ripgrep` results, including the offline variant;
4. applied-but-unverified work, an unstarted source, recovery choices, and rollback limits;
5. active user and policy-blocked system `rustup` installations;
6. launch-at-login plus a redacted-diagnostics route back to failed work; and
7. a fresh Environment Brief, partial source failure, reviewed safe recommendation, verified Action Receipt, and strict redacted summary.

The embedded first-run objects follow the version `1.0.0` contracts in `docs/contracts/first-run/`. Their consent and action states describe simulated workflow events only. The top-level safety contract always wins: consumers must not scan the host, contact the network, or execute a mutation while this dataset is selected.

The top-level `snapshot` is a normalized record catalog, not one claim that every condition occurs simultaneously. Each scenario's `recordIds` defines its visible starting state; consumers may resolve referenced dependencies such as manager metadata, but must not leak findings or actions from another scenario into the active task.

## Loading Contract

`WholeWorkflowResearchDatasetLoader` decodes the JSON and rejects semantic drift, including unresolved task references or missing task-defining records. Debug builds can resolve an absolute fixture path from `HELM_WAYFINDER_RESEARCH_DATASET`; Release builds ignore that key. Task 1 consumes the selector through the shipping Wayfinder popover and exact failed-verification Activity route. Task 2 consumes it through the shipping Plan, inspector, and confirmation path in read-only mode. Task 3 consumes it through Library, global search, the package inspector, progressive cached/remote presentation, the offline-deferred variant selected by `HELM_WAYFINDER_RESEARCH_OFFLINE`, and a bounded read-only install confirmation. Task 4 consumes it through Activity and the contextual inspector, preserving applied-versus-verified truth, an unstarted unchanged source, before/after facts, rollback limits, ordered recovery choices, and strictly redacted synthetic diagnostics without starting service work. Task 5 consumes it through Environment and manager provenance. Task 6 consumes it through General Settings with a fixture-local launch-at-login value and through Activity for strictly redacted diagnostics. While either research fixture is active, Settings does not construct, read, or mutate the main-app or privileged-helper `SMAppService` paths and does not inspect, install, or remove the host CLI shim. Missing/invalid Task 6 data and non-Task-6 fixtures present localized unavailable truth rather than host state. Task 7 remains unprojected and must project this same corpus through its production first-run path before the owner-moderated checkpoint begins.

Never replace this file in place after research evidence cites it. Add a new versioned dataset and update the active contract instead.
