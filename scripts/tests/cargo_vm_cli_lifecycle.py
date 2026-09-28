#!/usr/bin/env python3
"""Opt-in Cargo/Helm CLI lifecycle in a disposable macOS VM.

Builds pinned old crates from crates.io, then verifies a real two-package Plan,
empty-store removal/reinstall and preserved native build options. Requires
registry access, an already installed Rust toolchain and Command Line Tools.
Never installs a toolchain, deletes evidence, or uses the caller's Cargo store.
"""

import argparse
from contextlib import suppress
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import signal
import sqlite3
import subprocess
import sys
import tempfile
import time


def run_process(argv, env, cwd, timeout=1800):
    started = time.monotonic()
    timed_out = False
    with subprocess.Popen(argv, env=env, cwd=cwd, stdin=subprocess.DEVNULL,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                          text=True, start_new_session=True) as process:
        try:
            stdout, stderr = process.communicate(timeout=timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            with suppress(ProcessLookupError):
                os.killpg(process.pid, signal.SIGTERM)
            try:
                stdout, stderr = process.communicate(timeout=3)
            except subprocess.TimeoutExpired:
                with suppress(ProcessLookupError):
                    os.killpg(process.pid, signal.SIGKILL)
                stdout, stderr = process.communicate()
        except BaseException:
            with suppress(ProcessLookupError):
                os.killpg(process.pid, signal.SIGTERM)
            try:
                process.communicate(timeout=3)
            except subprocess.TimeoutExpired:
                with suppress(ProcessLookupError):
                    os.killpg(process.pid, signal.SIGKILL)
                process.communicate()
            raise
    return {'argv': argv, 'exit': process.returncode, 'stdout': stdout,
            'stderr': stderr, 'timed_out': timed_out,
            'seconds': round(time.monotonic() - started, 3)}


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--helm', required=True, type=Path)
    parser.add_argument('--rustup', required=True, type=Path)
    parser.add_argument('--rustup-home', required=True, type=Path)
    parser.add_argument('--toolchain', required=True)
    parser.add_argument('--artifacts', required=True, type=Path)
    parser.add_argument('--confirm-disposable-environment', action='store_true')
    args = parser.parse_args()
    if not args.confirm_disposable_environment or sys.platform != 'darwin':
        parser.error('requires explicit opt-in inside a disposable macOS environment')
    if sys.version_info < (3, 11):
        parser.error('requires Python 3.11 or newer')
    for path in (args.helm, args.rustup):
        if not path.is_absolute() or not path.is_file() or not os.access(path, os.X_OK):
            parser.error(f'expected an existing absolute executable: {path}')
    if not args.rustup_home.is_absolute() or not args.rustup_home.is_dir():
        parser.error('rustup-home must be an existing absolute directory')
    if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]*', args.toolchain):
        parser.error('toolchain must name an existing toolchain, not a path or option')
    toolchain = args.rustup_home / 'toolchains' / args.toolchain
    for name in ('cargo', 'rustc'):
        if not (toolchain / 'bin' / name).is_file():
            parser.error(f'toolchain must already contain {name}; no automatic installation')
    if not args.artifacts.is_absolute():
        parser.error('artifacts must be an absolute directory')
    os.umask(0o077)
    args.artifacts.mkdir(parents=True, exist_ok=True)
    root = Path(tempfile.mkdtemp(prefix='cargo-cli-', dir=args.artifacts))
    for name in ('home', 'tmp', 'install', 'bin', 'target', 'config', 'cache', 'data'):
        (root / name).mkdir()
    (root / 'bin/cargo').symlink_to(args.rustup)
    env = {
        'HOME': str(root / 'home'), 'TMPDIR': str(root / 'tmp') + '/',
        'PATH': f'{root}/bin:{toolchain}/bin:/usr/bin:/bin:/usr/sbin:/sbin',
        'CARGO_HOME': str(root / 'home/.cargo'), 'CARGO_INSTALL_ROOT': str(root / 'install'),
        'CARGO_TARGET_DIR': str(root / 'target'), 'CARGO_BUILD_JOBS': '4',
        'RUSTUP_HOME': str(args.rustup_home), 'RUSTUP_TOOLCHAIN': args.toolchain,
        'RUSTUP_AUTO_INSTALL': '0', 'RUSTC': str(toolchain / 'bin/rustc'),
        'XDG_CONFIG_HOME': str(root / 'config'), 'XDG_CACHE_HOME': str(root / 'cache'),
        'XDG_DATA_HOME': str(root / 'data'), 'LC_ALL': 'C',
        'HELM_DB_PATH': str(root / 'helm.db'), 'HELM_ACCEPT_LICENSE': '1',
        'HELM_ACCEPT_DEFAULTS': '1',
    }
    report = {'status': 'running', 'macos': platform.mac_ver()[0],
              'architecture': platform.machine(), 'helm_sha256': digest(args.helm),
              'cargo_sha256': digest(toolchain / 'bin/cargo'),
              'rustc_sha256': digest(toolchain / 'bin/rustc'), 'toolchain': args.toolchain,
              'scope': 'isolated CLI/core public crates.io lifecycle; not GUI or all-crate certification'}
    records = []
    print(f'Evidence: {root}', flush=True)

    def run(label, argv):
        result = run_process([str(argument) for argument in argv], env, root)
        result['label'] = label
        records.append(result)
        (root / 'commands.json').write_text(json.dumps(records, indent=2) + '\n')
        assert result['exit'] == 0 and not result['timed_out'], f'{label}: inspect {root}/commands.json'
        print(f'{label}: passed', flush=True)
        return result

    def cli(label, *arguments):
        result = run(label, [args.helm, '--json', '--wait', '--timeout', '1800', *arguments])
        payloads = [json.loads(line) for line in result['stdout'].splitlines() if line.strip()]
        assert len(payloads) == 1, (label, payloads)
        return payloads[0]['data']

    def receipt():
        return json.loads((root / 'install/.crates2.json').read_text())['installs']

    def inventory(label):
        return {item['package']['name']: item['installed_version']
                for item in cli(label, 'packages', 'list')['packages']
                if item['package']['manager'] == 'cargo'}

    cargo = root / 'bin/cargo'
    try:
        report['cargo_version'] = run('cargo-version', [cargo, '--version'])['stdout'].strip()
        for name, version in (('sd', '0.7.6'), ('hyperfine', '1.18.0')):
            run('seed-' + name, [cargo, 'install', name, '--version', version, '--locked'])
        original = receipt()
        (root / 'receipt-before.json').write_text(json.dumps(original, indent=2) + '\n')
        cli('detect', 'managers', 'detect', 'cargo')
        cli('refresh', 'refresh', '--manager', 'cargo')
        assert inventory('old-inventory') == {'sd': '0.7.6', 'hyperfine': '1.18.0'}
        plan = cli('review-two-package-plan', 'updates', 'preview', '--manager', 'cargo')
        assert plan['total_steps'] == 2, plan
        targets = {step['packageName']: step['candidateVersion'] for step in plan['steps']}
        assert set(targets) == {'sd', 'hyperfine'} and all(targets.values()), targets
        assert all(step['reviewedScope'].startswith('cargo-review-v1:') for step in plan['steps'])
        report['reviewed_targets'] = targets
        result = cli('run-two-package-plan', 'updates', 'run', '--manager', 'cargo', '--yes')
        assert result['total_steps'] == 2 and result['failed_steps'] == 0, result
        assert len(result['results']) == 2 and all(item['success'] for item in result['results']), result
        assert inventory('verified-plan-inventory') == targets
        updated = receipt()
        assert len(updated) == 2
        for name, version in targets.items():
            key = f'{name} {version} (registry+https://github.com/rust-lang/crates.io-index)'
            previous = next(value for key, value in original.items() if key.startswith(name + ' '))
            for field in ('bins', 'features', 'all_features', 'no_default_features', 'profile', 'target'):
                assert updated[key][field] == previous[field], (name, field)
            assert run('native-' + name, [root / 'install/bin' / name, '--version'])['stdout'].strip() == f'{name} {version}'
        cli('refresh-after-plan', 'refresh', '--manager', 'cargo')
        assert cli('no-remaining-plan', 'updates', 'preview', '--manager', 'cargo')['total_steps'] == 0
        assert cli('completed-plan-noop', 'updates', 'run', '--manager', 'cargo', '--yes')['total_steps'] == 0
        peer_hash = digest(root / 'install/bin/hyperfine')
        cli('remove-preview', 'packages', 'uninstall', 'sd', '--manager', 'cargo', '--preview')
        assert inventory('preview-is-read-only') == targets
        cli('remove-first-tool', 'packages', 'uninstall', 'sd', '--manager', 'cargo', '--yes')
        assert inventory('peer-preserved') == {'hyperfine': targets['hyperfine']}
        assert digest(root / 'install/bin/hyperfine') == peer_hash
        cli('remove-last-tool', 'packages', 'uninstall', 'hyperfine', '--manager', 'cargo', '--yes')
        cli('empty-refresh', 'refresh', '--manager', 'cargo')
        assert not inventory('empty-inventory')
        assert not receipt()
        for name in targets:
            assert not os.path.lexists(root / 'install/bin' / name)
        cli('fresh-install-after-empty', 'packages', 'install', 'sd', '--manager', 'cargo', '--version', targets['sd'])
        assert inventory('fresh-install-persisted') == {'sd': targets['sd']}
        run('native-build-options', [cargo, 'install', 'sd', '--version', targets['sd'], '--locked',
                                     '--force', '--no-default-features', '--profile', 'dev'])
        configured = receipt()
        cli('option-preserving-reinstall', 'packages', 'install', 'sd', '--manager', 'cargo', '--version', targets['sd'])
        assert receipt() == configured
        options = next(iter(configured.values()))
        assert options['no_default_features'] is True and options['profile'] == 'dev'
        cli('remove-final-fixture', 'packages', 'uninstall', 'sd', '--manager', 'cargo', '--yes')
        assert not inventory('final-empty-inventory')
        tasks = cli('terminal-tasks', 'tasks', 'list')['tasks']
        assert tasks and all(task['status'] == 'completed' for task in tasks)
        with sqlite3.connect(root / 'helm.db') as database:
            assert database.execute('PRAGMA integrity_check').fetchone()[0] == 'ok'
        report['status'] = 'passed'
    except BaseException as error:
        report.update(status='failed', error=f'{type(error).__name__}: {error}')
        raise
    finally:
        report['commands'] = len(records)
        (root / 'report.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
