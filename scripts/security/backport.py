#!/usr/bin/env python3
"""Prepare narrowly scoped security backport PRs; never merge or publish them."""

import argparse
from dataclasses import asdict, dataclass
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile


ALLOWED_PATHS = {"core/rust/Cargo.lock", "web/package-lock.json"}
ADVISORY = re.compile(r"(?:RUSTSEC-\d{4}-\d{4}|CVE-\d{4}-\d{4,}|GHSA-[23456789cfghjmpqrvwx]{4}-[23456789cfghjmpqrvwx]{4}-[23456789cfghjmpqrvwx]{4})")
SHA = re.compile(r"[0-9a-f]{40}")
MARKER = "helm-security-backport-v1"


class BackportError(RuntimeError):
    pass


def require(condition, message):
    if not condition:
        raise BackportError(message)


@dataclass(frozen=True)
class Request:
    source_pr: int
    commit: str
    delivery: str
    paths: tuple
    advisories: tuple

    def __post_init__(self):
        require(type(self.source_pr) is int and self.source_pr > 0, "invalid source PR")
        require(bool(SHA.fullmatch(self.commit)), "use a full lowercase 40-character commit SHA")
        require(self.delivery in ("app", "website"), "delivery must be app or website")
        require(bool(self.paths) and len(set(self.paths)) == len(self.paths), "paths must be nonempty and unique")
        require(set(self.paths) <= ALLOWED_PATHS, "only existing Rust/website lockfiles are automated; use a manual backport for other files")
        expected = {"core/rust/Cargo.lock"} if self.delivery == "app" else {"web/package-lock.json"}
        require(set(self.paths) == expected, "keep app and website backports separate")
        require(0 < len(self.advisories) <= 12 and len(set(self.advisories)) == len(self.advisories), "supply 1-12 unique advisory IDs")
        require(all(ADVISORY.fullmatch(value) for value in self.advisories), "invalid advisory ID")

    @property
    def encoded(self):
        return json.dumps(asdict(self), sort_keys=True, separators=(",", ":"))

    @property
    def branch(self):
        digest = hashlib.sha256(self.encoded.encode()).hexdigest()[:16]
        return f"hotfix/security-pr-{self.source_pr}-{digest}"

    @property
    def marker(self):
        return f"<!-- {MARKER} {self.encoded} -->"


def parse_command(body, source_pr):
    parts = body.strip().split()
    require(len(parts) == 5 and parts[0] == "/security-backport", "expected: /security-backport FULL_SHA app|website path advisory[,advisory]")
    return Request(source_pr, parts[1], parts[2], tuple(sorted(parts[3].split(","))), tuple(sorted(parts[4].split(","))))


def run(args, *, data=None, env=None, check=True):
    result = subprocess.run(args, input=data, capture_output=True, env=env, check=False)
    if check and result.returncode:
        # Do not echo arguments/environment: callers may be authenticated.
        raise BackportError(result.stderr.decode(errors="replace")[:2000] or "command failed")
    return result


class GitHub:
    def __init__(self, repo):
        require(bool(re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repo)), "invalid repository")
        self.repo = repo

    def api(self, path, payload=None, method=None):
        args = ["gh", "api", "--hostname", "github.com", f"repos/{self.repo}/{path}"]
        if method:
            args += ["--method", method]
        data = None
        if payload is not None:
            args += ["--input", "-"]
            data = json.dumps(payload).encode()
        output = run(args, data=data).stdout
        return json.loads(output) if output.strip() else None

    def pages(self, path):
        values = []
        separator = "&" if "?" in path else "?"
        for page in range(1, 101):
            batch = self.api(f"{path}{separator}per_page=100&page={page}")
            require(isinstance(batch, list), "expected paginated list")
            values.extend(batch)
            if len(batch) < 100:
                return values
        raise BackportError("pagination bound exceeded; manual review required")


def git(*args, env=None, data=None, check=True):
    return run(["git", "-c", "core.hooksPath=/dev/null", *args], env=env, data=data, check=check)


def validate_source(request, pr, commits, repo):
    require(pr.get("merged") is True and pr.get("state") == "closed", "source PR must already be merged")
    require(pr.get("base", {}).get("ref") == "dev", "source PR must target dev")
    require(pr.get("base", {}).get("repo", {}).get("full_name", "").lower() == repo.lower(), "source repository mismatch")
    require(pr.get("commits", 251) <= 250, "source PR exceeds GitHub's commit-list limit")
    require(request.commit in {item["sha"] for item in commits}, "selected commit does not belong to source PR")
    require(bool(SHA.fullmatch(pr.get("merge_commit_sha") or "")), "source merge SHA missing")


def verify_history(request, pr, dev):
    for sha in (request.commit, pr["merge_commit_sha"]):
        require(git("merge-base", "--is-ancestor", sha, dev, check=False).returncode == 0,
                "source commit/merge is not reachable from current dev; squashed/rebased selections need a manual backport")
    parents = git("rev-list", "--parents", "-n", "1", request.commit).stdout.decode().split()
    require(len(parents) == 2, "select one non-merge commit, not a whole PR merge")
    return parents[1]


