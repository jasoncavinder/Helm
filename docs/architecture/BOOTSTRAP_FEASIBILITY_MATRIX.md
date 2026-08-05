# Bootstrap Feasibility and Typed-Action Matrix

This document audits the current manager lifecycle planners and typed repair/action registry to establish what is feasible for `0.19.x` first-run planning.

*Rule: Do not claim support that is not present in current code.*

## Manager Matrix

| Manager | Detected/Installed | Installation Methods | Privilege Reqs | Network Reqs | Typed Action Exists? | Verification Method | Rollback/Recovery | Safe for 0.19 Planning |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Homebrew** | Yes | `system_curl` | `sudo` (initial only) | Yes | Yes | Binary exists, `brew doctor` | Recovery only | **Yes** |
| **mise** | Yes | `system_curl` | User local | Yes | No (Deferred) | Binary exists | Recovery only | No (Deferred) |
| **rustup** | Yes | `system_curl` | User local | Yes | No | Binary exists, `rustc --version` | Recovery only | No |
| **asdf** | Yes | Git clone | User local | Yes | No | Binary exists | Recovery only | No |
| **mas** | Yes | `brew install mas` | User local | Yes | No | Binary exists | Recovery only | No |
| **Xcode CLI Tools** | Yes | `softwareupdate` | `sudo` | Yes | Yes (Guarded/Status) | `xcode-select -p` | Recovery only | **Yes** |
| **MacPorts** | Yes | PKG Installer | `sudo` | Yes | No | Binary exists, `port version` | Recovery only | No |

## Analysis and Constraints

1. **Homebrew**: Fully supported. Provenance is established via the official GitHub install script. Typed actions exist for basic lifecycle. Safe for `0.19.x` first-run.
2. **Xcode Command Line Tools**: Fully supported as a guarded/status manager. Provenance via Apple `softwareupdate`. Safe for `0.19.x` first-run.
3. **mise, rustup, asdf, mas, MacPorts**: Detection is supported, but typed lifecycle actions (install, repair) are not yet implemented in the Rust registry. Explicitly deferred for initial `0.19.x` planning to prevent arbitrary shell execution.

## Verification & Rollback
- All supported managers currently rely on forward-recovery (e.g., clearing broken caches, re-linking) rather than strict rollback. 
- Honest rollback is architecturally limited by the third-party package managers themselves.
