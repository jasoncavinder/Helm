import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import backport


SHA = "a" * 40
REPO = "jasoncavinder/Helm"
ROOT = Path(__file__).resolve().parents[3]


def request(**kwargs):
    values = dict(source_pr=643, commit=SHA, delivery="app", paths=("core/rust/Cargo.lock",),
                  advisories=("RUSTSEC-2026-0346",))
    values.update(kwargs)
    return backport.Request(**values)


def source_pr(**kwargs):
    result = {"merged": True, "state": "closed", "base": {"ref": "dev", "repo": {"full_name": REPO}},
              "commits": 3, "merge_commit_sha": "b" * 40}
    result.update(kwargs)
    return result


class RequestTests(unittest.TestCase):
    def test_command_records_exact_scope_and_identity(self):
        value = backport.parse_command(f"/security-backport {SHA} app core/rust/Cargo.lock RUSTSEC-2026-0346", 643)
        self.assertEqual(value, request())
        self.assertTrue(value.branch.startswith("hotfix/security-pr-643-"))
        self.assertEqual(value.branch, backport.Request(**json.loads(value.encoded)).branch)
        self.assertNotEqual(value.branch, request(advisories=("RUSTSEC-2026-0347",)).branch)

    def test_rejects_shell_injection_traversal_and_non_lockfiles(self):
        for paths in (("../Cargo.lock",), (".github/workflows/ci.yml",), ("web/package.json",),
                      ("core/rust/Cargo.lock;touch /tmp/x",), ("core/rust/Cargo.lock",) * 2, ()):
            with self.subTest(paths=paths), self.assertRaises(backport.BackportError):
                request(paths=paths)

    def test_rejects_ambiguous_commit_delivery_and_advisories(self):
        for change in ({"commit": "HEAD"}, {"commit": "a" * 7}, {"commit": "A" * 40},
                       {"source_pr": True}, {"source_pr": -1}, {"delivery": "website"},
                       {"advisories": ()}, {"advisories": ("RUSTSEC-2026-0346",) * 2},
                       {"advisories": ("$(echo hi)",)}):
            with self.subTest(change=change), self.assertRaises(backport.BackportError):
                request(**change)

    def test_website_request(self):
        request(delivery="website", paths=("web/package-lock.json",), advisories=("GHSA-68fv-2mgg-jv7q", "CVE-2026-93749"))

    def test_exact_command_only(self):
        for body in ("/security-backport", f"/security-backport {SHA} app core/rust/Cargo.lock RUSTSEC-2026-0346 extra",
                     f"Some text /security-backport {SHA} app core/rust/Cargo.lock RUSTSEC-2026-0346"):
            with self.subTest(body=body), self.assertRaises(backport.BackportError):
                backport.parse_command(body, 643)

    def test_source_membership_and_merge_required(self):
        backport.validate_source(request(), source_pr(), [{"sha": SHA}], REPO)
        for change in ({"merged": False}, {"state": "open"}, {"merge_commit_sha": None}, {"commits": 251},
                       {"base": {"ref": "main", "repo": {"full_name": REPO}}},
                       {"base": {"ref": "dev", "repo": {"full_name": "attacker/Helm"}}}):
            with self.subTest(change=change), self.assertRaises(backport.BackportError):
                backport.validate_source(request(), source_pr(**change), [{"sha": SHA}], REPO)
        with self.assertRaisesRegex(backport.BackportError, "does not belong"):
            backport.validate_source(request(), source_pr(), [{"sha": "c" * 40}], REPO)

    def test_body_does_not_claim_validation_or_delivery(self):
        body = backport.pr_body(request(), REPO, "d" * 40, ["docs/unfinished-v020.md"])
        for text in ("docs/unfinished-v020.md", "not a dev-to-main merge", "remain required", "Delivery remains pending", "does not update installed"):
            self.assertIn(text, body)


class FakeGitHub:
    repo = REPO

    def __init__(self, response=None):
        self.response = response
        self.calls = []

    def api(self, path, payload=None, method=None):
        self.calls.append((path, payload, method))
        return self.response

    def pages(self, path):
        self.calls.append((path, None, None))
        return self.response


