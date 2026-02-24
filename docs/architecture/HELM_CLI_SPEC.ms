# Helm CLI Specification (Draft)

Status: Draft 0.2  
Last Updated: 2026-02-24  
Owner: Helm Core

---

## 1. Purpose

Define the command-line and terminal-UI companion for Helm with full operational parity targets against the macOS GUI, while reusing the same core orchestration/runtime logic.

This document is a contract-first specification. It clarifies identifiers, JSON contracts, non-interactive behavior, and parity safety for the current read-only implementation slice.

---

## 2. Product Scope

### 2.1 Primary Goals

- Deliver a first-party `helm` binary for terminal users.
- Achieve feature parity with current GUI capabilities over phased delivery.
- Preserve Helm architectural invariants:
  - all mutating operations execute as tasks
  - manager authority ordering is enforced
  - per-manager task serialization is enforced
  - cancellation is process-level where possible
  - no shell command-string construction
- Keep GUI and CLI behavior consistent via one shared coordinator contract.

### 2.2 Out of Scope (Initial CLI Release)

- Third-party command/plugin extension system.
- Cloud-managed control plane behavior.
- Non-macOS platform support.

---

## 3. Naming and Distribution

- Executable name: `helm`
- Distribution package labels may use `helm-cli` (for example Homebrew/MacPorts formula naming).
- User-facing invocation remains `helm`.
- Distribution profile and provenance source of truth:
  - `docs/architecture/BUILD_VARIANTS.md`
  - machine-readable contract: `docs/contracts/distribution-profiles.json`

---

## 4. Invocation and Non-Interactive Behavior

### 4.1 No-Argument Behavior

- If stdout is a TTY:
  - launch full-screen TUI.
- If stdout is not a TTY:
  - print help and exit `0`.

### 4.2 Version and Help

- `helm -V`
- `helm --version`
- `helm help`
- `helm <command> help`

### 4.3 Non-Interactive Clarifications

- `--json` is valid in both TTY and non-TTY contexts.
- JSON mode emits machine-oriented payloads only (no table formatting, no prose status text in `data`).
- `--ndjson` emits newline-delimited JSON envelopes:
  - scalar/object payloads emit one line.
  - list payloads emit one line per list item.
  - empty list payloads emit one line with `data: []` (never zero lines).
- Future mutation commands that require confirmation must be explicit in non-interactive mode:
  - fail fast without required confirmation flags (for example `--yes`) rather than prompting.

### 4.4 CLI First-Run Onboarding

- CLI maintains first-run onboarding state in Helm settings storage.
- Required state gates:
  - accepted CLI license terms version
  - CLI onboarding completed flag
- Gate behavior:
  - commands other than `help`, `--version`, `completion`, and `onboarding` are blocked until first-run onboarding is complete.
  - interactive TTY flow is terminal/menu based (not TUI).
  - once onboarding completes, the original command continues in the same invocation.
- Script/automation flags:
  - `--accept-license`
  - `--accept-defaults`
  - both flags are first-run aids and are ignored after onboarding is complete.
- Machine mode:
  - if onboarding/license is still required and invocation uses `--json`/`--ndjson`, command fails with a descriptive JSON error payload.

---

## 5. Top-Level Command Surface

Baseline top-level commands:

- `helm` (TUI in TTY, help in non-TTY)
- `helm status`
- `helm refresh`
- `helm search`
- `helm ls` (alias for `helm packages list`)
- `helm updates`
- `helm packages`
- `helm tasks`
- `helm managers`
- `helm settings`
- `helm diagnostics`
- `helm onboarding`
- `helm self`
- `helm completion`
- `helm help`

Ergonomic aliases:

- `helm ls` -> `helm packages list`
- `helm up` -> `helm updates list`
- `helm ps` -> `helm tasks list`
- `helm mgr` -> `helm managers`
- `helm config` -> `helm settings`

---

## 6. Command Tree (Normative Draft)

