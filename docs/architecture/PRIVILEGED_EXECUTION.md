# Privileged Execution Contract

## Purpose

Helm needs administrator authority for a small set of manager operations without turning the app or its background service into a general root command runner.

This contract defines the boundary between ordinary process execution and a future native macOS privileged helper. It does not redesign updater planning, manager authority, or confirmation policy.

## Current Checkpoint

The core process request now carries both:

- `requires_elevation`, retained temporarily for current executor compatibility
- exactly one typed `PrivilegedOperation` whenever elevation is required

Validation fails closed when those fields disagree. The current `sudo -A` askpass path remains the fallback until the signed helper, Service Management registration UX, and release packaging land together.

Current implementation also keeps Mac App Store mutations on the existing askpass path even if a trusted privileged executor is configured, because the MAS constraint below is unresolved.

When the embedded service programmatically configures a trusted absolute executor, the process executor forwards a structured invocation:

```text
<executor> --operation <stable-operation-id> --program <resolved-program> -- <arguments...>
```

The executor path must be a regular executable, must not be a symlink, must be owned by root or the current user, and must not be group/world writable. Configuration is process-local and one-time; it is not accepted from the inherited environment or exposed as a user-facing setting.

## Stable Operations

The initial operation namespace is deliberately finite:

| Operation family | Stable identifiers |
|---|---|
| Mac App Store | `mac_app_store.install`, `mac_app_store.get`, `mac_app_store.uninstall`, `mac_app_store.upgrade` |
| Apple system updates | `software_update.install_all`, `xcode_command_line_tools.update`, `rosetta.install` |
| MacPorts | `macports.install`, `macports.uninstall`, `macports.upgrade`, `macports.manager_uninstall`, `macports.delete_account`, `macports.delete_group`, `macports.remove_files` |

Adding an operation requires adapter request-shape tests, helper-side allowlist validation, and documentation review. A generic shell, script, executable, or argument passthrough operation is prohibited.

## Target Native Architecture

The Developer ID app will use a notarized launch daemon registered with `SMAppService.daemon(plistName:)`. Apple requires launch-daemon approval and exposes approval state through `SMAppService.Status`; Helm must explain the need before registration and route the user to Login Items when approval is required.

The target boundary is:

1. The sandboxed app presents intent, selection, and native approval status.
2. The existing embedded Helm service submits only typed operations produced by core policy.
3. The root helper authenticates the caller by code-signing requirement and audit/process identity.
4. The helper independently validates the operation, executable, arguments, environment, and any filesystem targets.
5. Output, cancellation, timeout, and terminal status remain visible through the existing task model.

The helper must never trust a command merely because the app or service supplied it. Request validation is duplicated at the privilege boundary by design.

Apple references:

- [Service Management](https://developer.apple.com/documentation/servicemanagement/)
- [Updating helper executables from earlier versions of macOS](https://developer.apple.com/documentation/servicemanagement/updating-helper-executables-from-earlier-versions-of-macos)
- [Authorization Services](https://developer.apple.com/documentation/security/authorization-services)

## Trust Rules

- The helper accepts only Helm's designated signed caller, not every process signed by the same team.
- The helper never invokes a shell or concatenates a command string.
- The helper uses fixed absolute Apple tool paths for `softwareupdate` and related system operations.
- MacPorts mutation requires a canonical, root-owned, non-writable MacPorts installation before helper execution.
- Request environment variables are not forwarded wholesale into a root process.
- Cancellation or caller invalidation terminates the associated privileged child.
- Registration, approval, unavailable-helper, denied-operation, and cancellation outcomes are explicit task failures or user-action states, never silent fallback.

## Mac App Store Constraint

The Homebrew-installed `mas` executable is commonly user-owned and unsigned. A persistent root daemon must not execute an arbitrary current-user-writable `mas` path, because replacement of that file would become a local privilege-escalation path.

Before native helper routing can replace the existing MAS elevation path, implementation must choose and validate one of these bounded approaches:

- ship and sign a reviewed App Store operation component as part of Helm
- split unprivileged `mas` discovery/download work from narrowly scoped, fixed-system-tool installation work
- keep MAS mutation interactive and route the user to the App Store when no safe automated boundary is available

The helper must not solve this constraint by broadly allowlisting Homebrew paths or by running a user-selected executable as root.

## Distribution And Release Rules

- Developer ID: the launch daemon must be embedded, signed with the app, notarized, registered through Service Management, and verified in the release DMG.
- Mac App Store: helper availability and entitlement policy require a separate channel decision; Developer ID assumptions must not leak into the MAS profile.
- Setapp and Fleet: registration and update authority remain channel-specific. Fleet may later manage helper approval through deployment policy, but cannot weaken the operation allowlist.
- Unsigned development builds may test request validation and helper protocol behavior, but cannot be treated as evidence that launch-daemon registration, approval, signing, or notarization works.

## Completion Gates

Native privileged execution is not complete until all of the following pass:

- helper target, launch-daemon plist, and app embedding are present
- caller validation and operation allowlist tests pass
- app registration/approval UX handles every `SMAppService.Status`
- elevated output, cancellation, timeout, and relaunch behavior pass
- MAS has a bounded execution design rather than a broad user-writable executable exception
- release packaging verifies the nested helper signature and launch-daemon plist
- a signed/notarized installed build passes approval, denial, success, cancellation, and update-replacement QA
