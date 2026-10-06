#!/usr/bin/env python3
"""Run pre-signed private-XPC QA copies in the disposable VM only; no updates."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import plistlib
import shutil
import subprocess


def command(args, **kwargs):
    return subprocess.run(args, capture_output=True, text=True, timeout=35, **kwargs)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check_app(app, logs):
    for name, args in [
        ("signature", ["/usr/bin/codesign", "--verify", "--strict", "--deep", str(app)]),
        ("gatekeeper", ["/usr/sbin/spctl", "--assess", "--type", "execute", "--verbose=4", str(app)]),
    ]:
        result = command(args)
        (logs / (name + ".txt")).write_text(result.stdout + result.stderr)
        if result.returncode != 0:
            raise RuntimeError(f"{name} rejected {app.name}")
    info = plistlib.loads((app / "Contents/Info.plist").read_bytes())
    expected_id = "com.jasoncavinder.Helm" + (".WrongCaller" if app.stem == "WrongCaller" else "")
    expected_channel = "mas" if app.stem == "WrongChannel" else "developer_id"
    if info.get("CFBundleIdentifier") != expected_id or info.get("HelmDistributionChannel") != expected_channel:
        raise RuntimeError("caller control metadata mismatch")
    result = command(["/usr/bin/codesign", "--display", "--entitlements", ":-", str(app)])
    (logs / "caller-entitlements.txt").write_text(result.stdout + result.stderr)
    if result.returncode != 0:
        raise RuntimeError("caller entitlement inspection failed")
    entitlements = plistlib.loads(result.stdout.encode()) if result.stdout.strip() else {}
    expected = {} if app.stem == "UnsandboxedCaller" else {"com.apple.security.app-sandbox": True}
    if entitlements != expected:
        raise RuntimeError("caller control entitlements mismatch")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--confirm-disposable-vm", action="store_true", required=True)
    parser.add_argument("--payload", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--target", type=Path, required=True)
    args = parser.parse_args()
    if platform.system() != "Darwin" or not args.confirm_disposable_vm or os.getuid() == 0 or os.getuid() != os.geteuid():
        parser.error("run only as the unprivileged account in the designated disposable macOS VM")
    scope = args.output.absolute()
    scope.mkdir(mode=0o700)
    payload = scope / "bundles"
    shutil.copytree(args.payload, payload, symlinks=True)
    expected_apps = {name + ".app" for name in ["Valid", "WrongCaller", "WrongChannel", "UnsandboxedCaller", "ImpostorHelper"]}
    if {app.name for app in payload.glob("*.app")} != expected_apps:
        raise RuntimeError("unexpected or missing signed control")
    if any(payload.glob("impostor-*")):
        raise RuntimeError("impostor evidence must start empty")
    target_info = args.target / "Contents/Info.plist"
    metadata = plistlib.loads(target_info.read_bytes())
    identifier, build = metadata["CFBundleIdentifier"], metadata["CFBundleVersion"]
    target_binary = args.target / "Contents/MacOS" / metadata["CFBundleExecutable"]
    target_before = (digest(target_info), digest(target_binary))
    binary_hashes = {}
    for app in sorted(payload.glob("*.app")):
        logs = scope / (app.stem + "-validation")
        logs.mkdir()
        check_app(app, logs)
        service = app / "Contents/XPCServices/com.jasoncavinder.Helm.SparkleExternalUpdater.xpc"
        executable = "Impostor" if app.stem == "ImpostorHelper" else "HelmSparkleExternalUpdater"
        binary_hashes[app.stem] = {"caller": digest(app / "Contents/MacOS/BootstrapProbe"),
                                  "service": digest(service / "Contents/MacOS" / executable)}
        if app.stem != "ImpostorHelper":
            # A separate native self-preflight excludes broken packaging as the
            # explanation for a caller-negative result. Not a launch-boundary proof.
            result = command([str(service / "Contents/MacOS/HelmSparkleExternalUpdater"), "--preflight"],
                             env=dict(os.environ, HELM_DB_PATH=str(scope / "unused-development.db")))
            (logs / "helper-preflight.txt").write_text(result.stdout + result.stderr)
            if result.returncode != 0 or json.loads(result.stdout)["directUpdatesEnabled"] is not False:
                raise RuntimeError("helper self-preflight failed")
    preflight = ["--preflight", str(args.target), identifier, build]
    cases = [
        ("hello", "Valid", [], None),
        ("target", "Valid", preflight, "unresolved"),
        ("changed-build", "Valid", ["--preflight", str(args.target), identifier, build + "0"], "targetChanged"),
        ("missing-target", "Valid", ["--preflight", "/Applications/Helm-XPC-QA-Absent.app", identifier, build], "observationFailed"),
        ("wrong-caller", "WrongCaller", preflight, "reject"),
        ("wrong-channel", "WrongChannel", preflight, "reject"),
        ("unsandboxed-caller", "UnsandboxedCaller", preflight, "reject"),
        ("impostor-helper", "ImpostorHelper", preflight, "reject"),
        ("fresh-positive", "Valid", preflight, "unresolved"),
    ]
    results = []
    for name, variant, intent, expected in cases:
        logs = scope / name
        logs.mkdir()
        stdout, stderr = logs / "stdout.txt", logs / "stderr.txt"
        launched = command(["/usr/bin/open", "-W", "-n", "-a", str(payload / (variant + ".app")),
                            "--stdout", str(stdout), "--stderr", str(stderr),
                            "--env", "HELM_DB_PATH=" + str(scope / "unused-development.db"),
                            "--args", "--bundled-service", *intent])
        (logs / "launch.txt").write_text(launched.stdout + launched.stderr)
        output = stdout.read_text() if stdout.exists() else ""
        error = stderr.read_text() if stderr.exists() else ""
        report = {"case": name, "launcherExit": launched.returncode, "stdout": output, "stderr": error}
        (logs / "result.json").write_text(json.dumps(report, indent=2) + "\n")
        if launched.returncode != 0:
            raise RuntimeError("launch failure is not peer rejection: " + name)
        events = [json.loads(line) for line in output.splitlines()]
        ready = [event for event in events if event.get("event") == "authenticated_bootstrap_ready"]
        completed = [event for event in events if event.get("event") == "preflight_completed"]
        if expected == "reject":
            if events or error.strip() != "Bootstrap rejected: transport":
                raise RuntimeError("untrusted peer was not rejected: " + name)
        else:
            if len(ready) != 1 or "Bootstrap rejected:" in error:
                raise RuntimeError("valid peer did not complete handshake: " + name)
            if expected is None and completed:
                raise RuntimeError("hello unexpectedly performed preflight")
            if expected is not None and (len(completed) != 1 or completed[0].get("assessment") != expected
                                         or completed[0].get("canUpdate") is not False):
                raise RuntimeError("incorrect preflight: " + name)
        results.append(report)
    if not (payload / "impostor-started").exists() or not (payload / "impostor-hello").exists():
        raise RuntimeError("impostor did not receive the data-free hello; do not claim response-gate coverage")
    if (payload / "impostor-target-received").exists():
        raise RuntimeError("target intent reached the impostor")
    if target_before != (digest(target_info), digest(target_binary)) or (scope / "unused-development.db").exists():
        raise RuntimeError("target or development DB state changed")
    summary = {"platform": command(["/usr/bin/sw_vers"]).stdout, "cases": results, "binaryHashes": binary_hashes,
               "targetUnchanged": True, "directUpdatesEnabled": False}
    (scope / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
