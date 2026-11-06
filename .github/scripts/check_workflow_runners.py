#!/usr/bin/env python3
"""Require statically verifiable GitHub-hosted runners in every workflow.

Install runner-policy-requirements.txt before running this checker locally.
Update the explicit label allowlist when adopting another supported image:
https://docs.github.com/en/actions/reference/runners/github-hosted-runners
"""

import argparse
from copy import deepcopy
from itertools import product
from pathlib import Path
import re

import yaml


HOSTED_LABELS = frozenset(
    {
        "ubuntu-latest",
        "ubuntu-24.04",
        "ubuntu-24.04-arm",
        "macos-latest",
        "macos-15",
        "macos-15-intel",
        "windows-latest",
        "windows-11-arm",
    }
)
MATRIX_SELECTOR = re.compile(r"\$\{\{\s*matrix\.([\w.]+)\s*\}\}")


class PolicyError(ValueError):
    """A workflow cannot be proven to use only approved hosted runners."""


class WorkflowLoader(yaml.BaseLoader):
    """Read scalars as strings, including the YAML 1.2 `on` key."""


def unique_mapping(loader, node):
    result = {}
    for key_node, value_node in node.value:
        key = loader.construct_object(key_node)
        if key in result:
            raise PolicyError(f"duplicate YAML key {key!r} at {key_node.start_mark}")
        result[key] = loader.construct_object(value_node)
    return result


WorkflowLoader.add_constructor(
    yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, unique_mapping
)


def matrix_rows(matrix):
    if not isinstance(matrix, dict) or not matrix:
        raise PolicyError("runner expressions require a nonempty static matrix")
    axes = {k: v for k, v in matrix.items() if k not in {"include", "exclude"}}
    if any(not isinstance(values, list) or not values for values in axes.values()):
        raise PolicyError("matrix axes must be nonempty static lists")
    includes = matrix.get("include", [])
    excludes = matrix.get("exclude", [])
    if any(
        not isinstance(items, list) or any(not isinstance(v, dict) for v in items)
        for items in [includes, excludes]
    ):
        raise PolicyError("matrix include/exclude must be lists of mappings")
    originals = (
        [dict(zip(axes, values)) for values in product(*axes.values())] if axes else []
    )
    originals = [
        row
        for row in originals
        if not any(
            all(row.get(k) == v for k, v in exclusion.items()) for exclusion in excludes
        )
    ]
    rows = deepcopy(originals)
    added = []
    for inclusion in includes:
        matched = False
        for original, row in zip(originals, rows):
            if all(k not in original or original[k] == v for k, v in inclusion.items()):
                row.update(inclusion)
                matched = True
        if not matched:
            added.append(inclusion)
    if not rows and not added:
        raise PolicyError("runner matrix has no runnable combinations")
    return rows + added


def runner_labels(job):
    selector = job.get("runs-on")
    if not isinstance(selector, str):
        raise PolicyError(
            "runs-on must be a hosted label or a static matrix expression; groups and label arrays are forbidden"
        )
    match = MATRIX_SELECTOR.fullmatch(selector)
    if match is None:
        labels = [selector]
    else:
        labels = []
        strategy = job.get("strategy", {})
        if not isinstance(strategy, dict):
            raise PolicyError("runner expressions require a static strategy")
        for row in matrix_rows(strategy.get("matrix")):
            value = row
            for key in match[1].split("."):
                if not isinstance(value, dict) or key not in value:
                    raise PolicyError(
                        f"matrix combination is missing runner field {match[1]}"
                    )
                value = value[key]
            labels.append(value)
    for label in labels:
        if not isinstance(label, str) or label not in HOSTED_LABELS:
            raise PolicyError(f"unapproved or retired runner label: {label!r}")
    return labels


def check_workflows(directory):
    directory = Path(directory).resolve()
    paths = sorted([*directory.glob("*.yml"), *directory.glob("*.yaml")])
    if not paths:
        raise PolicyError(f"no workflow files found in {directory}")
    documents = {}
    for path in paths:
        try:
            documents[path] = yaml.load(path.read_text(), Loader=WorkflowLoader)
        except (yaml.YAMLError, PolicyError) as error:
            raise PolicyError(f"{path.name}: {error}") from error
    jobs_count = 0
    for path, document in documents.items():
        jobs = document.get("jobs") if isinstance(document, dict) else None
        if not isinstance(jobs, dict) or not jobs:
            raise PolicyError(f"{path.name}: expected a nonempty jobs mapping")
        for name, job in jobs.items():
            try:
                if not isinstance(job, dict):
                    raise PolicyError("job must be a mapping")
                if "uses" not in job:
                    runner_labels(job)
                    jobs_count += 1
                    continue
                if "runs-on" in job:
                    raise PolicyError(
                        "a reusable workflow call cannot also set runs-on"
                    )
                reference = job["uses"]
                if not isinstance(reference, str) or not reference.startswith(
                    "./.github/workflows/"
                ):
                    raise PolicyError("only local reusable workflows can be verified")
                target = (
                    directory / reference.removeprefix("./.github/workflows/")
                ).resolve()
                if target not in documents:
                    raise PolicyError(
                        f"missing or nonlocal reusable workflow: {reference}"
                    )
                callee = documents[target]
                events = callee.get("on", {}) if isinstance(callee, dict) else {}
                if events != "workflow_call" and (
                    not isinstance(events, (dict, list))
                    or "workflow_call" not in events
                ):
                    raise PolicyError(
                        f"reusable workflow lacks workflow_call: {reference}"
                    )
            except PolicyError as error:
                raise PolicyError(f"{path.name}/{name}: {error}") from error
    return len(paths), jobs_count


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--workflows",
        type=Path,
        default=Path(__file__).resolve().parents[1] / "workflows",
    )
    args = parser.parse_args()
    try:
        workflows, jobs = check_workflows(args.workflows)
    except PolicyError as error:
        parser.exit(1, f"runner-policy: {error}\n")
    print(
        f"runner-policy: {workflows} workflows, {jobs} runner jobs use approved GitHub-hosted labels"
    )


if __name__ == "__main__":
    main()
