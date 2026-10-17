# v0.20 Positioning and Support Copy

Status: REVIEW ONLY - NOT FOR PUBLICATION AS CURRENT TERMS. Prepared 2026-10-17.

All quoted copy below is staged for the separately approved licensing activation.
The root [LICENSE](../../../LICENSE) still restricts commercial use. This document
does not authorize workplace use, sales, a website deployment or new in-app text.
See the [licensing proposal](../../legal/proposals/V020_LICENSING_POSITIONING.md)
and [draft terms](../../licenses/proposals/V020_SOURCE_AVAILABLE_LICENSE.md).

## Message and Product Names

Use "Helm Free", "Helm Pro" and "Helm Fleet" in the proposed public explanation.
Fleet refers to the planned separate Helm Business product, not a third runtime
tier of the consumer app. Do not describe unimplemented features as available.
Do not describe source-available Helm as open source.

The effective version must be explicit. Replace `[COVERED VERSION]` and
`[VERSIONED TERMS LINK]` only after approval; do not imply older beta artifacts
permit professional use. No placeholder may reach a public page or product build.

## Staged Copy

### README / Website Introduction

> Starting with [COVERED VERSION], Helm is free during beta for personal and
> professional use, including freelance work and company-owned Macs. A useful
> Free edition will remain available after 1.0. Pro capabilities and a separate
> Fleet product are planned paid offerings. Voluntary sponsorship supports ongoing
> development and is not a purchase of a future license.

Add nearby: "Helm is proprietary, source-available software. See
[VERSIONED TERMS LINK] for permissions, limitations and version coverage. Earlier
releases retain their distributed terms. Beta software does not include an
enterprise support commitment."

### Editions

> Free gives you everyday local package and toolchain management, including
> essential safety, diagnostics and recovery. Pro is planned for advanced personal
> workflows and insights. Fleet is planned for centralized organizational policy,
> reporting and coordinated management. Using Free at work does not require Fleet.

### Website FAQ

**Can I use Helm for paid work or on a company laptop?**

> Yes, for the official consumer releases covered by the new terms, starting with
> [COVERED VERSION]. Your employer, occupation, device ownership and number of
> coworkers do not change Free eligibility. Earlier releases retain their terms.

**Does my employer need Fleet to distribute Helm through MDM?**

> No. Internal distribution of official consumer binaries and scripting available
> Free CLI operations are permitted under the covered terms. Fleet is a planned
> separate management product, not a charge for ordinary deployment or employment.

**What happens after beta?**

> A useful Free edition will remain. Future optional Pro capabilities and Fleet
> products may have paid terms disclosed before purchase or upgrade. Rights to
> capabilities included in a covered release do not acquire a retroactive fee.
> This is not a promise of free future Pro versions or indefinite maintenance.

This answer must be reconciled with earlier access promises and any actual
offers before publication; the proposal does not decide existing users' rights.

**Does sponsorship buy Pro?**

> No. Sponsorship is voluntary support, not a future license, feature request
> guarantee, lifetime entitlement or delivery commitment. Any separately offered
> product purchase has its own explicit terms.

### In-App License Summary

Proposed replacement for `app.onboarding.license.summary` in the covered build:

> This version of Helm permits personal and professional use of official consumer
> binaries, including use at work. Source and redistribution rights have separate
> limits. By selecting Agree and Continue, you acknowledge that you have reviewed
> and agree to this version's license terms.

Show the exact terms version and accessible complete terms, including offline
access, before acknowledgment. Existing first-run acceptance must not be silently
upgraded. Keep GUI, CLI and FFI validation consistent. Translate the approved
summary and any new copy into `en`, `es`, `de`, `fr`, `pt-BR`, `ja`, `hu` under both
`locales/` and `apps/macos-ui/Helm/Resources/locales/`. Preserve keyboard/VoiceOver
and first-run history. This PR changes neither locale tree nor product behavior.

