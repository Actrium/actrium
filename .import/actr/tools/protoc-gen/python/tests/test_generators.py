import unittest

from framework_codegen_python.__main__ import normalize_proto_path
from framework_codegen_python.generators import (
    generate_metadata,
    generate_unified_dispatch,
    validate_spec_hash,
)


class GeneratorTests(unittest.TestCase):
    def test_normalize_proto_path(self) -> None:
        self.assertEqual(normalize_proto_path(r".\remote\ask"), "remote/ask.proto")
        self.assertEqual(normalize_proto_path("./remote/ask.proto"), "remote/ask.proto")

    def test_validate_spec_hash_round_trip(self) -> None:
        import hashlib
        import json

        spec = {
            "schema_version": "v1",
            "routes": [
                {
                    "route_key": "echo.EchoService.Echo",
                    "proto_file": "local/echo.proto",
                    "proto_service": "echo.EchoService",
                    "method": "Echo",
                    "input_type": "echo.EchoRequest",
                    "output_type": "echo.EchoResponse",
                    "payload_type": "rpcReliable",
                    "reply_mode": "return",
                }
            ],
        }
        canonical = json.dumps(spec, sort_keys=True, separators=(",", ":"))
        expected = hashlib.sha256(canonical.encode("utf-8")).hexdigest()
        # Should not raise.
        validate_spec_hash({**spec, "spec_hash": expected}, expected)

    def test_validate_spec_hash_accepts_v1_without_reply_mode(self) -> None:
        import hashlib
        import json

        spec = {
            "schema_version": "v1",
            "routes": [
                {
                    "route_key": "EchoService.Echo",
                    "proto_file": "echo.proto",
                    "proto_service": "EchoService",
                    "method": "Echo",
                    "input_type": "EchoRequest",
                    "output_type": "EchoResponse",
                    "payload_type": "rpcReliable",
                }
            ],
        }
        expected = hashlib.sha256(
            json.dumps(spec, sort_keys=True, separators=(",", ":")).encode("utf-8")
        ).hexdigest()
        legacy_spec = {**spec, "spec_hash": expected}

        validate_spec_hash(legacy_spec, expected)

        self.assertEqual(legacy_spec["routes"][0]["reply_mode"], "return")

    def test_validate_spec_hash_mismatch(self) -> None:
        with self.assertRaises(ValueError):
            validate_spec_hash({"schema_version": "v1"}, "deadbeef")

    def test_validate_spec_rejects_duplicate_routes(self) -> None:
        route = {
            "route_key": "echo.EchoService.Echo",
            "proto_file": "echo.proto",
            "proto_service": "echo.EchoService",
            "method": "Echo",
            "input_type": "echo.EchoRequest",
            "output_type": "echo.EchoResponse",
            "payload_type": "rpcReliable",
            "reply_mode": "return",
        }
        with self.assertRaisesRegex(ValueError, "duplicate route_key"):
            validate_spec_hash(
                {"schema_version": "v1", "routes": [route, route]}, "deadbeef"
            )

    def test_generate_unified_dispatch_local_only(self) -> None:
        route = {
            "route_key": "echo.EchoService.Echo",
            "proto_file": "local/echo.proto",
            "proto_service": "echo.EchoService",
            "method": "Echo",
            "input_type": "echo.EchoRequest",
            "output_type": "echo.EchoResponse",
            "payload_type": "rpcReliable",
            "reply_mode": "return",
        }
        type_to_owner = {
            "echo.EchoRequest": ("echo", "local/echo.proto", ("EchoRequest",)),
            "echo.EchoResponse": ("echo", "local/echo.proto", ("EchoResponse",)),
        }
        content = generate_unified_dispatch([route], [], type_to_owner)

        # ActrDispatchHandler Protocol with async method.
        self.assertIn("class ActrDispatchHandler(Protocol):", content)
        self.assertIn("async def echo_service_echo(", content)
        self.assertIn("if TYPE_CHECKING:", content)
        self.assertIn("    from actr_workload import ActrContext", content)
        self.assertNotIn("from actr_workload import Reply", content)

        # UnknownRoute exception.
        self.assertIn("class UnknownRoute(Exception):", content)

        # actr_dispatch function — local route, no call_raw_with_timeout.
        self.assertIn("async def actr_dispatch(", content)
        self.assertIn('if route_key == "echo.EchoService.Echo":', content)
        self.assertIn("resp = await handler.echo_service_echo(req, ctx)", content)
        self.assertIn("response = resp.SerializeToString()", content)
        self.assertIn("if ctx.reply_enabled:", content)
        self.assertIn("await ctx.reply_raw(response)", content)
        self.assertIn('return b""', content)
        self.assertIn("raise UnknownRoute(route_key)", content)

        # Must NOT contain forbidden symbols.
        self.assertNotIn("30000", content)
        self.assertNotIn("DEFAULT_RPC_TIMEOUT_MS", content)
        self.assertNotIn("call_raw_with_timeout", content)
        self.assertNotIn("call_with_timeout", content)

    def test_generate_unified_dispatch_remote_route(self) -> None:
        local_route = {
            "route_key": "echo.EchoService.Echo",
            "proto_file": "local/echo.proto",
            "proto_service": "echo.EchoService",
            "method": "Echo",
            "input_type": "echo.EchoRequest",
            "output_type": "echo.EchoResponse",
            "payload_type": "rpcReliable",
            "reply_mode": "return",
        }
        remote_route = {
            "route_key": "ask.AskService.Prompt",
            "proto_file": "remote/ask/ask.proto",
            "proto_service": "ask.AskService",
            "method": "Prompt",
            "input_type": "ask.PromptRequest",
            "output_type": "ask.PromptResponse",
            "payload_type": "rpcReliable",
            "reply_mode": "return",
            "dependency_alias": "ask-service",
        }
        type_to_owner = {
            "echo.EchoRequest": ("echo", "local/echo.proto", ("EchoRequest",)),
            "echo.EchoResponse": ("echo", "local/echo.proto", ("EchoResponse",)),
            "ask.PromptRequest": ("ask", "remote/ask/ask.proto", ("PromptRequest",)),
            "ask.PromptResponse": ("ask", "remote/ask/ask.proto", ("PromptResponse",)),
        }
        content = generate_unified_dispatch(
            [local_route], [remote_route], type_to_owner
        )

        # Remote route: resolve_dependency, discover, raw forwarding.
        self.assertIn('if route_key == "ask.AskService.Prompt":', content)
        self.assertIn(
            'actr_type = await ctx.resolve_dependency("ask-service")', content
        )
        self.assertIn("target = await ctx.discover(actr_type)", content)
        self.assertIn(
            'response = await ctx.call_raw(target, route_key, payload, timeout_ms, "rpcReliable")',
            content,
        )
        self.assertIn(
            "if ctx.reply_enabled:\n"
            "            await ctx.reply_raw(response)\n"
            "            return b\"\"\n"
            "        return response",
            content,
        )

    def test_remote_dispatch_preserves_rpc_signal_payload_type(self) -> None:
        remote_route = {
            "route_key": "ask.AskService.Prompt",
            "proto_file": "remote/ask/ask.proto",
            "proto_service": "ask.AskService",
            "method": "Prompt",
            "input_type": "ask.PromptRequest",
            "output_type": "ask.PromptResponse",
            "payload_type": "rpcSignal",
            "reply_mode": "return",
            "dependency_alias": "ask-service",
        }

        content = generate_unified_dispatch([], [remote_route], {})

        self.assertIn(
            'ctx.call_raw(target, route_key, payload, timeout_ms, "rpcSignal")',
            content,
        )

    def test_generate_unified_dispatch_remote_only_uses_marker_handler(self) -> None:
        remote_route = {
            "route_key": "ask.AskService.Prompt",
            "proto_file": "remote/ask/ask.proto",
            "proto_service": "ask.AskService",
            "method": "Prompt",
            "input_type": "ask.PromptRequest",
            "output_type": "ask.PromptResponse",
            "payload_type": "rpcReliable",
            "reply_mode": "return",
            "dependency_alias": "ask-service",
        }
        type_to_owner = {
            "ask.PromptRequest": ("ask", "remote/ask/ask.proto", ("PromptRequest",)),
            "ask.PromptResponse": ("ask", "remote/ask/ask.proto", ("PromptResponse",)),
        }
        content = generate_unified_dispatch([], [remote_route], type_to_owner)

        # Remote-only dispatch keeps the unified five-argument signature.
        self.assertIn("class ActrDispatchHandler(Protocol):", content)
        self.assertIn("handler: ActrDispatchHandler", content)
        self.assertIn("async def actr_dispatch(", content)
        self.assertIn("ctx: ActrContext", content)
        self.assertIn("route_key: str", content)
        self.assertIn("payload: bytes", content)
        self.assertIn("timeout_ms: int", content)

        self.assertNotIn("remoteTargets", content)
        self.assertNotIn("ActrType(", content)

        metadata = __import__("json").loads(
            generate_metadata(
                {"schema_version": "v1", "spec_hash": "abc", "routes": [remote_route]},
                ["actr_dispatch.py"],
                [],
                [remote_route],
                type_to_owner,
            )
        )
        self.assertEqual(metadata["language"], "python")
        self.assertEqual(metadata["generated_remote_routes"], ["ask.AskService.Prompt"])
        self.assertIn("actr-gen-meta.json", metadata["generated_files"])
        self.assertEqual(
            metadata["remote_services"][0]["dependency_alias"], "ask-service"
        )
        self.assertNotIn("actr_type", metadata["remote_services"][0])

    def test_generate_unified_dispatch_zero_routes(self) -> None:
        content = generate_unified_dispatch([], [], {})

        # Marker handler, no routes — just UnknownRoute.
        self.assertIn("class ActrDispatchHandler(Protocol):", content)
        self.assertIn("class UnknownRoute(Exception):", content)
        self.assertIn("async def actr_dispatch(", content)
        self.assertIn("raise UnknownRoute(route_key)", content)

    def test_generate_unified_dispatch_explicit_route_uses_reply_handle(self) -> None:
        route = {
            "route_key": "echo.EchoService.Echo",
            "proto_file": "local/echo.proto",
            "proto_service": "echo.EchoService",
            "method": "Echo",
            "input_type": "echo.EchoRequest",
            "output_type": "echo.EchoResponse",
            "payload_type": "rpcReliable",
            "reply_mode": "explicit",
        }
        type_to_owner = {
            "echo.EchoRequest": ("echo", "local/echo.proto", ("EchoRequest",)),
            "echo.EchoResponse": ("echo", "local/echo.proto", ("EchoResponse",)),
        }
        content = generate_unified_dispatch([route], [], type_to_owner)

        self.assertIn(
            "reply: Reply[local_echo_pb2.EchoResponse]", content
        )
        self.assertIn("from actr_workload import Reply", content)
        self.assertIn("reply = Reply(ctx,", content)
        self.assertIn("await handler.echo_service_echo(req, ctx, reply)", content)
        self.assertIn('return b""', content)


if __name__ == "__main__":
    unittest.main()
