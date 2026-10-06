# External Sparkle Update Boundary

Status: shared-core policy and native/helper-packaging foundations, 2026-10-02.
This is not a working external updater, a security attestation, or permission to
relax Helm's sandbox. The existing vendor-app handoff remains unchanged.

## Authority and Scope

`helm_core::external_update` defines a deterministic review/confirmation contract.
The versioned `ReviewRequest`, separate `AdoptionRequest` and read-only
`PreflightRequest` accept client intent. Each is bounded to 8 KiB,
rejects unknown fields, and accepts operation identity, target path, bundle ID,
and expected installed/candidate builds, not commands, environment, feed URLs,
signing requirements, roots, or caller-supplied trust decisions.

The future native helper must independently collect target, candidate and boundary
observations. Their Rust types deliberately do not implement `Deserialize`.
Constructing these types or setting a Boolean is not authentication. A helper
must validate its live caller using a system-enforced code-signing requirement,
not trust a PID lookup or a client assertion. The selected helper identifier is
`com.jasoncavinder.Helm.SparkleExternalUpdater`; the policy requires Helm's exact
direct-consumer identity and team, a separately signed/notarized helper, and an
unchanged sandbox on Helm itself. The standalone development helper target is
not embedded in Helm, registered by the product, or permitted to install updates.

Initial eligibility is intentionally narrow: positively standalone or explicitly
user-adopted, signed Sparkle 2 app
bundles in locally authorized `/Applications` or the current user's
`~/Applications`, with HTTPS feeds and Ed25519 keys. Other manager authority,
unadopted unknown provenance, App Store receipts, translocation, writable-by-others
locations, nested app bundles, and Helm's own bundle namespace fail closed.
Native collection must resolve symlinks and assess parents/ownership; lexical
path validation in this model does not replace filesystem authorization.

Candidate observations must come from Sparkle's accepted update, not Helm's
abbreviated inventory. The first boundary admits full ZIP application updates
in the default channel with a bounded length and signature; packages, deltas,
custom channels and other unsupported cases retain Open App. Sparkle remains
responsible for platform compatibility, archive signature checking, installation
and native authorization. Helm must not inject the vendor's embedded updater or
invoke its private installer binary directly.

## Review, Execution and Verification

- Review fingerprints bind the complete request, target device/inode/signing and
  update configuration, exact candidate, and helper identity. Confirmation
  re-observes these facts and expires after 120 monotonic seconds. Drift or a
  backwards clock fails closed. This in-memory consumption is not durable
  single-use authorization; a runtime must add that independently.
- The runtime must record `InstallationWillBegin` before handing control to an
  installer. Download completion only reaches `ReadyToInstall`; installer
  completion only reaches `AwaitingVerification`.
- Cancellation, failure or connection loss after handoff produces `Unverified`,
  never a claim that the app was unchanged. Before handoff the terminal state is
  explicitly before installation, not a guarantee against unrelated actors.
  Native callbacks must not interpret a dismissed Sparkle dialog as cancellation
  if Sparkle has deferred installation.
- Reconciliation requires fresh native bundle/signature/authority observations
  at the exact reviewed path and expected candidate build. Replacement may change
  inode and code-directory hash. Changed identity/team/update configuration or
  an old version remains unverified. A verified version does not prove relaunch
  or attribute all external changes to Helm. Key rotation is conservatively
  unsupported by this initial contract.
- Events and reconciliation are bound to operation identity. Invalid transitions
  leave state unchanged. No automatic retry, rollback, process termination,
  password capture, or elevation fallback exists in this module.

## Required Before Activation

Implement and independently review a native observer and an authenticated,
uniquely identified helper with its own bundled Sparkle framework. Use the
[durable session foundation](../validation/v0.20-sparkle-durable-session.md) for
one-shot authorization, target reservations and conservative loss quarantine.
Complete native installer-quiescence/recovery reconciliation and bounded private
diagnostics. Connect the state machine to real Sparkle callbacks
and shared GUI/CLI receipts; do not expose an automatic capability merely because
this policy module exists. Preserve fallback for unsupported apps.

