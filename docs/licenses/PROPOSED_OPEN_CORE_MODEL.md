# ⚠️ PROPOSED OPEN CORE MODEL — NOT IN EFFECT

This document describes a potential future open core model for Helm.

It is provided for planning purposes only and is NOT legally binding.

The current governing license is defined in the root LICENSE file.

---

# Helm Open Core Strategy (Proposed)

## 1. Overview

Helm may adopt an **open core model**, where:

- Core functionality is open source
- Advanced functionality remains proprietary

This approach balances:

- community adoption
- commercial sustainability

---

## 2. Core vs Commercial Boundary

A clear separation must be maintained between:

### Open Core (Free)

- Local package management
- Basic update orchestration
- Manager adapters (brew, rustup, etc.)
- Core CLI functionality
- Basic UI

### Commercial Features (Pro / Enterprise)

- Advanced orchestration logic
- Policy enforcement
- Multi-device management
- Audit and compliance
- Enterprise integrations
- Automation workflows

---

## 3. Architectural Requirements

To support open core:

- Core functionality must be modular
- Commercial features must be separable
- APIs must be stable and documented
- No circular dependencies between core and proprietary layers

---

## 4. Licensing Split

### Core

- Open source license (e.g., MIT / Apache-2.0)
- Community contributions encouraged

### Commercial Layers

- Proprietary license
- Source may or may not be available

---

## 5. Contributor Considerations

All contributions are governed by the CLA.

This ensures:

- Contributions can be used in commercial features
- Code can be moved between core and proprietary modules
- Licensing remains flexible

---

## 6. Risks

Open core introduces:

- Maintenance complexity
- Boundary disputes (what is "core" vs "commercial")
- Potential for forks

Mitigations:

- Keep core small and stable
- Clearly define feature boundaries
- Use trademark protection

---

## 7. Benefits

- Increased adoption
- Community contributions
- Developer trust
- Easier integration into ecosystems

---

## 8. Non-Goals

Open core does NOT guarantee:

- full open source
- permissive licensing for all features

---

## 9. Current Status

Helm is NOT currently open source.

This model is under consideration for post-1.0.

---

## 10. Decision Criteria

A decision on open core should consider:

- community demand
- revenue sustainability
- competitive landscape
- maintenance cost

---

For current licensing, see the root LICENSE file.
