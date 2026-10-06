"""Portable packaging regressions; no signing, launch services or installer."""

import importlib.util
import os
from pathlib import Path
import plistlib
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "package_external_updater", Path(__file__).resolve().parents[1] / "package_external_updater.py"
)
PACKAGE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PACKAGE)


class PackageTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.binary = self.root / "helper"
        self.binary.write_bytes(b"compiled helper fixture")
        self.binary.chmod(0o755)
        self.framework = self.root / "Sparkle.framework"
        resources = self.framework / "Versions/B/Resources"
        resources.mkdir(parents=True)
        self.write_plist(resources / "Info.plist", {
            "CFBundleIdentifier": "org.sparkle-project.Sparkle",
            "CFBundleShortVersionString": PACKAGE.SPARKLE_VERSION,
        })
        (self.framework / "Versions/Current").symlink_to("B")
        (self.framework / "Resources").symlink_to("Versions/Current/Resources")
        (self.framework / "Versions/B/Sparkle").write_bytes(b"framework fixture")
        (self.framework / "Sparkle").symlink_to("Versions/Current/Sparkle")
        for child in PACKAGE.NESTED_CODE:
            path = self.framework / child
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"nested code fixture")
        self.output = self.root / "Helper.app"
        self.rpaths = ["/usr/lib/swift", "@executable_path/../Frameworks", "@loader_path"]
        self.arches = "arm64"
        self.framework_arches = "arm64 x86_64"
        self.dependencies = [PACKAGE.SPARKLE_LIBRARY, "/usr/lib/libSystem.B.dylib"]
        self.minos = "13.0"
        self.calls = []
        self.runner = patch.object(PACKAGE, "run", side_effect=self.tool)
        self.runner.start()
        self.addCleanup(self.runner.stop)

    @staticmethod
    def write_plist(path, value):
        path.write_bytes(plistlib.dumps(value))

    def tool(self, args):
        self.calls.append(args)
        if args[0] == "/usr/bin/lipo":
            return self.framework_arches if args[-1].endswith("/Sparkle") else self.arches
        if args[0] == "/usr/bin/otool" and "-l" in args:
            return "cmd LC_BUILD_VERSION\n minos " + self.minos + "\n" + "".join(
                f"cmd LC_RPATH\ncmdsize 48\npath {value} (offset 12)\n" for value in self.rpaths
            )
        if args[0] == "/usr/bin/otool" and "-L" in args:
            return "header:\n" + "".join(
                f"\t{value} (compatibility version 1.0.0, current version 1.0.0)\n"
                for value in self.dependencies
            )
        if args[0] == "/usr/bin/install_name_tool":
            self.assertNotEqual(Path(args[-1]), self.binary)
            self.rpaths.remove(args[2])
            return ""
        self.fail(f"unexpected tool: {args}")

    def stage(self):
        return PACKAGE.stage(self.binary, self.framework, self.output, "1")

    def test_stages_private_bundle_without_signing_or_running(self):
        result = self.stage()
        self.assertEqual(result["architectures"], ["arm64"])
        self.assertFalse(result["directUpdatesEnabled"])
        self.assertFalse(result["signatureAndNotarizationVerified"])
        self.assertEqual(self.output.stat().st_mode & 0o777, 0o700)
        self.assertTrue((self.output / "Contents/Frameworks/Sparkle.framework/Versions/Current").is_symlink())
        self.assertEqual(self.binary.read_bytes(), b"compiled helper fixture")
        self.assertEqual({call[0] for call in self.calls}, {
            "/usr/bin/lipo", "/usr/bin/otool", "/usr/bin/install_name_tool"
        })

    def test_existing_output_is_never_replaced(self):
        self.output.mkdir()
        with self.assertRaisesRegex(ValueError, "never replaced"):
            self.stage()

    def test_private_xpc_format_is_explicit_and_updates_remain_disabled(self):
        self.output = self.root / "Helper.xpc"
        result = PACKAGE.stage(self.binary, self.framework, self.output, "1", "xpc")
        info = PACKAGE.read_plist(self.output / "Contents/Info.plist")
        self.assertEqual(info["CFBundlePackageType"], "XPC!")
        self.assertEqual(info["XPCService"], {"ServiceType": "Application"})
        self.assertNotIn("LSUIElement", info)
        self.assertFalse(result["directUpdatesEnabled"])
        self.assertFalse(result["signatureAndNotarizationVerified"])
        self.assertFalse(any(call[0].endswith(("codesign", "launchctl", "open")) for call in self.calls))

    def test_format_and_suffix_must_agree(self):
        with self.assertRaisesRegex(ValueError, "new .xpc"):
            PACKAGE.stage(self.binary, self.framework, self.output, "1", "xpc")
        self.assertFalse(self.output.exists())
        with self.assertRaisesRegex(ValueError, "unsupported"):
            PACKAGE.bundle_info("1", "other")

    def test_xpc_service_configuration_cannot_be_broadened(self):
        self.output = self.root / "Helper.xpc"
        PACKAGE.stage(self.binary, self.framework, self.output, "1", "xpc")
        path = self.output / "Contents/Info.plist"
        pristine = PACKAGE.read_plist(path)
        for service in [{}, {"ServiceType": "System"}, {"ServiceType": True},
                        {"ServiceType": "Application", "RunLoopType": "dispatch_main"},
                        {"ServiceType": "Application", "JoinExistingSession": True}]:
            with self.subTest(service=service):
                self.write_plist(path, {**pristine, "XPCService": service})
                with self.assertRaises(ValueError):
                    PACKAGE.verify(self.output)

    def test_app_metadata_cannot_silently_become_a_service(self):
        self.stage()
        path = self.output / "Contents/Info.plist"
        self.write_plist(path, PACKAGE.bundle_info("1", "xpc"))
        with self.assertRaises(ValueError):
            PACKAGE.verify(self.output)

    def test_dangling_output_symlink_is_never_followed(self):
        self.output.symlink_to(self.root / "absent")
        with self.assertRaisesRegex(ValueError, "never replaced"):
            self.stage()
        self.assertTrue(self.output.is_symlink())

    def test_rejects_input_executable_symlink(self):
        alias = self.root / "alias"
        alias.symlink_to(self.binary)
        with self.assertRaisesRegex(ValueError, "concrete executable"):
            PACKAGE.stage(alias, self.framework, self.output, "1")

    def test_rejects_set_id_executable(self):
        self.binary.chmod(0o4755)
        with self.assertRaisesRegex(ValueError, "set-id"):
            self.stage()

    def test_rejects_escaping_framework_symlink(self):
        (self.framework / "escape").symlink_to(self.binary)
        with self.assertRaisesRegex(ValueError, "escaping"):
            self.stage()
        self.assertFalse(self.output.exists())

    def test_rejects_special_framework_entry(self):
        os.mkfifo(self.framework / "pipe")
        with self.assertRaisesRegex(ValueError, "unsupported bundle entry"):
            self.stage()

    def test_rejects_wrong_framework_version(self):
        self.write_plist(self.framework / "Resources/Info.plist", {
            "CFBundleIdentifier": "org.sparkle-project.Sparkle", "CFBundleShortVersionString": "2.8.0"
        })
        with self.assertRaisesRegex(ValueError, "pinned"):
            self.stage()

    def test_rejects_missing_framework_member(self):
        (self.framework / PACKAGE.NESTED_CODE[0]).unlink()
        with self.assertRaisesRegex(ValueError, "incomplete"):
            self.stage()

    def test_rejects_wrong_minimum_os(self):
        self.minos = "14.0"
        with self.assertRaisesRegex(ValueError, "13.0"):
            self.stage()

    def test_rejects_unsupported_architecture(self):
        self.arches = "i386"
        with self.assertRaisesRegex(ValueError, "architectures"):
            self.stage()

    def test_rejects_missing_framework_slice(self):
        self.framework_arches = "x86_64"
        with self.assertRaisesRegex(ValueError, "lacks helper architecture"):
            self.stage()

    def test_rejects_target_app_framework_dependency(self):
        self.dependencies.append("/Applications/Target.app/Contents/Frameworks/Other.framework/Other")
        with self.assertRaisesRegex(ValueError, "non-system"):
            self.stage()

    def test_rejects_missing_packaged_rpath(self):
        self.rpaths.remove("@executable_path/../Frameworks")
        with self.assertRaisesRegex(ValueError, "every helper slice"):
            self.stage()

    def test_rejects_unsafe_rpath_in_staged_bundle(self):
        self.stage()
        self.rpaths.append("/tmp/other-frameworks")
        with self.assertRaisesRegex(ValueError, "confined"):
            PACKAGE.verify(self.output)

    def test_rejects_enabled_or_added_capability(self):
        self.stage()
        path = self.output / "Contents/Info.plist"
        pristine = PACKAGE.read_plist(path)
        for key, value in [("HelmExternalDirectUpdatesEnabled", True), ("SUEnableAutomaticChecks", True),
                           ("HelmExternalUpdaterProtocolVersion", True), ("CFBundleVersion", 1),
                           ("UnexpectedKey", "anything")]:
            with self.subTest(key=key):
                self.write_plist(path, {**pristine, key: value})
                with self.assertRaises((ValueError, TypeError)):
                    PACKAGE.verify(self.output)

    def test_rejects_invalid_builds(self):
        for build in ["0", "01", "-1", "1.2", "1234567890", "1\n"]:
            with self.subTest(build=build), self.assertRaises(ValueError):
                PACKAGE.bundle_info(build)


if __name__ == "__main__":
    unittest.main()
