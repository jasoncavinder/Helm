# Native macOS Research and Validation Package

Status: protocol approved; no human participant sessions claimed
Owner checkpoint: required before v0.20 workflow migration and repeated before v0.22 UI lock

## Evidence Status

This package replaces the obsolete v0.13 assumption that a scenario checklist and historical mockups are sufficient validation.

Completed in v0.18 planning:

- Source and repository-capture audit.
- Apple HIG review.
- Expert cognitive walkthrough of the current implementation and target prototypes.
- Participant profile, moderated script, rubric, accessibility protocol, consent/privacy, and success thresholds.

Not completed:

- No human participants were recruited or observed in this design-definition lane.
- No completion rate, preference, comprehension, or accessibility result below is presented as human evidence.

The moderated study is an explicit **owner-run closure checkpoint**. Repository planning closure may complete with this checkpoint open, but v0.20 core-workflow sign-off and v0.22 UI lock may not treat it as passed until actual sessions and findings are recorded.

## Research Questions

1. Can people identify Helm's current condition and next action within 10 seconds?
2. Do people predict that Settings is in the App menu and diagnostics is attached to failed work?
3. Does the approved Original Wayfinder Dashboard/Plan/Library/Activity plus contextual Environment model match task expectations?
4. Can first-time users explain what Helm inspected, changed, did not change, and needs permission/network access?
5. Can people distinguish recommendation, reviewed plan, applied action, verification, and Action Receipt?
6. Can users recover from partial manager failure, failed verification, interruption, and limited rollback without unsafe assumptions?
7. Can keyboard and VoiceOver users complete the same primary jobs without pointer-only or visually inferred state?
8. Do minimum/expanded sizes, light/dark/inactive states, and localization preserve hierarchy and safety content?

## Participant Profiles

Target: 10 moderated participants for the first checkpoint. Accessibility needs can overlap expertise groups.

| Profile | Target | Inclusion criteria | Why included |
|---|---:|---|---|
| Mac package novice | 3 | Uses macOS weekly; little/no Homebrew or multi-manager experience | Tests plain language, first run, and safe default comprehension. |
| Developer/operator | 3 | Uses at least two of Homebrew, mise/asdf/rustup, npm/pip/cargo, or MacPorts | Tests authority, provenance, density, keyboard efficiency, and plan trust. |
| Existing Helm/beta user | 2 | Has used Helm's current popover/Control Center | Tests migration costs, retained mental model, and discoverability regressions. |
| VoiceOver-primary or frequent user | at least 2 total | Regular macOS VoiceOver use | Tests semantics, headings, groups, announcements, and nonvisual recovery. |
| Keyboard/alternative-input user | at least 2 total | Relies on Full Keyboard Access, Voice Control, Switch Control, or reduced pointer use | Tests focus, commands, target sizing, and pointer independence. |

Recruitment balance:

- At least three participants use a non-English Helm locale where feasible, including one German or Hungarian expansion-sensitive language and one Japanese participant/test facilitator.
- Include Apple silicon and Intel familiarity if Intel remains in the supported production matrix; prototype tasks do not require participants to own both.
- Do not recruit only contributors or people who already know Helm's architecture.
- Avoid collecting package lists, usernames, paths, employer identity, or real machine diagnostics.

## Session Format

- 60 minutes remote or in person.
- Prototype/test database contains synthetic package, source, task, and receipt data.
- Moderator uses one script and records prompts verbatim.
- Think-aloud is encouraged but never required for accessibility participants when it interferes with assistive-technology use.
- Participant chooses appearance/input setup; core tasks are repeated under required accessibility settings in the dedicated protocol.
- Record screen/audio only with separate explicit consent. Notes alone are sufficient.

## Moderated Task Script

### Opening, 5 minutes

Moderator says:

> We are evaluating Helm's design, not you. Some parts are prototypes and may not work. Please say what you expect before selecting something. I may ask what you think a label means, but I will not teach you where commands live. You can stop at any time.

Questions:

- How do you currently keep third-party tools and packages updated on a Mac?
- What would make you trust or distrust an app that can change developer tools?
- Which input and accessibility settings should we preserve for this session?

### Task 1: Ambient health, 5 minutes

Starting state: Helm is backgrounded; popover has partial current data and one failed verification.

Prompt:

> Without opening a large window at first, determine whether Helm needs your attention and what happened most recently.

Observe:

- Status icon interpretation without relying on color.
- Popover condition/freshness comprehension.
- Whether participant finds the exact failure and opens Activity context.
- Navigation reversals and duplicate-surface confusion.

Success:

- Identifies failed verification, partial coverage/freshness, and Review action without assistance.

### Task 2: Review updates, 8 minutes

