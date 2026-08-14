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

    def test_authoring_workload_wraps_invocation_context(self) -> None:
        from actr_workload import ActrContext, Workload

        seen: list[ActrContext] = []

        class UserWorkload(Workload):
            async def on_signaling_connecting(self, ctx) -> None:
                seen.append(ctx)

            async def on_error(self, event, ctx) -> None:
                seen.append(ctx)

        raw = _Invocation()

        async def exercise() -> None:
            workload = UserWorkload()
            await workload.on_signaling_connecting(raw)
            await workload.on_error("event", ctx=raw)

        asyncio.run(exercise())
        self.assertEqual(len(seen), 2)
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
            ("on_credential_renewed", "event, ctx"),
            ("on_credential_expiring", "event, ctx"),
            ("on_mailbox_backpressure", "event, ctx"),
            ("on_data_chunk", "chunk, sender, ctx"),
        )
        methods = [
            "    async def dispatch(self, envelope, ctx):\n"
            "        return b\"\"\n"
        ]
        methods.extend(
            f"    async def {name}(self, {arguments}):\n        return None\n"
            for name, arguments in hooks
        )
        workload_source = (
            "from actr_workload import Workload as WorkloadProtocol\n\n"
            "class Workload(WorkloadProtocol):\n"
            + "\n".join(methods)
            + "\nassert hasattr(Workload, '_async_start_on_signaling_connecting')\n"
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


if __name__ == "__main__":
    unittest.main()
