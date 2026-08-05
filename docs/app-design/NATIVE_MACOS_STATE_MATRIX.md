# Native macOS Surface and Workflow State Matrix

Status: approved design-definition baseline aligned with the v0.18 Project WOW schemas in `docs/contracts/first-run/`

## State Grammar

Every release-critical surface answers four questions:

1. **What is known?** Include coverage and freshness.
2. **What is happening?** Distinguish queued, running, applying, and verifying.
3. **What remains unchanged or unavailable?** Never imply more coverage or recovery than exists.
4. **What can the user do next?** Present a valid action or an honest terminal state.

Canonical presentation terms:

| Term | Meaning | Must not be presented as |
|---|---|---|
| Loading | No useful value is available for this scope yet. | Empty or failed. |
| Cached | Last-known value is useful but freshness is not current. | Current without age. |
| Refreshing | Useful cached/current structure is visible while newer value arrives. | Blocking loading. |
| Healthy | Covered scope has no actionable finding. | Proof that unscanned scope is healthy. |
| Empty | Covered scope contains zero entities/results. | Loading or not applicable. |
| Not Applicable | Capability does not apply to the selected source/item. | Disabled, failed, or missing permission. |
| Partial | Some declared scope produced useful results and some did not. | Healthy, complete, or total failure. |
| Policy Blocked | A controlling policy prohibits the action; escalation is not offered unless policy explicitly permits it. | Permission blocked. |
| Permission Blocked | The action may proceed after user/system authorization or permission repair. | Policy blocked or generic failure. |
| Offline | Network is unavailable or intentionally not used. Local work may continue. | Service failure. |
| Deferred | Work is valid but intentionally postponed until a condition such as network availability. | Failed or queued for immediate execution. |
| Queued | Accepted by orchestration but not started. | Running. |
| Running | Process/read-only task is actively executing. | Applying a mutation unless mutation has begun. |
| Cancelling | Cancellation requested; process/task is not terminal yet. | Cancelled. |
| Cancelled | Terminal; requested work stopped. Applied changes, if any, are stated separately. | Rolled back. |
| Interrupted | Session/workflow ended without a clean terminal outcome and can require reconciliation. | Cancelled or failed. |
| Applied | Approved mutation reported success but verification is not complete. | Verified. |
| Verifying | Helm is re-observing affected state. | Applying or completed. |
| Verified | Expected postcondition was observed. | Merely exit code 0. |
| Failed Verification | Apply may have succeeded, but expected postcondition was not observed. | Apply failure or rollback. |
| Failed | The attempted stage did not complete its declared result. | Partial if no useful scope remains. |
| Recovery Available | At least one currently valid resume/retry/restore action is provided by core contracts. | Guaranteed rollback. |
| Rollback Limited | Some or all effects lack a verified inverse. | Reversible. |
| Rolled Back | A tested inverse restored and verified the declared pre-action state. | Cancelled, merely retried, or assumed restored. |
| No Changes | Observation or plan completed without mutation. | Verified improvement. |

## Cross-Surface Matrix

Each cell describes the minimum visible behavior. `N/A` means the state does not belong on that surface and should deep-link elsewhere.

