# External Updater Native Observation

Development foundation only. This package is not embedded in Helm and exposes no
installer operation, feed request, manager mutation, privileged operation or
updater command transport. Its data-free XPC bootstrap and authenticated read-only
app preflight do not enable direct updates. Native signing/notarization trust
evaluation remains under macOS control.

## Standalone Helper Package

`HelmSparkleExternalUpdater` is a separate executable, linked to its own exact
Sparkle 2.9.5 distribution through SwiftPM (including the resolved revision and
upstream binary checksum). It never initializes `SPUUpdater`. Its only commands
are `--preflight` and `--serve-bootstrap`; both require native self-identity
validation and the actual loaded framework to reside inside its own bundle.
It never loads the target application's private updater or framework.

`scripts/package_external_updater.py` stages a **new** unembedded app bundle or
validates its layout. It checks the pinned framework, required nested components,
architectures, Ventura deployment target, concrete executable, bounded tree,
contained symlinks, sealed-metadata contract and confined loader search paths.
It removes SwiftPM's developer-toolchain/loader-directory rpaths only from the
new staged executable. It does not replace an existing output, sign, notarize,
register or execute anything. Its JSON explicitly does **not** attest signature
or notarization. Source inputs must come from the pinned build; structural
checks do not authenticate an arbitrary supplied framework.

Host compilation/staging, without runtime execution:

```sh
HELM_EXTERNAL_POLICY_LIB_DIR="$(bash scripts/build_external_update_bridge.sh arm64 release)"
export HELM_EXTERNAL_POLICY_LIB_DIR
swift build --package-path service/external-updater --arch arm64 -c release
python3 scripts/package_external_updater.py stage \
  --binary service/external-updater/.build/arm64-apple-macosx/release/HelmSparkleExternalUpdater \
  --framework service/external-updater/.build/artifacts/sparkle/Sparkle/Sparkle.xcframework/macos-arm64_x86_64/Sparkle.framework \
  --output /existing/private/qa/HelmSparkleExternalUpdater.app --build 1
```

The package links the private Rust static library, not the application FFI
runtime. Run the build helper again after every Rust change and use its printed
absolute directory for every Swift invocation. It pins macOS 13 for Rust and its
bundled C dependencies, matches the requested architecture/profile and copies
the archive to a content-hashed directory: a changed Rust archive therefore
invalidates SwiftPM's link command instead of silently reusing old executables.
Use `arm64 debug` for the tests below, or `x86_64 release` with Swift `--arch x86_64`.
The corresponding Rust target must be installed. This override is only a build
input, never accepted from an updater client at runtime. CI sets it before both
package tests and the unsigned packaging check; CodeQL uses the same build path.

Signing and Apple submission are separately authorized steps. Sign nested
Sparkle code inside-out with hardened runtime, preserving the downloader's
upstream entitlements, then the framework and helper. The helper itself needs
no entitlements. Do not use deep signing, weaken the requirements or copy
credentials to the VM. Keep the main app's entitlements unchanged.

The bootstrap uses the fixed per-user Mach service
`com.jasoncavinder.Helm.SparkleExternalUpdater.bootstrap`, with the same exact
bidirectional code requirements as the anonymous transport. A manually prepared,
temporary VM-only LaunchAgent is test infrastructure, **not** shipped service
registration. The host accepts at most one admitted connection, re-observes its
own identity before admission and exits within 120 seconds. After the
version/nonce handshake, it accepts at most eight sequential read-only app checks
on the same connection. Each has an 8 KiB strict request and a 15-second deadline.
No feeds, database paths, trusted observations or authorization grants are accepted.
Cancellation is local connection invalidation, not an exported update command.

`helm-external-bootstrap-probe` is a QA test host. Without arguments it tests only
the data-free hello; `--preflight PATH BUNDLE_ID INSTALLED_BUILD` sends bounded
intent only after authenticated readiness and prints a read-only assessment.
Testing real
acceptance requires a separately signed/notarized sandboxed app wrapper with the
exact caller identity/channel and only the named Mach-lookup sandbox exception.
Never install or launch that identity-matching test app on the production host.
Do not ship the probe, fixture wrapper or launch registration. The signed VM
handshake now passes, as do notarized wrong-identity/channel/unsandboxed-caller
and impostor-helper rejection controls. This proves bounded transport acceptance,
not permission to update or a shipping sandbox/launch strategy. Those older
signed binaries do not certify the newly added app request.
See [package evidence and remaining gates](../../docs/validation/v0.20-sparkle-helper-package.md).

## Native Observation And Authentication

`NativeTargetObserver.observeForPolicy(path:)` now sends a successful fresh
native observation directly through a private, synchronous C ABI to the same
Rust gate used by adoption review/resolution. It maps known exclusions to
`OtherManager` and absent markers to `Unknown`, never `Standalone` or
`UserAdopted`. The read-only `helm-external-policy-probe` exposes the diagnostic
with `canUpdate: false`; the original observation probe's output is unchanged.
No JSON/XPC parser can construct trusted observations, roots, signing results or
authority. No caller can set a boundary or candidate through this bridge. A
rejected native observation never becomes an empty successful scan. The native
entrypoint uses OS-account roots, not `HOME`; Rust also validates root shape.

This preflight does not read saved adoption or assert complete manager-exclusion
coverage. Authenticated ledger resolution/revocation, explicit consent UI and
Sparkle-accepted candidates remain separate integration work. A test fixture
exercises mapped evidence against real SQLite adoption, but is not a shipping
database or authorization endpoint. See the
[integration evidence](../../docs/validation/v0.20-sparkle-native-core-bridge.md).

