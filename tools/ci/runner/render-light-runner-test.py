#!/usr/bin/env python3
"""Validate private settings handling and atomic runner manifest output."""

import importlib.util
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("render-light-runner.py")
SPEC = importlib.util.spec_from_file_location("render_light_runner", SCRIPT)
RENDERER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RENDERER)


def settings() -> dict:
    return {
        "RUNNER_NAMESPACE": "runner-system",
        "RUNNER_GITHUB_ORGANIZATION": "example",
        "ARC_CONTROLLER_VERSION": "1.2.3",
        "RUNNER_GITHUB_CONFIG_SECRET": "github-app",
        "RUNNER_GITHUB_CONFIG_URL": "https://github.com/example",
        "RUNNER_LIGHT_ENV_CONFIGMAP": "light-proxy",
        "RUNNER_IMAGE": "registry.example.invalid/runner@sha256:" + "a" * 64,
    }


class RenderLightRunnerTests(unittest.TestCase):
    def test_settings_are_required_and_closed(self) -> None:
        for name in RENDERER.FIELDS:
            values = settings()
            del values[name]
            with self.subTest(missing=name), self.assertRaisesRegex(ValueError, name):
                RENDERER.render(values)
        values = settings()
        values["UNEXPECTED_CREDENTIAL"] = "not-for-rendering"
        with self.assertRaisesRegex(ValueError, "unexpected"):
            RENDERER.render(values)

    def test_image_requires_immutable_reference(self) -> None:
        for value in ["runner:latest", "runner@sha256:short", "${UNRESOLVED}"]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                RENDERER.render({**settings(), "RUNNER_IMAGE": value})

    def test_values_cannot_inject_yaml_fields(self) -> None:
        values = settings()
        values["RUNNER_NAMESPACE"] = 'quoted" # text'
        result = RENDERER.render(values)
        self.assertIn("  namespace: " + json.dumps(values["RUNNER_NAMESPACE"]), result)
        self.assertNotIn("${", result)
        values["RUNNER_NAMESPACE"] = "namespace\nextra: field"
        with self.assertRaisesRegex(ValueError, "RUNNER_NAMESPACE"):
            RENDERER.render(values)

    def test_file_output_is_private_and_not_logged(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            config = Path(directory) / "settings.json"
            output = Path(directory) / "manifest.yaml"
            config.write_text(json.dumps(settings()))
            output.write_text("previous content")
            output.chmod(0o644)
            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--config",
                    str(config),
                    "--output",
                    str(output),
                ],
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout, "")
            self.assertEqual(stat.S_IMODE(output.stat().st_mode), 0o600)
            self.assertEqual(output.read_text(), RENDERER.render(settings()))

    def test_invalid_input_preserves_existing_output(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "manifest.yaml"
            output.write_text("keep existing deployment")
            environment = {
                key: value
                for key, value in os.environ.items()
                if key not in RENDERER.FIELDS
            }
            result = subprocess.run(
                [sys.executable, str(SCRIPT), "--output", str(output)],
                env=environment,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(output.read_text(), "keep existing deployment")
            self.assertEqual(result.stdout, "")


if __name__ == "__main__":
    unittest.main()