class AuthorizationTests(unittest.TestCase):
    def test_requires_current_write_permission(self):
        event = {"repository": {"full_name": REPO}}
        for role in ("read", "triage", "none"):
            github = FakeGitHub({"permission": role})
            with self.subTest(role=role), self.assertRaisesRegex(backport.BackportError, "write access"):
                backport.authorize_event(github, event, "contributor")
            self.assertTrue(all(call[1] is None for call in github.calls))
        for role in ("write", "maintain", "admin"):
            backport.authorize_event(FakeGitHub({"permission": role}), event, "maintainer")

    def test_event_repo_and_actor_are_bound(self):
        with self.assertRaisesRegex(backport.BackportError, "repository mismatch"):
            backport.authorize_event(FakeGitHub({"permission": "admin"}), {"repository": {"full_name": "other/repo"}}, "maintainer")
        with self.assertRaisesRegex(backport.BackportError, "invalid actor"):
            backport.authorize_event(FakeGitHub(), {}, "../../admin")

    def test_existing_closed_pr_is_not_reopened(self):
        pr = {"body": request().marker, "state": "closed", "html_url": "https://github.com/test/pr/1"}
        github = FakeGitHub([pr])
        self.assertEqual(backport.existing_pr(github, request()), pr)
        self.assertTrue(all(call[1] is None for call in github.calls))

    def test_existing_pr_with_different_request_is_not_reused(self):
        with self.assertRaisesRegex(backport.BackportError, "exact request"):
            backport.existing_pr(FakeGitHub([{"body": "something else"}]), request())


class GitTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        previous = os.getcwd()
        os.chdir(self.temp.name)
        self.addCleanup(os.chdir, previous)
        self.git("init", "-b", "main")
        self.git("config", "user.name", "Test")
        self.git("config", "user.email", "test@example.invalid")
        self.git("remote", "add", "origin", f"https://github.com/{REPO}.git")
        self.write("core/rust/Cargo.lock", 'name = "rustls"\nversion = "old"\n')
        self.write("app.txt", "stable\n")
        self.git("add", ".")
        self.git("commit", "-m", "stable")
        self.base = self.git("rev-parse", "HEAD")
        self.git("checkout", "-b", "dev")
        self.write("app.txt", "unfinished v020\n")
        self.write("core/rust/Cargo.lock", 'name = "rustls"\nversion = "new"\n')
        self.write("docs/dev-evidence.md", "new feature evidence\n")
        self.git("add", ".")
        self.git("commit", "-m", "mixed source")
        self.source = self.git("rev-parse", "HEAD")
        self.req = request(commit=self.source)
        self.git("checkout", "main")
        self.git("update-ref", "refs/remotes/origin/main", self.base)
        self.git("update-ref", "refs/remotes/origin/dev", self.source)

    def git(self, *args):
        return subprocess.run(["git", *args], capture_output=True, check=True, text=True).stdout.strip()

    def write(self, path, content):
        destination = Path(path)
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(content)

    def tree(self, base=None):
        return backport.prepare_tree(self.req, base or self.base, self.base)

    def test_mixed_source_only_transfers_selected_lockfile(self):
        original_index = self.git("write-tree")
        tree, omitted = self.tree()
        self.assertEqual(self.git("show", f"{tree}:app.txt"), "stable")
        self.assertEqual(self.git("show", f"{tree}:core/rust/Cargo.lock"), 'name = "rustls"\nversion = "new"')
        self.assertEqual(omitted, ["app.txt", "docs/dev-evidence.md"])
        self.assertEqual(self.git("write-tree"), original_index)
        self.assertEqual(self.git("status", "--porcelain"), "")
        self.assertEqual(self.git("branch", "--show-current"), "main")

    def test_conflict_stops_without_mutating_worktree(self):
        self.write("core/rust/Cargo.lock", "different stable contents\n")
        self.git("add", ".")
        self.git("commit", "-m", "stable drift")
        with self.assertRaisesRegex(backport.BackportError, "conflicts"):
            self.tree(self.git("rev-parse", "HEAD"))
        self.assertEqual(self.git("status", "--porcelain"), "")

    def test_already_applied_is_not_a_new_backport(self):
        tree, _ = self.tree()
        with self.assertRaisesRegex(backport.BackportError, "already present"):
            self.tree(tree)

    def test_symlink_in_source_rejected(self):
        self.git("checkout", "dev")
        Path("core/rust/Cargo.lock").unlink()
        Path("core/rust/Cargo.lock").symlink_to("../../app.txt")
        self.git("add", ".")
        self.git("commit", "-m", "unsafe link")
        self.req = request(commit=self.git("rev-parse", "HEAD"))
        with self.assertRaisesRegex(backport.BackportError, "regular files"):
            self.tree()

    def test_unreachable_and_merge_commits_rejected(self):
        pr = source_pr(merge_commit_sha=self.source)
        self.assertEqual(backport.verify_history(self.req, pr, self.source), self.base)
        with self.assertRaisesRegex(backport.BackportError, "not reachable"):
            backport.verify_history(self.req, pr, self.base)
        self.git("merge", "--no-ff", "dev", "-m", "merge")
        merge = self.git("rev-parse", "HEAD")
        with self.assertRaisesRegex(backport.BackportError, "non-merge"):
            backport.verify_history(request(commit=merge), source_pr(merge_commit_sha=merge), merge)

    def orchestration(self, apply=False, moved=False, collision=False, wrong_tree=False, allow_fetch=True):
        owner = self
        tree, _ = self.tree()

        class API(FakeGitHub):
            def pages(self, path):
                self.calls.append((path, None, None))
                return [{"sha": owner.source}] if path.endswith("/commits") else []

            def api(self, path, payload=None, method=None):
                self.calls.append((path, payload, method))
                if path == "pulls/643":
                    return source_pr(merge_commit_sha=owner.source)
                if path == "git/ref/heads/main":
                    return {"object": {"sha": "d" * 40 if moved else owner.base}}
                if path.startswith("git/matching-refs/"):
                    return [{"ref": "collision"}] if collision else []
                if path == f"git/commits/{owner.base}":
                    return {"tree": {"sha": owner.git("rev-parse", f"{owner.base}^{{tree}}")}}
                if path == "git/trees":
                    return {"sha": "f" * 40 if wrong_tree else tree}
                if path == "git/commits":
                    return {"sha": "e" * 40}
                if path == "git/refs":
                    return {}
                if path == "pulls":
                    return {"html_url": "https://github.com/test/pull/1"}
                raise AssertionError(path)

        api = API()
        original_git = backport.git

        def offline_git(*args, **kwargs):
            if args[0] == "fetch":
                self.assertTrue(allow_fetch, "fetch must not run before repository validation")
                return subprocess.CompletedProcess(args, 0, b"", b"")
            return original_git(*args, **kwargs)

        with patch.object(backport, "git", offline_git):
            try:
                result = backport.prepare(api, self.req, apply=apply)
            except backport.BackportError:
                writes = [path for path, payload, _ in api.calls if payload is not None]
                self.assertEqual(writes, ["git/trees"] if wrong_tree else [])
                raise
        return api, result

    def test_dry_run_has_no_remote_mutations(self):
        api, result = self.orchestration()
        self.assertEqual(result["status"], "dry_run")
        self.assertFalse(any(call[1] is not None for call in api.calls))

    def test_apply_creates_only_main_based_branch_and_pr(self):
        api, result = self.orchestration(apply=True)
        writes = {path: payload for path, payload, _ in api.calls if payload is not None}
        self.assertEqual(set(writes), {"git/trees", "git/commits", "git/refs", "pulls"})
        self.assertEqual(writes["git/commits"]["parents"], [self.base])
        self.assertEqual(writes["git/refs"]["ref"], f"refs/heads/{self.req.branch}")
        self.assertEqual(writes["pulls"]["base"], "main")
        self.assertEqual([entry["path"] for entry in writes["git/trees"]["tree"]], list(self.req.paths))
        self.assertEqual(result["delivery"], "not_verified")

    def test_main_drift_and_branch_collision_fail_before_writes(self):
        for kwargs in ({"moved": True}, {"collision": True}):
            with self.subTest(kwargs=kwargs), self.assertRaises(backport.BackportError):
                self.orchestration(apply=True, **kwargs)

    def test_wrong_origin_fails_before_fetch_or_remote_writes(self):
        self.git("remote", "set-url", "origin", "https://github.com/other/Helm.git")
        with self.assertRaisesRegex(backport.BackportError, "origin repository mismatch"):
            self.orchestration(apply=True, allow_fetch=False)

    def test_wrong_remote_tree_never_creates_commit_branch_or_pr(self):
        with self.assertRaisesRegex(backport.BackportError, "differs from verified local tree"):
            self.orchestration(apply=True, wrong_tree=True)