```text
helm
  status
  refresh [--manager <id>|--all]
  search <query> [--manager <id>] [--remote]

  packages
    list [--manager <id>] [--status installed|upgradable|available|pinned]
    search <query> [--manager <id>] [--local|--remote]
    show <name> [--manager <id>]
    install <name> --manager <id>
    uninstall <name> --manager <id>
    upgrade <name> --manager <id>
    pin <name> --manager <id> [--version <v>]
    unpin <name> --manager <id>

  updates
    list [--manager <id>]
    summary
    preview [--include-pinned] [--allow-os-updates]
    run [--include-pinned] [--allow-os-updates] [--yes]

  tasks
    list [--status queued|running|completed|failed|cancelled]
    show <task-id>
    logs <task-id> [--level info|warn|error] [--status ...]
    output <task-id> [--stream]
    follow <task-id>
    cancel <task-id>

  managers
    list
    show <manager-id>
    detect [<manager-id>|--all]
    enable <manager-id>
    disable <manager-id>
    install <manager-id>
    update <manager-id>
    uninstall <manager-id>
    executables list <manager-id>
    executables set <manager-id> <path|path-default>
    install-methods list <manager-id>
    install-methods set <manager-id> <method-id>
    priority list
    priority set <manager-id> --rank <n>
    priority reset

  settings
    list
    get <key>
    set <key> <value>
    reset <key>

  diagnostics
    summary
    task <task-id>
    manager <manager-id>
    export [--json|--path <file>]

  onboarding
    status
    run
    reset

  self
    check
    status
    update
    auto-check enable
    auto-check disable
    auto-check frequency <minutes>

  completion [bash|zsh|fish]
  help [command]
```

---

## 7. Identifier Contracts

### 7.1 `ManagerId`

- Type: stable string identifier.
- Canonical examples: `mise`, `macports`, `rubygems`, `homebrew_formula`, `xcode_command_line_tools`.
- Case handling: case-sensitive.
- Canonical source of truth:
  - `core/rust/crates/helm-core/src/models/manager.rs`:
    - `ManagerId::as_str()`
    - `impl FromStr for ManagerId`
- CLI/API contract:
  - only canonical values are accepted.
  - display names are not accepted as identifiers.

Manager policy note:

- Detection and manageability are separate contracts.
- Base-system language-manager executables (currently RubyGems/Bundler/pip system paths) are detectable/selectable but policy-ineligible for enablement and mutating actions.
- `helm managers enable <manager-id>` must fail deterministically while policy ineligibility is true.

### 7.2 `PackageRef`

Core model:

- Canonical structure in core:
  - `name` (`String`, required)
  - `manager` (`ManagerId`, required in core type)
- Canonical source:
  - `core/rust/crates/helm-core/src/models/package.rs`

CLI selector contract (for package-targeting commands):

- Logical selector fields:
  - `name` (required)
  - `manager` (optional unless ambiguous)
  - `version` (optional for commands that support explicit version targeting)
- Recommended shorthand selector format:
  - `<name>@<manager_id>`
  - example: `ripgrep@homebrew_formula`

Ambiguity behavior:

- If multiple managers provide the same package name and manager is omitted:
  - command must fail with an explicit ambiguity error and list candidate manager IDs.
- If manager is specified but not detected/installed:
  - command must fail with an explicit manager-availability error.

### 7.3 `TaskId`

- Type: structured numeric identifier (`u64` in core, wrapped by `TaskId(pub u64)`).
- Format in CLI arguments: base-10 unsigned integer string.
- Scope/stability:
  - stable within one Helm state store (`HELM_DB_PATH` / per-user database).
  - not globally unique across users or databases.
- JSON serialization:
  - numeric value, not string.
- Canonical source:
  - `core/rust/crates/helm-core/src/models/task.rs`

---

## 8. Shared Coordinator Architecture (GUI + CLI)

### 8.1 Decision

