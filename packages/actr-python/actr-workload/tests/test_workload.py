from __future__ import annotations

import asyncio
import os
import shutil
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from types import SimpleNamespace
from unittest.mock import patch


PACKAGE_ROOT = Path(__file__).resolve().parents[1]
SOURCE_ROOT = PACKAGE_ROOT / "src"
WIT_PATH = SOURCE_ROOT / "actr_workload" / "wit" / "actr-workload.wit"
if str(SOURCE_ROOT) not in sys.path:
    sys.path.insert(0, str(SOURCE_ROOT))


class _Invocation:
    self_id = "self"
    caller_id = "caller"
    request_id = "request"
    ctx_token = 7
    reply_enabled = True


def _minimal_workload_source() -> str:
    """Workload using only `from actr_workload import Workload`.

    Deliberately free of any static `import actr_workload_bindings...`
    statement: host-import availability inside the component must come from
    the SDK itself (issue #638), not from per-workload workarounds.
    """

    hooks = (
        ("on_start", "ctx"),
        ("on_ready", "ctx"),
        ("on_stop", "ctx"),
        ("on_error", "event, ctx"),
        ("on_signaling_connecting", "ctx"),
        ("on_signaling_connected", "ctx"),
        ("on_signaling_disconnected", "ctx"),
        ("on_websocket_connecting", "event, ctx"),
        ("on_websocket_connected", "event, ctx"),
        ("on_websocket_disconnected", "event, ctx"),
        ("on_webrtc_connecting", "event, ctx"),
        ("on_webrtc_connected", "event, ctx"),
        ("on_webrtc_disconnected", "event, ctx"),
        ("on_webrtc_state_changed", "event, ctx"),
        ("on_credential_renewed", "event, ctx"),
        ("on_credential_expiring", "event, ctx"),
        ("on_mailbox_backpressure", "event, ctx"),
        ("on_data_chunk", "chunk, sender, ctx"),
    )
    methods = ["    async def dispatch(self, envelope, ctx):\n        return b\"\"\n"]
    methods.extend(
        f"    async def {name}(self, {arguments}):\n        return None\n"
        for name, arguments in hooks
    )
    return (
        "from actr_workload import Workload as WorkloadProtocol\n\n"
        "class Workload(WorkloadProtocol):\n" + "\n".join(methods)
    )


def _write_world_package(root: Path, name: str) -> None:
    package = root / name
    (package / "imports").mkdir(parents=True)
    (package / "exports").mkdir()
    (package / "__init__.py").write_text("")
    (package / "exports" / "__init__.py").write_text("class Workload:\n    pass\n")
    (package / "imports" / "__init__.py").write_text("")
    (package / "imports" / "host.py").write_text("VALUE = 'host'\n")
    (package / "imports" / "types.py").write_text("VALUE = 'types'\n")


