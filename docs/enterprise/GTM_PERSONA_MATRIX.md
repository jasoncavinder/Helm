# Helm Enterprise GTM Persona Matrix

This document maps Helm business value to enterprise buying and operator personas.

## 1. Positioning

Helm is not an MDM replacement.

Helm is a software-governance control plane for developer environments on macOS that integrates with existing MDM and identity infrastructure.

## 2. Persona Messaging Matrix

| Persona | Top Problems | Helm Value Narrative | Proof Points | Buyer Language |
|---|---|---|---|---|
| IT Admin / Endpoint Engineer | Script sprawl, inconsistent toolchains, brittle update workflows | "Standardize dev software lifecycle without replacing MDM." | MDM-native deployment, scoped baselines, deterministic orchestration, ringed rollouts | Operational efficiency, fewer escalations, simpler fleet hygiene |
| Security / GRC | Unapproved binaries, weak auditability, privileged script risk | "Turn package operations into policy-enforced and auditable actions." | Signed policy/entitlement checks, explicit guardrails, manager/task/action attribution, drift reporting | Risk reduction, audit readiness, policy enforcement evidence |
| Engineering Leadership (VP Eng, Platform Eng) | Slow onboarding, team drift, build inconsistency | "Keep teams autonomous while enforcing reproducible environments." | Toolchain-first ordering, pinning strategy, baseline profiles by team, reduced setup variance | Developer productivity, delivery predictability, lower platform toil |

## 3. Objection Handling

1. "We already have Jamf/Intune."
- Response: Helm complements MDM. MDM installs and configures Helm; Helm manages package-manager lifecycle consistency.

2. "Homebrew is enough."
- Response: Homebrew is a package tool, not a fleet governance system with scoped policy, drift analytics, and audit workflow.

3. "Central policy will slow developers down."
- Response: Helm supports scoped baselines and ringed rollout so teams can move quickly within safe guardrails.

## 4. Packaging and Offer Strategy

- Helm (Consumer): Free + Pro entitlement model for local control-plane workflows.
- Helm Business (Fleet): separate fleet product for central management, policy scope, compliance, and audit integrations.

Commercial packaging should keep one shared core codebase while separating consumer and fleet release artifacts/lifecycles.

## 5. Pilot Motions by Persona

1. IT-led pilot (recommended first)
- Scope: one engineering department, 50-200 Macs
- Success metrics:
  - baseline compliance rate
  - update failure rate
  - mean time to remediate drift

2. Security-led pilot
- Scope: high-control environment with audit requirements
- Success metrics:
  - critical patch SLA adherence
  - audit evidence completeness
  - unauthorized package reduction

3. Engineering productivity pilot
- Scope: one platform team + one product team
- Success metrics:
  - new-hire time-to-first-commit
  - environment-related incident count
  - toolchain mismatch incidents

## 6. KPI Framework

Primary business KPIs:

- environment compliance percentage by scope
- drift recurrence rate
- patch SLA attainment for managed toolchains
- onboarding time reduction
- reduction in ad hoc privileged scripts

## 7. Sales Narrative Structure

1. Current state:
- MDM controls devices, but developer package ecosystems remain fragmented.

2. Cost of status quo:
- inconsistent environments, audit gaps, avoidable incidents, manual toil.

3. Helm outcome:
- policy-driven consistency, measurable compliance, safer updates, and better developer velocity.