Starting state: 12 updates across authoritative, standard, and guarded sources; one pinned; one requires authorization; OS update excluded.

Prompt:

> Review what Helm proposes. Explain the order, what will not run, whether authorization or restart may be needed, and then prepare to run only the non-OS plan. Stop before final confirmation until I ask.

Observe:

- Finds Updates.
- Understands authority stage order and pin exclusion.
- Distinguishes plan review from applied work.
- Uses table/inspector and final confirmation.

Success:

- Correctly explains order/exclusion/consequences and reaches the right confirmation without accidental apply.

### Task 3: Find and install a package, 7 minutes

Starting state: local cached result appears immediately; remote alternate arrives later; offline variant is available.

Prompt:

> Find `ripgrep`, determine which source Helm recommends and why, and start an install through that source.

Observe:

- Command-F expectation and local/remote distinction.
- Source/member selection.
- Context menu versus inspector discoverability.
- Offline deferred understanding in variant.

Success:

- Selects the intended source, identifies cached/remote status, and reaches correct bounded confirmation.

### Task 4: Recover from failure, 8 minutes

Starting state: one update applied but verification failed; another source never started.

Prompt:

> Find the problem, determine what changed, and choose the safest next step. Explain whether Helm can roll everything back.

Observe:

- Activity versus Dashboard findability.
- Applied versus verified distinction.
- Before/after and rollback-limit comprehension.
- Retry Verification, Restore, Keep, and diagnostics interpretation.

Success:

- Does not call the action fully successful; states rollback limits accurately; chooses valid recovery.

### Task 5: Inspect a source, 7 minutes

Starting state: two rustup installations, one policy-blocked system path, one active user path.

Prompt:

> Determine which rustup Helm will manage, why, and what you can change. Keep both installations, then find how you would revisit that decision.

Observe:

- Finds Environment and active provenance.
- Understands policy blocked versus permission blocked.
- Uses instance/context commands and acknowledgment.
- Finds persistent/default preference in Settings only when appropriate.

Success:

- Identifies active source and controlling policy; completes acknowledgment without trying to authorize a policy block.

### Task 6: Find Settings and diagnostics, 5 minutes

Prompt:

> Change launch-at-login, then return to the failed verification and copy redacted diagnostics.

Observe:

- Command-Comma/App menu expectation.
- Predicts that live diagnostics are not a Settings preference.
- Focus restoration between windows.

Success:

- Changes setting in Settings and obtains diagnostics from Activity without assistance.

### Task 7: Project WOW first run, 10 minutes

Starting state: fresh synthetic profile; local scan has one partial source failure and one safe recommendation.

Prompt:

> Start Helm for the first time. Use it as you naturally would. When you reach a recommendation, explain what has happened so far and what will happen if you continue. Apply the safe improvement and show me proof of the result.

Observe:

- Trust-label comprehension.
- Time to first useful result.
- Network/mutation assumptions.
- Environment Brief, plan, progress, verification, and receipt comprehension.
- Reaction to partial failure and optional Use Helm Now.

Success:

- Correctly explains inspected/changed/unchanged/network state, applies only after review, and finds verified receipt.

### Closing, 5 minutes

- What is Helm for, in your own words?
- Where would you look next time for updates, active work, manager/source choices, Settings, and diagnostics?
- What did Helm change in the first-run task? What did it not change?
- Which state or action felt least trustworthy?
- Did any part feel unlike a Mac app? Why?

## First-Run Comprehension Questions

Ask at first useful result, before plan apply, and after receipt:

1. What has Helm inspected so far?
2. Did Helm use the network? What tells you that?
3. Has Helm changed anything? What tells you that?
4. Are these results complete? Which sources are current, cached, failed, or deferred?
5. Is this recommendation already applied?
6. What files/settings could the plan change?
7. Will macOS request authorization? At what point?
8. How will Helm decide the action succeeded?
9. If you stop now, what remains changed?
10. Which changes can Helm restore, and which cannot it roll back?
11. What evidence does the Action Receipt provide?
12. What would be included in a copied summary by default?

Score each answer: Correct, Partially correct, Incorrect, or Not answered. A dangerous misconception about mutation, privilege, verification, or rollback is always severity S1 or higher regardless of task completion.

## Observation Record

For each task capture:

- Start/end time and completion status.
- First click/command and navigation path.
- Assistance level: none, neutral prompt, directional prompt, takeover.
- Navigation reversals and dead ends.
- Incorrect actions and near misses.
- State interpretation in participant's words.
- Focus/VoiceOver issues and workarounds.
- Window resize/inspector/sidebar behavior.
- Confidence 1-5 and trust rationale.
- Exact quote only when consent allows.

