#!/usr/bin/env python3
"""Opt-in uv CLI source/auth/cancellation checks in a disposable macOS guest.

Uses generated inert wheels and a loopback-only Basic-auth index with synthetic
credentials. This is not public-index, enterprise-auth, GUI, or TLS certification.
"""

import argparse
import base64
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import importlib.util
import json
import os
from pathlib import Path
import platform
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time

from uv_vm_cli_lifecycle import run_process


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("helm", "uv", "fixture", "artifacts"):
        parser.add_argument("--" + name, required=True, type=Path)
    parser.add_argument("--confirm-disposable-environment", action="store_true")
    args = parser.parse_args()
    if sys.platform != "darwin" or not args.confirm_disposable_environment:
        parser.error("requires explicit disposable macOS opt-in")
    for path in (args.helm, args.uv, args.fixture):
        if not path.is_absolute() or not path.is_file():
            parser.error(f"requires an absolute file: {path}")
    if not args.artifacts.is_absolute():
        parser.error("artifacts must be absolute")
    os.umask(0o077)
    args.artifacts.mkdir(parents=True, exist_ok=True)
    root = Path(tempfile.mkdtemp(prefix="uv-network-", dir=args.artifacts))
    print(f"Evidence: {root}", flush=True)
    for name in ("home", "config", "cache", "data", "tmp", "tools", "bin", "wheels"):
        (root / name).mkdir()
    spec = importlib.util.spec_from_file_location("wheel_fixture", args.fixture)
    fixture = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(fixture)
    package = "helm-uv-network"
    stalled, release = threading.Event(), threading.Event()
    mode = {"stall": False, "deny": False}
    requests = []
    auth = "Basic " + base64.b64encode(b"fixture:synthetic-token").decode()

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass

        def do_HEAD(self):
            self.do_GET()

        def do_GET(self):
            authorized = self.headers.get("Authorization") == auth and not mode["deny"]
            requests.append({"path": self.path, "authorized": authorized})
            if not authorized:
                self.send_response(401)
                self.send_header("WWW-Authenticate", 'Basic realm="Helm fixture"')
                self.end_headers()
                return
            if mode["stall"]:
                stalled.set()
                release.wait(timeout=90)
            if self.path == f"/simple/{package}/":
                body = "\n".join(f'<a href="/wheels/{p.name}">{p.name}</a>'
                                 for p in sorted((root / "wheels").glob("*.whl"))).encode()
                content_type = "text/html"
            elif self.path.startswith("/wheels/") and Path(self.path).name == self.path[8:]:
                wheel = root / "wheels" / Path(self.path).name
                if not wheel.is_file():
                    self.send_error(404)
                    return
                body, content_type = wheel.read_bytes(), "application/octet-stream"
            else:
                self.send_error(404)
                return
            self.send_response(200)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            if self.command == "HEAD":
                return
            try:
                self.wfile.write(body)
            except (BrokenPipeError, ConnectionResetError):
                pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    server.daemon_threads = True
    threading.Thread(target=server.serve_forever, daemon=True).start()
    config = root / "config/uv.toml"
    config.write_text(f'index-url = "http://fixture:synthetic-token@127.0.0.1:{server.server_port}/simple"\n')
    env = {"HOME": str(root / "home"), "PATH": f"{args.uv.parent}:/usr/bin:/bin:/usr/sbin:/sbin",
           "TMPDIR": str(root / "tmp"), "XDG_CONFIG_HOME": str(root / "config"),
           "XDG_CACHE_HOME": str(root / "cache"), "XDG_DATA_HOME": str(root / "data"),
           "UV_CONFIG_FILE": str(config), "UV_TOOL_DIR": str(root / "tools"),
           "UV_TOOL_BIN_DIR": str(root / "bin"), "UV_CACHE_DIR": str(root / "cache"),
           "UV_PYTHON": sys.executable, "UV_PYTHON_DOWNLOADS": "never",
           "UV_NO_CACHE": "true", "UV_KEYRING_PROVIDER": "disabled",
           "HELM_DB_PATH": str(root / "helm.db"), "HELM_ACCEPT_LICENSE": "1",
           "HELM_ACCEPT_DEFAULTS": "1", "LC_ALL": "C"}
    records = []
    report = {"status": "running", "macos": platform.mac_ver()[0],
              "helm_sha256": hashlib.file_digest(args.helm.open("rb"), "sha256").hexdigest(),
              "uv_sha256": hashlib.file_digest(args.uv.open("rb"), "sha256").hexdigest(),
              "harness_sha256": hashlib.file_digest(Path(__file__).open("rb"), "sha256").hexdigest(),
              "scope": "synthetic loopback authenticated index and detached CLI cancellation"}
    coordinator = None
    coordinator_log = (root / "coordinator.log").open("w")

    def cli(label, *arguments, success=True, detach=False):
        result = run_process([str(args.helm), "--json", "--detach" if detach else "--wait", *arguments], env, root)
        result["label"] = label
        records.append(result)
        (root / "commands.json").write_text(json.dumps(records, indent=2))
        assert (result["exit"] == 0) == success, (label, result)
        print(f"{label}: expected {'success' if success else 'rejection'}", flush=True)
        return json.loads(result["stdout"].splitlines()[-1])["data"]

    def publish(version):
        fixture.make_wheel(root / "wheels", package, version, [package])

    try:
        publish("1.0")
        report["uv_version"] = cli("detect", "managers", "detect", "uv")["version"]
        cli("enable", "managers", "enable", "uv")
        cli("authenticated-install", "packages", "install", package, "--manager", "uv")
        receipt = root / "tools" / package / "uv-receipt.toml"
        original = receipt.read_bytes()
        publish("1.1")
        cli("authenticated-refresh", "refresh", "--manager", "uv")
        candidates = cli("candidates", "updates", "list", "--manager", "uv")
        assert candidates["updates"][0]["candidate_version"] == "1.1"
        saved_config = config.read_bytes()
        try:
            config.write_text(saved_config.decode() + '\n[[index]]\nname = "unsupported-fixture"\nurl = "https://example.invalid/simple"\n')
            cli("unsupported-source-policy", "refresh", "--manager", "uv", success=False)
            assert cli("unsupported-policy-retains-cache", "updates", "list", "--manager", "uv") == candidates
        finally:
            config.write_bytes(saved_config)
        mode["deny"] = True
        cli("authentication-failure", "refresh", "--manager", "uv", success=False)
        assert cli("cache-retained", "updates", "list", "--manager", "uv") == candidates
        assert receipt.read_bytes() == original
        mode["deny"] = False
        cli("authentication-recovery", "refresh", "--manager", "uv")
        cli("authenticated-upgrade", "packages", "upgrade", package, "--manager", "uv")
        installed = cli("upgraded", "packages", "list")["packages"]
        assert len(installed) == 1 and installed[0]["installed_version"] == "1.1"
        publish("1.2")
        cli("next-candidate", "refresh", "--manager", "uv")
        original = receipt.read_bytes()
        with sqlite3.connect(root / "helm.db") as db:
            previous_upgrade_count = db.execute(
                "SELECT count(*) FROM task_records WHERE task_type = 'upgrade'"
            ).fetchone()[0]
        coordinator = subprocess.Popen([str(args.helm), "__coordinator__", "serve"],
                                       env=env, cwd=root, stdin=subprocess.DEVNULL,
                                       stdout=coordinator_log, stderr=coordinator_log)
        deadline = time.monotonic() + 10
        while not list((root / "tmp").glob("helm-cli-coordinator-*/ready.json")):
            assert coordinator.poll() is None, "coordinator exited before becoming ready"
            assert time.monotonic() < deadline, "coordinator did not become ready"
            time.sleep(0.1)
        mode["stall"] = True
        submission = cli("submit-stalled-upgrade", "packages", "upgrade", package, "--manager", "uv", detach=True)
        task_id = str(submission["task_id"])
        assert stalled.wait(timeout=20), "update never reached the stalled source"
        cli("cancel", "tasks", "cancel", task_id)
        deadline = time.monotonic() + 20
        while True:
            task = cli("cancel-state", "tasks", "show", task_id)["task"]
            if task["status"] == "cancelled":
                break
            assert time.monotonic() < deadline, task
            time.sleep(0.2)
        with sqlite3.connect(root / "helm.db") as db:
            upgrade_count = db.execute(
                "SELECT count(*) FROM task_records WHERE task_type = 'upgrade'"
            ).fetchone()[0]
        assert upgrade_count == previous_upgrade_count + 1, "duplicate upgrade submission"
        assert receipt.read_bytes() == original
        mode["stall"] = False
        release.set()
        cli("after-cancel-refresh", "refresh", "--manager", "uv")
        assert cli("cancel-preserves-installed", "packages", "list")["packages"] == installed
        cli("remove", "packages", "uninstall", package, "--manager", "uv", "--yes")
        assert not cli("removed", "packages", "list")["packages"]
        assert any(item["authorized"] for item in requests)
        assert all("synthetic-token" not in item["stdout"] + item["stderr"] for item in records)
        report["status"] = "passed"
    except Exception as error:
        report.update(status="failed", error=f"{type(error).__name__}: {error}")
        raise
    finally:
        release.set()
        if coordinator is not None:
            coordinator.terminate()
            try:
                coordinator.wait(timeout=5)
            except subprocess.TimeoutExpired:
                coordinator.kill()
                coordinator.wait()
        coordinator_log.close()
        server.shutdown()
        server.server_close()
        (root / "requests.json").write_text(json.dumps(requests, indent=2))
        (root / "report.json").write_text(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
