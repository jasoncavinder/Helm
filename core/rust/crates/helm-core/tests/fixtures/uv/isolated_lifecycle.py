"""Exercise real uv with local generated wheels, never the host tool store.

Invoked by the ignored Rust contract test with explicit uv/Python executables.
Only Python's standard library is used; no build backend or fixture entrypoint
executes. Artifacts are retained, including on failure, for bounded evidence.
"""

import base64
import csv
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import zipfile


def make_wheel(directory, name, version, executables):
    normalized = name.replace("-", "_")
    info = f"{normalized}-{version}.dist-info"
    files = {
        f"{normalized}.py": b"def main():\n    return 0\n",
        f"{info}/METADATA": (
            f"Metadata-Version: 2.1\nName: {name}\nVersion: {version}\n"
            "Requires-Python: >=3.9\n\nHelm isolated uv contract fixture.\n"
        ).encode(),
        f"{info}/WHEEL": (
            "Wheel-Version: 1.0\nGenerator: helm-contract-fixture\n"
            "Root-Is-Purelib: true\nTag: py3-none-any\n"
        ).encode(),
        f"{info}/entry_points.txt": (
            "[console_scripts]\n"
            + "".join(f"{exe} = {normalized}:main\n" for exe in executables)
        ).encode(),
    }
    record = io.StringIO(newline="")
    writer = csv.writer(record)
    for path, data in files.items():
        digest = base64.urlsafe_b64encode(hashlib.sha256(data).digest()).rstrip(b"=")
        writer.writerow([path, "sha256=" + digest.decode(), len(data)])
    writer.writerow([f"{info}/RECORD", "", ""])
    files[f"{info}/RECORD"] = record.getvalue().encode()
    with zipfile.ZipFile(directory / f"{normalized}-{version}-py3-none-any.whl", "x") as wheel:
        for path, data in files.items():
            wheel.writestr(path, data)


def main():
    uv = Path(sys.argv[1])
    artifact_parent = Path(sys.argv[2])
    list_commands = json.loads(sys.argv[3])
    if not uv.is_absolute() or not uv.is_file() or not os.access(uv, os.X_OK):
        raise ValueError("uv must be an explicit absolute executable path")
    artifact_parent.mkdir(parents=True, exist_ok=True)
    root = Path(tempfile.mkdtemp(prefix="uv-real-", dir=artifact_parent)).resolve()
    print(f"Isolated uv artifacts: {root}", file=sys.stderr, flush=True)
    paths = {key: root / key for key in (
        "home", "tools", "bin", "cache", "python", "tmp", "config", "data", "wheels"
    )}
    for path in paths.values():
        path.mkdir()

    # Start from nothing: inherited UV_*, pip settings, proxies, credentials,
    # Python hooks, and shell configuration must not affect this fixture store.
    env = {
        "PATH": os.pathsep.join((str(paths["bin"]), "/usr/bin", "/bin")),
        "HOME": str(paths["home"]),
        "TMPDIR": str(paths["tmp"]),
        "XDG_CONFIG_HOME": str(paths["config"]),
        "XDG_DATA_HOME": str(paths["data"]),
        "XDG_CACHE_HOME": str(paths["cache"]),
        "UV_TOOL_DIR": str(paths["tools"]),
        "UV_TOOL_BIN_DIR": str(paths["bin"]),
        "UV_CACHE_DIR": str(paths["cache"]),
        "UV_PYTHON_INSTALL_DIR": str(paths["python"]),
        "UV_PYTHON_DOWNLOADS": "never",
        "UV_OFFLINE": "true",
        "UV_NO_CONFIG": "true",
        "UV_PYTHON": sys.executable,
        "UV_NO_MANAGED_PYTHON": "true",
        "UV_KEYRING_PROVIDER": "disabled",
        "PYTHONNOUSERSITE": "1",
        "LC_ALL": "C",
    }
    records = {}

    def run(label, args, expected=0, from_builder=False):
        prefix = [] if from_builder else ["--color", "never", "--no-progress"]
        result = subprocess.run(
            [str(uv), *prefix, *args],
            cwd=root, env=env, stdin=subprocess.DEVNULL, capture_output=True,
            text=True, encoding="utf-8", timeout=30, check=False,
        )
        records[label] = {"code": result.returncode, "stdout": result.stdout, "stderr": result.stderr}
        (root / "commands.json").write_text(json.dumps(records, indent=2), encoding="utf-8")
        if expected is not None and result.returncode != expected:
            raise RuntimeError(f"{label}: unexpected exit {result.returncode}; inspect {root}")
        return result

    def listing(label, outdated=False):
        run(label, list_commands["latest" if outdated else "installed"], from_builder=True)

    run("version", ["--version"])
    listing("empty")
    listing("empty_latest", outdated=True)

    name = "helm-uv-smoke"
    pinned = "helm-uv-pinned"
    executables = [name, name + "-alt"]
    make_wheel(paths["wheels"], name, "1.0", executables)
    make_wheel(paths["wheels"], pinned, "1.0", [pinned])
    install_options = ["--no-index", "--find-links", str(paths["wheels"]), "--no-build"]
    run("install_range", ["tool", "install", name + "<2", *install_options])
    run("install_pin", ["tool", "install", pinned + "==1.0", *install_options])
    listing("installed")
    listing("current_latest", outdated=True)

    make_wheel(paths["wheels"], name, "1.1", executables)
    make_wheel(paths["wheels"], name, "2.0", executables)
    make_wheel(paths["wheels"], pinned, "2.0", [pinned])
    listing("latest", outdated=True)
    run("upgrade", ["tool", "upgrade", name, pinned, "--no-build"])
    listing("after_upgrade")
    listing("latest_after_upgrade", outdated=True)

    failed = run("failed_install", ["tool", "install", "helm-uv-absent", *install_options], expected=None)
    if failed.returncode == 0:
        raise RuntimeError("missing fixture unexpectedly installed")
    listing("after_failed_install")

    # Damage only this run's synthetic receipt. Restore it even if uv fails.
    receipt = paths["tools"] / pinned / "uv-receipt.toml"
    original = receipt.read_bytes()
    try:
        receipt.write_text("[deliberately incomplete fixture", encoding="utf-8")
        listing("broken_receipt")
        listing("broken_receipt_latest", outdated=True)
    finally:
        receipt.write_bytes(original)
    listing("restored")

    run("uninstall_range", ["tool", "uninstall", name])
    listing("one_remaining")
    for executable in executables:
        if os.path.lexists(paths["bin"] / executable):
            raise RuntimeError("removed fixture left an executable behind")
    if not (paths["bin"] / pinned).is_file():
        raise RuntimeError("removing one fixture damaged the other")
    run("uninstall_pin", ["tool", "uninstall", pinned])
    listing("final_empty")
    listing("final_empty_latest", outdated=True)
    if os.path.lexists(paths["bin"] / pinned):
        raise RuntimeError("removed pinned fixture left an executable behind")
    print(json.dumps({"artifacts": str(root), "python": sys.version, "records": records}))


if __name__ == "__main__":
    main()
