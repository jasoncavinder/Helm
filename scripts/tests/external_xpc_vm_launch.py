#!/usr/bin/env python3
"""Disposable-VM-only launch experiment; ad-hoc fixtures, never production trust."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import plistlib
import shutil
import subprocess
import time
import uuid

HOST = "com.jasoncavinder.Helm.XPCLaunchProbe"
SERVICE = HOST + ".Service"


def command(args, **kwargs):
    return subprocess.run(args, check=True, capture_output=True, text=True, timeout=45, **kwargs)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def plist(path, values):
    with path.open("xb") as file:
        plistlib.dump(values, file)


def bundle(path, binary, identifier, executable, kind, extra):
    (path / "Contents/MacOS").mkdir(parents=True)
    shutil.copy2(binary, path / "Contents/MacOS" / executable)
    plist(path / "Contents/Info.plist", {
        "CFBundleIdentifier": identifier, "CFBundleExecutable": executable,
        "CFBundlePackageType": kind, "CFBundleVersion": "1",
        "LSMinimumSystemVersion": "13.0", **extra,
    })


def run_case(root, host_binary, service_binary, sandbox_service):
    root.mkdir(mode=0o700)
    run_id = str(uuid.uuid4())
    sentinel = root / "sentinel"
    with sentinel.open("xb") as file:
        file.write(os.urandom(32))
    sentinel.chmod(0o600)
    expected = digest(sentinel)
    app = root / "LaunchProbe.app"
    bundle(app, host_binary, HOST, "Host", "APPL", {
        "LSUIElement": True, "HelmProbeRun": run_id,
        "HelmProbeSkipChild": sandbox_service,
    })
    service = app / "Contents/XPCServices/Probe.xpc"
    bundle(service, service_binary, SERVICE, "Probe", "XPC!", {"XPCService": {"ServiceType": "Application"}})
    entitlements = root / "sandbox.plist"
    plist(entitlements, {"com.apple.security.app-sandbox": True})
    signing = ["/usr/bin/codesign", "--sign", "-", "--options", "runtime"]
    command(signing + (["--entitlements", str(entitlements)] if sandbox_service else []) + [str(service)])
    command(signing + ["--entitlements", str(entitlements), str(app)])
    command(["/usr/bin/codesign", "--verify", "--strict", "--deep", str(app)])
    for name, path in [("host", app), ("service", service)]:
        result = command(["/usr/bin/codesign", "-d", "--entitlements", ":-", str(path)])
        (root / (name + "-entitlements.txt")).write_text(result.stdout + result.stderr)
    report_path = Path.home() / "Library/Containers" / HOST / "Data/Library/Application Support" / (run_id + ".json")
    if report_path.exists():
        raise RuntimeError("refusing an existing report")
    command(["/usr/bin/open", "-W", "-n", str(app)])
    deadline = time.monotonic() + 5
    while not report_path.exists() and time.monotonic() < deadline:
        time.sleep(0.1)
    report = json.loads(report_path.read_text())
    (root / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    if digest(sentinel) != expected:
        raise RuntimeError("sentinel changed")
    if report.get("failure") is not None:
        raise RuntimeError(f"fixture failed: {report}")
    for name in (["host", "service"] if sandbox_service else ["host", "inherited"]):
        if report[name].get("digest") is not None or report[name]["error"] not in (1, 13):
            raise RuntimeError(f"expected sandbox denial for {name}: {report}")
    if not sandbox_service and report["service"] != {"digest": expected, "error": 0}:
        raise RuntimeError(f"unsandboxed XPC service did not read the fixture: {report}")
    return {"case": root.name, "run": run_id, "sentinelSHA256": expected,
            "hostSHA256": digest(app / "Contents/MacOS/Host"),
            "serviceSHA256": digest(service / "Contents/MacOS/Probe"), "report": report}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--confirm-disposable-vm", action="store_true", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--host-binary", type=Path, required=True)
    parser.add_argument("--service-binary", type=Path, required=True)
    args = parser.parse_args()
    if platform.system() != "Darwin" or not args.confirm_disposable_vm:
        parser.error("run only in the designated disposable macOS VM")
    # New scope only: retain every prior run, bundle, sentinel and report.
    args.output.mkdir(mode=0o700)
    results = []
    for name, sandboxed in [("separate-service", False), ("sandboxed-service-control", True), ("fresh-positive", False)]:
        results.append(run_case(args.output / name, args.host_binary, args.service_binary, sandboxed))
    summary = {"platform": command(["/usr/bin/sw_vers"]).stdout, "results": results,
               "productionTrustCertified": False, "directUpdatesEnabled": False}
    (args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