Use public peer-code-signing APIs available on the Ventura baseline; validate
actual signed/notarized artifacts, distribution entitlements and sandbox access.
Run representative real download/install/relaunch/version, cancellation, stale
review, authorization denial, helper loss and recovery checks in the VM. Native
App Management consent must remain under macOS control. Packaging and release
mutation approvals remain separate from implementing this contract.

The release gate stays open until this integration and evidence are complete.

### Helper Self-Observation

The native package's [helper-identity observer](../validation/v0.20-sparkle-helper-identity.md)
now obtains the live process from `SecCodeCopySelf`, validates the fixed notarized
helper requirement and compares fresh native signing/bundle observations. It
requires hardened runtime, signed Developer ID channel metadata and no helper
entitlement grants. It does not accept a caller-selected path, PID or requirement.
Its non-deserializable evidence is only one input to the future runtime boundary:
it neither authenticates an XPC caller nor establishes target authority, sandbox
inheritance, installer quiescence or permission to execute an update. No trusted
`BoundaryObservation` is synthesized from this evidence alone.

The [helper filesystem boundary](../validation/v0.20-sparkle-path-authority.md)
also checks the helper's complete bounded bundle tree and every parent through
the filesystem root. Only root/current-account ownership is accepted; mutation
grants in mode bits or ACLs fail closed, apart from `/Applications`' existing
root-owned admin-group mode allowance. Full filesystem snapshots must agree
around signing validation and across the two self-observations. Contained,
resolvable framework symlinks remain supported; permission/metadata descriptor
opens refuse aliases in any component. This is a read-only observation, not an
atomic lease, a shipping installation-root policy or proof of manager provenance.
Re-observation at future review/confirmation/handoff boundaries remains required.

### Native Competing-Manager Observation

The [native manager observer](../validation/v0.20-sparkle-manager-authority.md)
adds read-only exclusion facts to `NativeTargetEvidence`. It reads Homebrew's
installed moved-app references at `/opt/homebrew/Caskroom` and
`/usr/local/Caskroom` without running brew or evaluating cask definitions. Exact
absolute/relative link destinations must identify the target path; names or
bundle IDs alone do not match. The scan never follows those links or descends
into app payloads. Standard-prefix scope is fixed locally, not supplied by XPC,
HOME, PATH, manager preferences or cached package inventories.
Reference normalization collapses path components in memory only, without
filesystem standardization, tilde expansion or symlink resolution. All scanner
URL construction supplies explicit directory hints so Foundation cannot infer
directory status by consulting artifact destinations outside the scan.

Each scan has a shared 10,000-entry bound. Directory opens refuse symlinks in
every component, and missing roots differ from unreadable/unsupported paths.
Directory/entry identity and link text snapshots must agree across target
signature validation. Existing receipt containers, even empty or aliased ones,
conservatively exclude App Store candidates based on the already checked bundle
tree. Apps beneath an authorized application's `Setapp` directory are excluded
on a component boundary. These markers are not authenticated receipts or a
complete provenance catalog. Group-writable cask metadata may supply a denial
signal; it can never authorize installation.

The [receipt/Setapp follow-up](../validation/v0.20-sparkle-receipt-exclusions.md)
queries the OS receipt service through fixed `/usr/sbin/pkgutil` arguments. The
[payload follow-up](../validation/v0.20-sparkle-payload-receipts.md) now queries
every entry in the inspected native bundle tree, not only the app root,
Info.plist and executable. Repeated options batch at most 32 paths per child;
every echoed path and XML document must be present in order. At most four file-info
children run at once, with deterministic result ordering and joined completion.
Each file-info child has closed
input, a sanitized environment, a one-second deadline and a combined 64 KiB
output bound. A pass has at most 4096 paths, 4 MiB of file-info output and a shared
three-second monotonic budget including catalog queries and parsing. The catalog
uses at most 1024 root-volume receipt IDs, exact ID/volume metadata binding and
exports for overlapping non-root install locations. Those locations must be
applied to relative payload paths; the VM's file-info lookup alone omits such
claims. Overlap and payload containment use fixed-locale case-folded, canonically
composed lexical keys. These conservatively exclude case-equivalent claims even
on case-sensitive volumes; they are not filesystem identity or permission evidence.
Exact catalog/export binding and raw-reply drift comparisons remain unchanged.
Catalog output has a separate 4 MiB aggregate cap, with 256 KiB for the
ID list, 64 KiB metadata batches and 2 MiB per relevant export. Malformed,
incomplete, oversized, failed or changed replies reject
the observation. Both receipt snapshots must agree across signature validation;
the final bundle/ancestor checks run after those external queries. Existing
Setapp framework, public-key and resource-bundle markers in the checked tree
also deny adoption, even outside a Setapp folder. Receipt IDs/SDK markers are
conservative exclusions, not authenticated historical installation provenance.

