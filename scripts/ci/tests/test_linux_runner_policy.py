import copy
from pathlib import Path
import sys
import tempfile
import unittest

import yaml

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from check_linux_runner_policy import (  # noqa: E402
    CANARY_WORKFLOW,
    PolicyError,
    UniqueKeyLoader,
    check_directory,
    validate_workflow,
)


ROOT = Path(__file__).resolve().parents[3]
WORKFLOWS = ROOT / ".github/workflows"


def load_workflow(name):
    return yaml.load((WORKFLOWS / name).read_text(), Loader=UniqueKeyLoader)


class LinuxRunnerPolicyTests(unittest.TestCase):
    def test_repository_policy(self):
        self.assertGreater(check_directory(WORKFLOWS), 0)

    def test_every_existing_ubuntu_selection_is_pinned(self):
        # The initial mitigation covers all 18 selections, not only PR jobs.
        count = 0
        for path in WORKFLOWS.glob("*.yml"):
            data = yaml.load(path.read_text(), Loader=UniqueKeyLoader)
            for job in data["jobs"].values():
                selection = job.get("runs-on", "")
                if selection == "ubuntu-24.04":
                    count += 1
                for row in job.get("strategy", {}).get("matrix", {}).get("include", []):
                    if row.get("os") == "ubuntu-24.04":
                        count += 1
        self.assertGreaterEqual(count, 18)

    def test_each_direct_linux_job_rejects_alias(self):
        for path in WORKFLOWS.glob("*.yml"):
            original = load_workflow(path.name)
            for job_id, job in original["jobs"].items():
                if job.get("runs-on") != "ubuntu-24.04":
                    continue
                with self.subTest(workflow=path.name, job=job_id):
                    changed = copy.deepcopy(original)
                    changed["jobs"][job_id]["runs-on"] = "ubuntu-latest"
                    with self.assertRaisesRegex(PolicyError, "Linux runner"):
                        validate_workflow(path.name, changed)

    def test_each_codeql_linux_matrix_entry_rejects_alias(self):
        for index in range(3):
            data = load_workflow("codeql.yml")
            data["jobs"]["analyze"]["strategy"]["matrix"]["include"][index]["os"] = "ubuntu-latest"
            with self.subTest(index=index), self.assertRaisesRegex(PolicyError, "Linux runner"):
                validate_workflow("codeql.yml", data)

    def test_axis_matrix_and_label_arrays(self):
        for selection, strategy in (
            ("${{ matrix.os }}", {"matrix": {"os": ["ubuntu-latest", "macos-26"]}}),
            (["ubuntu-latest"], {}),
        ):
            data = {"jobs": {"test": {"runs-on": selection, "strategy": strategy}}}
            with self.subTest(selection=selection), self.assertRaisesRegex(PolicyError, "Linux runner"):
                validate_workflow("test.yml", data)

    def test_quoted_yaml_and_yaml_extension(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            (directory / "test.yaml").write_text('jobs:\n  test:\n    runs-on: "ubuntu-latest"\n')
            with self.assertRaisesRegex(PolicyError, "test.yaml.*Linux runner"):
                check_directory(directory)

    def test_26_only_allowed_in_designated_canary(self):
        data = {"jobs": {"canary": {"runs-on": "ubuntu-26.04"}}}
        validate_workflow(CANARY_WORKFLOW, data)
        with self.assertRaisesRegex(PolicyError, "Linux runner"):
            validate_workflow("ci-test.yml", data)
        data["jobs"]["canary"]["runs-on"] = "ubuntu-latest"
        with self.assertRaisesRegex(PolicyError, "Linux runner"):
            validate_workflow(CANARY_WORKFLOW, data)

    def test_dynamic_runner_or_matrix_requires_review(self):
        for data in (
            {"runs-on": "${{ inputs.runner }}"},
            {"runs-on": "${{ matrix.os }}", "strategy": {"matrix": "${{ fromJSON(inputs.matrix) }}"}},
            {"runs-on": "${{ matrix.os }}", "strategy": {"matrix": {}}},
        ):
            with self.subTest(job=data), self.assertRaises(PolicyError):
                validate_workflow("test.yml", {"jobs": {"test": data}})

    def cache_fixture(self):
        data = load_workflow("ci-test.yml")
        job = data["jobs"]["rust-tests"]
        cache = next(step for step in job["steps"] if step.get("uses", "").startswith("actions/cache@"))
        return data, job, cache

    def test_cache_key_requires_every_dimension(self):
        for old, new in (
            ("ubuntu-24.04", "${{ runner.os }}"),
            ("${{ runner.arch }}", ""),
            ("1.97.1", "stable"),
            ("${{ hashFiles('core/rust/Cargo.lock') }}", ""),
            ("cargo-v2", "cargo"),
        ):
            data, _, cache = self.cache_fixture()
            cache["with"]["key"] = cache["with"]["key"].replace(old, new)
            with self.subTest(dimension=old), self.assertRaisesRegex(PolicyError, "Cargo key"):
                validate_workflow("ci-test.yml", data)

    def test_broad_restore_fallback_is_rejected_even_after_safe_prefix(self):
        for fallback in ("${{ runner.os }}-cargo-", "cargo-v2-ubuntu-24.04-", "cargo-v2-"):
            data, _, cache = self.cache_fixture()
            cache["with"]["restore-keys"] += "\n" + fallback
            with self.subTest(fallback=fallback), self.assertRaisesRegex(PolicyError, "restore prefixes"):
                validate_workflow("ci-test.yml", data)

    def test_changed_toolchain_requires_matching_key_and_restore(self):
        data, job, _ = self.cache_fixture()
        step = next(step for step in job["steps"] if step.get("uses", "").startswith("dtolnay/rust-toolchain@"))
        step["with"]["toolchain"] = "1.98.1"
        with self.assertRaisesRegex(PolicyError, "Cargo key"):
            validate_workflow("ci-test.yml", data)

    def test_omitted_restore_is_safe(self):
        data, _, cache = self.cache_fixture()
        del cache["with"]["restore-keys"]
        validate_workflow("ci-test.yml", data)

    def test_restore_and_save_actions_cannot_bypass_cache_policy(self):
        for action in ("actions/cache/restore", "actions/cache/save"):
            data, _, cache = self.cache_fixture()
            cache["uses"] = action + "@test"
            cache["with"]["key"] = "old-cache"
            with self.subTest(action=action), self.assertRaisesRegex(PolicyError, "Cargo key"):
                validate_workflow("ci-test.yml", data)

    def test_dynamic_cache_path_cannot_hide_compiled_artifacts(self):
        data, _, cache = self.cache_fixture()
        cache["with"]["path"] = "${{ env.CARGO_TARGET_DIR }}"
        with self.assertRaisesRegex(PolicyError, "cache paths must be static"):
            validate_workflow("ci-test.yml", data)

    def test_duplicate_yaml_keys_rejected(self):
        with self.assertRaisesRegex(PolicyError, "duplicate YAML key"):
            yaml.load("jobs:\n  test:\n    runs-on: ubuntu-latest\n    runs-on: ubuntu-24.04\n", Loader=UniqueKeyLoader)

    def test_empty_workflow_directory_rejected(self):
        with tempfile.TemporaryDirectory() as tmp, self.assertRaisesRegex(PolicyError, "no workflows"):
            check_directory(Path(tmp))

    def test_policy_and_regressions_are_wired_to_required_contract_job(self):
        data = load_workflow("release-contract-checks.yml")
        scripts = "\n".join(step.get("run", "") for step in data["jobs"]["release-contracts"]["steps"])
        self.assertIn("scripts/ci/requirements.txt", scripts)
        self.assertIn("scripts/ci/check_linux_runner_policy.py", scripts)
        self.assertIn("test_linux_runner_policy.py", scripts)


if __name__ == "__main__":
    unittest.main()