Do not infer intent from silence. Ask a neutral follow-up such as `What do you expect this to do?`

## Severity Rubric

| Severity | Definition | Examples | Required response |
|---|---|---|---|
| S0 Critical | Enables unsafe mutation, data loss, false authorization/rollback belief, or blocks all release-critical use for an assistive-technology group | Recommendation mistaken as applied; irreversible action labeled reversible; keyboard trap in legal/confirmation sheet | Stop affected implementation/release; redesign and retest. |
| S1 High | Prevents primary job completion without assistance or causes materially wrong state/consequence understanding | Cannot find failure recovery; Applied mistaken for Verified; Settings/diagnostics impossible with keyboard | Must resolve before milestone exit. |
| S2 Medium | Causes delay, repeated reversals, or workaround but task completes safely | Search destroys origin; inspector hides needed context; long label truncates | Resolve or explicitly schedule before v0.22 with owner. |
| S3 Low | Noticeable inconsistency or polish issue with little task impact | Minor spacing, noncritical wording hesitation | Fix during fit/finish when economical. |
| S4 Note | Preference or idea without observed task impact | Alternate icon preference | Record; no automatic backlog item. |

Escalation modifiers:

- Raise one level when repeated by 3+ participants.
- Raise one level when it affects VoiceOver, keyboard-only, Increased Contrast, Reduce Motion/Transparency, or a non-English locale and no equivalent path exists.
- Never lower safety misunderstanding because the participant eventually completed the task.

## Accessibility Protocol

Run at least these dedicated passes on a production-fidelity prototype/build:

### Full Keyboard Access

- Enable macOS Full Keyboard Access.
- Complete all seven tasks without pointer use.
- Record every Tab/Shift-Tab sequence, arrow behavior, default/cancel behavior, focus loss, and focus restoration.
- Resize to minimum and expanded widths and repeat sidebar/inspector navigation.
- Verify app-menu and context-command parity.

### VoiceOver

- Use participant's normal verbosity where applicable.
- Inspect window title, destination heading, toolbar, source list, content collection, inspector groups, sheets, alerts, progress, receipts, and output.
- Use VO rotor for headings, controls, tables, and windows.
- Validate streaming Environment Brief announcements with no row storm.
- Confirm live output is silent until entered/read.
- Confirm deep links land on the intended selected object and focus target.

### Appearance and motion

- Increased Contrast on, then Differentiate Without Color if available.
- Reduce Motion and Reduce Transparency on.
- Light and dark, key and inactive windows.
- Verify focus/selection distinction and all state words/icons.

### Localization/text expansion

- German, Japanese, Hungarian, and +40% pseudolocalized fixtures at minimum width.
- Inspect final confirmation, policy/permission block, failed verification, rollback limit, and receipt.
- No clipped safety content or unreachable action.

### Alternative input

- At minimum, keyboard-only plus VoiceOver passes.
- If Switch Control/Voice Control participants are available, run Tasks 1, 4, 6, and 7 and verify all custom actions expose stable names.

## Success Thresholds

Owner-run checkpoint passes only when:

- At least 8 of 10 participants identify Helm's value after two minutes of first run.
- At least 8 of 10 accurately identify what Helm inspected, changed, and did not change.
- At least 8 of 10 recover from the injected source failure or failed verification without unsafe assistance.
- At least 8 of 10 find Updates, package install, Activity recovery, Sources provenance, and Settings without a directional prompt.
- At least 9 of 10 distinguish recommendation, reviewed plan, Applied, Verifying, and Verified.
- Zero participants mistake a recommendation for an applied action.
- Zero participants believe rollback exists where the receipt says it does not.
- Median first-click navigation reversals are <=1 per task; no participant exceeds 3 on a primary job without an S1/S2 finding.
- 100% of release-critical tasks are completable in keyboard-only and VoiceOver protocol runs.
- Zero S0 and zero unresolved S1 findings remain.
- All S2 findings have an owner, milestone, and retest condition.

If recruitment yields fewer than 10 sessions, report raw `n`, do not convert thresholds into percentages that imply adequate power, and keep the checkpoint open.

## Consent and Privacy

- Use synthetic environment fixtures. Do not scan a participant's real machine.
- Explain that Helm prototype data is fictitious and no software mutation occurs unless a dedicated sandbox machine is used.
- Collect the minimum participant profile data needed for segmentation.
- Screen/audio recording requires opt-in separate from study participation; declining recording does not exclude participation.
- Redact names, faces, voices where requested, employer details, paths, usernames, package lists, and accessibility medical details from repository notes.
- Store consent and raw recordings outside the repository under the owner's approved retention policy.
- Repository reports use participant IDs such as P01 and contain only redacted observations.
- Participants may stop, skip a question, or withdraw use of their recording/quotes under the consent terms.
- Do not upload diagnostics, environment composition, or analytics as part of the study.