The [MacPorts follow-up](../validation/v0.20-sparkle-macports-ownership.md)
also excludes apps under `/Applications/MacPorts` on a case-folded component
boundary, and exact/payload claims from the standard
`/opt/local/var/macports/registry/registry.db`. Both registered and actual paths,
including retained inactive records, are conservative exclusions. A prefix
collision or a general parent directory claim is not an app-specific match.
Native queries use fixed `/usr/bin/sqlite3` arguments, safe/read-only mode,
disabled startup configuration, query-only/untrusted-schema settings and a
bounded constant query. No port command, dynamic SQL, database creation,
migration, checkpoint or journal recovery is performed.

Only schema 1.215 is supported. Registry input is limited to 64 MiB, 100,000 file
rows, 4 MiB output and a one-second child deadline. Path components and sidecars
are descriptor-inspected without following symlinks. Immutable SQLite reads avoid
creating sidecars; nonempty WAL/rollback journals reject rather than hide pending
claims. Empty journals and at most 1 MiB of existing shared-memory metadata are
allowed and bound into the filesystem snapshot. Missing registries with orphaned
sidecars, unsupported schemas, malformed rows, query errors and limits fail closed.
Before/after identities and raw query output must agree across target signature
validation. This is observation, not an atomic exclusion of other writers or
authenticated ownership. MacPorts absence never supplies standalone provenance
or complete manager coverage. Custom prefixes, configuration overrides, legacy
registries and alias spellings outside the inspected scope remain unresolved.

The only dispositions are `otherManager` and `unresolved`; there is deliberately
no native standalone grant yet. Nonstandard Homebrew prefixes, root-location
receipts for historical/deleted paths or alternate symlink spellings absent from
the inspected tree, receipt-less custom/managed installations, marker-less
Setapp integrations and other authority sources still require coordinator-owned
resolution. Even a successful empty scan keeps
`requiresAuthorityResolution` true. Re-observation is not an atomic lease; the
future runtime must repeat it and bind positive provenance/candidate/durable
facts before confirmation or installation. The operational release gate remains
open, and no automatic capability or new wire method is exposed.

### Explicit Per-App Adoption

The owner-approved [adoption contract](../validation/v0.20-sparkle-adoption.md)
adds a distinct `UserAdopted` authority, not a new native `Standalone` assertion.
The coordinator must first successfully collect fresh native evidence, preserving
all known manager exclusions; incomplete/unreadable collection is not adoptable.
Only then may it offer the separate explicit per-app review. Migration 24 stores
that local consent/revocation history independently of update sessions and task
history. Unknown apps without consent still require Open App.

Adoption confirmation consumes a 120-second in-memory review binding the exact
observed target and authenticated boundary. Ongoing permission binds the path,
bundle ID, team, key, feed and Sparkle major version; version/inode/cdhash remain
bound by each new review instead of requiring re-adoption after every vendor
update. Changed identity or configuration requires a new adoption review. The
permission does not attest to historical ownership or enable unattended updates.

The [existing-only core adoption path](../validation/v0.20-sparkle-ledger-adoption.md)
is a separate native-integration prerequisite, not a helper wire capability.
Preparation reads an already-current ledger without migrations. Confirmation
consumes the review before 120 seconds, rechecks fresh target/boundary evidence
and the exact database path, then atomically compares epoch/revision, safe mode,
consent-ID reuse and active path/inode reservations before appending consent.
Missing, corrupt or incompatible storage is never repaired or recreated.
The future authenticated coordinator must retain its private filesystem lease
and admit confirmation while the session is live; core does not collect or
authenticate native observations. A storage error/lost reply is not retry
permission. No adoption grant approves an update session or an installer.

