# External Sparkle Supported Ownership Scope

Status: owner-approved policy, 2026-10-06; based on merged #639 (`b9ca25b9`).
The owner approved explicit confirmation about custom/unsupported management.
This decision does not approve a complete-ownership result, relax existing guards,
enable adoption or replace the [external boundary](EXTERNAL_SPARKLE_BOUNDARY.md).

## Approved Decision

The owner has already approved explicit per-app adoption of unknown-origin apps,
without overriding known manager ownership or incomplete native checks. The
approved policy describes authority outside a finite supported scan:

- After supported checks are complete, require the user to confirm
  that no custom or unsupported tool manages this particular app. Clearly state
  the scan's limits. Record user adoption, never verified standalone provenance.
- Independently provable standalone provenance remains a distinct authority;
  unknown history must never be relabeled as verified installation provenance.

The decision does not permit a user to override an observed competing-manager claim,
an unreadable source, an unsupported format in a required source or a known
coverage gap. A checkbox is an acknowledgment of an explicit scope limit, not
native evidence that resolves a failed check.

## Current Evidence And Missing Work

The native observer currently returns only `otherManager` or `unresolved`.
The production adoption provider is absent. This table distinguishes implemented
exclusions from work required before proposing a supported-complete result.

| Source | Current bounded observation | Remaining activation requirement |
|---|---|---|
| Homebrew casks | Standard `/opt/homebrew/Caskroom` and `/usr/local/Caskroom` references, installed app/suite declarations, absolute generic targets, literal removal paths and saved appdir settings; every token contributes claims and gaps. | Complete remaining artifact forms and alias coverage. Missing/legacy/empty declarations and uninspected artifacts remain blockers, including in unrelated tokens. No token-name shortcut. Custom prefixes are outside these scanners, not proved absent. |
| MacPorts | Standard application location and `/opt/local/var/macports/registry/registry.db`, schema 1.215, including inactive payload claims. | Preserve busy/unknown/unsafe-registry rejection. Custom prefixes, alternate layouts and aliases are not covered by standard-root absence. |
| Installer receipts | Fixed `pkgutil` queries for the inspected bundle tree and overlapping non-root receipt exports. | Resolve root-install-location historical/deleted payload claims and alias limitations. Do not label the current successful query sequence complete while these known gaps remain. |
| App Store | Receipt-container markers in the inspected bundle. | Preserve marker denial. Marker absence is not authenticated installation history; do not present it as such. |
| Setapp | Standard application locations and known SDK/resource markers. | Preserve marker denial. Marker-less or custom integrations are not proved absent. |
| Custom tools and managed deployments | No universal registry or generic history collector is implemented. | Require the approved explicit user confirmation after supported checks pass, or retain Open App. An observed indication of another owner still blocks; it cannot be dismissed as outside scope. |
| Filesystem, signing and helper boundary | Bounded native target/helper identity, filesystem and live peer observations. | Revalidate at the appropriate review/admission/handoff boundaries. These checks establish neither historical provenance nor other-manager absence. |

All existing bounds, exact evidence comparisons, no-follow reads, fixed process
arguments and fail-closed error handling remain requirements. No destination
probing, manager execution, Ruby evaluation or arbitrary script execution may be
introduced to make receipt coverage look complete.

### Cask Artifact Classification

The disposable VM's read-only installed receipts for Docker, Parallels and Setapp
contain app declarations alongside uninstall/zap directives; some also contain
binary links and install hooks. Consequently #639 reports global
`uninspectedArtifacts` gaps even when reviewing unrelated Rectangle. This is
not a claim that Homebrew owns Rectangle.