| State | Status item / popover | Health | Updates | Packages | Activity | Sources | Settings | First run / Receipt |
|---|---|---|---|---|---|---|---|---|
| Loading | Stable shell; `Starting Helm...` only if no cached condition | Structure and stage label; no zero-value metrics | Plan placeholder and `Building plan` | Table structure and local-cache query state | Activity structure | Source structure and detection stage | Local form renders; saving state is inline | Shell + trust statement + first stage |
| Cached-first | Condition + age; refresh nonblocking | Cached findings and exact freshness/coverage | Cached plan is review-only until revalidated | Cached package rows usable per contract; age shown | Persisted tasks/receipts visible | Cached source status with age | N/A for local settings | Cached context can seed brief but labeled cached |
| Refreshing | Keep condition; subtle progress/freshness | Existing rows stable; stage and coverage update | Keep selected plan row; mark stale/revalidating | Keep rows, selection, filters, scroll | New work/state updates in place | Keep rows and selection | Save only affected control | Stream findings without moving actions/focus |
| Healthy/success | `No action needed` and last checked | Quiet summary and complete coverage | `No updates in selected scope` after current check | Up-to-date rows remain filterable | Verified/no-change outcomes in history | Ready sources, setup complete | Saved value reflected | Brief states no action required; Use Helm Now |
| Empty | No active-work region | No findings, with coverage | No plan items, explain pins/scope if relevant | No packages/results plus clear filter/search reset | No activity yet | No supported/detected sources plus bootstrap path | No empty window; panes still explain options | No managers: audit details/bootstrap/normal entry |
| Not applicable | N/A | Finding says capability unavailable | Row says Not Applicable and why | Action omitted/disabled with reason | Recorded as skipped/not applicable when in workflow | Capability field explains | Managed/unsupported setting explanation | Recommendation omitted or marked not applicable |
| Partial data | `7 of 9 current`; primary action reviews gaps | Useful findings plus failed/deferred scope | Valid steps usable; incomplete sources excluded and named | Available rows remain; source coverage banner | Workflow branches show independent outcomes | Per-source current/cached/failed status | Local settings unaffected | Brief states exact mapped count and retries |
| Partial failure | Highest-priority failure, not generic Error | Failure findings plus retained successful results | Completed and failed authority/source groups; retry failed only | Rows from successful sources remain | Workflow summary with successful/failed/untouched | Failed source row with diagnostics/retry | Save failure only on affected preference | Keep verified changes; retry/resume/keep/rollback options |
| Policy blocked | Condition only if user action is required | Finding names controlling authority | Plan row nonselectable or requires policy-compliant alternative | Action disabled/omitted; policy reason in inspector | Block recorded as not attempted | Source shows Policy Blocked and remediation authority | Managed control disabled with organization label | Plan cannot include blocked action; receipt says unchanged |
| Permission blocked | `Authorization needed` only at just-in-time stage | Finding says permission and next step | Plan discloses authorization; denial stops dependent branch | Action offers retry after permission repair | Denied stage and dependent skipped/deferred stages | Source says permission needed, not policy | Save retry if local permission is relevant | Separate JIT confirmation; denial preserves other branches |
| Offline | Local condition + deferred count | Local/cached results and freshness | Local preview; network plan steps Deferred | Local/cached search; remote group Deferred | Local tasks continue; network task deferred/fails by contract | Local detection/provenance available | Local settings remain usable | Local brief complete; network stage optional/deferred |
| Queued | Active row with Queued | Related finding can say scheduled | Plan row Queued under workflow | Row action disabled only for duplicate mutation | Queued group ordered by workflow | Operation state Queued | N/A | Progress stage Queued |
| Running/applying | Highest current workflow and progress | Finding links to Activity | Plan row Running/Applying; later phases Waiting | Row state and duplicate action disabled | Live stage, elapsed, Stop/Cancel if valid | Source operation state | Inline Saving only | Continuous progress; changes begun explicitly stated |
| Cancelling | `Stopping...`; do not close as if terminal | Related finding unchanged until terminal refresh | Future phases not submitted; active stage Cancelling | Row says Cancelling | Cancellation consequence and process state | Source says Cancelling | Save cancellation only if supported | Explain applied/active/unstarted work |
| Cancelled | Popover may return to prior condition plus receipt link | Re-scan determines finding state | Cancelled rows and untouched future stages | Refresh affected row; do not claim rollback | Terminal Cancelled with completed effects summary | Re-detect affected source | N/A | Receipt: cancelled, verified changes, unchanged work |
| Interrupted | `Setup needs attention` / `Work interrupted` | Finding to reconcile persisted session | Plan may be stale and requires revalidation | Package state marked unknown/cached until refresh | Interrupted session at top with Resume/Reconcile | Source state re-detected before action | N/A | Resume/retry/keep/rollback review; never auto-apply |
| Verified completion | Optional completion condition when backgrounded | Finding resolved only after covered scan | Plan stage Verified; summary before/after | Version/state updates after verified refresh | Verified outcome and receipt | Source ready with verified setup/path | Saved setting shown | `Helm is ready`; Action Receipt |
| Failed verification | Urgent condition with Review | Finding remains or becomes verification finding | Step `Applied; verification failed`; later dependency policy decides | Show observed state, not intended version | Distinct stage and recovery actions | Source `Needs attention`, applied value plus observed mismatch | Saved/unsaved truth explicit | Receipt is Failed Verification or Partially Verified |
| Failed | Failure condition if actionable | Failure finding with exact scope | Failed before apply vs during apply distinguished | Row retains last-known state | Failure detail, diagnostics, retry if safe | Source failure and diagnostics | Preference remains prior value unless confirmed saved | Failed stage with partial results and next action |
| Recovery available | Primary action Resume/Review when highest priority | Recovery finding | Retry/review revised plan | Retry or choose alternate source if valid | Resume, Retry, Restore, Keep, Diagnostics | Repair/source selection | Troubleshooting link only | Persisted session and valid options |
| Rollback limited | Summary says `Completed changes remain` | Finding explains limit | Confirmation and result state limits | Package downgrade absent unless verified | Receipt explicitly lists irreversible effects | Manager uninstall only when ownership/dependency permits | Local preference snapshot can restore | Never label action reversible without tested inverse |

