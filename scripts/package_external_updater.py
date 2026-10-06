#!/usr/bin/env python3
"""Stage/check an unembedded QA helper. Never sign, register, launch or publish."""

import argparse
import json
import os
from pathlib import Path
import plistlib
import re
import shutil
import stat
import subprocess

IDENTIFIER = "com.jasoncavinder.Helm.SparkleExternalUpdater"
EXECUTABLE = "HelmSparkleExternalUpdater"
SPARKLE_VERSION = "2.9.5"
SPARKLE_LIBRARY = "@rpath/Sparkle.framework/Versions/B/Sparkle"
RPATHS = {"/usr/lib/swift", "@executable_path/../Frameworks"}
NESTED_CODE = (
    "Versions/B/XPCServices/Installer.xpc",
    "Versions/B/XPCServices/Downloader.xpc",
    "Versions/B/Autoupdate",
    "Versions/B/Updater.app",
)


def run(arguments):
    return subprocess.run(arguments, check=True, capture_output=True, text=True).stdout


def read_plist(path):
    if not path.is_file() or path.stat().st_size > 2 * 1024 * 1024:
        raise ValueError(f"missing/unbounded plist: {path}")
    with path.open("rb") as source:
        value = plistlib.load(source)
    if not isinstance(value, dict):
        raise ValueError(f"not a dictionary: {path}")
    return value


def inspect_tree(root):
    """Bounded, non-following traversal before copying any supplied framework."""
    root = root.resolve(strict=True)
    count = 0
    for parent, directories, files in os.walk(root, followlinks=False):
        for name in directories + files:
            path = Path(parent) / name
            count += 1
            if count > 100_000:
                raise ValueError("bundle exceeds entry limit")
            mode = path.lstat().st_mode
            if stat.S_ISLNK(mode):
                resolved = path.resolve(strict=True)
                if not resolved.is_relative_to(root):
                    raise ValueError(f"escaping bundle symlink: {path}")
            elif not (stat.S_ISDIR(mode) or stat.S_ISREG(mode)):
                raise ValueError(f"unsupported bundle entry: {path}")
            elif mode & (stat.S_ISUID | stat.S_ISGID):
                raise ValueError(f"set-id bundle entry: {path}")


def inspect_framework(framework):
    if framework.name != "Sparkle.framework" or framework.is_symlink() or not framework.is_dir():
        raise ValueError("expected a concrete Sparkle.framework directory")
    inspect_tree(framework)
    info = read_plist(framework / "Resources/Info.plist")
    if (info.get("CFBundleIdentifier") != "org.sparkle-project.Sparkle"
            or info.get("CFBundleShortVersionString") != SPARKLE_VERSION):
        raise ValueError("framework is not the pinned Sparkle version")
    for child in ("Sparkle", *NESTED_CODE):
        if not (framework / child).exists():
            raise ValueError(f"incomplete Sparkle distribution: {child}")


def inspect_macho(binary):
    architectures = set(run(["/usr/bin/lipo", "-archs", str(binary)]).split())
    if not architectures or not architectures <= {"arm64", "x86_64"}:
        raise ValueError("unsupported helper architectures")
    paths = set()
    for arch in sorted(architectures):
        loads = run(["/usr/bin/otool", "-arch", arch, "-l", str(binary)])
        versions = re.findall(r"\bminos (\d+)\.(\d+)(?:\.\d+)?", loads)
        if versions != [("13", "0")]:
            raise ValueError("helper must target macOS 13.0 exactly")
        architecture_paths = set(re.findall(r"cmd LC_RPATH\s+cmdsize \d+\s+path (.+) \(offset \d+\)", loads))
        if "@executable_path/../Frameworks" not in architecture_paths:
            raise ValueError("every helper slice must have the packaged-framework rpath")
        paths.update(architecture_paths)
        dependencies = run(["/usr/bin/otool", "-arch", arch, "-L", str(binary)])
        libraries = re.findall(r"^\s+(.+) \(compatibility version ", dependencies, re.MULTILINE)
        if libraries.count(SPARKLE_LIBRARY) != 1:
            raise ValueError("helper must link its own Sparkle framework")
        if any(not (lib == SPARKLE_LIBRARY or lib.startswith(("/usr/lib/", "/System/Library/"))) for lib in libraries):
            raise ValueError("non-system dependency outside the embedded Sparkle framework")
    return architectures, paths