The [private adoption bridge](../validation/v0.20-sparkle-adoption-bridge.md)
transports native target and boundary evidence through a synchronous C ABI.
The six boundary facts (live authenticated caller, valid Developer ID signatures,
notarization, preserved Helm sandbox, unsandboxed helper, direct channel) have no
default-success inference; absent or unknown flags reject the request. These are
trusted local observations, never deserialized IPC assertions. In particular,
the ABI does not prove inherited sandbox state or complete manager coverage.

Preparation returns an opaque owned pointer, not a serializable review/token.
Confirmation consumes it on all outcomes and returns only 51 (consent recorded),
52 (review changed/rejected), or 53 (uncertain outcome). No result authorizes an
update. The internal Swift owner clears its handle before entering core, frees
abandoned reviews, and also consumes attempts rejected by local target mapping
or account-root drift. The owner is worker-confined, not a concurrency guard.
That bridge slice adds no XPC method or production boundary-evidence constructor;
the later private-service boundary below supplies connection-scoped facts. The
authenticated coordinator must bind review lifetime to one connection, retain
the private existing-only lease, enforce deadlines/cancellation and fresh native
checks, admit the commit once, then suppress success after failed postchecks or
session loss. Uncertainty requires fresh history and a new human decision, not
automatic replay.

The [staged adoption coordinator](../validation/v0.20-sparkle-adoption-coordinator.md)
implements that lifecycle through `reviewAdoption`/`confirmAdoption`, but the
production server supplies no provider and rejects both methods. A Debug-only
server initializer accepts injected local observations for isolated tests; no
runtime preference, environment flag or wire field enables that path. The client
does not auto-confirm or retry. A successful hello advertises no adoption capability.

Review intent contains only version, consent ID, target path, bundle ID and build.
Core rejects unknown/duplicate fields, invalid roots and Helm self-update intent
before native inspection. A 32-byte random handle maps to one worker-confined
review on one connection; new/malformed reviews replace the pending review, and
every confirmation attempt consumes it. Handles cannot cross connections or
substitute for revocation handles. Methods share the eight-request ceiling,
8 KiB request limit and 15-second request/120-second session deadlines with the
existing diagnostics and revocation operations. Code 50 is a review handle;
51/52/53 are consent-recorded/review-changed/outcome-unknown, never update permission.

The processor requires complete local ownership observations, rechecks the entire
native snapshot/account root before preparation and confirmation, and holds the
private existing-only filesystem lease around the core calls. Snapshot equality
includes raw manager evidence, not just a summarized allow/deny result. Admission
is synchronous on the connection gate queue immediately before entering core;
the native worker never blocks timeout/invalidation handling. After admission,
failed observation/lease checks return uncertainty. Connection loss clears the
pending review and suppresses late success; a possibly committed grant is not
automatically replayed. The coordinator itself supplies neither native boundary
nor ownership proof: operational enabling and signed-helper grant checks remain open.

The store atomically rechecks the current grant at session claim, install handoff
and successful version verification. Revocation, including before the first grant,
fences pending adoption reviews; re-adoption changes the update-review fingerprint.
A random database epoch prevents reset/downgrade from reusing old revisions.
These are local concurrency/durability protections, not tamper-proof storage or
protection against restoring a historical database backup. A handoff committed
before revocation still requires normal installer completion/recovery handling.

This core slice exposes no helper wire method, GUI/CLI command or automatic
capability. A [private native-to-core preflight bridge](../validation/v0.20-sparkle-native-core-bridge.md)
now maps successful local observations into the exact shared adoption target gate.
It accepts neither wire evidence nor a client authority override; its diagnostic
`unresolved` result grants no permission and does not consult saved consent.
The [authenticated preflight request](../validation/v0.20-sparkle-authenticated-preflight.md)
now delivers that diagnostic after native peer readiness, without opening a ledger
or accepting serialized observations. Operational authenticated ledger/session integration, completeness policy for additional manager
claims, adoption/revocation UX, actual accepted candidate and shipping-helper
integration remain open under #607. Do not turn `unresolved` into an adoption
grant or trust an output receipt supplied by the client.

### Standalone Package And Bootstrap

