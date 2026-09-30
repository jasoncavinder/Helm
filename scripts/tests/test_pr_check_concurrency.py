import importlib.util
from pathlib import Path
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    'pr_check_concurrency', ROOT / 'scripts/ci/check_pr_check_concurrency.py')
CONTRACT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CONTRACT)
VALID = ('name: Fixture\n\nconcurrency:\n'
         f'  group: {CONTRACT.GROUP}\n'
         f'  cancel-in-progress: {CONTRACT.CANCEL}\n\njobs:\n  test: {{}}\n')


class PRCheckConcurrencyTests(unittest.TestCase):
    def test_repository_workflows_match_contract(self):
        self.assertEqual(CONTRACT.check_directory(ROOT / '.github/workflows'), [])

    def test_valid_block_and_comments(self):
        CONTRACT.validate(VALID)
        CONTRACT.validate(VALID.replace('concurrency:\n', 'concurrency:\n  # PR only\n'))

    def test_missing_or_job_only_block_is_rejected(self):
        for text in ('name: Fixture\n', VALID.replace('concurrency:', '    concurrency:')):
            with self.subTest(text=text), self.assertRaises(ValueError):
                CONTRACT.validate(text)

    def test_duplicate_blocks_and_fields_are_rejected(self):
        for text in (VALID + VALID,
                     VALID.replace('concurrency:\n', f'concurrency:\n  group: {CONTRACT.GROUP}\n')):
            with self.subTest(text=text), self.assertRaises(ValueError):
                CONTRACT.validate(text)

    def test_cross_workflow_and_cross_pr_groups_are_rejected(self):
        for group in ('${{ github.event.pull_request.number }}', '${{ github.workflow }}',
                      '${{ github.head_ref }}', '${{ github.workflow }}-${{ github.ref }}'):
            with self.subTest(group=group), self.assertRaises(ValueError):
                CONTRACT.validate(VALID.replace(CONTRACT.GROUP, group))

    def test_non_pr_and_queued_run_collisions_are_rejected(self):
        # cancel-in-progress=false does not prevent pending-run replacement.
        for group in (CONTRACT.GROUP.replace("format('run-{0}', github.run_id)", "github.ref"),
                      CONTRACT.GROUP.replace("github.event_name == 'pull_request' && ", '')):
            with self.subTest(group=group), self.assertRaises(ValueError):
                CONTRACT.validate(VALID.replace(CONTRACT.GROUP, group))

    def test_unconditional_cancellation_or_no_cancellation_is_rejected(self):
        for cancel in ('true', 'false', "${{ github.ref != 'refs/heads/main' }}"):
            with self.subTest(cancel=cancel), self.assertRaises(ValueError):
                CONTRACT.validate(VALID.replace(CONTRACT.CANCEL, cancel))

    def test_directory_reports_each_missing_or_invalid_workflow(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.assertEqual(len(CONTRACT.check_directory(root)), 3)
            for filename in CONTRACT.WORKFLOWS:
                (root / filename).write_text(VALID, encoding='utf-8')
            self.assertEqual(CONTRACT.check_directory(root), [])
            (root / CONTRACT.WORKFLOWS[0]).write_text('name: invalid\n', encoding='utf-8')
            errors = CONTRACT.check_directory(root)
            self.assertEqual(len(errors), 1)
            self.assertTrue(errors[0].startswith(CONTRACT.WORKFLOWS[0] + ':'))


if __name__ == '__main__':
    unittest.main()