def bundle_info(build, bundle_format="app"):
    if bundle_format not in {"app", "xpc"}:
        raise ValueError("unsupported helper bundle format")
    if not re.fullmatch(r"[1-9][0-9]{0,8}", build):
        raise ValueError("build must be a positive integer of at most nine digits")
    info = {
        "CFBundleIdentifier": IDENTIFIER,
        "CFBundleExecutable": EXECUTABLE,
        "CFBundleName": "Helm Sparkle External Updater",
        "CFBundlePackageType": "APPL",
        "CFBundleVersion": build,
        "CFBundleShortVersionString": "0.1.0",
        "LSMinimumSystemVersion": "13.0",
        "LSUIElement": True,
        "HelmDistributionChannel": "developer_id",
        "HelmExternalUpdaterProtocolVersion": 1,
        "HelmExternalDirectUpdatesEnabled": False,
        "SUEnableAutomaticChecks": False,
        "SUAutomaticallyUpdate": False,
        "SUEnableInstallerLauncherService": False,
        "SUEnableDownloaderService": False,
    }
    if bundle_format == "xpc":
        info["CFBundlePackageType"] = "XPC!"
        info["XPCService"] = {"ServiceType": "Application"}
        del info["LSUIElement"]
    return info


def verify(app):
    if app.is_symlink() or not app.is_dir() or app.suffix not in {".app", ".xpc"}:
        raise ValueError("expected a concrete application or private XPC bundle")
    inspect_tree(app)
    info = read_plist(app / "Contents/Info.plist")
    expected = bundle_info(info.get("CFBundleVersion", ""), app.suffix[1:])
    if info.keys() != expected.keys() or any(type(info[key]) is not type(value) or info[key] != value for key, value in expected.items()):
        raise ValueError("unexpected helper metadata or enabled updater capability")
    binary = app / "Contents/MacOS" / EXECUTABLE
    if binary.is_symlink() or not binary.is_file() or not os.access(binary, os.X_OK):
        raise ValueError("missing concrete executable")
    architectures, paths = inspect_macho(binary)
    if "@executable_path/../Frameworks" not in paths or not paths <= RPATHS:
        raise ValueError("helper rpaths are not confined to the packaged framework/system Swift")
    framework = app / "Contents/Frameworks/Sparkle.framework"
    inspect_framework(framework)
    framework_arches = set(run(["/usr/bin/lipo", "-archs", str(framework / "Sparkle")]).split())
    if not architectures <= framework_arches:
        raise ValueError("framework lacks helper architecture")
    return {"app": str(app.resolve()), "architectures": sorted(architectures),
            "frameworkVersion": SPARKLE_VERSION, "directUpdatesEnabled": False,
            "signatureAndNotarizationVerified": False}


def stage(binary, framework, output, build, bundle_format="app"):
    info = bundle_info(build, bundle_format)
    if binary.is_symlink() or not binary.is_file() or not os.access(binary, os.X_OK):
        raise ValueError("expected a concrete executable input")
    if binary.stat().st_mode & (stat.S_ISUID | stat.S_ISGID):
        raise ValueError("set-id helper input")
    inspect_framework(framework)
    _, paths = inspect_macho(binary)
    if "@executable_path/../Frameworks" not in paths:
        raise ValueError("build is missing the packaged-framework rpath")
    # Normalize only the parent. resolve() on output would follow an existing
    # destination symlink before the no-overwrite check.
    output = output.parent.resolve(strict=True) / output.name
    if output.suffix != "." + bundle_format or output.exists() or output.is_symlink():
        raise ValueError(f"output must be a new .{bundle_format}; existing paths are never replaced")
    if output.is_relative_to(framework.resolve()):
        raise ValueError("output must not be inside the input framework")
    output.mkdir(mode=0o700)
    contents = output / "Contents"
    (contents / "MacOS").mkdir(parents=True)
    (contents / "Frameworks").mkdir()
    staged_binary = contents / "MacOS" / EXECUTABLE
    shutil.copy2(binary, staged_binary)
    shutil.copytree(framework, contents / "Frameworks/Sparkle.framework", symlinks=True)
    with (contents / "Info.plist").open("wb") as target:
        plistlib.dump(info, target, sort_keys=True)
    # SwiftPM may add developer-toolchain and loader-directory search paths.
    # Rewrite only the newly staged copy, before any authorized signing step.
    for path in sorted(paths - RPATHS):
        run(["/usr/bin/install_name_tool", "-delete_rpath", path, str(staged_binary)])
    return verify(output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    create = sub.add_parser("stage")
    create.add_argument("--binary", type=Path, required=True)
    create.add_argument("--framework", type=Path, required=True)
    create.add_argument("--output", type=Path, required=True)
    create.add_argument("--build", required=True)
    create.add_argument("--format", choices=("app", "xpc"), default="app")
    check = sub.add_parser("verify")
    check.add_argument("app", type=Path)
    args = parser.parse_args()
    try:
        result = stage(args.binary, args.framework, args.output, args.build, args.format) if args.action == "stage" else verify(args.app)
        print(json.dumps(result, sort_keys=True))
    except (OSError, ValueError, TypeError, subprocess.CalledProcessError, plistlib.InvalidFileException) as error:
        parser.exit(1, f"external-updater packaging failed: {error}\n")


if __name__ == "__main__":
    main()