The [private ledger boundary](../validation/v0.20-sparkle-private-ledger.md) adds
an explicit native helper preparation command, not an XPC mutation. Its fixed
OS-account namespace is separate from Helm's app/development databases. Native
owner-only storage, descriptor revalidation and a cooperative exclusive lock
surround strict core initialization/reopen. Missing or incomplete existing
storage, unsafe sidecars and drift fail closed; no automatic reset, consent
import or authority grant occurs. The hello and ordinary app preflight remain database-free.
This is not tamper-proof storage against the same account/root or historical
backup restoration, nor an atomic exclusion of unrelated processes. Native
session integration must retain the lease around every future ledger operation,
not cache a path or expose a bare store after validation.

The separate [authenticated consent-history method](../validation/v0.20-sparkle-consent-status.md)
uses that lease in existing-only mode. Fresh helper/target observations surround
the read. Its six advisory states expose neither consent IDs nor epoch/sequence
tokens and never resolve `UserAdopted` authority. SQLite opens read-only with
no-follow, validates the current migration ledger/epoch/tables and uses a read
transaction that includes committed WAL content. It performs no schema/consent
mutation or automatic recovery. SQLite may still use normal WAL/SHM bookkeeping;
this is not an immutable-file or tamper-proof guarantee. A result is history at
the read snapshot, not permission valid until another refresh. Preflight and
history share the eight-request budget, sequence, single-flight and deadlines.

The [denial-only revocation methods](../validation/v0.20-sparkle-consent-revocation.md)
share that same gate. Review accepts only a strict allowed-root app path and
request ID, not target trust assertions, a database path or a ledger revision.
It need not revalidate app eligibility: a missing or newly manager-owned app's
permission must still be removable. The helper retains the reviewed epoch and
path revision behind one random, connection-local, single-use handle. Confirm
rechecks helper/account/storage and admits the mutation only while the request
and session are live. Core atomically compares epoch/revision and appends a
revocation; newer consent is never silently revoked by an older review.
Cancellation before admission denies writes. Admission before cancellation can
commit with no delivered result; report uncertainty and never blindly replay.
This does not abort an installer already handed off or free its reservation.
Granting consent, accepted candidates and live installers remain gated.

The [helper package](../validation/v0.20-sparkle-helper-package.md) links its own
exact Sparkle 2.9.5 framework and checks the actual loaded framework path against
its signed bundle. Staging validates Ventura slices, framework completeness,
bounded/contained filesystem entries and fixed metadata, then removes developer
loader paths from the copied executable before separately authorized signing.
Structural validation does not substitute for upstream artifact checksums,
codesign or notarization. `SPUUpdater` is never initialized in this foundation.

The fixed per-user bootstrap Mach service has no caller-selected name or trust
override. Both endpoints use the existing exact requirements and account checks.
The helper re-observes itself, admits at most one connection, and bounds lifetime
to 120 seconds; the existing five-second handshake deadline still applies.
The first exchange contains only protocol version/random challenges. The current
helper additionally accepts a bounded read-only preflight on that same connection
after readiness; no target data is sent before an authenticated helper reply.
Cancellation is local connection invalidation, not installer cancellation. Temporary
VM-only launch registration and the sandboxed test-host wrapper are QA fixtures,
not a product launch strategy or shipping entitlement change. Successful native
peer acceptance cannot be inferred from compilation, unsigned tests or successful
structural packaging. The separately notarized arm64 VM test now passes actual
helper self-observation and valid peer acceptance, plus notarized wrong-caller
identity/channel/sandbox and impostor-helper rejection. The impostor receives
the non-sensitive hello but cannot return a trusted reply. Do not send operational
data before authenticated readiness. These bounded tests do not establish a
shipping launch strategy, target authority or inherited-sandbox proof.

## Verification

### Staged Private XPC Launch

The [private-service slice](../validation/v0.20-sparkle-private-xpc.md) adds an
optional `.xpc` package, not automatic embedding or registration. Its sealed
configuration is `XPC!` with exactly `XPCService/ServiceType = Application`, no
helper entitlement grants, and runtime placement under an app's
`Contents/XPCServices`. Code, account, filesystem and loaded-framework checks
remain mandatory. Signed bundle shape is not itself live sandbox evidence.

