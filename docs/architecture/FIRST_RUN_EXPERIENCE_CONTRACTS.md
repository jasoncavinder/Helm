# First-Run Experience Contracts

Status: normative contract, with the bounded v0.20 production entry enabled by
default; independent review and signed Release-path acceptance remain pending.
Generalized persisted setup parity remains v0.21 scope. Debug-only research
previews are not a shipping setup implementation.

This document defines architecture, semantics, and boundaries for Helm's Project WOW first-run setup experience across GUI, CLI, and TUI presentation surfaces.

Machine contracts: `docs/contracts/first-run/`
Native macOS presentation: `docs/app-design/NATIVE_MACOS_PROTOTYPES.md`
State presentation matrix: `docs/app-design/NATIVE_MACOS_STATE_MATRIX.md`
Research checkpoint: `docs/app-design/NATIVE_MACOS_RESEARCH_VALIDATION.md`

The shared versioned acknowledgment API is implemented separately from production
entry activation. See [the implementation boundary and validation record](../validation/v0.20-first-run-acknowledgment.md).

## 1. Discovery Stages and Consent Boundary

### 1.1 Local observation (automatic, no mutation)

After any required legal acceptance, Helm may perform bounded local observations needed to construct an Environment Brief:

- OS version and architecture.
- Active shell and Helm distribution/update authority.
- Known manager executable detection, versions, install instances, provenance, eligibility, dependencies, and supported setup checks.
- Helm-owned preferences, markers, and CLI shim state.
- Read-only installed-package inventory and local Doctor findings through supported manager operations.
- Explicitly manager-scoped configuration needed for a declared finding.

Helm does not inspect shell history, credentials, secrets, arbitrary home-directory content, or unrelated projects. It does not probe administrator credentials or trigger an authorization request during discovery.

Implemented initial observation is narrower than the full intended scope above:
the [non-executing local observer](../validation/v0.20-first-run-local-observation.md)
returns candidate-file evidence separately from cached detections and stored
preferences. It does not prove installation/readiness, run adapters, reconcile
preferences, or refresh catalogs. Ordinary detection/status APIs have additional
effects and must not substitute for this boundary. The implemented
[startup boundary](../validation/v0.20-first-run-startup-gate.md) prepares storage
before explicit activation and can require the saved experience acknowledgment.
The versioned route now uses a dedicated post-acknowledgment startup handoff:
real detection runs once per service process, then refresh consumes its durable
results. Unknown/offline path state defers network work until reconnection.
This does not grant new network/install/update consent. See
[startup discovery evidence](../validation/v0.20-first-run-startup-discovery.md).
The [shipping activation](../validation/v0.20-shipping-first-run.md) makes the real
entry the ordinary Debug/Release default. It persists and re-reads required legal
acceptance before observation, and explicit acknowledgment before activation.
Optional finite preference repair retains its separate review, consent,
revalidation and durable receipt. Use Helm Now discloses resuming normal saved
behavior; no package mutation or privilege consent is inferred. The generalized
setup-session/action/receipt and session network-consent model below remains
v0.21 scope, not a claim about this bounded implementation.

### 1.2 Network disclosure

- Remote status, catalogs, downloads, and enrichment require an explicit disclosed action such as `Check Now`.
- The Environment Brief remains useful without network access.
- `consent.networkAllowed` records session consent; each plan action still declares whether it requires network access.

### 1.3 Mutation and privilege confirmation

- Every mutation requires a reviewed plan and explicit confirmation.
- Privileged/system work receives a separate just-in-time confirmation.
- `consent.mutationAllowed` and `consent.privilegeAllowed` constrain execution but never weaken core policy.
- Permission denial stops only the dependent branch when unrelated work can continue safely.

## 2. Contract Layers and States

Presentation must not collapse session, plan, action, verification, and receipt states into one generic task status.

### 2.1 Setup Session

Schema: `setup-session.schema.json`

