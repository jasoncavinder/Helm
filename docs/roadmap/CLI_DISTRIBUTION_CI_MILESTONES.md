# CLI Distribution CI Milestones (Future Work)

Status: Planned  
Owner: Helm Release Engineering  
Last Updated: 2026-02-23

---

## Scope

This addendum tracks CI milestones that are intentionally deferred beyond the current CLI direct-installer rollout.

Current delivered baseline:

- direct CLI release workflow with website metadata publication (`release-cli-direct.yml`)
- installer validation workflow (`cli-installer-checks.yml`)

---

## Future Milestones

### M1. MAS CI Packaging

Goal:

- automate channel-profiled MAS archive/export checks
- validate App Store-specific channel invariants in CI

Expected outputs:

- MAS packaging job templates
- signed/exported artifact validation checkpoints (credentials-gated)

### M2. Setapp CI Packaging

Goal:

- automate Setapp channel build/profile checks
- validate Setapp-targeted metadata and packaging conventions

Expected outputs:

- Setapp packaging job templates
- release gating checks for Setapp channel

### M3. MacPorts Automation (Optional)

Goal:

- automate formula/port metadata sync for CLI distribution
- verify package metadata updates against release tag/version

Expected outputs:

- optional workflow(s) for MacPorts metadata update assistance
- manual-review handoff instructions

### M4. Business PKG CI + Notarization Pipeline

Goal:

- define and automate signed PKG creation + notarization for business/fleet variant
- enforce managed update-policy defaults in produced artifacts

Expected outputs:

- credentials-gated PKG build workflow
- notarization/stapling verification
- managed-policy artifact contract checks

---

## Acceptance Guardrails

- Must preserve GUI direct Sparkle flow behavior for `developer_id` channel.
- Must keep CLI public command/flag compatibility unless explicitly versioned and documented.
- Must remain consistent with `docs/architecture/BUILD_VARIANTS.md`.