## Expert Cognitive Walkthrough

Method: desk review of current source, current design-lineage captures, approved IA, and annotated prototypes. Reviewers simulated the six primary jobs plus Project WOW first run using four questions:

1. Will the user try to achieve the right effect?
2. Will the user notice the correct control?
3. Will the user associate the control with the intended effect?
4. After acting, will the user understand progress and outcome?

These are expert findings, not participant results.

| ID | Severity | Current finding | Why task knowledge fails | Prototype response | Human retest |
|---|---|---|---|---|---|
| CW-01 | S1 | Settings appears as sidebar destination, status submenu split, popover overlay, and empty platform Settings scene. | Mac users expect App menu/Command-Comma; operational actions pollute the preference model. | Dedicated Settings and command; move operations to Health/Activity/Sources. | Task 6 first-click and keyboard route. |
| CW-02 | S1 | Fixed window and always-visible inspector prevent user-controlled work layout. | Users cannot apply standard resize/minimize/zoom expectations; narrow-display strategy is absent. | `860x600` minimum, adaptive inspector, restorable panes. | All tasks at minimum/expanded and multi-display. |
| CW-03 | S1 | Configuration wizard and spotlight tour precede personalized value. | User cannot connect setup choices to observed machine state and may assume scan/mutation semantics. | Streaming Environment Brief before Customize; contextual tips only. | Task 7 and comprehension questions. |
| CW-04 | S1 | Generic Completed/Failed task model does not consistently expose Applied versus Verified. | Exit success can be mistaken for observed outcome; recovery truth is hidden. | Explicit Apply -> Verify -> Receipt stages. | Tasks 2, 4, and 7. |
| CW-05 | S1 | Custom scroll/card rows do not guarantee Tab/arrows/focus restoration. | Correct controls may be visually available but unreachable or lose context nonvisually. | Native source list/table and focus-group budget. | Full Keyboard Access and VoiceOver protocols. |
| CW-06 | S2 | Popover contains metrics, managers, task output, search, settings, about, quit, and plan sheet. | Users can enter deep work in a transient surface and lose it on dismissal. | One condition, action, short active work, Open Helm. | Task 1 and interruption during active work. |
| CW-07 | S2 | Overview card grid gives healthy, attention, and failure similar weight. | User must scan all cards to infer priority; partial coverage is absent. | Prioritized Health findings and collapsed healthy summary. | Task 1 time and first selection. |
| CW-08 | S2 | Updates page and upgrade sheet are two different preview depths. | User may execute from the thinner surface without understanding full consequence. | One reviewed plan plus bounded final confirmation. | Task 2 consequence recall. |
| CW-09 | S2 | Packages search navigates while typing and mixes local/cache/remote without explicit scope. | User loses origin and cannot judge completeness/offline behavior. | Search overlay with labeled result provenance; accept navigates. | Task 3 online/offline variants. |
| CW-10 | S2 | Managers cards combine row selection, pointer reorder, enable switch, and action icons. | Hit targets compete; reorder is undiscoverable and pointer-only. | Sources table, inspector commands, keyboard reorder. | Task 5 pointer and keyboard. |
| CW-11 | S2 | Diagnostics is a tabbed large popover from a narrow inspector and is also copied from Settings. | User cannot predict location or maintain window context. | Activity detail/dedicated diagnostics window; Settings only configures export defaults. | Tasks 4 and 6. |
| CW-12 | S2 | Increased Contrast, Reduce Transparency, inactive selection, and offline states lack explicit current handling. | Visual and state cues can become ambiguous outside default appearance/network. | System primitives plus state/quality matrices. | Accessibility and offline protocol. |
| CW-13 | S3 | Pointer cursor is manually forced across many standard-looking controls. | Feels web-like and can imply only hand-cursor regions are actionable. | Remove pointer modifier; native hover and focus. | Qualitative Mac-likeness question. |
| CW-14 | S3 | Settings currently duplicates the CLI card. | Repetition reduces confidence in setting scope and source of truth. | Single CLI pane during Settings migration. | Visual content review. |

## Study Output Template

After real sessions, add a dated report under `docs/validation/` containing:

- Build/prototype commit and fixture version.
- Participant count and profile distribution.
- Tasks completed with assistance counts, raw times, and reversals.
- Comprehension response counts.
- Findings ordered by severity with evidence and participant IDs.
- Accessibility configuration and results.
- Threshold pass/fail without overstating sample confidence.
- Decisions, owners, milestones, and retest conditions.
- Explicit statement of whether the owner-run checkpoint is open or closed.
