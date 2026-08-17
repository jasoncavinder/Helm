# Native macOS Moderated Research Session Record

Status: template only; no participant evidence

Use one copy per participant. Store consent and recordings outside the repository. Keep only the participant ID and redacted observations here.

## Session Metadata

| Field | Value |
|---|---|
| Participant ID | `P__` |
| Session date | `YYYY-MM-DD` |
| Facilitator |  |
| Build version and commit |  |
| Synthetic dataset/fixture version |  |
| Session format | Remote / in person |
| Locale |  |
| Appearance | Light / dark |
| Input method |  |
| Accessibility settings |  |
| Recording consent | None / screen / audio / both |
| Consent retained outside repository | Yes / No |

## Participant Profile

Record only the minimum segmentation data required by the protocol.

| Attribute | Selection |
|---|---|
| Experience group | Mac package novice / developer-operator / existing Helm-beta user |
| VoiceOver use | Primary / frequent / occasional / none |
| Keyboard or alternative input use | Frequent / occasional / none |
| Non-English Helm locale | Yes / No |

Do not record employer, medical details, usernames, paths, real package lists, or machine diagnostics.

## Environment Preconditions

- [ ] The build uses Helm's development database namespace or a dedicated synthetic test host.
- [ ] The dataset contains only synthetic package, manager, task, failure, and receipt data.
- [ ] No participant machine was scanned.
- [ ] No real software mutation is possible, unless this is an approved dedicated sandbox host.
- [ ] The fixture manifest covers the starting state for every task being run.
- [ ] Screen/audio capture matches the recorded consent choice.

## Opening Questions

| Question | Redacted notes |
|---|---|
| Current update-management approach |  |
| Trust or distrust signals |  |
| Input/accessibility setup to preserve |  |

## Task Summary

Use `Complete`, `Partial`, `Incomplete`, or `Skipped`. Assistance is `None`, `Neutral`, `Directional`, or `Takeover`.

| Task | Start-end | Result | Assistance | Reversals | Confidence 1-5 | Finding IDs |
|---|---|---|---|---:|---:|---|
| 1. Ambient health |  |  |  |  |  |  |
| 2. Review updates |  |  |  |  |  |  |
| 3. Find/install package |  |  |  |  |  |  |
| 4. Recover from failure |  |  |  |  |  |  |
| 5. Inspect a source |  |  |  |  |  |  |
| 6. Settings/diagnostics |  |  |  |  |  |  |
| 7. Project WOW first run |  |  |  |  |  |  |

## Detailed Task Record

Duplicate this section for every task run.

### Task __: __

| Observation | Record |
|---|---|
| Starting fixture/state |  |
| Start/end time |  |
| Completion status |  |
| First click or command |  |
| Navigation path |  |
| Assistance level and exact prompt |  |
| Reversals/dead ends |  |
| Incorrect actions/near misses |  |
| State interpretation in participant's words |  |
| Focus/VoiceOver behavior and workaround |  |
| Window/sidebar/inspector behavior |  |
| Confidence and trust rationale |  |
| Consent-approved exact quote |  |
| Moderator note, clearly separated from observation |  |

## First-Run Comprehension

Use `Correct`, `Partially correct`, `Incorrect`, or `Not answered`. Any dangerous misconception about mutation, privilege, verification, or rollback creates an S1-or-higher finding regardless of task completion.

| # | Question | First useful result | Before apply | After receipt | Notes/finding ID |
|---:|---|---|---|---|---|
| 1 | What has Helm inspected? |  |  |  |  |
| 2 | Did Helm use the network? |  |  |  |  |
| 3 | Has Helm changed anything? |  |  |  |  |
| 4 | Are results complete/current/cached/failed/deferred? |  |  |  |  |
| 5 | Is the recommendation already applied? |  |  |  |  |
| 6 | What files/settings could change? |  |  |  |  |
| 7 | Will macOS request authorization, and when? |  |  |  |  |
| 8 | How will Helm verify success? |  |  |  |  |
| 9 | If stopped now, what remains changed? |  |  |  |  |
| 10 | What can and cannot be restored? |  |  |  |  |
| 11 | What evidence does the Action Receipt provide? |  |  |  |  |
| 12 | What is copied by default? |  |  |  |  |

## Accessibility Passes

| Pass | Tasks run | Result | Finding IDs |
|---|---|---|---|
| Full Keyboard Access |  |  |  |
| VoiceOver |  |  |  |
| Increased Contrast / Differentiate Without Color |  |  |  |
| Reduce Motion / Reduce Transparency |  |  |  |
| Minimum / expanded size |  |  |  |
| Localization / text expansion |  |  |  |
| Alternative input |  |  |  |

## Findings

Use the severity definitions from `docs/app-design/NATIVE_MACOS_RESEARCH_VALIDATION.md`.

| ID | Severity | Task/state | Observed behavior | Impact | Workaround | Retest condition |
|---|---|---|---|---|---|---|
| `R-P__-__` | S0-S4 |  |  |  |  |  |

## Closing Responses

| Question | Redacted response |
|---|---|
| What is Helm for? |  |
| Where are updates, active work, source choices, Settings, and diagnostics? |  |
| What did first run change and not change? |  |
| Least trustworthy state/action |  |
| What felt unlike a Mac app? |  |

## Session Closeout

- [ ] Notes distinguish observation from facilitator inference.
- [ ] Quotes are retained only when consent permits.
- [ ] Names, paths, package lists, employer details, and medical details are removed.
- [ ] No consent document or recording is committed to the repository.
- [ ] Every S0-S2 observation has a finding ID and retest condition.
- [ ] The aggregate checkpoint report was updated using raw counts.