`NativeHelperObserver.observeSelf()` collects the current process through
Security.framework, not a caller-supplied path or PID. It enforces the fixed
notarized helper requirement, validates the running and fresh on-disk signatures,
requires hardened runtime and the signed Developer ID channel, and rejects
entitlement grants. Two observations must agree on bundle/code identity and
account. The helper's bundle tree and all ancestors are also checked for unsafe
ownership, mode and ACL mutation grants and compared around signature validation.
Only root/current-account owners are accepted; group-admin write mode is allowed
only for the root-owned `/Applications` directory. Internal framework links must
resolve inside the bundle. Dangling/cyclic links, special files and set-ID bundle
entries fail closed. The Encodable-only snapshot has no public initializer and grants no
update authority. This is not proof of launch-time sandbox inheritance, installed
helper packaging, accepted signed peers or authority over any target app. Native
trust evaluation is left to macOS; no offline/trust fallback exists. Debug-only
fixture capture is absent from Release builds. See the bounded
[evidence and remaining gates](../../docs/validation/v0.20-sparkle-helper-identity.md).

`NativeTargetObserver` reads a concrete application beneath `/Applications` or
the current OS account's `Applications` directory. It rejects aliases, nested
apps, paths outside those roots, escaping symlinks, special files, changed
filesystem snapshots and unbounded trees. It checks local ownership/mode/ACL
facts, reads bounded framework metadata, and validates a Developer ID application
signature with Security.framework's strict/all-architectures/nested-code checks.
It does not enable Security's certificate-network flag. Bundle metadata comes
from Security's secured Info.plist, not CFBundle's cached presentation dictionary.
No Command Line Tools or external process is used by the observer.
The shared filesystem reader opens permission and metadata descriptors with
`O_NOFOLLOW_ANY`, not just final-component `O_NOFOLLOW`. The helper rejects unsafe
tree permissions; target evidence still reports them without granting authority.
See the [filesystem checks and evidence](../../docs/validation/v0.20-sparkle-path-authority.md).

Evidence is intentionally Encodable-only and always reports unresolved manager
authority. A valid signature or Sparkle framework does not establish standalone
ownership, authenticate a client, prove notarization/App Management consent, or
authorize installation. Mutation-grant ACLs are conservatively unsafe even if
the grant would ultimately resolve only to the current user. `/Applications`
retains its normal administrator-group mode allowance, but not arbitrary
mutation-grant ACLs; other ancestor directories have no such mode exception.

The read-only `helm-external-observe` executable is a VM development probe. Its
JSON includes local paths and app identity, so retain output locally or redact
it before sharing. It is not the shipped Helm CLI and must not be bundled or
advertised as an updater.

`ExternalUpdaterPeerAuthentication` prepares inactive anonymous or fixed-service
XPC listeners and connections with bidirectional macOS code-signing requirements. It
does not expose an updater command protocol. For the bootstrap, the helper
delegate passes each newly accepted, inactive connection directly to
`ExternalUpdaterBootstrapServer`, whose initializer calls `admit` exactly once
before exporting or activating it. Do not call `admit` separately first:
Foundation permits configuring the connection requirement only once. A future
alternative exported object must perform that same setup before activation.
The returned Boolean means gate configuration, not successful peer authentication
or operation authorization. Never use it to populate a trusted boundary
observation. Native message delivery enforces the live requirement.

Incoming peers require the exact sandboxed Developer ID consumer Helm identity,
team and signed distribution field; responses require the exact separate helper
identity/team without a sandbox entitlement. Both require notarization and exclude
debug injection entitlements. There is no environment/preference override, PID
lookup, developer-build allowlist or weaker fallback. Same-account incoming
connections are required and root/setuid process initialization is rejected.
Interruption invalidates the connection instead of silently reconnecting. A
future runtime must quarantine its durable operation on connection loss.

A client uses `ExternalUpdaterBootstrapClient` to complete the authenticated,
data-free nonce/version handshake and validate the peer account before a
read-only app request can be sent. It fails closed on malformed replies, timeout,
cancellation or connection loss, with no reconnect. Readiness is not update
consent, and the session nonce is not an authorization token. Response
authentication alone does
not prove that an outgoing request was never observed by an impostor endpoint.
The preflight API therefore waits for authenticated readiness; it never sends
app intent with the hello or retries it on a new connection. The notarized VM
impostor control demonstrates this distinction: it receives the data-free hello,
but the real client rejects its reply. The accepted/rejected peer tests use a
temporary per-user service and isolated QA app wrappers, not shipping service
registration or a complete inherited-sandbox proof. Unsigned negative/control
tests remain separate evidence.

The new [request contract and evidence](../../docs/validation/v0.20-sparkle-authenticated-preflight.md)
describe the native self/target re-observation and Rust-owned validation. An
`unresolved` result never grants adoption or an update. Inspection results after
cancellation, session loss or either monotonic deadline are discarded; no ledger
or installer is involved in this API.

Compile on the host if needed; execute only in the designated VM or CI:

```sh
swift test --package-path service/external-updater
swift run --package-path service/external-updater helm-external-observe /Applications/Example.app
```

Tests use isolated filesystem fixtures and an injected signer for deterministic
race/path/metadata cases. The native unsigned-fixture rejection is separate from
that injected coverage. A real signed-app VM probe and future signed helper
integration must remain separately attributable evidence.
