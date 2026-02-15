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
- Introduces product editions (Free, Pro, Enterprise)
- Maintains control over core intellectual property
- Supports sustainable development

### Core Principles

Helm’s licensing strategy will prioritize:

- **Sustainability** — funding ongoing development
- **User trust** — predictable and transparent licensing
- **Flexibility** — ability to evolve the model over time
- **Control** — preventing unauthorized commercial forks

---

## 5. Product Editions (Planned)

Helm is being designed as a **multi-edition product**, not just a codebase.

### Planned Editions

#### Free Edition

- Core functionality
- Local device management
- Basic update orchestration
- No organizational features

#### Pro Edition

- Advanced workflows
- Automation features
- Enhanced UI/UX
- Extended integrations

#### Enterprise Edition

- Policy enforcement
- Organization-wide management
- Compliance and audit features
- Managed environments
- Role-based access

### Implementation

Edition differences will be enforced via:

- Feature gating
- Entitlement systems
- Configuration policies

---

## 6. Potential Licensing Models (Under Consideration)

Helm has not yet committed to a final post-1.0 licensing model.

The following approaches are under consideration:

---

### Option A — Proprietary Source-Available

- Source remains visible
- Commercial use requires a paid license
- Redistribution is restricted

**Pros:**
- Maximum control
- Strong monetization

**Cons:**
- Not open source
- Lower community adoption

---

### Option B — Open Core

- Core functionality is open source (permissive license)
- Advanced features remain proprietary

**Pros:**
- Community adoption
- Commercial viability

**Cons:**
- Increased maintenance complexity
- Potential fragmentation

---

### Option C — Full Open Source + Commercial Add-Ons

- Entire codebase open source
- Revenue via hosted services, enterprise tools, or support

**Pros:**
- Maximum adoption
- Strong community contributions

**Cons:**
- Weaker control over monetization
- Risk of competitive forks

---

### Current Position

Helm has **not yet selected a final model**.

The architecture is being designed to support multiple possible outcomes.

---

## 7. Transition Plan

Helm is expected to evolve its licensing model at or after version 1.0.

### Planned Changes

- Remove non-commercial restriction
- Introduce product licensing (Free / Pro / Enterprise)
- Define commercial usage terms
- Maintain or revise source availability

### Transition Strategy

- Licensing changes will be announced in advance
- Existing users will not lose access to functionality they already have
- Versioned licensing may be used (e.g., different versions under different terms)

---

## 8. Binary Distribution

Helm may be distributed via:

- GitHub Releases
- Package managers
- Installer tools
- Third-party distribution systems

### Policy Direction

- Official binaries will be distributed by the Helm project
- Redistribution of unmodified binaries may be allowed
- Redistribution of modified versions may be restricted
- Branding may not be used without permission

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
- Multi-edition (Free / Pro / Enterprise)
- Sustainable and professionally supported

The exact licensing model remains intentionally flexible.

---

For legal terms, see the root `LICENSE` file.
