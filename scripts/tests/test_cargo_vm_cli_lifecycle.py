"""Harness regressions only; real registry lifecycle remains explicitly opt-in."""

import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

from cargo_vm_cli_lifecycle import OwnedSessions, run_process


class CargoCLIProcessTests(unittest.TestCase):
    def assert_process_stopped(self, pid):
        result = subprocess.run(['/bin/ps', '-p', str(pid), '-o', 'stat='],
                                capture_output=True, text=True, timeout=3)
        # An orphan may briefly remain a zombie until init reaps it; it cannot
        # continue the mutation. Direct Popen children are separately reaped.
        self.assertTrue(not result.stdout.strip() or result.stdout.strip().startswith('Z'),
                        f'owned fixture {pid} is still running: {result.stdout!r}')

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

    def test_timeout_stops_owned_child_in_a_separate_process_group(self):
        # Helm's executor gives Cargo its own process group. Killing only the
        # CLI waiter's group must not leave that mutation alive after timeout.
        with tempfile.TemporaryDirectory() as temporary:
            script = (
                'import subprocess, sys, time; '
                'child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(20)"], '
                'stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, '
                'process_group=0); '
                'print(child.pid, flush=True); time.sleep(20)'
            )
            result = run_process([sys.executable, '-c', script], {}, temporary, 1)
            pid = int(result['stdout'].strip())
            try:
                self.assertTrue(result['timed_out'])
                self.assert_process_stopped(pid)
            finally:
                # Only this fixture's recorded child can outlive a failing
                # assertion. Its finite sleep is a second independent bound.
                try:
                    os.kill(pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass

    def test_later_timeout_stops_earlier_coordinator_but_not_unrelated_session(self):
        with tempfile.TemporaryDirectory() as temporary:
            sessions = OwnedSessions(grace=0.2)
            script = (
                'import subprocess, sys; '
                'child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(20)"], '
                'stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, '
                'process_group=0); print(child.pid, flush=True)'
            )
            with subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(20)'],
                                  stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                  stderr=subprocess.DEVNULL, start_new_session=True) as unrelated:
                pid = None
                try:
                    first = run_process([sys.executable, '-c', script], {}, temporary, 5,
                                        owned_sessions=sessions)
                    pid = int(first['stdout'].strip())
                    self.assertEqual(first['exit'], 0)
                    os.kill(pid, 0)
                    later = run_process([sys.executable, '-c', 'import time; time.sleep(20)'],
                                        {}, temporary, 1, owned_sessions=sessions)
                    self.assertTrue(later['timed_out'])
                    self.assert_process_stopped(pid)
                    self.assertIsNone(unrelated.poll())
                finally:
                    sessions.stop()
                    if pid is not None:
                        try:
                            os.kill(pid, signal.SIGTERM)
                        except ProcessLookupError:
                            pass
                    unrelated.terminate()
                    unrelated.wait(timeout=3)

    def test_timeout_escalates_when_owned_process_ignores_sigterm(self):
        with tempfile.TemporaryDirectory() as temporary:
            sessions = OwnedSessions(grace=0.2)
            result = run_process([sys.executable, '-c',
                                  'import os, signal, time; signal.signal(signal.SIGTERM, signal.SIG_IGN); '
                                  'print(os.getpid(), flush=True); time.sleep(20)'],
                                 {}, temporary, 1, owned_sessions=sessions)
            self.assertTrue(result['timed_out'])
            self.assertEqual(result['exit'], -signal.SIGKILL)
            self.assertLess(result['seconds'], 6)
            self.assert_process_stopped(int(result['stdout'].strip()))

    def test_completed_session_ids_are_not_retained_for_later_reuse(self):
        with tempfile.TemporaryDirectory() as temporary:
            sessions = OwnedSessions()
            result = run_process([sys.executable, '-c', 'print("done")'], {}, temporary, 5,
                                 owned_sessions=sessions)
            self.assertEqual(result['exit'], 0)
            self.assertFalse(sessions.sessions)

    def test_interruption_stops_and_reaps_owned_process(self):
        with tempfile.TemporaryDirectory() as temporary:
            communicate = subprocess.Popen.communicate
            interrupted_pid = None

            def interrupt_wait(process, *args, **kwargs):
                nonlocal interrupted_pid
                if process.args[0] == sys.executable:
                    interrupted_pid = int(process.stdout.readline().strip())
                    raise KeyboardInterrupt('controlled interruption')
                return communicate(process, *args, **kwargs)

            with mock.patch.object(subprocess.Popen, 'communicate', interrupt_wait):
                with self.assertRaises(KeyboardInterrupt):
                    run_process([sys.executable, '-c',
                                 'import os, time; print(os.getpid(), flush=True); time.sleep(20)'],
                                {}, temporary, 5)
            self.assertIsNotNone(interrupted_pid)
            with self.assertRaises(ProcessLookupError):
                os.kill(interrupted_pid, 0)

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
