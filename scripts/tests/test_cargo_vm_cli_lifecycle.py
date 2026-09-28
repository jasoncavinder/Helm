"""Harness regressions only; real registry lifecycle remains explicitly opt-in."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from cargo_vm_cli_lifecycle import run_process


class CargoCLIProcessTests(unittest.TestCase):
    def test_explicit_environment_directory_and_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            script = ('import json, os, sys; '
                      'print(json.dumps({"cwd": os.getcwd(), "scope": os.getenv("SCOPE"), '
                      '"unexpected": os.getenv("HELM_PARENT_SENTINEL")})); '
                      'print("diagnostic", file=sys.stderr)')
            previous = os.environ.get('HELM_PARENT_SENTINEL')
            os.environ['HELM_PARENT_SENTINEL'] = 'not-forwarded'
            try:
                result = run_process([sys.executable, '-c', script], {'SCOPE': temporary}, temporary, 10)
            finally:
                if previous is None:
                    os.environ.pop('HELM_PARENT_SENTINEL', None)
                else:
                    os.environ['HELM_PARENT_SENTINEL'] = previous
            self.assertEqual(result['exit'], 0)
            self.assertFalse(result['timed_out'])
            output = json.loads(result['stdout'])
            self.assertEqual(Path(output['cwd']).resolve(), Path(temporary).resolve())
            self.assertEqual(output['scope'], temporary)
            self.assertIsNone(output['unexpected'])
            self.assertEqual(result['stderr'].strip(), 'diagnostic')

    def test_failure_preserves_exit_and_diagnostic(self):
        with tempfile.TemporaryDirectory() as temporary:
            result = run_process([sys.executable, '-c', 'print("failure evidence"); raise SystemExit(7)'],
                                 {}, temporary, 10)
            self.assertEqual(result['exit'], 7)
            self.assertFalse(result['timed_out'])
            self.assertEqual(result['stdout'].strip(), 'failure evidence')

    def test_timeout_keeps_partial_output_and_reaps_child(self):
        with tempfile.TemporaryDirectory() as temporary:
            result = run_process([sys.executable, '-c',
                                  'import os, time; print(os.getpid(), flush=True); time.sleep(20)'],
                                 {}, temporary, 2)
            self.assertTrue(result['timed_out'])
            self.assertNotEqual(result['exit'], 0)
            self.assertLess(result['seconds'], 8)
            pid = int(result['stdout'].strip())
            with self.assertRaises(ProcessLookupError):
                os.kill(pid, 0)

    def test_no_opt_in_does_not_create_scope_or_launch_tools(self):
        with tempfile.TemporaryDirectory() as temporary:
            artifacts = Path(temporary) / 'must-not-exist'
            script = Path(__file__).with_name('cargo_vm_cli_lifecycle.py')
            result = subprocess.run([sys.executable, str(script), '--helm', sys.executable,
                                     '--rustup', sys.executable, '--rustup-home', temporary,
                                     '--toolchain', 'does-not-exist', '--artifacts', str(artifacts)],
                                    capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, 2)
            self.assertIn('requires explicit opt-in', result.stderr)
            self.assertFalse(artifacts.exists())


if __name__ == '__main__':
    unittest.main()
