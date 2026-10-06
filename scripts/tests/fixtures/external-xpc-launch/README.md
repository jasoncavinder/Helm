# Private XPC Launch Experiment

QA-only, ad-hoc-signed fixtures. They are not linked into Helm and deliberately
do not meet its production Developer ID/notarization requirements. They expose
one fixed read of a random 32-byte sibling sentinel, no path argument or updater
operation. The report contains its digest, not file content.

Compile without running on the host (use a task-owned module cache/output):

```sh
xcrun swiftc -module-cache-path artifacts/module-cache -target arm64-apple-macos13.0 \
  -o artifacts/launch-probe/Host scripts/tests/fixtures/external-xpc-launch/{Probe,Host}.swift
xcrun swiftc -module-cache-path artifacts/module-cache -target arm64-apple-macos13.0 \
  -o artifacts/launch-probe/Probe scripts/tests/fixtures/external-xpc-launch/{Probe,Service}.swift
```

Copy the binaries and `scripts/tests/external_xpc_vm_launch.py` to the disposable
VM, then run **in that VM only**, with a new output directory:

```sh
python3 external_xpc_vm_launch.py --confirm-disposable-vm \
  --host-binary Host --service-binary Probe --output /existing/private/scope/run
```

This stages and launches three fresh sandboxed test hosts. The first and third
must be denied access themselves, must observe the same denial from a directly
spawned helper, and must receive the exact sentinel digest from the private XPC
service. The middle control additionally sandboxes that service and must observe
denial. Only OS-managed private XPC launch differs from the child-process control.
The host/service have 20/30-second deadlines; all runs and their dedicated
`com.jasoncavinder.Helm.XPCLaunchProbe` container are retained. No production
identifier, database, account credentials or updater is used.

This is a launch-mechanics experiment, not proof of general filesystem authority,
production peer authentication, target ownership, or permission to install.
See the [attributed evidence and remaining gates](../../../../docs/validation/v0.20-sparkle-private-xpc.md).

## Signed Peer Controls

`Impostor.swift` is a separate, deliberately wrong-identity private service for
the signed-peer matrix. Compile with `swiftc -parse-as-library`; it is never
linked into Helm or the real updater. Its only actions are a fake data-free
hello and fixed marker files beside its containing QA app. A marker for target
intent is a failure. It has a 20-second lifetime and receives no database path,
feed or update command.

The [signed VM runner](../../external_xpc_signed_vm.py) consumes already signed,
notarized and stapled QA wrappers, never credentials. It validates Gatekeeper,
strict signatures and each real helper's self-preflight, then runs the private
route's hello, target inspection, changed/missing target, wrong caller/channel/
sandbox, impostor reply and fresh-positive checks. It uses fresh bundle copies
and logs, checks the target's metadata/executable remain unchanged, and requires
that no development database was created. The
[signed evidence record](../../../../docs/validation/v0.20-sparkle-private-peer.md)
lists the inputs, command and exact artifact hashes; do not run this on the host.
