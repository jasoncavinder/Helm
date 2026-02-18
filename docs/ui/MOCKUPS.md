# Helm Annotated Mockups (Text)

## 1) Menu Bar Popover (Triage)

```text
+-------------------------------------------+
| Helm                         10:42 AM      |
| System Health: Attention                  |
| 3 updates | 1 failure | Last refresh 2m   |
+-------------------------------------------+
| [Refresh]               [Upgrade All]      |
+-------------------------------------------+
| Manager Snapshot                             
| - mise           0 outdated   Healthy      |
| - Homebrew       2 outdated   Attention    |
| - npm            1 outdated   Attention    |
| - softwareupdate 0 outdated   Healthy      |
+-------------------------------------------+
| Active Tasks                                
| - Upgrade npm eslint           Running     |
| - Refresh Homebrew             Queued      |
+-------------------------------------------+
| [Open Control Center]                       |
+-------------------------------------------+
```

Reasoning:
- Top section answers "Do I need to act now?"
- Middle section enables immediate action.
- Bottom preserves continuity to deeper workflows.

## 2) Control Center Window (Default)

```text
+----------------+-----------------------------------------+----------------------+
| OVERVIEW       | Health Posture                          | Inspector            |
| UPDATES        | -------------------------------------   | Selected: Homebrew   |
| PACKAGES       | [Attention] 3 pending updates           | Authority: Guarded   |
| TASKS          | [Failure]  1 failed task                | Capabilities: list,  |
| MANAGERS       | [Running]  2 active tasks               | outdated, upgrade... |
| SETTINGS       |                                         | [Open Diagnostics]   |
|                | Manager Health Grid                     |                      |
|                | Homebrew   Attention                    |                      |
|                | mise       Healthy                      |                      |
|                | npm        Attention                    |                      |
|                | ...                                     |                      |
+----------------+-----------------------------------------+----------------------+
```

Reasoning:
- Sidebar creates predictable operational domains.
- Main panel focuses on current domain.
- Inspector avoids unnecessary navigation churn.

## 3) Updates Section (Execution Plan)

```text
+--------------------------------------------------------------------------+
| Updates                                           [Refresh Plan]          |
| ------------------------------------------------------------------------ |
| Execution Plan                                                           |
| Stage 1 Authoritative: mise (1), rustup (0)                             |
| Stage 2 Standard: npm (1), pip (0), cargo (0), mas (0)                  |
| Stage 3 Guarded: Homebrew (2), softwareupdate (0)                        |
| ------------------------------------------------------------------------ |
| Risk Flags                                                                |
| [ ] Requires admin privileges                                             |
| [ ] Reboot may be required                                                |
| ------------------------------------------------------------------------ |
| [Dry Run]                                   [Run Upgrade Plan]            |
+--------------------------------------------------------------------------+
```

Reasoning:
- Authority order is visible before execution.
- Risk flags are explicit and non-ambiguous.
- Dry-run and execute paths are parallel, not hidden.

## 4) Package Detail Interaction

```text
+--------------------------------------------------------------------------+
| Packages                                   Search: [eslint            ]   |
| ------------------------------------------------------------------------ |
| eslint                    npm     8.56.0 -> 9.1.0      [Update] [Pin]    |
| swiftformat               Homebrew 0.53.0 -> 0.54.2    [Update] [Pin]    |
| ripgrep                   Homebrew up to date          [Reinstall]        |
| ------------------------------------------------------------------------ |
| Detail (eslint)                                                           |
| Manager: npm | Source query: "eslint" | Cached: 40s ago                  |
| Notes: Global install scope only                                         |
| [View Task History]                                                       |
+--------------------------------------------------------------------------+
```

Reasoning:
- Row actions keep single-package maintenance fast.
- Detail area preserves provenance and confidence metadata.

## 5) Error Recovery Surface

```text
+--------------------------------------------------------------------------+
| Tasks                                                                     |
| ------------------------------------------------------------------------ |
| Failed: Upgrade Homebrew formulas                                         |
| Manager: Homebrew                                                         |
| Action: upgrade                                                           |
| Time: 10:39 AM                                                            |
| ------------------------------------------------------------------------ |
| Cause Summary                                                             |
| 2 packages failed checksum verification                                   |
| ------------------------------------------------------------------------ |
| Suggested Actions                                                         |
| 1) Retry failed packages                                                  |
| 2) Open manager diagnostics                                               |
| 3) Export incident report                                                 |
| ------------------------------------------------------------------------ |
| [Retry Failed]      [Open Diagnostics]      [Export Report]               |
+--------------------------------------------------------------------------+
```

Reasoning:
- Keeps attribution and next actions in one place.
- Avoids raw log overload while preserving escalation paths.
