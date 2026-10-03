"""Build-script contracts with a stub compiler; no toolchain or app execution."""

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "build_external_update_bridge.sh"


class BridgeBuildTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root / "scripts").mkdir()
        (self.root / "bin").mkdir()
        self.script = self.root / "scripts/build_external_update_bridge.sh"
        shutil.copyfile(SCRIPT, self.script)
        cargo = self.root / "bin/cargo"
        cargo.write_text("""#!/usr/bin/env python3
import json, os, pathlib, sys
args = sys.argv[1:]
pathlib.Path(os.environ['CALLS']).write_text(json.dumps([args, os.environ.get('MACOSX_DEPLOYMENT_TARGET')]))
if os.environ.get('FAIL_CARGO') == '1':
    sys.exit(9)
def option(name):
    return args[args.index(name) + 1]
profile = 'debug' if option('--profile') == 'dev' else 'release'
output = pathlib.Path(option('--target-dir')) / option('--target') / profile
output.mkdir(parents=True, exist_ok=True)
(output / 'libhelm_external_update_bridge.a').write_bytes(os.environ['ARCHIVE'].encode())
""")
        cargo.chmod(0o755)
        self.environment = dict(os.environ, PATH=str(self.root / "bin") + os.pathsep + os.environ["PATH"],
                                CALLS=str(self.root / "calls.json"), ARCHIVE="first", MACOSX_DEPLOYMENT_TARGET="26.0")

    def run_build(self, *args):
        return subprocess.run(["bash", str(self.script), *args], env=self.environment, text=True, capture_output=True)

    def test_pins_architecture_profile_deployment_and_locked_build(self):
        for arch, target in [("arm64", "aarch64-apple-darwin"), ("x86_64", "x86_64-apple-darwin")]:
            for profile in ["debug", "release"]:
                result = self.run_build(arch, profile)
                self.assertEqual(result.returncode, 0, result.stderr)
                args, deployment = json.loads((self.root / "calls.json").read_text())
                self.assertEqual(deployment, "13.0")
                self.assertEqual(args, ["build", "--locked", "--manifest-path", str(self.root / "core/rust/Cargo.toml"),
                                        "--target-dir", str(self.root / "artifacts/external-updater-rust"),
                                        "--target", target, "--profile", "dev" if profile == "debug" else profile,
                                        "-p", "helm-external-update-bridge"])
                destination = Path(result.stdout.strip())
                self.assertEqual(destination.name, hashlib.sha256(b"first").hexdigest())
                self.assertEqual((destination / "libhelm_external_update_bridge.a").read_bytes(), b"first")

    def test_changed_archive_changes_link_input_without_overwriting_old_build(self):
        first = self.run_build("arm64", "debug")
        self.assertEqual(first.returncode, 0, first.stderr)
        same = self.run_build("arm64", "debug")
        self.assertEqual(same.stdout, first.stdout)
        self.environment["ARCHIVE"] = "changed"
        second = self.run_build("arm64", "debug")
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertNotEqual(second.stdout, first.stdout)
        self.assertEqual((Path(first.stdout.strip()) / "libhelm_external_update_bridge.a").read_bytes(), b"first")
        self.assertEqual((Path(second.stdout.strip()) / "libhelm_external_update_bridge.a").read_bytes(), b"changed")

    def test_existing_wrong_archive_is_not_silently_replaced(self):
        result = self.run_build("arm64", "debug")
        self.assertEqual(result.returncode, 0, result.stderr)
        archive = Path(result.stdout.strip()) / "libhelm_external_update_bridge.a"
        archive.write_bytes(b"incorrect")
        result = self.run_build("arm64", "debug")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertEqual(archive.read_bytes(), b"incorrect")

    def test_compiler_failure_never_prints_a_library_path(self):
        self.environment["FAIL_CARGO"] = "1"
        result = self.run_build("arm64", "debug")
        self.assertEqual(result.returncode, 9)
        self.assertEqual(result.stdout, "")

    def test_invalid_inputs_never_invoke_compiler(self):
        for args in [(), ("arm64",), ("arm64", "debug", "extra"), ("bad", "debug"), ("arm64", "bad")]:
            result = self.run_build(*args)
            self.assertEqual(result.returncode, 2)
            self.assertFalse((self.root / "calls.json").exists())


if __name__ == "__main__":
    unittest.main()
