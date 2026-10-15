#!/usr/bin/env python3
"""Render the light runner template using private deployment settings."""

import argparse
import json
import os
from pathlib import Path
import re
import tempfile


FIELDS = {
    "RUNNER_NAMESPACE",
    "RUNNER_GITHUB_ORGANIZATION",
    "ARC_CONTROLLER_VERSION",
    "RUNNER_GITHUB_CONFIG_SECRET",
    "RUNNER_GITHUB_CONFIG_URL",
    "RUNNER_LIGHT_ENV_CONFIGMAP",
    "RUNNER_IMAGE",
}
TEMPLATE = Path(__file__).with_name("autoscaling-runner-set-light.yaml.tmpl")
PLACEHOLDER = re.compile(r"\$\{([A-Z_]+)\}")


def render(settings: dict) -> str:
    missing = FIELDS - settings.keys()
    unknown = settings.keys() - FIELDS
    if missing:
        raise ValueError("missing settings: " + ", ".join(sorted(missing)))
    if unknown:
        raise ValueError("unexpected deployment settings")
    for name, value in settings.items():
        if not isinstance(value, str) or not value or not value.isprintable():
            raise ValueError(f"{name} must be a nonempty printable string")
        if "${" in value:
            raise ValueError(f"{name} must not contain unresolved placeholders")
    image = settings["RUNNER_IMAGE"]
    if (
        not re.fullmatch(r"[a-z0-9][a-z0-9._:/-]*@sha256:[a-f0-9]{64}", image)
        or "//" in image
    ):
        raise ValueError("RUNNER_IMAGE must be an immutable digest-pinned image")
    source = TEMPLATE.read_text(encoding="utf-8")
    if set(PLACEHOLDER.findall(source)) != FIELDS:
        raise ValueError("template deployment settings do not match the renderer")
    # JSON string literals are valid YAML scalars and cannot inject YAML fields.
    return PLACEHOLDER.sub(lambda match: json.dumps(settings[match[1]]), source)


def write_private(destination: Path, content: str) -> None:
    destination.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="w", encoding="utf-8", dir=destination.parent, delete=False
        ) as output:
            temporary = Path(output.name)
            output.write(content)
        os.replace(temporary, destination)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--config",
        type=Path,
        help="private JSON settings file; defaults to environment",
    )
    parser.add_argument(
        "--output", type=Path, required=True, help="private rendered manifest file"
    )
    args = parser.parse_args()
    try:
        if args.config:
            settings = json.loads(args.config.read_text(encoding="utf-8"))
            if not isinstance(settings, dict):
                raise ValueError("deployment settings must be a JSON object")
        else:
            settings = {name: os.environ[name] for name in FIELDS if name in os.environ}
        write_private(args.output, render(settings))
    except (OSError, ValueError) as error:
        parser.error(str(error))


if __name__ == "__main__":
    main()
