from __future__ import annotations

import os
import shutil
import subprocess
import sys
from collections.abc import Iterable, Sequence
from pathlib import Path

from .constants import DEFAULT_WORLD, DEFAULT_WORLD_MODULE
from .paths import resolved_wit_path


def _path_arg(path: str | Path) -> str:
    return str(Path(path))


def _componentize_py_executable() -> str:
    sibling = Path(sys.executable).with_name("componentize-py")
    if sibling.exists():
        return str(sibling)
    return shutil.which("componentize-py") or "componentize-py"


def _run_componentize_py(
    command: Sequence[str], *, world_module: str | None = None
) -> subprocess.CompletedProcess[str]:
    env = None
    if world_module is not None:
        env = os.environ.copy()
        env["ACTR_WORKLOAD_WORLD_MODULE"] = world_module
    return subprocess.run(command, check=True, text=True, env=env)


def generate_bindings(
    out_dir: str | Path,
    *,
    wit: str | Path | None = None,
    world: str = DEFAULT_WORLD,
    world_module: str = DEFAULT_WORLD_MODULE,
) -> subprocess.CompletedProcess[str]:
    with resolved_wit_path(wit) as wit_path:
        command = [
            _componentize_py_executable(),
            "-w",
            world,
            "-d",
            _path_arg(wit_path),
            "--world-module",
            world_module,
            "bindings",
            _path_arg(out_dir),
        ]
        return _run_componentize_py(command)


def componentize(
    module_name: str,
    output: str | Path,
    *,
    project_dir: str | Path = ".",
    bindings_dir: str | Path = "bindings",
    wit: str | Path | None = None,
    world: str = DEFAULT_WORLD,
    world_module: str = DEFAULT_WORLD_MODULE,
    python_paths: Iterable[str | Path] = (),
) -> subprocess.CompletedProcess[str]:
    with resolved_wit_path(wit) as wit_path:
        command = [
            _componentize_py_executable(),
            "-w",
            world,
            "-d",
            _path_arg(wit_path),
            "--world-module",
            world_module,
            "componentize",
            module_name,
            "-p",
            _path_arg(project_dir),
            "-p",
            _path_arg(bindings_dir),
        ]
        for python_path in python_paths:
            command.extend(["-p", _path_arg(python_path)])
        command.extend(["-o", _path_arg(output)])
        return _run_componentize_py(command, world_module=world_module)