### Support Pane / README Support Section

> Help support Helm's development. Sponsorship is optional and does not unlock
> features or buy a future license. You can use Helm without sponsoring.

Primary action proposed: "Sponsor Helm" using the existing GitHub Sponsors
destination, after the owner confirms payout readiness. Keep secondary verified
destinations under a simple "Other ways to support" link/menu rather than equal
competing calls to action. Do not promise tax deductibility or lifetime access.

### Release Announcement and Demonstration

> Helm [COVERED VERSION] brings local software management together on your Mac:
> understand your environment, review a plan, and see what completed or needs
> follow-up. This release permits personal and professional use under its new
> source-available terms. Free remains useful; paid Pro and Fleet offerings are
> future plans, not requirements for using Helm at work.

Suggested 45-60 second demo: show a redacted Environment Brief, review selected
updates and risks, then show an actual verified outcome and the recovery path.
Record only supported, release-certified behavior in the disposable VM. Do not
stage a successful Sparkle direct update until its separate execution gates pass.
No claims about "all managers", unattended fleet control or guaranteed safety.

## Funding-Link Check (2026-10-17)

Sources inventoried: `README.md`, `.github/FUNDING.yml`, `HelmSupport` constants
in `apps/macos-ui/Helm/Core/HelmCore+Settings.swift`, and website FAQ support links.
No payment, account or funding configuration was modified; no purchase was made.

Public checks used an ordinary redirect-following HTTPS GET with a 20-second
timeout. GitHub's GraphQL `user.sponsorsListing` additionally confirms a public
listing whose description identifies Helm. HTTP success alone does not prove
recipient identity, checkout functionality, payout eligibility or ownership.

| Destination | Observed result | Proposed treatment before publication |
|---|---|---|
| [GitHub Sponsors](https://github.com/sponsors/jasoncavinder) | HTTP 200; public `sponsors-jasoncavinder` listing identifies Helm | Preferred primary; owner must confirm recipient/payout setup and current tier promises. |
| [Patreon](https://www.patreon.com/cw/jasoncavinder) | HTTP 404 in this check | Owner to confirm/correct destination and `.github/FUNDING.yml` account; do not advertise as verified. |
| [Buy Me a Coffee](https://buymeacoffee.com/jasoncavinder) | HTTP 404 in this check | Owner to confirm/correct; withhold from proposed verified alternatives. |
| [Ko-fi](https://ko-fi.com/jasoncavinder) | HTTP 403 in this check | Verification blocked, not proof the page is missing; owner/browser check required. |
| [PayPal](https://paypal.me/jasoncavinder) | HTTP 200 after redirect to `www.paypal.com/paypalme/jasoncavinder` | Candidate alternative; recipient, checkout and payout unverified. |
| [Venmo](https://www.venmo.com/u/JasonCavinder) | HTTP 200 after redirect to `venmo.com/u/JasonCavinder` | Candidate alternative; recipient, checkout, availability and payout unverified. |

Before activation: verify the primary destination in a normal browser, have the
owner check payout readiness and existing reward promises privately, and recheck
only alternatives intended for publication. No credentials need to be shared.
Funding issues do not justify blocking Free use or delaying unrelated engineering.

## Activation Surface Checklist

- [ ] Approved root license and immutable version coverage are effective first.
- [ ] Align README licensing/support, website licensing/landing/FAQ and contribution
      guidance without claiming open-source status or current paid availability.
- [ ] Reconcile licensing/monetization/edition/roadmap/build-variant planning docs;
      retire the incompatible blanket commercial-license draft explicitly.
- [ ] Coordinate GUI/CLI/FFI identifiers, complete terms access and locale mirrors;
      signed VM fresh-install/upgrade/offline acceptance checks pass.
- [ ] Verify support destination and owner payout/reward decisions; no payment
      settings change without separate explicit authorization.
- [ ] Owner approves exact copy and publication separately from this planning PR.