GUI and CLI target one shared per-user coordinator to avoid split-brain orchestration.

### 8.2 Coordinator Responsibilities

- Own task submission and scheduling.
- Enforce per-manager serialization and authority phase rules.
- Enforce manager enablement policy.
- Enforce task deduplication/reuse behavior.
- Provide shared task/history/diagnostics views.
- Centralize cancellation and process lifecycle control.

### 8.3 Discovery and Launch-On-Demand Contract

- CLI attempts coordinator discovery via local per-user service boundary (contract target).
- Coordinator is expected to be launch-on-demand.
- Always-on daemon is not required for initial CLI delivery.

### 8.4 Unavailable Coordinator Behavior

- Read-only commands may operate without coordinator using persisted state.
- Mutating/running-task commands must surface explicit availability errors if coordinator routing is required but unavailable.

### 8.5 Version Compatibility Contract

- CLI and coordinator are expected to be same release version for full parity.
- Minor-version mismatches may support read-only fallback only.
- Incompatible coordinator versions must fail with explicit compatibility errors.

### 8.6 Current Implementation Note (2026-02-23)

- Current read-only commands run without coordinator and read state directly from SQLite.
- Current launch-on-demand shared CLI coordinator is active for single-task mutation flows:
  - `packages install|uninstall|upgrade`
  - `managers detect <id>`
  - `managers install|update|uninstall`
  - `tasks cancel`
- Multi-step workflow detach is now coordinator-backed with workflow job IDs:
  - `refresh --detach`
  - `updates run --detach --yes`
  - `managers detect --all --detach`
- Manager disable now performs best-effort cancellation of queued/running tasks for that manager via coordinator task-cancel requests.
- GUI service now initializes a coordinator-compatible transport bridge and local coordinator host so GUI and CLI task submissions converge on one per-user authority.
- When a coordinator is already active (for example CLI-first startup), GUI mutation/cancellation paths now route through that external coordinator instead of launching an independent task authority.

---

## 9. Task and Mutation Semantics

- All mutating commands produce task records.
- Default mutation mode: `--wait`.
- Optional mutation mode: `--detach`:
  - single-task mutation commands return `task_id`.
  - multi-step workflow commands return `job_id`.
- Task status model: `queued`, `running`, `completed`, `failed`, `cancelled`.
- Cancellation must persist cancelled task state and terminate process execution where supported.

### 9.1 Exit Code Contract

Target contract:

- `0`: success (including successful read-only commands and accepted detached task submissions).
- `1`: usage/runtime error.
- `2`: task failed.
- `3`: partial failure.
- `4`: cancelled task.

Current implementation note (no behavior change in this pass):

- Read-only commands return `0` on success.
- CLI now maps task-oriented failures/cancellations to granular exit codes:
  - task failed => `2`
  - partial multi-manager failure => `3`
  - cancelled task => `4`
  - usage/runtime errors remain `1`
- key mutation/workflow failure paths now attach explicit exit-code markers internally; string-based exit-code heuristics are removed and untyped errors deterministically resolve to `1`.

---

## 10. Output Contract

### 10.1 JSON Envelope

All documented machine-mode payloads use this top-level shape:

```json
{
  "schema": "helm.cli.v1.<command>",
  "schema_version": 1,
  "generated_at": 1771807771,
  "data": {}
}
```

Rules:

- `schema`: stable schema identifier.
- `schema_version`: integer schema version.
- `generated_at`: Unix epoch seconds (UTC).
- `data`: command-specific payload object.

### 10.2 Read-Only JSON Schemas (Current)

#### 10.2.1 `helm status --json`

Schema: `helm.cli.v1.status`

```json
{
  "schema": "helm.cli.v1.status",
  "schema_version": 1,
  "generated_at": 1771807771,
  "data": {
    "installed_packages": 0,
    "updatable_packages": 0,
    "running_or_queued_tasks": 0,
    "enabled_managers": 25,
    "detected_enabled_managers": 0
  }
}
```