## Workflow Matrix: Environment Brief

| State | Trigger | Visible contract | Actions | Persistence/recovery | Announcement |
|---|---|---|---|---|---|
| Initial loading | Legal gate complete, no useful result | Real Health shell; `Local scan / No changes / No network` | Cancel Scan, Use Cached Results if present | Create/read setup session without mutation | `Local discovery started.` |
| First useful | Stage A or first manager result | Personalized OS/shell/source facts; coverage `n of total` | Use Helm Now, What Helm Checked | Persist stage and result revision | One summary, not every fact |
| Streaming | Additional manager/package results | Stable sorted groups, freshness and stage | Cancel, inspect finding | Cursor/focus/scroll remain stable | Batch no more often than every 2 seconds |
| Healthy complete | All intended local scopes succeed, no findings | Complete coverage, No Changes | Use Helm Now, optional Check Now | Session can close with no-change receipt | `Local mapping complete; no action required.` |
| Partial complete | Some local scopes fail or cancel | Exact successful/failed scopes and cached substitutions | Retry Failed, Use Helm Now | Keep partial result and failure evidence | `7 of 9 sources mapped; 2 need retry.` |
| Offline | Network unavailable or Stage D declined | Local brief complete; network checks Deferred | Use Helm Now, Review Deferred | Deferred items persist without failure count | One offline summary |
| Cancelled | User stops scan | Partial findings remain; no changes made | Resume Scan, Use Helm Now | Persist cursor where contracts permit | `Discovery stopped; partial results retained.` |
| Service failed | Local coordinator unavailable | Cached data if any, service error attribution | Retry, Diagnostics, Use Cached | No fabricated manager failures | One failure summary |

## Workflow Matrix: Recommendation and Reviewed Plan

| State | Visible contract | Valid action behavior |
|---|---|---|
| No recommendation | Explain environment is ready or no safe typed action applies | Use Helm Now; do not manufacture a plan. |
| Recommendation loading | Existing brief remains; `Preparing recommendations` | Cancel recommendation work if cancellable. |
| Recommendation ready | Observed issue, why it applies, manager/authority, typed action | Review Plan is primary; recommendation is visibly not applied. |
| Plan stale | Current state revision differs from plan input | Revalidate; preserve still-valid choices; execution disabled. |
| Policy blocked | Controlling authority and non-escalation reason | Cannot select/apply; offer compliant alternative if core supplies one. |
| Permission required | Exact just-in-time authorization and dependent branch | May select; confirmation states denial behavior. |
| Offline/deferred | Network-dependent item labeled Deferred | Local items remain reviewable; no generic failure. |
| Empty after filtering | All optional items excluded or no safe action | No Changes; Use Helm Now. |
| Final confirmation | Selected actions, network, authorization, files/settings, verification, rollback | Cancel and one default Apply action; no hidden extra action. |

## Workflow Matrix: Apply, Cancellation, and Verification