def prepare_tree(request, base, parent):
    for revision in (base, parent, request.commit):
        for path in request.paths:
            entry = git("ls-tree", revision, "--", path).stdout.decode().split()
            require(len(entry) == 4 and entry[0] == "100644" and entry[1] == "blob", "selected paths must be existing regular files")
    changed = git("diff", "--name-only", parent, request.commit, "--", *request.paths).stdout.decode().splitlines()
    require(set(changed) == set(request.paths), "every selected path must change in the source commit")
    patch = git("diff", "--binary", "--full-index", "--no-ext-diff", "--no-textconv", parent, request.commit, "--", *request.paths).stdout
    require(0 < len(patch) <= 1024 * 1024, "patch is empty or exceeds 1 MiB")
    # An isolated index applies the patch without checking out or executing source code.
    with tempfile.TemporaryDirectory(prefix="helm-security-index-") as directory:
        env = dict(os.environ, GIT_INDEX_FILE=str(Path(directory) / "index"))
        git("read-tree", base, env=env)
        if git("apply", "--cached", "--check", "-", env=env, data=patch, check=False).returncode:
            if git("apply", "--cached", "--reverse", "--check", "-", env=env, data=patch, check=False).returncode == 0:
                raise BackportError("selected patch is already present on main; do not create a duplicate")
            raise BackportError("patch conflicts with main; manual adaptation and review required")
        git("apply", "--cached", "-", env=env, data=patch)
        tree = git("write-tree", env=env).stdout.decode().strip()
        actual = git("diff", "--name-only", base, tree).stdout.decode().splitlines()
        require(set(actual) == set(request.paths), "candidate changed unexpected paths")
    omitted = sorted(set(git("diff", "--name-only", parent, request.commit).stdout.decode().splitlines()) - set(request.paths))
    return tree, omitted


def pr_body(request, repo, base, omitted):
    delivery = "signed app/CLI patch release and stable metadata verification" if request.delivery == "app" else "verified production website deployment"
    return f"""{request.marker}
## Security Backport

Source: https://github.com/{repo}/pull/{request.source_pr}
Selected commit: `{request.commit}`
Stable base: `{base}`
Advisories: {', '.join(request.advisories)}
Selected files: {', '.join(f'`{path}`' for path in request.paths)}
Omitted source-commit files: {', '.join(f'`{path}`' for path in omitted) or 'none'}

Only the selected patch is applied to `main`; this is not a dev-to-main merge.
Omitted files and stable compatibility require independent review. This request
is maintainer-selected, not an automated claim that the advisories are resolved.

## Verification And Delivery

- The source PR is merged into dev, and both source commit and merge are reachable there.
- The selected patch applies to the recorded stable base with no other file changes.
- Stable-branch CI, security audit and relevant regression/build checks remain required.
- Approve workflows to run if GitHub holds this bot-created PR's checks for approval.
- Human review and protected-branch merge are required; no auto-merge is enabled.
- Delivery remains pending: {delivery} under `docs/operations/RELEASE_FLOW.md`.
- Merging this PR does not update installed Helm copies or prove deployment.

No tag, release, appcast, version bump, signing change, or audit exception is included.
See `docs/operations/SECURITY_BACKPORTS.md` for the delivery evidence checklist.
"""


def existing_pr(github, request):
    prs = github.pages(f"pulls?state=all&base=main&head={github.repo.split('/')[0]}:{request.branch}")
    require(len(prs) <= 1, "multiple matching backport PRs; manual review required")
    if prs:
        pr = prs[0]
        require(request.marker in (pr.get("body") or ""), "existing PR does not match this exact request")
        return pr
    return None


def prepare(github, request, apply=False):
    pr = github.api(f"pulls/{request.source_pr}")
    validate_source(request, pr, github.pages(f"pulls/{request.source_pr}/commits"), github.repo)
    remote = git("remote", "get-url", "origin").stdout.decode().strip().removesuffix(".git").lower()
    require(remote in (f"https://github.com/{github.repo.lower()}", f"git@github.com:{github.repo.lower()}"), "origin repository mismatch")
    git("fetch", "origin", "main", "dev")
    base = git("rev-parse", "origin/main").stdout.decode().strip()
    dev = git("rev-parse", "origin/dev").stdout.decode().strip()
    parent = verify_history(request, pr, dev)
    prior = existing_pr(github, request)
    if prior:
        return {"status": "existing_pr", "url": prior["html_url"], "state": prior["state"], "delivery": "not_verified"}
    tree, omitted = prepare_tree(request, base, parent)
    report = {"status": "dry_run", "branch": request.branch, "base": base, "dev": dev,
              "request": asdict(request), "omitted_paths": omitted, "delivery": "not_verified"}
    if not apply:
        return report
    # Check again before any remote writes. No credential-bearing git remote is needed.
    require(github.api("git/ref/heads/main")["object"]["sha"] == base, "main moved; rerun against its new tip")
    refs = github.api(f"git/matching-refs/heads/{request.branch}")
    require(not refs, "backport branch already exists without a matching PR; inspect it manually, never overwrite")
    entries = []
    for path in request.paths:
        content = git("show", f"{tree}:{path}").stdout.decode("utf-8")
        entries.append({"path": path, "mode": "100644", "type": "blob", "content": content})
    base_tree = github.api(f"git/commits/{base}")["tree"]["sha"]
    remote_tree = github.api("git/trees", {"base_tree": base_tree, "tree": entries})["sha"]
    require(remote_tree == tree, "GitHub candidate tree differs from verified local tree")
    commit = github.api("git/commits", {"message": f"fix(security): backport #{request.source_pr} ({', '.join(request.advisories)})\n\nSource-commit: {request.commit}\n{request.marker}", "tree": remote_tree, "parents": [base]})["sha"]
    github.api("git/refs", {"ref": f"refs/heads/{request.branch}", "sha": commit})
    created = github.api("pulls", {"title": f"fix(security): backport #{request.source_pr} to stable", "head": request.branch,
                                     "base": "main", "body": pr_body(request, github.repo, base, omitted), "draft": False})
    report.update(status="pr_opened", url=created["html_url"], commit=commit)
    return report