Homebrew's artifact vocabulary includes moved applications, arbitrary artifact
destinations and executable installation steps. Non-app declarations therefore
cannot all be classified as irrelevant cleanup. See the upstream
[Cask Cookbook](https://docs.brew.sh/Cask-Cookbook).

Before narrowing a gap, specify the exact accepted grammar and its possible
target/payload effects. Retain destination and removal claims where applicable;
reject scripts, hooks, unresolved variables/globs and other effects that cannot
be bounded without execution. A supported app declaration must not hide an
unsupported sibling declaration. Unknown future keys and shapes remain blocked.
Coverage decisions need positive, malformed, mixed and unrelated-token fixtures,
plus read-only real receipt controls. Do not modify installed receipts to obtain
a positive test result.

The [implemented path grammar](../validation/v0.20-sparkle-cask-artifact-paths.md)
now retains app/suite destinations, absolute generic-artifact targets and literal
absolute `delete`/`trash`/`rmdir` claims from uninstall/zap dictionaries. Equality,
descendant and ancestor overlap all deny. Mixed unknown directives and
expansion-dependent paths remain gaps; a recognized sibling cannot erase them.
No alias/history completeness or operational authority is inferred.

## Adoption Contract

The request and durable acknowledgment binding are implemented as described
below. Complete native collection, operational integration and UX remain pending:

1. Define a versioned, finite required-source policy. Distinguish a complete scan
   of an absent source from unreadable, unsupported or incomplete observations.
   Encode reasons for blocked results rather than exposing a universal-clearance
   Boolean. Only the native collector may construct supported-complete evidence.
2. Return the inspected scope and its limits in the separate per-app review.
   Require explicit confirmation about unsupported/custom owners;
   no preselected checkbox, bulk adoption or automatic acceptance during refresh.
   The client supplies intent, not a trusted ownership assessment.
3. Bind the reviewed scope version, acknowledgment, target identity, current
   native claims/gaps and authenticated boundary to the single-use review.
   Re-observe before admission. Scope/evidence drift consumes the review; possible
   post-commit drift or lost replies retain explicit uncertainty, not safe retry.
4. Persist the consent's reviewed policy version and acknowledgment. Do not
   silently reinterpret existing/unversioned grants under a different scope.
   Require a new review when the required scope changes. Any schema change must
   use a new append-only migration, not rewrite a shared migration.
5. On resolution and before update handoff, collect fresh required observations.
   A new competing claim or incomplete check blocks use of saved permission;
   neither history nor a previously clear scan bypasses that denial. Preserve
   revocation access when the target becomes absent or ineligible.
6. Keep adoption distinct from each accepted candidate's review, durable
   one-shot installation session, native authorization and post-install
   verification. Both GUI and CLI use the same core decision and reason codes.
   No product capability is enabled until the signed operational gates pass.

The current native evidence compares raw receipt snapshots within an observation;
it does not retain every raw receipt byte in the cross-review target value.
Implementation must explicitly define any additional scope/evidence fingerprint
needed by the new contract instead of claiming that binding already exists.

### Implemented Request And Ledger Binding

`AdoptionRequest` schema 2 requires `ownershipScopeVersion: 1` and
`confirmsNoUnsupportedOwner: true`. Missing, false, duplicate, mistyped and unknown
versions reject rather than acquiring defaults. These are untrusted user intent,
not native clearance. The Swift initializer requires an explicit Boolean argument;
future UI must obtain it from a separate, unselected acknowledgment after showing
the exact scope. No GUI/CLI adoption control is activated by this change.

The request participates in the existing single-use review fingerprint.
Migration 25 appends nullable scope/acknowledgment columns, preserving old records
without inventing consent. Confirmation persists the reviewed fields atomically.
Old/unversioned or different-scope grants produce advisory `scopeChanged` history
and cannot resolve authority. Claim, handoff and verification also recheck the
stored scope in their existing transactions, fencing previously cached tokens.
Native coverage remains a separate obligation; production's provider stays absent.

Explicit downgrade through migration 25 appends revocations for live grants and
rotates the ledger epoch before removing scope columns. It does not expose
stripped grants to older readers or resurrect pending reviews. Active external
update reservations block that downgrade. Existing migration definitions are
unchanged, and ordinary consent reads/reviews do not migrate old helper ledgers.
The [validation record](../validation/v0.20-sparkle-supported-scope.md) identifies
the tested source, VM suites, negative controls and operational limitations.

## Delivery And Acceptance

Complete the selected ownership policy, required-source coverage and
native/core/durable binding together before enabling the staged provider.
Avoid enabling a grant path merely because the current gap list is empty.

- Regression matrix: known claims; absent versus unreadable sources; all retained
  gap reasons; unsupported artifacts/formats; changed scope/acknowledgment;
  stale, replayed, lost-connection and post-commit outcomes; old stored grants.
- Signed VM matrix: fresh review/confirmation and revocation using the exact
  production caller/helper requirements, independently observed positive control,
  manager-owned negatives and before/after native evidence. Synthetic complete
  facts or unsigned tests do not certify operational adoption.
- Later execution matrix: Sparkle-accepted candidates, real installation,
  cancellation/authorization denial, helper loss, verification and recovery,
  then shared GUI/CLI presentation and final integrated-release acceptance.

Keep #606/#607/#608 open and Open App available. Do not repeat accepted package
lifecycles or owner QA without a relevant change. Runtime work remains in the
disposable VM; no host production app, stable database, signing configuration or
publication is changed by this contract implementation.
