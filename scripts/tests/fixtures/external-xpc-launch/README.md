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