#### 10.2.2 `helm managers list --json`

Schema: `helm.cli.v1.managers.list`

```json
{
  "schema": "helm.cli.v1.managers.list",
  "schema_version": 1,
  "generated_at": 1771807771,
  "data": {
    "managers": [
      {
        "managerId": "npm",
        "displayName": "npm",
        "authority": "standard",
        "detected": true,
        "version": "11.10.1",
        "executablePath": "/.../npm",
        "enabled": true,
        "isImplemented": true,
        "isOptional": false,
        "isDetectionOnly": false,
        "supportsRemoteSearch": true,
        "supportsPackageInstall": true,
        "supportsPackageUninstall": true,
        "supportsPackageUpgrade": true,
        "selectedExecutablePath": null,
        "selectedInstallMethod": null,
        "isEligible": true,
        "ineligibleReasonCode": null,
        "ineligibleReasonMessage": null
      }
    ]
  }
}
```

#### 10.2.3 `helm packages list --json`

Schema: `helm.cli.v1.packages.list`

```json
{
  "schema": "helm.cli.v1.packages.list",
  "schema_version": 1,
  "generated_at": 1771807772,
  "data": {
    "packages": [
      {
        "package": {
          "manager": "npm",
          "name": "typescript"
        },
        "installed_version": "5.6.2",
        "pinned": false
      }
    ]
  }
}
```

#### 10.2.4 `helm updates summary --json`

Schema: `helm.cli.v1.updates.summary`

```json
{
  "schema": "helm.cli.v1.updates.summary",
  "schema_version": 1,
  "generated_at": 1771807779,
  "data": {
    "total_updates": 12,
    "pinned_updates": 2,
    "restart_required_updates": 1,
    "by_manager": {
      "homebrew_formula": 6,
      "npm": 4,
      "pip": 2
    }
  }
}
```

#### 10.2.5 `helm tasks show <task-id> --json`

Schema: `helm.cli.v1.tasks.show`

```json
{
  "schema": "helm.cli.v1.tasks.show",
  "schema_version": 1,
  "generated_at": 1771807793,
  "data": {
    "task": {
      "id": 42,
      "manager": "npm",
      "taskType": "detection",
      "status": "completed",
      "createdAtUnix": 1771807787
    }
  }
}
```

---

## 11. Global Flags (Initial Set)

- `--json`
- `--ndjson`
- `-v`, `--verbose`
- `-q`, `--quiet`
- `--no-color`
- `--locale <id>`
- `--timeout <seconds>`
- `--yes`
- `--wait`
- `--detach`

Current implementation note:

- Implemented now: `--json`, `--ndjson`, `-v|--verbose`, `-q|--quiet`, `--no-color`, `--locale <id>`, `--timeout <seconds>`, `--wait`, `--detach`.
- `--timeout` drives coordinator request timeouts.
- `--version` human output remains semver-only (`D9`); build profile/channel are exposed via JSON (`helm --json --version`) and verbose diagnostics.
- `--no-color` is contract-stable but currently a no-op for most commands because default human output is plain text without ANSI styling.

Flag conflicts:

- `--wait` and `--detach` are mutually exclusive.
- `--quiet` and `--verbose` are mutually exclusive.
- verbose diagnostics should be emitted to `stderr` so `stdout` remains contract-safe for human tables and JSON envelopes.

---

## 12. TUI Specification

### 12.1 Entry

- TTY no-arg launch enters TUI.
- Non-TTY invocation does not enter TUI.

### 12.2 Sections

The TUI primary sections mirror GUI sections:

- Updates
- Packages
- Tasks
- Managers
- Settings
- Diagnostics

### 12.3 Behavior

- TUI operations dispatch the same task intents as non-interactive CLI commands.
- Task status and output update in near real-time.
- Filter/search/scope controls map to the same manager/package/task state as GUI.

### 12.4 Library Direction

- `ratatui` + `crossterm` is the preferred implementation direction.

