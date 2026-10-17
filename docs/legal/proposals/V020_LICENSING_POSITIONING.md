# v0.20 Licensing and Positioning Proposal

Status: REVIEW ONLY - NOT IN EFFECT. Prepared 2026-10-17.

The owner approved preparing a capability-based Free/Pro/Fleet proposal, not
activating new legal terms. The root [LICENSE](../../../LICENSE) remains the
governing license. Publishing or merging this planning document grants no new
rights, changes no accepted terms and does not authorize a release or deployment.
The draft is not legally validated; software-licensing counsel should review it
before the owner approves activation.

## Review Package

- [Proposed license text](../../licenses/proposals/V020_SOURCE_AVAILABLE_LICENSE.md)
  contains a complete discussion draft, not a replacement for `LICENSE`.
- [Positioning and support copy](../../business/proposals/V020_POSITIONING_COPY.md)
  contains staged README, website, app and release wording plus funding checks.
- The clause comparison below explains the legal-policy changes under review.
  No live product text, locale, license identifier or payment setting changes here.

## Objective and Boundaries

Make ordinary local use legitimate for hobbyists, students, freelancers,
employees and organizations, including on employer-owned devices. Charge for
additional capabilities and support, not employment, company size, device
ownership or how many coworkers independently use Helm.

Keep Helm proprietary/source-available. Free is an edition, not an open-source
license. Open-source licensing would be a separate decision: access to source
alone does not meet the [Open Source Definition](https://opensource.org/osd).

The intended boundary remains two products: Helm with Free/Pro capabilities,
and the separately distributed Helm Business/Fleet product. The shorter public
label "Helm Fleet" does not merge their binaries, update authorities or lifecycles.
App Store, Setapp and enterprise channels remain planned, not available merely
because they appear in a product matrix.

## Current-State Audit

Reviewed against `dev` commit `042ff79ccead01b43fbfb853164ca9077863859e`.

| Surface | Current fact or conflict | Proposed disposition |
|---|---|---|
| Root `LICENSE`, sections 1-4 | Commercial production/internal business use is prohibited; business evaluation only. | Permit official consumer binary use for personal and professional purposes by people and organizations. |
| `docs/legal/LICENSING_STRATEGY.md`, sections 2 and 7 | Says personal modifications are allowed despite section 4(b) of `LICENSE`; defers commercial permission until 1.0. | Correct the current summary at activation; explicitly decide source permissions and move binary workplace permission to the approved v0.20 release. The current license wins meanwhile. |
| `docs/licenses/PROPOSED_COMMERCIAL_LICENSE.md` | Nonbinding draft requires paid licensing for ordinary business use. | Retire or rewrite at activation; do not layer this restriction over Free. |
| `docs/licenses/PROPOSED_ENTERPRISE_TERMS.md` | Lists multi-device use, MDM deployment and signed updates as enterprise value. | Distinguish ordinary deployment and base security from paid central governance/support. |
| `docs/licenses/PROPOSED_OPEN_CORE_MODEL.md` | Separate nonbinding alternative. | Mark as unselected if this proposal activates; no open-core conversion is approved. |
| `docs/enterprise/EDITIONS_AND_ENTITLEMENTS.md` | Strong Free execution/logging guarantees, but generic bulk operations, history and automation also appear in Pro. | Classify precise capabilities using the matrix below; do not paywall current basic bulk actions, logs or recovery. Planned entitlement examples are not proof that billing exists. |
| `docs/PROJECT_BRIEF.md`, `docs/ROADMAP.md`, `docs/architecture/BUILD_VARIANTS.md` | Free/Pro plus a separate Fleet product; base first-run safety/recovery; post-1.0 commercial infrastructure. | Preserve architecture and safety; add adoption/legal work without pulling billing or Fleet implementation into v0.20. |
| `docs/business/MONETIZATION_STRATEGY.md` | Subscription/lifetime options and beta access are planning material. | Treat pricing as research; no lifetime offer, sponsor entitlement or deadline is created here. |
| `README.md`, `web/src/content/docs/licensing.md`, `index.mdx`, `guides/faq.md` | Live copy states non-commercial use and future editions; some copy implies later Pro gating. | Replace together only after exact terms and version coverage are approved. |
| `CONTRIBUTING.md`, `docs/legal/CLA.md` | Contributions are invited; CLA grants relicensing rights, but root source-modification restrictions impede the described workflow. | Review actual contributor rights/assent and permit a bounded contribution workflow explicitly; do not assume the presence of a CLA proves every contribution is covered. |
| App and CLI license acceptance | GUI `AppUpdateConfiguration`, CLI `main.rs` and FFI `lib.rs` recognize `helm-source-available-license-v1.0-pre1.0`. App `HelmSupport` and CLI terms links point to `main/LICENSE`. | Coordinate identifiers, stored-acceptance compatibility and immutable/version-specific terms; never silently reinterpret an earlier acceptance. |
| `locales/*/app.json` and bundled mirrors | `app.onboarding.license.summary` says non-commercial use in seven locales. | Translate approved text in both trees, then verify GUI/CLI/FFI and first-run parity. No locale edits in this PR. |
| Third-party licenses and notices | Helm's terms are separate from dependency obligations. | Preserve `THIRD_PARTY_LICENSES.md` and `NOTICE.md`; changing Helm's license cannot relicense dependencies. |

## Proposed Edition Matrix

This is a proposed allocation, not a claim that all listed capabilities ship.
Capabilities absent today remain roadmap work. Existing package/manager policy,
consent, authority ordering and unsupported-operation limits apply in every tier.

| Capability | Free | Pro (future paid) | Fleet (future paid) |
|---|---|---|---|
| Personal, freelance and employee use; internal organization deployment | Allowed | Allowed | Not required merely for workplace use |
| Detection, inventory, search, install/remove/update, pins, reviewed bulk Plan, GUI/CLI parity | Base capabilities | Same base | Same safe local base |
| First-run value, safe execution, essential diagnostics/logs, verification, receipts/recovery, localization/accessibility, offline operation | Always base | Never required to unlock these | Never required to unlock these |
| Existing basic refresh/self-update preferences and locally scripted Free CLI operations | Remain usable | No retroactive charge for doing paid work | MDM distribution or script use alone is not Fleet |
| Advanced personal scheduling, reusable Blueprints, richer historical analysis and advisory enrichment | Basic operations remain complete | Candidate paid additions; exact boundary reviewed before charging | May reuse approved capabilities under separate terms |
| Central policy/baselines, cross-machine reporting, rollout approvals and organizational integrations | Not promised | Not an individual Pro requirement | Paid product value to validate |
| Contracted organizational support or service commitments | No SLA promised | Only if explicitly purchased | Separately agreed support scope |

Examples under the proposed terms:

- A freelancer updating tools for paid client work uses Free without a commercial surcharge.
- An employer distributing official Free binaries through MDM is not purchasing Fleet.
- An internal admin scheduling the existing Free CLI with existing management tools
  does not owe Fleet fees merely for automation. This grants no unsupported technical capability.
- A team wanting Helm's central governance/reporting product would evaluate Fleet.
- An MSP may assist a customer with authorized local installation/use; offering
  Helm itself as a hosted product or redistributing a modified commercial fork
  requires a separate agreement. Counsel must confirm this distinction in final terms.

## Clause Comparison

Numbers on the right refer to the proposed license, not current law or terms.

| Current license | Proposed replacement | Reason / review concern |
|---|---|---|
| Sections 1-3: non-commercial definitions/grant and business-evaluation exception | Sections 1-2: official consumer releases usable by individuals and organizations for any lawful local purpose, including paid work | Remove the adoption barrier without granting Fleet features. |
| Section 2: broadly revocable grant | Sections 2 and 8: version-specific continuing grant, subject to breach termination | Avoid discretionary retroactive withdrawal merely because pricing changes; cure procedure needs legal review. |
| Section 2(d): redistribution only for non-commercial use | Section 3: official unmodified binary sharing/internal deployment, notices preserved, no resale of software rights | Cover employer deployment and authorized distribution channels; paid support is distinct from selling the binary. |
| Section 4(b): no source copying/modification without permission | Section 4: limited inspection, private evaluation/builds and upstream contributions | An additional proposed permission, not an existing right. Private modified production distributions remain outside this draft. |
| Section 4(d): broad competing-product prohibition | Section 5: restrictions on redistributing/reselling Helm or offering Helm as a service | Ordinary development of other software using a Helm-managed toolchain is not forbidden. No independent-development noncompete. |
| Section 4(e): broad managed-service prohibition | Sections 3 and 5: internal administration and customer assistance expressly distinguished from offering Helm as a service | Avoid accidentally requiring Fleet for ordinary IT work. |
| Sections 5-6: ownership/CLA | Sections 4 and 7: retain rights holders, contribution terms and third-party licenses | Verify contribution provenance; do not treat the CLA as copyright assignment. |
| Section 7: future versions may use different terms | Section 6: separate future paid capabilities; no retrospective fees for granted versions | Reconcile existing promises before changing public transition copy. |
| Sections 8-11: termination, warranty, liability, jurisdiction | Sections 8-11: proposed cure language, retained disclaimers, explicit jurisdiction placeholder | Counsel review required; do not invent the owner's jurisdiction or assume enforceability. |

## Proposed Transition

Recommend prospective activation for an explicitly identified v0.20 release and
source revision. No old beta/stable tag or artifact is silently relicensed by
editing the current branch or its documentation. Earlier versions retain their
distributed terms unless the owner separately approves a counsel-reviewed
additional grant identifying those exact versions.

Preserve the old license text at an immutable reference and provide a clear
version-to-terms mapping wherever older binaries' existing `main/LICENSE` links
land. An old app showing a newly edited page must not be treated as acceptance
of new terms or evidence that its original grant changed.

For releases actually distributed under the new terms, propose continued use of
their included capabilities without later fees or a retroactive beta expiry.
Future optional releases may introduce clearly disclosed paid capabilities;
that is not perpetual free access to all future Pro features, upgrades or support.
This does not promise ongoing updates to an old binary or technical compatibility.

Before charging, audit what users already received and the existing statement
that users "will not lose access to functionality they already have." Determine
whether grandfathering or additional transition protection is appropriate. Do
not reinterpret that statement as a narrower binding promise through this PR.
Existing sponsorship/purchase commitments, if any, must be checked independently.

## Approval and Legal Questions

Owner agreement with the strategy is not approval of every clause. Record
approval against an exact draft revision before replacing the governing license.

- Confirm covered release/source revisions, effective date, legal licensor and
  governing jurisdiction; do not ship placeholders or a floating `main` terms URL.
- Review contributor permissions/CLA assent, employer-owned contributions and
  third-party notices before asserting authority to grant expanded rights.
  [Copyright Office ownership guidance](https://www.copyright.gov/title17/92chap2.html#201)
  is background, not a validation of this repository's rights chain.
- Approve or narrow private source evaluation/build and contribution-fork permissions.
  This is broader than current section 4(b); official-binary professional use
  does not itself require allowing production use of modified builds.
- Review redistribution, internal automation, MSP/customer assistance and hosted
  service language for accidental restrictions on ordinary Free use.
- Review termination/cure, warranty/liability, mandatory consumer rights, territory
  differences and compatibility with future App Store/Setapp agreements.
- Resolve beta-access promises and any actual lifetime/sponsor offers before
  selecting a future charging model. No prices or sales terms are approved here.
- Decide whether old releases receive a separate professional-use permission.
  Default for this proposal: no retroactive change and no rewritten release artifacts.

## Small v0.20 Plan

1. Review this proposal and draft license; record owner decisions and counsel
   feedback. This PR can merge as documentation without activating any terms.
2. Prepare a separate activation PR only after explicit owner authorization. It
   must update the root license, version coverage, contradictory planning docs,
   README/website/app copy, contribution guidance and all seven locale mirrors.
3. Coordinate the app/CLI/FFI terms-version checks and immutable offline-accessible
   license text. Decide acknowledgment behavior explicitly; never auto-accept new
   terms or reset first-run acknowledgment, manager choices, task data or consent.
   Verify fresh install and upgrade/relaunch in the signed VM, including offline
   terms access, GUI/CLI parity and preservation of prior acceptance history.
4. Confirm the support destination and owner payout readiness; use one primary
   link with optional verified alternatives. Review the short adoption demo/copy.
5. Publish only the approved, version-qualified wording after activation is
   effective for the referenced artifacts. Follow normal release/deployment
   authorization and gates; legal approval is not release authorization.

Open legal questions block new professional-use claims, not unrelated v0.20
engineering. If unresolved at release time, retain current terms/copy and make
the decision visible to the owner. Do not quietly claim the intended policy is live.

Deferred: billing, entitlement infrastructure, Fleet services/integrations,
new pricing commitments and lifetime offers. Research Pro purchase-plus-major-
upgrades versus subscriptions; interview IT/platform administrators about concrete
gaps, integrations and willingness to fund a narrow Fleet pilot before expanding it.

## Proposal Acceptance

- [ ] Owner and independent reviewers agree on the capability matrix and transition.
- [ ] Counsel questions are answered or explicitly remain activation blockers.
- [ ] Draft terms and staged messaging agree on official binaries, source rights,
      workplace deployment, version coverage and sponsorship.
- [ ] `LICENSE`, README, website, app/CLI/FFI identifiers, locales and funding
      configuration remain unchanged by this proposal-only PR.
- [ ] No release, deployment, payment change or automatic activation follows merge.

There is no new telemetry, licensing server, database migration, product gate,
manager permission or update-authority change in this proposal.
