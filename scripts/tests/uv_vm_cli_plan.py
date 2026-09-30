#!/usr/bin/env python3
"""Opt-in offline uv/Helm multi-package Plan certification in a disposable macOS VM.

Uses only generated wheels, private stores and an existing Python installation.
Retains evidence on failure. No registry, account or GUI certification is implied.
"""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import sqlite3
import sys
import tempfile
import tomllib

from uv_vm_cli_lifecycle import run_process


def semantic_receipt(data):
    parsed = tomllib.loads(data.decode())
    options = parsed.get("tool", {}).get("options", {})
    # uv re-saves a global find-links source beside the identical receipt source.
    # Remove only exact repeats; preserve ordering and every other receipt field.
    if "find-links" in options:
        options["find-links"] = list(dict.fromkeys(options["find-links"]))
    return parsed


def environment(root, uv, python):
    return {
        "HOME": str(root / "home"), "TMPDIR": str(root / "tmp") + "/",
        "PATH": f"{uv.parent}:/usr/bin:/bin:/usr/sbin:/sbin",
        "XDG_CONFIG_HOME": str(root / "config"), "XDG_DATA_HOME": str(root / "data"),
        "XDG_CACHE_HOME": str(root / "cache"), "UV_CONFIG_FILE": str(root / "config/uv.toml"),
        "UV_TOOL_DIR": str(root / "tools"), "UV_TOOL_BIN_DIR": str(root / "bin"),
        "UV_CACHE_DIR": str(root / "cache"), "UV_PYTHON_INSTALL_DIR": str(root / "python"),
        "UV_PYTHON": str(python), "UV_PYTHON_DOWNLOADS": "never", "UV_OFFLINE": "true",
        "UV_NO_MANAGED_PYTHON": "true", "UV_KEYRING_PROVIDER": "disabled",
        "PYTHONNOUSERSITE": "1", "HELM_DB_PATH": str(root / "helm.db"),
        "HELM_ACCEPT_LICENSE": "1", "HELM_ACCEPT_DEFAULTS": "1", "LC_ALL": "C",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--helm", required=True, type=Path)
    parser.add_argument("--uv", required=True, type=Path)
    parser.add_argument("--artifacts", required=True, type=Path)
    parser.add_argument("--confirm-disposable-environment", action="store_true")
    args = parser.parse_args()
    if not args.confirm_disposable_environment or sys.platform != "darwin":
        parser.error("requires explicit opt-in inside a disposable macOS environment")
    if sys.version_info < (3, 11):
        parser.error("requires Python 3.11 or newer")
    for path in (args.helm, args.uv):
        if not path.is_absolute() or not path.is_file() or not os.access(path, os.X_OK):
            parser.error(f"requires an existing absolute executable: {path}")
    if not args.artifacts.is_absolute():
        parser.error("artifacts must be an absolute directory")
    fixture = Path(__file__).resolve().parents[2] / "core/rust/crates/helm-core/tests/fixtures/uv/isolated_lifecycle.py"
    spec = importlib.util.spec_from_file_location("uv_fixture", fixture)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    os.umask(0o077)
    args.artifacts.mkdir(parents=True, exist_ok=True)
    root = Path(tempfile.mkdtemp(prefix="uv-plan-", dir=args.artifacts)).resolve()
    for name in ("home", "tmp", "config", "data", "cache", "tools", "bin", "python", "wheels"):
        (root / name).mkdir()
    (root / "config/uv.toml").write_text(
        'no-index = true\nno-build = true\nfind-links = [' + json.dumps(str(root / "wheels")) + ']\n')
    env = environment(root, args.uv, Path(sys.executable))
    names = ("helm-uv-plan-a", "helm-uv-plan-b", "helm-uv-plan-pinned")
    records = []
    report = {"status": "running", "macos": platform.mac_ver()[0],
              "architecture": platform.machine(), "python": sys.version,
              "scope": "isolated offline CLI/core multi-package Plan; not GUI acceptance"}
    for label, path in (("helm", args.helm), ("uv", args.uv)):
        with path.open("rb") as stream:
            report[label + "_sha256"] = hashlib.file_digest(stream, "sha256").hexdigest()
    print(f"Evidence: {root}", flush=True)

    def run(label, argv):
        try:
            result = run_process([str(value) for value in argv], env, root)
        except BaseException as error:
            records.append({"label": label, "error": type(error).__name__})
            (root / "commands.json").write_text(json.dumps(records, indent=2))
            raise
        result["label"] = label
        records.append(result)
        (root / "commands.json").write_text(json.dumps(records, indent=2))
        assert result["exit"] == 0, f"{label}: inspect {root}/commands.json"
        print(label + ": passed", flush=True)
        return result

    def cli(label, *arguments):
        result = run(label, [args.helm, "--json", "--wait", *arguments])
        payloads = [json.loads(line) for line in result["stdout"].splitlines() if line.strip()]
        assert len(payloads) == 1, (label, payloads)
        return payloads[0]["data"]

    def inventory(label):
        return {row["package"]["name"]: row["installed_version"]
                for row in cli(label, "packages", "list")["packages"]}

    try:
        report["uv_version"] = run("uv-version", [args.uv, "--version"])["stdout"].strip()
        for name in names:
            module.make_wheel(root / "wheels", name, "1.0", [name])
            requirement = name + ("==1.0" if name == names[2] else ">=1,<2")
            run("seed-" + name, [args.uv, "tool", "install", requirement])
        receipts = {name: (root / "tools" / name / "uv-receipt.toml").read_bytes() for name in names}
        (root / "original-receipts.json").write_text(json.dumps(
            {name: value.decode() for name, value in receipts.items()}, indent=2))
        for name in names:
            module.make_wheel(root / "wheels", name, "1.1", [name])
        assert cli("detect", "managers", "detect", "uv")["installed"]
        cli("enable", "managers", "enable", "uv")
        cli("refresh", "refresh", "--manager", "uv")
        assert inventory("initial-inventory") == dict.fromkeys(names, "1.0")
        candidates = cli("review-cached-candidates", "updates", "list", "--manager", "uv")["updates"]
        assert {row["package"]["name"]: row["candidate_version"] for row in candidates} == dict.fromkeys(names[:2], "1.1")
        assert all(row["package_identifier"].startswith("uv-tool:") for row in candidates), candidates
        plan = cli("review-two-updates", "updates", "preview", "--manager", "uv")
        assert plan["total_steps"] == 2, plan
        assert {step["packageName"] for step in plan["steps"]} == set(names[:2]), plan
        cli("pin-one-update", "packages", "pin", names[0], "--manager", "uv")
        pinned_plan = cli("review-pin-exclusion", "updates", "preview", "--manager", "uv")
        assert pinned_plan["total_steps"] == 1, pinned_plan
        assert pinned_plan["steps"][0]["packageName"] == names[1], pinned_plan
        for name in names:
            assert (root / "tools" / name / "uv-receipt.toml").read_bytes() == receipts[name]
        cli("unpin-update", "packages", "unpin", names[0], "--manager", "uv")
        result = cli("run-two-updates", "updates", "run", "--manager", "uv", "--yes")
        assert result["total_steps"] == 2 and result["failed_steps"] == 0, result
        assert len(result["results"]) == 2 and all(row["success"] for row in result["results"]), result
        expected = dict.fromkeys(names[:2], "1.1") | {names[2]: "1.0"}
        assert inventory("verified-inventory") == expected
        for name in names:
            updated = (root / "tools" / name / "uv-receipt.toml").read_bytes()
            assert semantic_receipt(updated) == semantic_receipt(receipts[name]), name
            if name == names[2]:
                assert updated == receipts[name]
            assert (root / "bin" / name).is_file()
        cli("refresh-after-plan", "refresh", "--manager", "uv")
        assert cli("no-updates-left", "updates", "preview", "--manager", "uv")["total_steps"] == 0
        assert cli("second-run-noop", "updates", "run", "--manager", "uv", "--yes")["total_steps"] == 0
        assert inventory("noop-preserves-state") == expected
        tasks = cli("terminal-tasks", "tasks", "list")["tasks"]
        assert tasks and all(row["status"] == "completed" for row in tasks), tasks
        with sqlite3.connect(f"file:{root / 'helm.db'}?mode=ro", uri=True) as database:
            assert database.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
        report["status"] = "passed"
    except BaseException as error:
        report.update(status="failed", error=f"{type(error).__name__}: {error}")
        raise
    finally:
        report["commands"] = len(records)
        (root / "report.json").write_text(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