class WorkflowTests(unittest.TestCase):
    def test_trusted_workflow_has_no_candidate_execution_or_release(self):
        workflow = (ROOT / ".github/workflows/security-backport.yml").read_text()
        self.assertIn("issue_comment:", workflow)
        self.assertNotIn("pull_request_target:", workflow)
        self.assertIn("github.workflow_sha", workflow)
        self.assertIn("persist-credentials: false", workflow)
        self.assertIn("github.ref == 'refs/heads/main'", workflow)
        for forbidden in ("pull_request.head", "secrets.", "npm ci", "cargo test", "gh pr merge", "release/runbook", "ubuntu-latest"):
            self.assertNotIn(forbidden, workflow)
        self.assertIn("scripts/security/tests", (ROOT / ".github/workflows/release-contract-checks.yml").read_text())

    def test_tracking_never_equates_merge_to_publication(self):
        value = request()
        pr = {"number": 1, "head": {"ref": value.branch, "repo": {"full_name": REPO}},
              "base": {"ref": "main", "repo": {"full_name": REPO}}, "body": value.marker,
              "merged": True, "state": "closed", "html_url": "https://github.com/test/pull/1"}

        class API(FakeGitHub):
            def pages(self, path):
                return [pr]

        result = backport.status(API(pr))
        self.assertEqual(result["backports"][0]["stage"], "merged_to_main_delivery_unverified")
        self.assertIn("not delivery evidence", result["note"])
        self.assertEqual(result["warnings"], [])