The private connection uses only `com.jasoncavinder.Helm.SparkleExternalUpdater`.
`NSXPCListener.service()` cannot apply a listener-level code requirement; each
new connection therefore receives the exact production caller requirement in
the server constructor before export/activation. The client retains the exact
notarized helper requirement and data-free handshake before target intent.
Anonymous/Mach-service test transports keep their existing listener gates.
The 120-second process deadline is armed before resuming the service listener.

Apple documents private XPC services as independently sandboxed system-launched
processes; directly spawned children instead inherit the parent's restrictions.
The ad-hoc VM experiment confirms this distinction for a fixed isolated read,
including a sandboxed-service negative control. It does not populate a native
`externalHelperUnsandboxed` fact or close the production signed-launch gate.
No adoption provider, installer, auto-retry or shipped sandbox exception is added.

The subsequent [signed private-peer matrix](../validation/v0.20-sparkle-private-peer.md)
uses the real helper nested inside notarized sandboxed QA callers, without any
Mach-lookup entitlement. The unchanged production requirements accept the valid
handshake/read-only preflight and reject wrong identity, channel and sandbox
controls. A wrong-identity service receives the data-free hello but no target
intent; a fresh valid connection then succeeds. This closes the bounded signed
private-transport gate on the tested arm64 VM, not native boundary-fact mapping,
complete ownership observation, operational adoption or installation. Neither
these test results nor a successful hello can supply an authority flag at runtime.

### Live Private-Service Boundary

The [runtime-binding follow-up](../validation/v0.20-sparkle-live-boundary.md)
owns `NSXPCListener.service()` inside the native library. Only that listener's
delegate creates a private acceptance context, after fresh helper identity
validation; the server still configures the exact production caller requirement
before export/activation. `NSXPCConnection.current()` must match that accepted
connection synchronously on hello and every exported request, before dispatch.
Configuring a requirement or directly calling an exported method is not delivery.

After a valid hello, worker observations are bracketed by native helper
revalidation and connection/account/lifetime checks. Local cancellation and
invalidation/interruption callbacks close the boundary immediately, independent
of the server queue's later result cleanup. Monotonic expiry also rejects when
the timer has not fired. Existing per-request budgets, deadlines, cancellation
and one-time mutation admission remain mandatory and independent.

The internal boundary maps the delivered fixed caller requirement plus fresh
native self-evidence and owned private-service launch into the six ABI facts.
The unsandboxed-helper fact combines actual service-listener provenance with
sealed service configuration/placement and absent helper entitlements, not the
absence of entitlements alone or a test result. It does not imply unrestricted
filesystem access, bypass native App Management/TCC or authorize a target.
Mach/anonymous transport and standalone self-preflight do not create these facts.
No path, PID, Boolean, peer identity or launch assertion is accepted from IPC.

This mapping currently brackets private-route diagnostic/revocation work only;
the mapped value is not passed to the grant bridge. The production adoption
provider remains nil until complete supported ownership and coordinator wiring
are reviewed. No new XPC method, shipping embedding or installer is introduced.

### Earlier Policy Evidence

Twelve new pure-policy tests cover strict requests, roots, helper/target/candidate
eligibility, identity drift, expiry, operation isolation and uncertain outcomes.
All 821 core unit tests pass in the disposable macOS 27 arm64 VM. Host workspace
Clippy and formatting checks pass; no host runtime or app launch was used. The
first VM attempt lacked four existing Bundler fixtures; after transferring the
fixture tree, the complete unchanged suite passed. This is not live Sparkle
installation, signed-helper, Ventura-runtime or Intel certification.

## References

- [Sparkle: updating other bundles](https://sparkle-project.org/documentation/bundles/)
  distinguishes host and application bundles and recommends separate updater
  processes for other applications.
- [Sparkle updater delegate](https://sparkle-project.org/documentation/api-reference/Protocols/SPUUpdaterDelegate.html)
  provides candidate/permission hooks; Sparkle's checks and deferred-installation
  behavior must be preserved rather than inferred from dialog dismissal.
- [Sparkle sandboxing](https://sparkle-project.org/documentation/sandboxing/)
  documents installation XPC services and sandbox boundaries.
- [Apple peer code-signing requirements](https://developer.apple.com/documentation/foundation/nsxpcconnection/setcodesigningrequirement(_:))
  provide the native connection authentication primitive for the future helper.