---

## 13. Update Semantics

### 13.1 Package Updates

- `helm updates ...` is scoped to package-manager package updates only.

### 13.2 Helm Self-Update

- `helm self ...` namespace handles Helm app/CLI update operations.
- `helm self update` is explicit user action.
- Policy is provenance-driven:
  - marker file: `~/.config/helm/install.json`
  - fallback path heuristics when marker is missing
  - `update_policy` controls whether self-update is allowed (`self`) or channel-managed (`channel` / `managed` / `none`)

### 13.3 Automatic Check for Updates

Initial model:

- GUI checks while app is running and when due.
- CLI performs opportunistic due checks on command invocation.
- No mandatory always-on background process in initial release.

Current persisted settings keys:

- `auto_check_for_updates` (`bool`)
- `auto_check_frequency_minutes` (`u32`)

Planned extension key:

- `updates.last_checked_at` (`timestamp`)

---

## 14. Security and Policy

- Preserve structured process invocation (no shell command-string construction).
- Preserve safe mode and guarded-operation policy checks.
- Guarded update operations require explicit opt-in flags (for example `--allow-os-updates`) and/or confirmation prompts.
- Error payloads retain attribution:
  - manager
  - task
  - action
  - code

---

## 15. Localization

- Human-facing CLI/TUI messages use localization keys and locale resolution policy.
- Debug/trace logs remain English.
- JSON payload keys remain stable English identifiers.

---

## 16. Compatibility and Versioning

- CLI semantic version follows Helm release version.
- JSON schema changes should be additive when possible.
- Breaking JSON changes require schema version bump and changelog note.

---

## 17. Implementation Phases

Phase 1:

- Command parser and read-only command scaffold with `--json`.
- Current delivered read-only/inspection slice includes:
  - `status`
  - `packages list`
  - `updates list|summary`
  - `tasks list|show`
  - `managers list|show`
  - `settings list|get`
  - `search` (local cache)
  - `diagnostics summary|task|manager|export` inspection/export command surface
  - `completion bash|zsh|fish` shell completion-script generation
  - JSON envelope consistency (`schema`, `schema_version`, `generated_at`, `data`) for read-only command outputs, including `search`, `managers show`, `settings list`, and `settings get`
  - `updates` default dispatch safety: `helm updates` and `helm updates --json` route to list behavior without panic when no explicit subcommand is provided
  - read-only list row-capping with `--limit` for `packages list`/`ls`, `updates list` (including top-level `updates` flag-form), and `tasks list` (including top-level `tasks` flag-form)
  - `tasks list --status` filtering (`queued|running|completed|failed|cancelled`)

Phase 2:

- Mutating command coverage for package/manager/task operations with task wait/detach behavior.
- Current delivered slice:
  - command-scoped help across top-level and nested command namespaces (`helm <command> help`, `helm <command> <subcommand> help`, and `helm help <command> [subcommand]`)
  - `updates preview` and `updates run --yes` command surface for cached-snapshot plan/run flows (`--include-pinned`, `--allow-os-updates`)
  - `packages install|uninstall|upgrade|pin|unpin` with explicit manager targeting (`--manager` or `name@manager`)
  - `packages show` ambiguity-safe manager resolution behavior
  - `managers install|update|uninstall` for currently supported manager lifecycle targets with method-aware routing
  - multi-step detach workflow coverage (`refresh`, `updates run`, `managers detect --all`) with coordinator workflow job IDs
  - `packages pin|unpin --detach` acceptance
  - `settings set|reset` for implemented mutable settings keys
  - mutation JSON payload hardening so mutation outputs consistently use the standard envelope (`schema`, `schema_version`, `generated_at`, `data`)
  - `tasks logs`, `tasks output`, and `tasks follow` inspection commands (`follow` currently uses a timeout guard to avoid indefinite non-terminal polling)
  - `tasks cancel` now routes through the shared CLI coordinator
  - `managers disable` now performs best-effort cancellation of manager-scoped queued/running tasks via coordinator cancellation requests

