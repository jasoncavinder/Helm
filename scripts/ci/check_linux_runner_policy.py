#!/usr/bin/env python3
"""Validate hosted Linux baselines and compiled Cargo cache isolation."""

import argparse
from pathlib import Path
import re
import sys

import yaml


BASELINE = "ubuntu-24.04"
CANARY_WORKFLOW = "ubuntu-2604-canary.yml"
LOCK_HASH = "${{ hashFiles('core/rust/Cargo.lock') }}"


class PolicyError(ValueError):
    pass


class UniqueKeyLoader(yaml.SafeLoader):
    """Reject ambiguous duplicate YAML keys rather than validate the last one."""

    def construct_mapping(self, node, deep=False):
        self.flatten_mapping(node)
        result = {}
        for key_node, value_node in node.value:
            key = self.construct_object(key_node, deep=deep)
            if key in result:
                raise PolicyError(f"duplicate YAML key: {key}")
            result[key] = self.construct_object(value_node, deep=deep)
        return result


def runner_labels(job):
    selection = job.get("runs-on")
    if isinstance(selection, str):
        expression = re.fullmatch(r"\$\{\{\s*matrix\.([a-zA-Z_][\w-]*)\s*\}\}", selection)
        if expression:
            axis = expression.group(1)
            matrix = job.get("strategy", {}).get("matrix", {})
            if not isinstance(matrix, dict):
                raise PolicyError("runner matrix must be statically enumerable")
            values = matrix.get(axis, [])
            includes = matrix.get("include", [])
            if not isinstance(values, list) or not isinstance(includes, list):
                raise PolicyError("runner matrix values/include must be lists")
            values = list(values)
            for row in includes:
                if not isinstance(row, dict):
                    raise PolicyError("matrix include rows must be mappings")
                if axis in row:
                    values.append(row[axis])
                elif not values:
                    raise PolicyError("matrix include row has no runner")
            if not values:
                raise PolicyError("runner matrix has no static values")
            return values
        return [selection]
    if isinstance(selection, list) and selection:
        return selection
    raise PolicyError("runs-on must use static labels or a static matrix axis")


def validate_cache(job, labels):
    steps = job.get("steps", [])
    for step in steps:
        action = step.get("uses", "").split("@", 1)[0]
        if action not in {"actions/cache", "actions/cache/restore", "actions/cache/save"}:
            continue
        settings = step.get("with", {})
        paths = settings.get("path", "")
        if not isinstance(paths, str) or "${{" in paths:
            raise PolicyError("cache paths must be static text")
        if not re.search(r"(?:^|/)target(?:/|\s|$)", paths, re.MULTILINE):
            continue
        if set(labels) != {BASELINE}:
            raise PolicyError("compiled Cargo caches need one explicit reviewed baseline")
        toolchains = [
            entry.get("with", {}).get("toolchain")
            for entry in steps
            if entry.get("uses", "").startswith("dtolnay/rust-toolchain@")
        ]
        if len(toolchains) != 1 or not re.fullmatch(r"\d+\.\d+\.\d+", str(toolchains[0])):
            raise PolicyError("compiled Cargo cache needs one exact Rust toolchain")
        prefix = f"cargo-v2-{BASELINE}-${{{{ runner.arch }}}}-rust-{toolchains[0]}-"
        if settings.get("key") != prefix + LOCK_HASH:
            raise PolicyError("Cargo key must bind baseline, architecture, Rust and lockfile")
        restores = settings.get("restore-keys", "")
        if not isinstance(restores, str):
            raise PolicyError("Cargo restore prefixes must be text")
        # Omission is safe (cold cache); no restore may cross OS/arch/toolchain.
        if restores.split() not in ([], prefix.split()):
            raise PolicyError("Cargo restore prefixes must stay within the exact baseline/arch/Rust")


def validate_workflow(name, document):
    if not isinstance(document, dict) or not isinstance(document.get("jobs"), dict):
        raise PolicyError("workflow must contain a jobs mapping")
    for job_id, job in document["jobs"].items():
        if not isinstance(job, dict):
            raise PolicyError(f"{job_id}: job must be a mapping")
        if "uses" in job and "runs-on" not in job:
            continue
        try:
            labels = runner_labels(job)
            for label in labels:
                if not isinstance(label, str) or "${{" in label:
                    raise PolicyError("runner labels must resolve to static strings")
                if label.startswith("ubuntu-"):
                    if label != BASELINE and not (
                        name == CANARY_WORKFLOW and label == "ubuntu-26.04"
                    ):
                        raise PolicyError(f"Linux runner must be {BASELINE}, not {label}")
                elif not re.fullmatch(r"(?:macos|windows)-[\w.-]+", label):
                    raise PolicyError(f"runner requires explicit policy review: {label}")
            validate_cache(job, labels)
        except PolicyError as error:
            raise PolicyError(f"{job_id}: {error}") from error


def check_directory(directory):
    paths = sorted((*directory.glob("*.yml"), *directory.glob("*.yaml")))
    if not paths:
        raise PolicyError("no workflows found")
    for path in paths:
        try:
            document = yaml.load(path.read_text(encoding="utf-8"), Loader=UniqueKeyLoader)
            validate_workflow(path.name, document)
        except (PolicyError, yaml.YAMLError) as error:
            raise PolicyError(f"{path.name}: {error}") from error
    return len(paths)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--workflows-dir", type=Path, default=Path(__file__).resolve().parents[2] / ".github/workflows")
    args = parser.parse_args()
    try:
        count = check_directory(args.workflows_dir)
    except (OSError, PolicyError) as error:
        print(f"Linux runner policy failed: {error}", file=sys.stderr)
        return 1
    print(f"Linux runner and Cargo cache policy passed ({count} workflows).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
