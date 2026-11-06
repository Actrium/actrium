#!/usr/bin/env python3
"""Local regression coverage for runner-policy rejection and matrix resolution."""

from pathlib import Path
import tempfile
import unittest

from check_workflow_runners import PolicyError, check_workflows, runner_labels


class RunnerPolicyTests(unittest.TestCase):
    def test_literal_hosted_label(self):
        self.assertEqual(runner_labels({"runs-on": "ubuntu-24.04"}), ["ubuntu-24.04"])

    def test_rejects_self_hosted_custom_retired_and_dynamic_selectors(self):
        for value in [
            "self-hosted",
            "actrium-linux-light",
            "macos-14",
            "${{ inputs.runner }}",
            ["self-hosted", "Linux"],
            {"group": "private-pool", "labels": "ubuntu-24.04"},
        ]:
            with self.subTest(value=value), self.assertRaises(PolicyError):
                runner_labels({"runs-on": value})

    def test_resolves_nested_matrix(self):
        job = {
            "runs-on": "${{ matrix.target.os }}",
            "strategy": {
                "matrix": {
                    "target": [{"os": "ubuntu-24.04"}, {"os": "macos-15"}],
                    "mode": ["debug", "release"],
                }
            },
        }
        self.assertEqual(runner_labels(job), ["ubuntu-24.04"] * 2 + ["macos-15"] * 2)

    def test_resolves_include_only_matrix(self):
        job = {
            "runs-on": "${{ matrix.runner }}",
            "strategy": {
                "matrix": {
                    "include": [
                        {"runner": "ubuntu-24.04"},
                        {"runner": "windows-11-arm"},
                    ],
                }
            },
        }
        self.assertEqual(runner_labels(job), ["ubuntu-24.04", "windows-11-arm"])

    def test_include_cannot_introduce_a_custom_runner(self):
        job = {
            "runs-on": "${{ matrix.os }}",
            "strategy": {
                "matrix": {
                    "os": ["ubuntu-24.04"],
                    "include": [{"os": "custom-pool"}],
                }
            },
        }
        with self.assertRaises(PolicyError):
            runner_labels(job)

    def test_include_overwrites_added_runner_field(self):
        job = {
            "runs-on": "${{ matrix.runner }}",
            "strategy": {
                "matrix": {
                    "target": ["a", "b"],
                    "include": [
                        {"runner": "ubuntu-24.04"},
                        {"target": "b", "runner": "macos-15"},
                    ],
                }
            },
        }
        self.assertEqual(runner_labels(job), ["ubuntu-24.04", "macos-15"])

    def test_excluded_runner_is_rechecked_when_reintroduced(self):
        matrix = {"os": ["ubuntu-24.04", "custom"], "exclude": [{"os": "custom"}]}
        job = {"runs-on": "${{ matrix.os }}", "strategy": {"matrix": matrix}}
        self.assertEqual(runner_labels(job), ["ubuntu-24.04"])
        matrix["include"] = [{"os": "custom"}]
        with self.assertRaises(PolicyError):
            runner_labels(job)

    def test_unresolved_matrices_fail_closed(self):
        for matrix in [
            "${{ fromJSON(needs.detect.outputs.matrix) }}",
            {},
            {"os": []},
            {"include": [{"platform": "linux"}]},
            {"os": "${{ inputs.runners }}"},
        ]:
            with self.subTest(matrix=matrix), self.assertRaises(PolicyError):
                runner_labels(
                    {"runs-on": "${{ matrix.os }}", "strategy": {"matrix": matrix}}
                )

    def test_yaml_files_and_local_reusable_workflows_are_checked(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "caller.yml").write_text(
                "on: push\njobs:\n  call:\n    uses: ./.github/workflows/callee.yaml\n"
            )
            callee = root / "callee.yaml"
            callee.write_text(
                "on:\n  workflow_call:\njobs:\n  build:\n    runs-on: ubuntu-24.04\n"
            )
            self.assertEqual(check_workflows(root), (2, 1))
            callee.write_text(
                "on:\n  workflow_call:\njobs:\n  build:\n    runs-on: self-hosted\n"
            )
            with self.assertRaises(PolicyError):
                check_workflows(root)

    def test_external_or_missing_reusable_workflows_are_rejected(self):
        for reference in [
            "owner/repo/.github/workflows/ci.yml@main",
            "./.github/workflows/missing.yml",
            "./.github/workflows/../outside.yml",
        ]:
            with (
                self.subTest(reference=reference),
                tempfile.TemporaryDirectory() as temp,
            ):
                root = Path(temp)
                (root / "ci.yml").write_text(
                    f"jobs:\n  build:\n    uses: {reference}\n"
                )
                with self.assertRaises(PolicyError):
                    check_workflows(root)

    def test_duplicate_runner_key_is_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "ci.yml").write_text(
                "jobs:\n  build:\n    runs-on: self-hosted\n    runs-on: ubuntu-24.04\n"
            )
            with self.assertRaises(PolicyError):
                check_workflows(root)

    def test_empty_directory_is_rejected(self):
        with tempfile.TemporaryDirectory() as temp, self.assertRaises(PolicyError):
            check_workflows(temp)


if __name__ == "__main__":
    unittest.main()