class WorkloadAuthoringTests(unittest.TestCase):
    def test_call_raw_preserves_explicit_payload_type(self) -> None:
        from actr_workload import ActrContext

        captured: list[tuple[object, ...]] = []

        class RpcSignal:
            pass

        async def call_raw(*args: object) -> bytes:
            captured.append(args)
            return b"response"

        host = SimpleNamespace(call_raw=call_raw)
        types = SimpleNamespace(PayloadType_RpcSignal=RpcSignal)

        async def exercise() -> bytes:
            ctx = ActrContext(_Invocation())
            return await ctx.call_raw(
                "target",
                "echo.EchoService.Echo",
                b"request",
                5_000,
                "rpcSignal",
            )

        with patch("actr_workload.context._bindings", return_value=(host, types)):
            response = asyncio.run(exercise())

        self.assertEqual(response, b"response")
        self.assertEqual(len(captured), 1)
        self.assertIsInstance(captured[0][-1], RpcSignal)

    def test_reply_send_error_forwards_explicit_error(self) -> None:
        from actr_workload import ActrContext, Reply

        captured: list[tuple[object, ...]] = []
        error = {"tag": "internal", "val": "deferred failure"}

        async def reply_error(*args: object) -> None:
            captured.append(args)

        host = SimpleNamespace(reply_error=reply_error)

        async def exercise() -> None:
            ctx = ActrContext(_Invocation())
            reply = Reply(ctx, lambda value: value)
            await reply.send_error(error)

        with patch("actr_workload.context._bindings", return_value=(host, None)):
            asyncio.run(exercise())

        self.assertEqual(captured, [(7, error)])

    def test_authoring_workload_wraps_invocation_context(self) -> None:
        from actr_workload import ActrContext, Workload

        seen: list[ActrContext] = []

        class UserWorkload(Workload):
            async def on_signaling_connecting(self, ctx) -> None:
                seen.append(ctx)

            async def on_error(self, event, ctx) -> None:
                seen.append(ctx)

            async def on_webrtc_state_changed(self, event, ctx) -> None:
                seen.append(ctx)
                await ctx.log("info", "state changed")

        raw = _Invocation()

        async def exercise() -> None:
            workload = UserWorkload()
            await workload.on_signaling_connecting(raw)
            await workload.on_error("event", ctx=raw)
            await workload.on_webrtc_state_changed("state", raw)
            await workload.on_webrtc_state_changed("state", ctx=raw)

        async def log_message(token, level, message):
            self.assertEqual((token, level, message), (7, "info", "state changed"))

        with patch(
            "actr_workload.context._bindings",
            return_value=(SimpleNamespace(log_message=log_message), None),
        ):
            asyncio.run(exercise())
        self.assertEqual(len(seen), 4)
        self.assertTrue(all(isinstance(ctx, ActrContext) for ctx in seen))
        self.assertEqual(seen[0].request_id, "request")

    def test_generated_export_base_is_used_when_available(self) -> None:
        """The authoring import must inherit componentize-py's async shims."""

        script = textwrap.dedent(
            """
            from actr_workload import ActrContext, Workload

            assert hasattr(Workload, "_async_start_on_signaling_connecting")

            class Invocation:
                self_id = "self"
                caller_id = "caller"
                request_id = "request"
                ctx_token = 1

            class UserWorkload(Workload):
                async def on_signaling_connecting(self, ctx):
                    assert isinstance(ctx, ActrContext)

            async def exercise():
                await UserWorkload().on_signaling_connecting(Invocation())

            import asyncio
            asyncio.run(exercise())
            """
        )

        with TemporaryDirectory() as directory:
            root = Path(directory)
            exports = root / "actr_workload_bindings" / "exports"
            exports.mkdir(parents=True)
            (root / "actr_workload_bindings" / "__init__.py").write_text("")
            (exports / "__init__.py").write_text(
                "class Workload:\n"
                "    def _async_start_on_signaling_connecting(self, borrows):\n"
                "        return borrows\n"
            )
            environment = os.environ.copy()
            environment["PYTHONPATH"] = os.pathsep.join(
                (str(root), str(SOURCE_ROOT))
            )
            result = subprocess.run(
                (sys.executable, "-c", script),
                check=False,
                capture_output=True,
                text=True,
                env=environment,
            )

        self.assertEqual(
            result.returncode,
            0,
            msg=f"generated-base subprocess failed:\n{result.stdout}\n{result.stderr}",
        )

    def test_componentize_environment_rejects_missing_runtime_export_base(self) -> None:
        environment = os.environ.copy()
        environment["PYTHONPATH"] = str(SOURCE_ROOT)
        environment["ACTR_WORKLOAD_WORLD_MODULE"] = "missing_runtime_bindings"

        result = subprocess.run(
            (sys.executable, "-c", "import actr_workload"),
            check=False,
            capture_output=True,
            text=True,
            env=environment,
        )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("componentize-py runtime exports are unavailable", result.stderr)

    @unittest.skipUnless(
        shutil.which("componentize-py"),
        "componentize-py is required for the component smoke test",
    )
    def test_componentize_loads_runtime_async_export_base(self) -> None:
        """Componentization must see the real private async export shims."""

        workload_source = _minimal_workload_source() + (
            "\nassert hasattr(Workload, '_async_start_on_signaling_connecting')\n"
        )

        with TemporaryDirectory() as directory:
            root = Path(directory)
            project = root / "project"
            bindings = root / "bindings"
            project.mkdir()
            bindings.mkdir()
            (project / "workload.py").write_text(workload_source)
            output = root / "workload.wasm"

            from actr_workload.build import componentize

            componentize(
                "workload",
                output,
                project_dir=project,
                bindings_dir=bindings,
                wit=WIT_PATH,
                python_paths=(SOURCE_ROOT,),
            )
            self.assertTrue(output.is_file())

    @unittest.skipUnless(
        shutil.which("componentize-py"),
        "componentize-py is required for the component smoke test",
    )
    def test_componentize_bundles_host_imports_without_static_import(self) -> None:
        """Host bindings must land in the component without workload help (#638).

        componentize-py snapshots whatever the workload module's top level
        imports. The SDK eagerly imports `imports.host` / `imports.types`, so
        the snapshot — and therefore the emitted wasm — retains them even
        though this workload never mentions `actr_workload_bindings` itself.
        """

        with TemporaryDirectory() as directory:
            root = Path(directory)
            project = root / "project"
            bindings = root / "bindings"
            project.mkdir()
            bindings.mkdir()
            (project / "workload.py").write_text(_minimal_workload_source())
            output = root / "workload.wasm"

            from actr_workload.build import componentize

            componentize(
                "workload",
                output,
                project_dir=project,
                bindings_dir=bindings,
                wit=WIT_PATH,
                python_paths=(SOURCE_ROOT,),
            )
            self.assertTrue(output.is_file())

            blob = output.read_bytes()
            self.assertIn(
                b"actr_workload_bindings.imports.host",
                blob,
                "host bindings missing from component; eager SDK import regressed",
            )
            self.assertIn(
                b"actr_workload_bindings.imports.types",
                blob,
                "types bindings missing from component; eager SDK import regressed",
            )

    def test_world_bindings_primed_from_env_override(self) -> None:
        """The world-module env override must drive the eager import."""

        script = textwrap.dedent(
            """
            import sys

            import actr_workload.context as context

            assert context._world_module() == "custom_world"
            assert "custom_world.imports.host" in sys.modules
            assert "custom_world.imports.types" in sys.modules
            """
        )

        with TemporaryDirectory() as directory:
            root = Path(directory)
            _write_world_package(root, "custom_world")
            environment = os.environ.copy()
            environment["PYTHONPATH"] = os.pathsep.join(
                (str(root), str(SOURCE_ROOT))
            )
            environment["ACTR_WORKLOAD_WORLD_MODULE"] = "custom_world"
            result = subprocess.run(
                (sys.executable, "-c", script),
                check=False,
                capture_output=True,
                text=True,
                env=environment,
            )

        self.assertEqual(
            result.returncode,
            0,
            msg=f"env-override subprocess failed:\n{result.stdout}\n{result.stderr}",
        )

    def test_world_package_discovered_by_layout_during_preinit(self) -> None:
        """Pre-init cannot see the env override, so the package is found on sys.path."""

        from actr_workload import context

        with TemporaryDirectory() as directory:
            root = Path(directory)
            _write_world_package(root, "probe_world")
            sys.path.insert(0, str(root))
            previous = context._discovered_world_module
            context._discovered_world_module = None
            try:
                with patch.dict(os.environ, {"PYTHONHOME": "/python"}):
                    context._prime_world_bindings()
                self.assertEqual(context._discovered_world_module, "probe_world")
                self.assertEqual(context._world_module(), "probe_world")
            finally:
                context._discovered_world_module = previous
                sys.path.remove(str(root))
                for name in list(sys.modules):
                    if name == "probe_world" or name.startswith("probe_world."):
                        del sys.modules[name]

    def test_import_tolerates_missing_world_bindings(self) -> None:
        """Authoring environments without generated bindings still import the SDK."""

        environment = os.environ.copy()
        environment["PYTHONPATH"] = str(SOURCE_ROOT)
        environment.pop("ACTR_WORKLOAD_WORLD_MODULE", None)

        result = subprocess.run(
            (
                sys.executable,
                "-c",
                "import actr_workload; "
                "from actr_workload import context; "
                "assert context._discovered_world_module is None; "
                "assert context._world_module() == 'actr_workload_bindings'",
            ),
            check=False,
            capture_output=True,
            text=True,
            env=environment,
        )

        self.assertEqual(
            result.returncode,
            0,
            msg=f"missing-bindings subprocess failed:\n{result.stdout}\n{result.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