- `queued`: accepted, discovery not started.
- `running`: discovery, evaluation, planning, or apply work is active.
- `verifying`: all submitted mutation work is terminal and post-verification is active.
- `completed`: terminal with complete declared coverage and receipt recording.
- `partially_completed`: terminal with useful verified/no-change results plus failed, deferred, or unverified scope.
- `failed`: terminal without the declared useful session result.
- `cancelled`: explicit user cancellation reached a terminal state; completed effects are recorded separately.
- `interrupted`: prior process/session ended without a clean terminal transition and requires reconciliation.
- `deferred`: user postponed valid work.

### 2.2 Reviewed Plan

Schema: `plan.schema.json`

- `proposed` -> `reviewed` -> `approved`.
- Any relevant environment or policy revision makes the plan `invalidated`.
- Successful pre-apply revalidation makes the plan `revalidated`; execution still requires the recorded approval/consent contract.

### 2.3 Action and verification

- `pending`, `queued`, `running` (read-only/process work), `applying` (mutation in progress), `cancelling`, `cancelled`, `applied`, `verifying`, `verified`, `failed_verification`, `failed`, `interrupted`, `no_changes`, `deferred`, `rolling_back`, or `rolled_back` as applicable.
- `applied` means execution reported success; it is not `verified`.
- Failed verification records expected versus observed state and must not be relabeled as apply failure.

### 2.4 Action Receipt

Schema: `action-receipt.schema.json`

Terminal receipt statuses are `no_changes`, `verified`, `partially_verified`, `failed_verification`, `cancelled`, `interrupted`, `failed`, or `rolled_back`. A session receipt aggregates per-action results without hiding individual verification or recovery limits.

## 3. Idempotency, Retry, Interruption, and Resume

- Every planned action has a stable action ID, pre-verification, and declared retry behavior.
- A retry first revalidates current state and becomes a no-op when the postcondition is already satisfied.
- An action that cannot be retried safely is marked manual/read-only rather than submitted automatically.
- On crash, terminal close, or process loss, the setup session persists as `interrupted` unless authoritative state proves a terminal result.
- Relaunch offers Resume/Reconcile; it never resumes mutation automatically.
- Resume revalidates the Environment Brief revision, plan, consent, policy, and unfinished action state before submitting further work.

## 4. Plan Invalidation and Partial Coverage

- The plan records the Environment Brief revision and relevant policy revision.
- Out-of-band changes invalidate the plan. The user sees `Plan changed`; execution remains disabled until revalidation.
- The session `coverage` object reports complete, cached, failed, cancelled, and deferred managers. The Environment Brief also records intended/current counts so excluded or skipped scope cannot disappear from the summary.
- Independent action failure does not block unrelated branches. A failed dependency prevents only dependent work from submission.
- Zero findings with incomplete coverage is Partial, not Healthy.

## 5. Verification Rules

1. **Pre-verification**: determine whether the result already holds and whether the action remains applicable.
2. **Execution**: submit the finite typed action through existing coordinator/orchestration authority.
3. **Post-verification**: re-observe the affected state using the action's declared verification method.
4. **Recording**: persist per-action result and session receipt, including unchanged and rollback-limited state.

Only step 3 can produce `verified`. An exit code, submitted task, or saved preference alone cannot.

## 6. Recovery vs Rollback

- **Recovery** is forward-moving correction such as re-detection, retry, alternate valid source, or cache repair.
- **Rollback** restores the exact declared pre-execution state through a tested inverse.
- `rollbackEligible=true` is permitted only when that inverse and ownership/precondition checks pass.
- Otherwise `recoveryLimits` is required and UI/CLI must not use Reversible or Roll Back.
- Cancellation and interruption do not imply rollback; receipts list applied/verified, active/unverified, and unstarted work.

## 7. Receipts, Retention, and Redaction