| Phase | Success | Failure | Cancellation/interruption | Recovery truth |
|---|---|---|---|---|
| Preflight | Proceed to apply | No mutation; explain changed prerequisite | Cancelled with No Changes | Revise/revalidate plan. |
| Queued | Waiting in canonical order | Queue rejection is Failed, no apply | Cancel removes unstarted work | No rollback needed. |
| Apply | Mark Applied, then verify | State whether no change, unknown change, or partial change | Cancelling until process terminal; later branches not submitted | Never infer rollback from cancellation. |
| Verify | Mark Verified only on observed postcondition | Failed Verification, retain observed state and apply result | Interrupted verification means Unverified, not Failed Apply | Retry verification, restore only if verified inverse exists. |
| Record | Persist receipt and redacted summary | Applied/verified result remains; receipt-record failure is separate | Resume receipt recording without reapplying | Diagnostics and local recovery state persist. |

## Workflow Matrix: Action Receipt

| Receipt status | Required sections | Actions |
|---|---|---|
| No Changes | Inspected scope, unchanged state, coverage/freshness | Copy Redacted Summary, Done |
| Verified | Before, approved actions, after, verification evidence, unchanged state, recovery | Copy Redacted Summary, Export, Done, valid Restore if any |
| Partially Verified | Verified, failed/unverified, untouched, exact coverage | Resume/Retry, Keep Verified, Review Rollback, Diagnostics |
| Failed Verification | Apply result, expected versus observed, rollback limit | Retry Verification, Restore if valid, Keep Change, Diagnostics |
| Cancelled | Applied/verified work, cancelled active work, unstarted work | Keep Changes, Review Rollback, Resume only if contract permits |
| Interrupted | Last durable stage, reconciliation requirement, known/unknown effects | Reconcile/Resume, Retry Unverified, Keep, Diagnostics |
| Failed | Failed stage, no-change/partial/unknown effect classification | Retry if safe, revise plan, Diagnostics |
| Rolled Back | Restored pre-action state, rollback verification evidence, any effects outside the inverse | Done, Diagnostics if residual effects remain |

## Content Requirements by Blocking Class

### Policy blocked

Required:

- `Blocked by policy` state phrase.
- Controlling authority, for example Helm safety policy or organization policy.
- Why escalation is unavailable or which policy owner can change it.
- The observed item remains visible.

Forbidden:

- Administrator lock icon implying local authentication will solve it.
- `Try Again` without a changed policy/precondition.
- Disabled control with no reason.

### Permission blocked

Required:

- `Permission required` or `Authorization denied` phrase.
- Requested scope and why it is necessary.
- Whether denial affects only one branch.
- Retry path after permission changes.

Forbidden:

- Asking for administrator authorization at launch.
- Conflating macOS privacy permission, file permission, and administrator authorization.

## Layout and Continuity Rules

- First useful render reserves stable geometry for status, primary action, and selection list.
- Streaming or polling cannot reset destination, selection, expanded disclosure, filters, sort, scroll anchor, or inspector tab.
- A removed selected entity produces a nonmodal `No longer available` state and returns focus to the collection; it does not select an unrelated row.
- Closing Control Center never means Cancel. Cancellation requires an explicit action.
- A notification deep link revalidates the target and opens the exact Activity/Health context or the nearest valid fallback.
- Error banners do not cover the primary content or duplicate an inspector explanation.
- Healthy and empty states include coverage; zero findings with incomplete coverage is Partial, not Healthy.

## State Fixture Minimum

Before v0.22 UI lock, deterministic fixtures must render at least:

- Health: loading, cached refreshing, healthy, partial, policy blocked, offline, failure.
- Updates: loading, empty, plan ready, stale, running, partial failure, cancelled, failed verification.
- Packages: local results, remote searching, no results, offline deferred, source partial failure, selected inactive window.
- Activity: empty, queued, running, cancelling, cancelled, interrupted, verified, failed verification, recovery.
- Sources: not installed, healthy, disabled, policy blocked, permission blocked, multi-instance attention, setup required, operation progress.
- First run: first useful, complete, no managers, partial, offline, service failure, plan, confirmation, applying, verifying, receipt variants.
- Settings: local save success/failure, managed setting, all current locales at narrow and expanded content.