Phase 3:

- Manager executable/install-method selection commands and manager-priority commands.
- Current delivered slice:
  - `managers executables list|set`
  - `managers install-methods list|set`
  - `managers priority list|set|reset` with persisted override ordering

Phase 4:

- TUI shell and section workflows.

Phase 5:

- `self` and automatic-check policy commands/settings persistence.
- Current delivered slice:
  - `self status`, `self check`, and `self update` are implemented for provenance-aware channels (`direct-script`, `brew`, `macports`, `cargo`, `app-bundle-shim`, `managed`, `unknown`)
  - `self auto-check status|enable|disable|frequency <minutes>` now maps to persisted auto-check settings
  - `auto_check_for_updates` and `auto_check_frequency_minutes` settings persist via `settings get|set|reset` and are reflected in `settings list` / `self status`
  - `auto_check_last_checked_unix` now persists scheduled check execution timestamps and is exposed in `settings list` / `self status` / `self auto-check status`
  - unsupported channel-managed installs return explicit recommended upgrade actions (for example `brew upgrade helm-cli`, `cargo install --locked helm-cli`, managed-policy guidance)
  - coordinator hosts now run due-based background auto-check ticks on persisted cadence

---

## 18. Validation Requirements

- Unit tests:
  - argument parsing
  - output rendering/serialization
  - error and exit code mapping
- Integration tests:
  - task submission lifecycle
  - cancellation behavior
  - manager enablement policy enforcement
  - authority-ordered update execution
  - shared coordinator conflict scenarios (GUI + CLI concurrent actions)
- End-to-end tests:
  - TUI section workflows
  - parity checks against GUI state snapshots

---

## 19. Current Open Items

- Confirm exact default for `updates run` confirmation behavior in non-interactive contexts.
- Confirm final `refresh` intent naming and behavior once shared coordinator routing is in place.
- Confirm packaging/signing policy for standalone CLI artifacts that must interact with coordinator under code-signing enforcement.
- GUI↔CLI non-TUI parity gaps pending:
  - progressive remote search orchestration parity (GUI currently debounces/cancels remote search; CLI search is local-cache only)
  - launch-at-login setting parity (GUI supports toggle; CLI settings surface does not yet expose it)
  - per-package Homebrew keg-policy parity (GUI supports package-level override; CLI currently exposes only global cleanup policy)
  - manager-scoped bulk-upgrade parity (`Upgrade All` per manager exists in GUI; CLI bulk run is global plan execution)
  - GUI-only onboarding/walkthrough/support flows remain intentionally GUI-scoped unless explicit CLI parity is requested

---

## Appendix A: Read-Only Parity Matrix

| Feature Area | GUI Capability | CLI Command(s) | JSON Support (Y/N) | Coordinator Required (Y/N) | Tests Required (Y/N) |
|---|---|---|---|---|---|
| Snapshot Overview | Overview cards (installed/updatable/tasks/managers) | `helm status` | Y | N | Y |
| Manager Inventory | Managers list with enablement/detection metadata | `helm managers list` | Y | N | Y |
| Manager Details | Inspector details for one manager | `helm managers show <manager-id>` | Y | N | Y |
| Installed Packages | Packages table/list | `helm packages list`, `helm ls` | Y | N | Y |
| Update Inventory | Upgradable packages view | `helm updates list` | Y | N | Y |
| Update Rollup | Control-center aggregate update counts | `helm updates summary` | Y | N | Y |
| Task List | Tasks table/list | `helm tasks list` | Y | N | Y |
| Task Details | Inspector details for selected task | `helm tasks show <task-id>` | Y | N | Y |
| Local Search | Cached package search | `helm search <query>`, `helm packages search <query>` | Y | N | Y |
| Settings Read | Settings view read path | `helm settings list`, `helm settings get <key>` | Y | N | Y |
