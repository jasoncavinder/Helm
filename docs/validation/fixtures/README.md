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

`WholeWorkflowResearchDatasetLoader` decodes the JSON and rejects semantic drift, including unresolved task references or missing task-defining records. Debug builds can resolve an absolute fixture path from `HELM_WAYFINDER_RESEARCH_DATASET`; Release builds ignore that key. No production surface consumes the selector yet. Each v0.20 workflow slice must project this same corpus through its production presentation path before the owner-moderated checkpoint begins.

Never replace this file in place after research evidence cites it. Add a new versioned dataset and update the active contract instead.