- Every setup session that reaches a terminal, cancelled, or interrupted record produces or resumes a session Action Receipt.
- The receipt aggregates immutable per-action result records and identifies before, changes, observed after, unchanged state, verification, and recovery limits through localization keys/arguments.
- Receipts stay local and are never uploaded automatically. Users can delete them through explicit local-data controls subject to future retention policy.
- Redacted Summary export strips absolute paths, usernames, hostnames, hardware serials, package names, organization identifiers, and other local identity by default unless the user explicitly includes a supported field class.

## 8. Offline Behavior

- `offlineBehavior` is `strict`, `opportunistic`, or `abort`.
- Local observation, cached results with freshness, local Doctor findings, plan review, bundled terms/help, and eligible integrity-verified cached work remain available.
- Network work not yet started becomes `deferred`, not Failed.
- A network operation that loses connectivity reports its actual terminal task result; unrelated local branches continue when safe.
- Offline state is distinct from local service failure.

## 9. GUI / CLI / TUI Semantic Parity

- No business logic or mutation lives in presentation.
- GUI and `helm setup` invoke the same setup-session/plan/action endpoints and consume the same schema versions.
- Existing `helm onboarding` compatibility is preserved until a separately reviewed CLI migration removes or aliases it.
- JSON/NDJSON stdout carries only machine envelopes; progress goes to stderr or a TTY presentation.
- Plain line-oriented output, no-color behavior, cancellation, and deterministic noninteractive errors are required.

## 10. Localization Boundary

- Machine JSON uses stable ASCII enum/property names.
- User-facing text uses `localizationKey` and named `localizationArgs`; presentation resolves keys against Helm locale resources.
- Schema values never carry commands, scripts, or executable content disguised as localized text.

## 11. Native Presentation and Human Validation

- The macOS shell and Project WOW prototypes use the approved Health/Updates/Packages/Activity/Sources IA, standard Settings separation, native list/table/inspector behavior, and complete appearance/input budgets.
- The owner-run moderated study remains required. No participant result is claimed by repository artifact completion.
- v0.19 may begin foundation work from these contracts; v0.20 workflow sign-off and v0.22 UI lock require the study thresholds and accessibility protocols to pass.

## 12. v0.20 Upgrade Entry and Acknowledgment

Owner-approved requirement: the new first-run value experience also appears on
upgrade from pre-v0.20, even when the user completed legacy onboarding. This is
the Environment Brief/Project WOW flow, not the obsolete feature walkthrough.
Track production implementation and signed upgrade-path QA in
[#561](https://github.com/jasoncavinder/Helm/issues/561).

- Use a persisted, versioned experience identifier independent of the legacy
  onboarding Boolean. A missing previous-app-version value must not exclude a
  legacy profile from the new experience.
- Fresh installs and pre-v0.20 profiles without this experience's acknowledgment
  enter the reviewed production route. Merely having run a research preview is
  not completion and must not create a production acknowledgment.
- Completing the experience or explicitly choosing Use Helm Now acknowledges
  that experience. Relaunches, later RCs, RC-to-stable and patch updates do not
  replay an already acknowledged experience.
- Window closure or process interruption before acknowledgment offers safe
  continuation. Persisted in-flight work is reconciled; mutation never resumes
  automatically. Acknowledgment is separate from action success or verification.
- Preserve existing language, manager enablement, pins, update preferences and
  channel, accepted terms, data, receipts and task history. Do not reset legacy
  preferences or require legal reacceptance unless the actual terms changed.
- Showing the introduction grants no new network, installation, shell-edit,
  privilege or automatic-update consent. Read-only discovery stays bounded;
  every proposed mutation retains plan review, current-state revalidation and
  explicit consent. Existing CLI onboarding compatibility is unchanged.
- Validate fresh install, v0.19.1 upgrade, absent legacy version metadata,
  acknowledged RC-to-stable/patch/relaunch, interruption, offline/partial state,
  accessibility/localization and preference preservation in isolated VM profiles.

Normal app launches now use the real versioned entry, never synthetic progress
or receipts. The [activation evidence](../validation/v0.20-shipping-first-run.md)
tracks separate implementation review and signed Release/candidate QA; enabling
the default does not itself close #561 or authorize publication.