class TrackingTests(unittest.TestCase):
    def pr(self, number=1, **kwargs):
        value = request()
        result = {"number": number, "head": {"ref": value.branch, "repo": {"full_name": REPO}},
                  "base": {"ref": "main", "repo": {"full_name": REPO}}, "body": value.marker,
                  "merged": True, "state": "closed", "html_url": f"https://github.com/{REPO}/pull/{number}"}
        result.update(kwargs)
        return result

    def tracking(self, records):
        class API(FakeGitHub):
            def pages(self, path):
                self.calls.append((path, None, None))
                return records

            def api(self, path, payload=None, method=None):
                self.calls.append((path, payload, method))
                return next(pr for pr in records if path == f"pulls/{pr['number']}")

        api = API()
        return api, backport.status(api)

    def test_foreign_or_missing_repository_is_ignored_before_marker_parsing(self):
        for endpoint in ("head", "base"):
            for repo in ({"full_name": "outsider/Helm"}, None, {}):
                for body in (request().marker, '<!-- helm-security-backport-v1 {"source_pr":1} -->'):
                    with self.subTest(endpoint=endpoint, repo=repo, body=body):
                        spoof = self.pr(2, body=body)
                        spoof[endpoint]["repo"] = repo
                        api, result = self.tracking([spoof, self.pr()])
                        self.assertEqual(len(result["backports"]), 1)
                        self.assertEqual(result["backports"][0]["backport"], self.pr()["html_url"])
                        self.assertEqual(result["warnings"], [])
                        self.assertEqual([call[0] for call in api.calls[1:]], ["pulls/1"])

    def test_wrong_base_is_ignored_and_repository_case_is_insensitive(self):
        wrong_base = self.pr(2)
        wrong_base["base"]["ref"] = "dev"
        valid = self.pr()
        for endpoint in ("head", "base"):
            valid[endpoint]["repo"]["full_name"] = REPO.upper()
        api, result = self.tracking([wrong_base, valid])
        self.assertEqual(len(result["backports"]), 1)
        self.assertEqual([call[0] for call in api.calls[1:]], ["pulls/1"])

    def test_malformed_same_repository_records_warn_without_hiding_valid_rows(self):
        malformed = [None, "no marker", '<!-- helm-security-backport-v1 {bad} -->',
                     '<!-- helm-security-backport-v1 {"source_pr":1} -->',
                     '<!-- helm-security-backport-v1 {"x":' + '[' * 2000 + '0' + ']' * 2000 + '} -->']
        values = json.loads(request().encoded)
        for key, bad_value in (("commit", None), ("paths", [[]]), ("advisories", [None]),
                               ("delivery", "invalid"), ("unexpected", True)):
            malformed.append(f"<!-- {backport.MARKER} {json.dumps(dict(values, **{key: bad_value}))} -->")
        records = [self.pr(number, body=body) for number, body in enumerate(malformed, 2)]
        mismatch = self.pr(len(records) + 2)
        mismatch["head"]["ref"] = "hotfix/security-pr-mismatch"
        records.append(mismatch)
        api, result = self.tracking(records + [self.pr()])
        self.assertEqual(len(result["backports"]), 1)
        self.assertEqual([item["pr"] for item in result["warnings"]], [pr["number"] for pr in records])
        self.assertEqual([call[0] for call in api.calls[1:]], ["pulls/1"])
        self.assertTrue(all("inspect this PR manually" in item["reason"] for item in result["warnings"]))

    def test_api_failure_is_not_misreported_as_invalid_marker(self):
        with patch.object(FakeGitHub, "pages", return_value=[self.pr()]), \
             patch.object(FakeGitHub, "api", side_effect=backport.BackportError("API unavailable")):
            with self.assertRaisesRegex(backport.BackportError, "API unavailable"):
                backport.status(FakeGitHub())

    def test_valid_open_and_closed_records_retain_distinct_stages(self):
        _, result = self.tracking([self.pr(1, merged=False, state="open"),
                                   self.pr(2, merged=False), self.pr(3)])
        self.assertEqual([row["stage"] for row in result["backports"]],
                         ["backport_open", "closed_without_merge", "merged_to_main_delivery_unverified"])
        self.assertEqual(result["warnings"], [])


if __name__ == "__main__":
    unittest.main()
