#!/usr/bin/env python3
"""Opt-in real uv/Helm CLI certification using disposable offline wheel stores.

Run inside a disposable macOS guest with explicit Helm and uv binaries. This is
not GUI, authenticated-index, public-network, or all-adapter certification.
Retains private evidence and the isolated database even after a failed check.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import signal
import subprocess
import sys
import time


def run_process(argv, env, cwd, timeout=120):
    started = time.monotonic()
    with subprocess.Popen(argv, env=env, cwd=cwd, stdin=subprocess.DEVNULL,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                          text=True, start_new_session=True) as process:
        try:
            stdout, stderr = process.communicate(timeout=timeout)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.communicate(timeout=3)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.communicate()
            raise
    return {"argv": argv, "exit": process.returncode, "stdout": stdout,
            "stderr": stderr, "seconds": round(time.monotonic() - started, 3)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--helm", required=True, type=Path)
    parser.add_argument("--uv", required=True, type=Path)
    parser.add_argument("--fixture", required=True, type=Path)
    parser.add_argument("--artifacts", required=True, type=Path)
    parser.add_argument("--confirm-disposable-environment", action="store_true")
    args = parser.parse_args()
    if not args.confirm_disposable_environment or sys.platform != "darwin":
        parser.error("requires explicit opt-in inside a disposable macOS environment")
    for path in (args.helm, args.uv, args.fixture):
        if not path.is_absolute() or not path.is_file():
            parser.error(f"requires an existing absolute file: {path}")
    if not args.artifacts.is_absolute():
        parser.error("artifacts must be an absolute directory")
    os.umask(0o077)
    args.artifacts.mkdir(parents=True, exist_ok=True)
    commands = {
        "installed": ["--color", "never", "--no-progress", "tool", "list", "--show-version-specifiers", "--offline"],
        "latest": ["--color", "never", "--no-progress", "tool", "list", "--show-version-specifiers", "--outdated"],
    }
    setup = run_process([sys.executable, str(args.fixture), str(args.uv), str(args.artifacts),
                         json.dumps(commands), "prepare-adapter"],
                        {"PATH": "/usr/bin:/bin"}, args.artifacts)
    if setup["exit"] != 0:
        raise RuntimeError(f"fixture setup failed: {setup['stderr']}")
    root = Path(json.loads(setup["stdout"])["artifacts"])
    print(f"Evidence: {root}", flush=True)
    (root / "setup.json").write_text(json.dumps(setup, indent=2))
    config = root / "config/uv.toml"
    config.write_text('no-index = true\nfind-links = [' + json.dumps(str(root / "wheels")) + ']\n')
    env = {
        "HOME": str(root / "home"), "PATH": f"{args.uv.parent}:/usr/bin:/bin:/usr/sbin:/sbin",
        "TMPDIR": str(root / "tmp"), "XDG_CONFIG_HOME": str(root / "config"),
        "XDG_DATA_HOME": str(root / "data"), "XDG_CACHE_HOME": str(root / "cache"),
        "UV_CONFIG_FILE": str(config), "UV_TOOL_DIR": str(root / "tools"),
        "UV_TOOL_BIN_DIR": str(root / "bin"), "UV_CACHE_DIR": str(root / "cache"),
        "UV_PYTHON": sys.executable, "UV_PYTHON_DOWNLOADS": "never", "UV_OFFLINE": "true",
        "HELM_DB_PATH": str(root / "helm.db"), "HELM_ACCEPT_LICENSE": "1", "HELM_ACCEPT_DEFAULTS": "1",
        "LC_ALL": "C",
    }
    records = []
    def cli(label, *arguments, success=True):
        result = run_process([str(args.helm), "--json", "--wait", *arguments], env, root)
        result["label"] = label
        records.append(result)
        (root / "cli-commands.json").write_text(json.dumps(records, indent=2))
        if (result["exit"] == 0) != success:
            raise AssertionError(f"{label}: unexpected exit {result['exit']}; inspect evidence")
        # Failed multi-manager commands emit result rows followed by an error envelope.
        payloads = [json.loads(line) for line in result["stdout"].splitlines() if line.strip()]
        if success:
            assert len(payloads) == 1
        payload = payloads[-1]["data"]
        print(f"{label}: expected {'success' if success else 'rejection'}", flush=True)
        return payload

    report = {"status": "running", "macos": platform.mac_ver()[0], "architecture": platform.machine(),
              "helm_sha256": hashlib.file_digest(args.helm.open("rb"), "sha256").hexdigest(),
              "uv_sha256": hashlib.file_digest(args.uv.open("rb"), "sha256").hexdigest(),
              "python": sys.version, "scope": "CLI/core isolated offline generated wheels"}
    smoke, pinned = "helm-uv-smoke", "helm-uv-pinned"
    receipt = root / "tools" / smoke / "uv-receipt.toml"
    original_receipt = receipt.read_bytes()
    other_receipt = (root / "tools" / pinned / "uv-receipt.toml").read_bytes()
    try:
        detected = cli("detect", "managers", "detect", "uv")
        assert detected["installed"] and detected["version"]
        report["uv_version"] = detected["version"]
        assert cli("detection-persisted", "managers", "show", "uv")["detected"]
        cli("enable", "managers", "enable", "uv")
        cli("refresh", "refresh", "--manager", "uv")
        packages = cli("inventory", "packages", "list")["packages"]
        assert {p["package"]["name"] for p in packages} == {smoke, pinned}
        updates = cli("updates", "updates", "list", "--manager", "uv")["updates"]
        assert len(updates) == 1 and updates[0]["candidate_version"] == "1.1"
        assert receipt.read_bytes() == original_receipt
        search = cli("adapter-search", "search", "helm-uv", "--manager", "uv", "--remote")
        assert len(search["merged_results"]) == 2 and not search["remote_errors"]
        assert len(cli("cached-search", "search", "helm-uv", "--manager", "uv", "--local")["merged_results"]) == 2
        cli("pin", "packages", "pin", smoke, "--manager", "uv")
        cli("pinned-rejection", "packages", "upgrade", smoke, "--manager", "uv", success=False)
        assert receipt.read_bytes() == original_receipt
        cli("unpin", "packages", "unpin", smoke, "--manager", "uv")
        cli("upgrade", "packages", "upgrade", smoke, "--manager", "uv")
        packages = cli("upgrade-persisted", "packages", "list")["packages"]
        assert next(p for p in packages if p["package"]["name"] == smoke)["installed_version"] == "1.1"
        assert (root / "bin" / smoke).is_file() and (root / "bin" / (smoke + "-alt")).is_file()
        cli("post-upgrade-refresh", "refresh", "--manager", "uv")
        assert not cli("current", "updates", "list", "--manager", "uv")["updates"]
        cli("uninstall-preview", "packages", "uninstall", smoke, "--manager", "uv", "--preview")
        assert receipt.exists()
        cli("uninstall", "packages", "uninstall", smoke, "--manager", "uv", "--yes")
        assert not os.path.lexists(root / "bin" / smoke)
        assert not os.path.lexists(root / "bin" / (smoke + "-alt"))
        assert (root / "bin" / pinned).is_file()
        cli("equivalent-version-install", "packages", "install", smoke, "--manager", "uv", "--version", "1.1.0")
        packages = cli("install-persisted", "packages", "list")["packages"]
        assert next(p for p in packages if p["package"]["name"] == smoke)["installed_version"] == "1.1"
        cli("missing-package-rejection", "packages", "install", "helm-uv-absent", "--manager", "uv", success=False)
        assert cli("failed-install-retains-inventory", "packages", "list")["packages"] == packages
        saved = receipt.read_bytes()
        try:
            receipt.write_text("[malformed-test-receipt")
            cli("malformed-receipt-rejection", "refresh", "--manager", "uv", success=False)
            assert cli("failure-retains-inventory", "packages", "list")["packages"] == packages
        finally:
            receipt.write_bytes(saved)
        cli("recovery", "refresh", "--manager", "uv")
        assert (root / "tools" / pinned / "uv-receipt.toml").read_bytes() == other_receipt
        cli("remove-smoke", "packages", "uninstall", smoke, "--manager", "uv", "--yes")
        cli("remove-pinned", "packages", "uninstall", pinned, "--manager", "uv", "--yes")
        # Some uv versions remove the empty store. That must not become a general
        # authorization to clear caches when an unrelated store disappears.
        cli("empty-refresh", "refresh", "--manager", "uv", success=(root / "tools").exists())
        assert not cli("empty-inventory", "packages", "list")["packages"]
        assert not cli("empty-updates", "updates", "list", "--manager", "uv")["updates"]
        cli("reinstall-after-store-removal", "packages", "install", smoke, "--manager", "uv", "--version", "1.1")
        packages = cli("reinstall-persisted", "packages", "list")["packages"]
        assert len(packages) == 1 and packages[0]["installed_version"] == "1.1"
        cli("reinstalled-refresh", "refresh", "--manager", "uv")
        cli("remove-reinstalled-last-tool", "packages", "uninstall", smoke, "--manager", "uv", "--yes")
        assert not cli("final-empty-inventory", "packages", "list")["packages"]
        tasks = cli("terminal-tasks", "tasks", "list")["tasks"]
        assert tasks and all(t["status"] in {"completed", "failed", "cancelled"} for t in tasks)
        report["status"] = "passed"
    except Exception as error:
        report.update(status="failed", error=f"{type(error).__name__}: {error}")
        raise
    finally:
        report["commands"] = len(records)
        (root / "report.json").write_text(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
