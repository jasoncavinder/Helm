from pathlib import Path
import unittest

import yaml


ROOT = Path(__file__).resolve().parents[3]


class DependencySecurityPolicyTests(unittest.TestCase):
    def setUp(self):
        path = ROOT / ".github/workflows/dependency-security.yml"
        self.workflow = yaml.load(path.read_text(), Loader=yaml.BaseLoader)
        self.job = self.workflow["jobs"]["npm-audit"]

    def test_full_audit_covers_prs_and_the_schedule_without_path_filters(self):
        triggers = self.workflow["on"]
        self.assertEqual(triggers["pull_request"], {"branches": ["main", "dev"]})
        self.assertTrue(triggers["schedule"][0]["cron"])
        self.assertNotIn("if", self.job)

    def test_audit_includes_dev_dependencies_and_fails_on_low_severity(self):
        self.assertEqual(self.job["defaults"]["run"]["working-directory"], "web")
        commands = [step["run"] for step in self.job["steps"] if "run" in step]
        self.assertEqual(commands, ["npm audit --package-lock-only --audit-level=low"])
        self.assertNotIn("continue-on-error", self.job)
        self.assertNotIn("env", self.job)
        self.assertNotIn("env", self.workflow)
        for step in self.job["steps"]:
            self.assertNotIn("if", step)
            self.assertNotIn("continue-on-error", step)
            self.assertNotIn("env", step)

    def test_audit_uses_read_only_permissions(self):
        self.assertEqual(self.workflow["permissions"], {"contents": "read"})
        self.assertEqual(self.job.get("permissions", {"contents": "read"}), {"contents": "read"})


if __name__ == "__main__":
    unittest.main()
