#!/usr/bin/env python3
"""Pin the PR-only cancellation contract; actionlint validates general YAML."""

import argparse
from pathlib import Path
import re


WORKFLOWS = ('ci-test.yml', 'swiftlint.yml', 'codeql.yml')
GROUP = ("${{ github.workflow }}-${{ github.event_name == 'pull_request' && "
         "format('pr-{0}', github.event.pull_request.number) || "
         "format('run-{0}', github.run_id) }}")
CANCEL = "${{ github.event_name == 'pull_request' }}"


def validate(text):
    blocks = re.findall(r'^concurrency:[ \t]*\n((?:[ \t]+[^\n]*\n|\n)*)',
                        text, re.MULTILINE)
    if len(blocks) != 1:
        raise ValueError('expected one workflow-level concurrency block')
    fields = {}
    for line in blocks[0].splitlines():
        if not line.strip() or line.lstrip().startswith('#'):
            continue
        match = re.fullmatch(r'  ([a-z-]+): (.+)', line)
        if match is None or match[1] in fields:
            raise ValueError('unexpected or duplicate concurrency field')
        fields[match[1]] = match[2]
    if fields != {'group': GROUP, 'cancel-in-progress': CANCEL}:
        raise ValueError('PR checks must cancel only within the same workflow/PR; '
                         'non-PR runs require unique run-ID groups')


def check_directory(directory):
    errors = []
    for filename in WORKFLOWS:
        try:
            validate((directory / filename).read_text(encoding='utf-8'))
        except (OSError, ValueError) as error:
            errors.append(f'{filename}: {error}')
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--workflows-dir', type=Path,
                        default=Path(__file__).resolve().parents[2] / '.github/workflows')
    args = parser.parse_args()
    errors = check_directory(args.workflows_dir)
    if errors:
        parser.exit(1, '\n'.join(errors) + '\n')
    print('PR-only check concurrency contracts passed.')


if __name__ == '__main__':
    main()
