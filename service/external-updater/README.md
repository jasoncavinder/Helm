# External Updater Native Observation

Development foundation only. This package is not embedded in Helm and has no
installer, network request, manager mutation, privileged operation or client
transport. It does not enable direct third-party updates.

`NativeTargetObserver` reads a concrete application beneath `/Applications` or
the current OS account's `Applications` directory. It rejects aliases, nested
apps, paths outside those roots, escaping symlinks, special files, changed
filesystem snapshots and unbounded trees. It checks local ownership/mode/ACL
facts, reads bounded framework metadata, and validates a Developer ID application
signature with Security.framework's strict/all-architectures/nested-code checks.
It does not enable Security's certificate-network flag. Bundle metadata comes
from Security's secured Info.plist, not CFBundle's cached presentation dictionary.
No Command Line Tools or external process is used by the observer.

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

Compile on the host if needed; execute only in the designated VM or CI:

```sh
swift test --package-path service/external-updater
swift run --package-path service/external-updater helm-external-observe /Applications/Example.app
```

Tests use isolated filesystem fixtures and an injected signer for deterministic
race/path/metadata cases. The native unsigned-fixture rejection is separate from
that injected coverage. A real signed-app VM probe and future signed helper
integration must remain separately attributable evidence.
