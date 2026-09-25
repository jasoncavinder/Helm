#!/usr/bin/env python3
"""Opt-in pnpm 12 CLI safety boundary check in a disposable macOS VM.

Creates a native two-package group in a fresh scope, then proves Helm can read it
but refuses mutations without removing its peer. Requires public registry access.
Retains the scope and evidence; does not uninstall managers or change host config.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile

from uv_vm_cli_lifecycle import run_process


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--helm', required=True, type=Path)
    parser.add_argument('--pnpm', required=True, type=Path)
    parser.add_argument('--artifacts', required=True, type=Path)
    parser.add_argument('--confirm-disposable-environment', action='store_true')
    args = parser.parse_args()
    if not args.confirm_disposable_environment or sys.platform != 'darwin':
        parser.error('requires explicit opt-in inside a disposable macOS VM')
    for path in (args.helm, args.pnpm):
        if not path.is_absolute() or not path.is_file():
            parser.error(f'expected an absolute executable path: {path}')
    if not args.artifacts.is_absolute():
        parser.error('artifacts must be an absolute path')
    os.umask(0o077)
    args.artifacts.mkdir(parents=True, exist_ok=True)
    root = Path(tempfile.mkdtemp(prefix='pnpm-boundary-', dir=args.artifacts))
    for name in ('home', 'pnpm/bin', 'tmp', 'config', 'cache', 'data'):
        (root / name).mkdir(parents=True)
    env = {
        'HOME': str(root / 'home'), 'PNPM_HOME': str(root / 'pnpm'),
        'TMPDIR': str(root / 'tmp'), 'XDG_CONFIG_HOME': str(root / 'config'),
        'XDG_CACHE_HOME': str(root / 'cache'), 'XDG_DATA_HOME': str(root / 'data'),
        'PATH': f'{root}/pnpm/bin:{args.pnpm.parent}:/usr/bin:/bin:/usr/sbin:/sbin',
        'HELM_DB_PATH': str(root / 'helm.db'), 'HELM_ACCEPT_LICENSE': '1',
        'HELM_ACCEPT_DEFAULTS': '1', 'LC_ALL': 'C',
    }
    records = []
    report = {'status': 'running', 'helm_sha256': hashlib.file_digest(args.helm.open('rb'), 'sha256').hexdigest()}
    print(f'Evidence: {root}', flush=True)

    def run(label, argv, success=True):
        result = run_process([str(arg) for arg in argv], env, root, 300)
        result['label'] = label
        records.append(result)
        (root / 'commands.json').write_text(json.dumps(records, indent=2))
        assert (result['exit'] == 0) == success, f'{label}: {result}'
        print(f'{label}: expected {"success" if success else "rejection"}', flush=True)
        return result

    def cli(label, *arguments, success=True):
        result = run(label, [args.helm, '--json', '--wait', *arguments], success)
        payloads = [json.loads(line) for line in result['stdout'].splitlines() if line.strip()]
        return payloads[-1]['data'] if payloads else None

    def installed():
        items = cli('inventory', 'packages', 'list')['packages']
        return {item['package']['name']: item['installed_version'] for item in items
                if item['package']['manager'] == 'pnpm'}

    try:
        version = run('native-version', [args.pnpm, '--version'])['stdout'].strip()
        report['pnpm_version'] = version
        assert version == '12.6.0', 're-certify new upstream versions explicitly'
        run('native-group-fixture', [args.pnpm, 'add', '-g', 'prettier@3.5.3,semver@7.6.3'])
        native_before = json.loads(run('native-before', [args.pnpm, 'ls', '-g', '--depth=0', '--json'])['stdout'])
        cli('detect', 'managers', 'detect', 'pnpm')
        cli('select', 'managers', 'executables', 'set', 'pnpm', str(args.pnpm))
        cli('enable', 'managers', 'enable', 'pnpm')
        cli('refresh', 'refresh', '--manager', 'pnpm')
        assert installed() == {'prettier': '3.5.3', 'semver': '7.6.3'}
        cli('pin-peer', 'packages', 'pin', 'semver', '--manager', 'pnpm')
        plan = cli('review', 'updates', 'preview', '--manager', 'pnpm')
        assert all(step.get('package_name', step.get('packageName')) != 'semver' for step in plan['steps'])
        for label, command in [
            ('blocked-install', ['install', 'prettier', '--version', '3.5.3']),
            ('blocked-upgrade', ['upgrade', 'prettier']),
            ('blocked-remove', ['uninstall', 'prettier', '--yes']),
        ]:
            rejection = cli(label, 'packages', *command, '--manager', 'pnpm', success=False)
            assert '[pnpm_global_mutation_unsupported]' in rejection['message']
            cli('refresh-after-rejection', 'refresh', '--manager', 'pnpm')
            assert installed() == {'prettier': '3.5.3', 'semver': '7.6.3'}
        native_after = json.loads(run('native-after', [args.pnpm, 'ls', '-g', '--depth=0', '--json'])['stdout'])
        assert native_before == native_after
        cli('unpin-peer', 'packages', 'unpin', 'semver', '--manager', 'pnpm')
        report['status'] = 'passed'
    finally:
        (root / 'report.json').write_text(json.dumps(report, indent=2))
    print('Boundary passed; this is not a pnpm 12 mutation certification.', flush=True)


if __name__ == '__main__':
    main()
