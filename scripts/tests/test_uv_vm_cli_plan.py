#!/usr/bin/env python3
"""Harness safety only; never invokes Helm, uv or a package installation."""

import contextlib
import io
import os
from pathlib import Path
import signal
import subprocess
import sys
import unittest
from unittest import mock

import uv_vm_cli_lifecycle as lifecycle
import uv_vm_cli_plan as plan


class HarnessSafetyTests(unittest.TestCase):
    def test_receipt_comparison_allows_only_identical_source_repeats(self):
        original = b'[tool.options]\nfind-links = ["a", "b"]\nno-index = true\n'
        repeated = b'[tool.options]\nfind-links = ["a", "b", "a"]\nno-index = true\n'
        self.assertEqual(plan.semantic_receipt(original), plan.semantic_receipt(repeated))
        for changed in (b'[tool.options]\nfind-links = ["b", "a"]\nno-index = true\n',
                        b'[tool.options]\nfind-links = ["a", "b"]\nno-index = false\n'):
            self.assertNotEqual(plan.semantic_receipt(original), plan.semantic_receipt(changed))

    def test_requires_disposable_opt_in_before_process_or_filesystem(self):
        argv = ["plan", "--helm", "/example/helm", "--uv", "/example/uv",
                "--artifacts", "/example/evidence"]
        with mock.patch.object(sys, "argv", argv), mock.patch.object(sys, "platform", "darwin"), \
                mock.patch.object(plan, "run_process") as run, \
                mock.patch.object(Path, "mkdir") as mkdir, contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit) as caught:
                plan.main()
        self.assertEqual(caught.exception.code, 2)
        run.assert_not_called()
        mkdir.assert_not_called()

    def test_rejects_non_macos_even_with_opt_in(self):
        argv = ["plan", "--helm", "/example/helm", "--uv", "/example/uv",
                "--artifacts", "/example/evidence", "--confirm-disposable-environment"]
        with mock.patch.object(sys, "argv", argv), mock.patch.object(sys, "platform", "linux"), \
                mock.patch.object(plan, "run_process") as run, \
                contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit):
                plan.main()
        run.assert_not_called()

    def test_environment_is_private_and_does_not_inherit_credentials_or_stores(self):
        with mock.patch.dict(os.environ, {"UV_TOOL_DIR": "/production/tools",
                                          "HELM_DB_PATH": "/production/helm.db",
                                          "UV_INDEX_PASSWORD": "do-not-inherit",
                                          "HTTPS_PROXY": "untrusted"}):
            env = plan.environment(Path("/private/fixture"), Path("/tools/uv"), Path("/tools/python"))
        self.assertEqual(env["UV_TOOL_DIR"], "/private/fixture/tools")
        self.assertEqual(env["HELM_DB_PATH"], "/private/fixture/helm.db")
        self.assertEqual(env["UV_PYTHON_DOWNLOADS"], "never")
        self.assertEqual(env["UV_OFFLINE"], "true")
        self.assertNotIn("UV_INDEX_PASSWORD", env)
        self.assertNotIn("HTTPS_PROXY", env)

    def exercise_interruption(self, error, cleanup_timeout=False, already_exited=False):
        process = mock.MagicMock()
        process.pid = 12345
        replies = [error]
        if cleanup_timeout:
            replies.append(subprocess.TimeoutExpired(["fixture"], 3))
        replies.append(("", ""))
        process.communicate.side_effect = replies
        with mock.patch.object(lifecycle.subprocess, "Popen") as popen, \
                mock.patch.object(lifecycle.os, "killpg") as kill:
            popen.return_value.__enter__.return_value = process
            if already_exited:
                kill.side_effect = ProcessLookupError()
            with self.assertRaises(type(error)):
                lifecycle.run_process(["fixture"], {}, Path("/private/fixture"), timeout=1)
        self.assertTrue(popen.call_args.kwargs["start_new_session"])
        expected = [mock.call(12345, signal.SIGTERM)]
        if cleanup_timeout:
            expected.append(mock.call(12345, signal.SIGKILL))
        self.assertEqual(kill.call_args_list, expected)

    def test_timeout_reaps_owned_group(self):
        self.exercise_interruption(subprocess.TimeoutExpired(["fixture"], 1))

    def test_keyboard_interrupt_escalates_after_bounded_term_wait(self):
        self.exercise_interruption(KeyboardInterrupt(), cleanup_timeout=True)

    def test_already_exited_group_does_not_mask_original_failure(self):
        self.exercise_interruption(subprocess.TimeoutExpired(["fixture"], 1), already_exited=True)


if __name__ == "__main__":
    unittest.main()
