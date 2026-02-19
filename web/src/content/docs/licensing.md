---
title: Licensing
description: Helm's licensing strategy and future plans.
---

Helm is currently distributed under a **source-available, non-commercial license**.

This page outlines the current licensing model and our plans for the future.

## Current License (Pre-1.0)

Helm is free to use for personal, non-commercial purposes.

- **Source Code**: Publicly visible for transparency and evaluation.
- **Personal Use**: Permitted for individual users.
- **Commercial Use**: Not permitted at this time.
- **Redistribution**: Unmodified binaries may be redistributed for non-commercial use.
- **Modification**: Modification and reuse of source code is not permitted.

See the [LICENSE](https://github.com/jasoncavinder/Helm/blob/main/LICENSE) file for the full legal text.

## Future Strategy (Post-1.0)

We intend to transition to a sustainable commercial model at or after version 1.0. The goal is to support ongoing development while maintaining transparency.

### Planned Product Model

We are designing Helm as two products:

- **Helm (Consumer)**: Free + Pro feature-gated editions.
- **Helm Business (Fleet)**: Separate binary for policy, compliance, and managed enterprise workflows.

### Planned Distribution and Licensing Architecture (Future State)

| Artifact | Product | Channel | Licensing Authority | Update Authority |
|---|---|---|---|---|
| Helm (MAS build) | Helm (Consumer) | Mac App Store | App Store commerce/receipt model | Mac App Store |
| Helm (Developer ID build) | Helm (Consumer) | Direct DMG, Homebrew, MacPorts | Helm consumer entitlement model | Sparkle (direct channel) |
| Helm (Setapp build) | Helm (Consumer) | Setapp | Setapp subscription/license model | Setapp |
| Helm Business (Fleet build) | Helm Business | Enterprise PKG deployment | Offline org license files | Admin-controlled PKG/MDM rollout |

Channel rules:

- Sparkle is planned only for the direct Developer ID consumer build.
- Sparkle is not planned for MAS, Setapp, or Helm Business fleet builds.
- Update transport and licensing authority remain decoupled by channel.
- This architecture is planned and staged; it is not fully implemented yet.

### Why not Open Source?

Helm is currently source-available to protect the project during its early development phase. We may explore open-source models (like Open Core) in the future, but our priority is building a sustainable, high-quality product.

## Contributing

We welcome contributions! To ensure we can evolve the licensing model in the future, all contributors must sign a [Contributor License Agreement 
(CLA)](https://github.com/jasoncavinder/Helm/blob/main/docs/legal/CLA.md).

This agreement grants the project the necessary rights to relicense contributions as part of commercial or open-source releases.
