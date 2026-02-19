# Helm Licensing Strategy

⚠️ **Non-Binding Document**

This document describes the intended and potential licensing direction for Helm.
It is provided for planning, transparency, and contributor awareness.

It is NOT legally binding.

The only legally binding license governing Helm is defined in the root `LICENSE` file.

---

## 1. Purpose

This document exists to:

- Clarify Helm’s current licensing model
- Communicate expected future changes
- Guide contributors and AI agents
- Preserve flexibility for commercial and open-source decisions
- Avoid ambiguity between current and future licensing

---

## 2. Current Licensing (Pre-1.0)

Helm is currently released under a **source-available, non-commercial license**.

### Summary

- Source code is publicly visible
- Modification is allowed for personal use
- Commercial use is prohibited
- Redistribution of modified source code is prohibited
- Redistribution of unmodified binaries may be allowed (see LICENSE)
- The Helm name, logo, and branding are not licensed

### Goals of Current Model

The pre-1.0 licensing model is designed to:

- Enable transparency and community feedback
- Protect the project from premature commercialization
- Retain full control over future licensing decisions
- Allow rapid iteration without ecosystem fragmentation

---

## 3. Contributor Licensing Model

All contributions to Helm are governed by the Contributor License Agreement (CLA).

### Purpose of the CLA

The CLA ensures that Helm:

- Has the right to relicense contributed code
- Can offer commercial editions
- Can change licensing models in the future
- Can incorporate contributions into proprietary or enterprise versions

### Implications for Contributors

By contributing, you agree that:

- Your contributions may be used in commercial versions of Helm
- Your contributions may be relicensed in the future
- You retain copyright, but grant broad rights to the project

---

## 4. Post-1.0 Licensing Goals

After version 1.0, Helm is expected to transition to a model that:

- Allows commercial usage
- Introduces product editions (Free, Pro, Business)
- Maintains control over core intellectual property
- Supports sustainable development

### Core Principles

Helm’s licensing strategy will prioritize:

- **Sustainability** — funding ongoing development
- **User trust** — predictable and transparent licensing
- **Flexibility** — ability to evolve the model over time
- **Control** — preventing unauthorized commercial forks

---

## 5. Product and Edition Model (Planned)

Helm is being designed as two products:

- **Helm (Consumer)** with Free + Pro entitlement-gated capabilities
- **Helm Business (Fleet)** as a separate product lifecycle for organizational governance

Edition differences are planned to be enforced via runtime feature gating and entitlement verification.

---

## 6. Licensing Model Selection Status

The exact post-1.0 legal license model remains intentionally flexible.

However, distribution and entitlement architecture direction is now defined for planning:

- Consumer and fleet products have separate release lifecycles.
- Licensing authority is channel-specific.
- Update authority is channel-specific and decoupled from licensing authority.

---

## 7. Transition Plan

Helm is expected to evolve its licensing model at or after version 1.0.

### Planned Changes

- Remove non-commercial restriction
- Introduce product licensing (Free / Pro / Business)
- Define commercial usage terms
- Maintain or revise source availability

### Transition Strategy

- Licensing changes will be announced in advance
- Existing users will not lose access to functionality they already have
- Versioned licensing may be used (e.g., different versions under different terms)

---

## 8. Binary Distribution

Planned distribution matrix:

| Artifact | Product | Channel | Licensing Authority | Update Authority |
|---|---|---|---|---|
| Helm (MAS build) | Helm (Consumer) | Mac App Store | App Store commerce/receipt model | Mac App Store |
| Helm (Developer ID build) | Helm (Consumer) | Direct DMG, Homebrew, MacPorts | Helm consumer entitlement model | Sparkle (direct channel only) |
| Helm (Setapp build) | Helm (Consumer) | Setapp | Setapp subscription/license model | Setapp |
| Helm Business (Fleet build) | Helm Business | Enterprise PKG deployment | Offline organizational license files | Admin-controlled PKG/MDM rollout |

Policy direction:

- Official binaries are distributed by the Helm project and approved channel partners.
- Sparkle is planned only for the direct Developer ID consumer build.
- Sparkle is not planned for MAS, Setapp, or Helm Business fleet builds.
- Homebrew and MacPorts are planned to redistribute the same Developer ID consumer binary.

---

## 9. Trademark and Branding

The Helm name, logo, and associated branding are not covered by the software license.

### Restrictions

You may not:

- Use the Helm name for derivative projects
- Redistribute modified versions under the Helm name
- Represent modified software as official Helm releases

Trademark protections may be formalized in the future.

---

## 10. AI Agent Guidance

AI agents working on this repository should:

- Treat the root `LICENSE` file as authoritative
- Treat this document as informational only
- Avoid introducing licensing changes without explicit instruction
- Preserve licensing headers in all files
- Ensure new contributions comply with the CLA

---

## 11. Future Evolution

This document will evolve alongside Helm.

Possible future additions include:

- Formal commercial license terms
- Enterprise licensing agreements
- Open-source component boundaries
- API licensing terms
- Marketplace or plugin licensing

---

## 12. Summary

Helm is currently:

- Source-available
- Non-commercial
- Fully controlled by the project owner

Helm is expected to become:

- Commercially available
- Multi-product (Helm Consumer + Helm Business fleet)
- Multi-edition where applicable (Free / Pro for consumer)
- Sustainable and professionally supported

---

For legal terms, see the root `LICENSE` file.