def status(github):
    rows = []
    warnings = []
    for pr in github.pages("pulls?state=all&base=main&sort=updated&direction=desc"):
        if not pr.get("head", {}).get("ref", "").startswith("hotfix/security-pr-"):
            continue
        # Branch names and PR bodies are contributor-controlled, including on forks.
        # Generated backports always have both endpoints in this repository.
        if pr.get("base", {}).get("ref") != "main" or any(
            (pr.get(endpoint, {}).get("repo") or {}).get("full_name", "").lower() != github.repo.lower()
            for endpoint in ("head", "base")
        ):
            continue
        body = pr.get("body") or ""
        try:
            match = re.search(r"<!-- " + MARKER + r" (\{[^\n]+\}) -->", body)
            require(match is not None, "backport tracking marker missing")
            values = json.loads(match.group(1))
            request = Request(**values)
            require(pr["head"]["ref"] == request.branch, "backport tracking marker/branch mismatch")
        except (BackportError, ValueError, TypeError, RecursionError):
            # Keep malformed historical records visible without losing valid rows.
            # Do not echo attacker-controlled marker data in the workflow summary.
            warnings.append({"pr": pr["number"], "reason": "Invalid or mismatched tracking marker; inspect this PR manually."})
            continue
        full = github.api(f"pulls/{pr['number']}")
        stage = "merged_to_main_delivery_unverified" if full.get("merged") else ("backport_open" if full["state"] == "open" else "closed_without_merge")
        rows.append({"source_pr": request.source_pr, "backport": full["html_url"], "stage": stage,
                     "delivery": request.delivery, "advisories": request.advisories})
    return {"status": "tracking", "backports": rows, "warnings": warnings,
            "note": "Merge state is not delivery evidence. Record verified release/deployment separately."}


def authorize_event(github, event, actor):
    require(bool(re.fullmatch(r"[A-Za-z0-9_-]+(?:\[bot\])?", actor)), "invalid actor")
    permission = github.api(f"collaborators/{actor}/permission")["permission"]
    require(permission in ("admin", "maintain", "write"), "backport requests require maintainer write access")
    require(event.get("repository", {}).get("full_name", "").lower() == github.repo.lower(), "event repository mismatch")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("prepare", "event", "status"))
    parser.add_argument("--repo", default=os.environ.get("GITHUB_REPOSITORY", "jasoncavinder/Helm"))
    parser.add_argument("--source-pr", type=int, default=os.environ.get("BACKPORT_SOURCE_PR") or None)
    parser.add_argument("--commit", default=os.environ.get("BACKPORT_COMMIT"))
    parser.add_argument("--paths", default=os.environ.get("BACKPORT_PATHS"))
    parser.add_argument("--advisories", default=os.environ.get("BACKPORT_ADVISORIES"))
    parser.add_argument("--delivery", choices=("app", "website"), default=os.environ.get("BACKPORT_DELIVERY"))
    parser.add_argument("--apply", action="store_true")
    args = parser.parse_args()
    github = GitHub(args.repo)
    if args.mode == "status":
        report = status(github)
    else:
        if args.mode == "event":
            event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text())
            authorize_event(github, event, event["sender"]["login"])
            require(event.get("action") == "created" and "pull_request" in event.get("issue", {}), "request must be a new comment on a PR")
            request = parse_command(event["comment"]["body"], event["issue"]["number"])
        else:
            if os.environ.get("GITHUB_ACTIONS") == "true":
                event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text())
                authorize_event(github, event, os.environ["GITHUB_ACTOR"])
            request = Request(args.source_pr, args.commit or "", args.delivery, tuple(sorted((args.paths or "").split(","))), tuple(sorted((args.advisories or "").split(","))))
        report = prepare(github, request, args.apply)
    output = json.dumps(report, indent=2)
    print(output)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as summary:
            summary.write("## Security Backport\n\n```json\n" + output + "\n```\n")


if __name__ == "__main__":
    try:
        main()
    except (BackportError, ValueError, KeyError) as error:
        raise SystemExit(f"security-backport: {error}")
