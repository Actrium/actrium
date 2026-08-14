from __future__ import annotations

import functools
import importlib
import os
from collections.abc import Awaitable, Callable
from typing import Any, ClassVar

from .constants import DEFAULT_RPC_TIMEOUT_MS, DEFAULT_WORLD_MODULE
from .protocol import ResponseT, RpcRequest


def _world_module() -> str:
    """Return the componentize-py world module selected for this workload.

    ``DEFAULT_WORLD_MODULE`` is the normal package name.  The environment
    override is used by the build helper when callers choose a custom
    ``world_module``; componentize-py imports the workload in a subprocess so
    this is the only configuration that needs to cross that boundary.
    """

    return os.environ.get("ACTR_WORKLOAD_WORLD_MODULE", DEFAULT_WORLD_MODULE)


def _bindings() -> tuple[Any, Any]:
    world_module = _world_module()
    host = importlib.import_module(f"{world_module}.imports.host")
    types = importlib.import_module(f"{world_module}.imports.types")
    return host, types


class ActrContext:
    """Minimal per-invocation adapter used by generated dispatchers."""

    def __init__(self, invocation_ctx: Any) -> None:
        self._invocation_ctx = invocation_ctx

    @property
    def self_id(self) -> Any:
        return self._invocation_ctx.self_id

    @property
    def caller_id(self) -> Any:
        return self._invocation_ctx.caller_id

    @property
    def request_id(self) -> str:
        return self._invocation_ctx.request_id

    async def call(
        self,
        target: Any,
        request: RpcRequest[ResponseT],
    ) -> ResponseT:
        """Typed RPC with default timeout, delegating to `call_with_timeout`."""
        return await self.call_with_timeout(target, request, DEFAULT_RPC_TIMEOUT_MS)

    async def call_with_timeout(
        self,
        target: Any,
        request: RpcRequest[ResponseT],
        timeout_ms: int,
    ) -> ResponseT:
        _validate_timeout(timeout_ms)
        host, types = _bindings()
        payload = await host.call(
            self._invocation_ctx.ctx_token,
            types.Dest_Peer(target),
            request.route_key,
            request.encode_request(),
            timeout_ms,
            _payload_type_to_wit(request.payload_type, types),
        )
        return request.decode_response(bytes(payload))

    async def call_raw(
        self,
        target: Any,
        route_key: str,
        payload: bytes,
    ) -> bytes:
        """Raw RPC on the reliable lane with default timeout."""
        return await self.call_raw_with_timeout(
            target, route_key, payload, DEFAULT_RPC_TIMEOUT_MS
        )

    async def call_raw_with_timeout(
        self,
        target: Any,
        route_key: str,
        payload: bytes,
        timeout_ms: int,
    ) -> bytes:
        return await self.call_raw_with_payload_type(
            target, route_key, payload, timeout_ms, "rpcReliable"
        )

    async def call_raw_with_payload_type(
        self,
        target: Any,
        route_key: str,
        payload: bytes,
        timeout_ms: int,
        payload_type: str,
    ) -> bytes:
        _validate_timeout(timeout_ms)
        host, types = _bindings()
        return bytes(
            await host.call_raw(
                self._invocation_ctx.ctx_token,
                target,
                route_key,
                payload,
                timeout_ms,
                _payload_type_to_wit(payload_type, types),
            )
        )

    async def discover(self, target_type: Any) -> Any:
        host, _ = _bindings()
        return await host.discover(self._invocation_ctx.ctx_token, target_type)

    async def resolve_dependency(self, alias: str) -> Any:
        host, _ = _bindings()
        return await host.resolve_dependency(self._invocation_ctx.ctx_token, alias)

    async def tell(self, target: Any, message: RpcRequest[Any]) -> None:
        host, types = _bindings()
        await host.tell(
            self._invocation_ctx.ctx_token,
            types.Dest_Peer(target),
            message.route_key,
            message.encode_request(),
            _payload_type_to_wit(message.payload_type, types),
        )

    async def log(self, level: str, message: str) -> None:
        host, _ = _bindings()
        await host.log_message(self._invocation_ctx.ctx_token, level, message)


def _validate_timeout(timeout_ms: int) -> None:
    if (
        isinstance(timeout_ms, bool)
        or not isinstance(timeout_ms, int)
        or timeout_ms <= 0
    ):
        raise ValueError(f"timeout_ms must be a positive integer, got {timeout_ms!r}")


def _payload_type_to_wit(payload_type: Any, types: Any) -> Any:
    """Convert generated dispatch metadata to a componentize-py WIT variant."""
    if not isinstance(payload_type, str):
        return payload_type

    variants = {
        "rpcReliable": "RpcReliable",
        "rpcSignal": "RpcSignal",
        "streamReliable": "StreamReliable",
        "streamLatencyFirst": "StreamLatencyFirst",
        "mediaRtp": "MediaRtp",
    }
    variant = variants.get(payload_type)
    if variant is None:
        raise ValueError(f"unknown payload type: {payload_type}")

    factory = getattr(types, f"PayloadType_{variant}", None)
    if factory is not None:
        return factory()

    payload_type_class = getattr(types, "PayloadType", None)
    if payload_type_class is not None:
        snake_variant = _camel_to_snake(variant)
        for name in (variant, snake_variant, snake_variant.upper()):
            value = getattr(payload_type_class, name, None)
            if value is not None:
                return value() if isinstance(value, type) else value

    raise RuntimeError(f"generated bindings do not expose payload type {payload_type}")


def _camel_to_snake(value: str) -> str:
    return "".join(
        f"_{char.lower()}" if char.isupper() and index else char.lower()
        for index, char in enumerate(value)
    )


def _bind_context(
    method: Callable[..., Awaitable[Any]],
) -> Callable[..., Awaitable[Any]]:
    @functools.wraps(method)
    async def wrapped(self: Any, *args: Any, **kwargs: Any) -> Any:
        if "ctx" in kwargs:
            if not isinstance(kwargs["ctx"], ActrContext):
                kwargs["ctx"] = ActrContext(kwargs["ctx"])
        elif args:
            context = args[-1]
            if not isinstance(context, ActrContext):
                args = (*args[:-1], ActrContext(context))
        return await method(self, *args, **kwargs)

    return wrapped


def _generated_workload_base() -> type[Any]:
    """Load componentize-py's runtime export base when it is available.

    ``componentize-py bindings`` writes typing stubs and the componentizer
    generates a second, runtime-connected copy in a temporary mount.  Only
    the latter contains the private ``_async_start_*`` methods used by the
    async component ABI.  Importing the base lazily keeps the authoring
    package usable before bindings have been generated, while componentized
    workloads still inherit the generated shims.
    """

    try:
        exports = importlib.import_module(f"{_world_module()}.exports")
        base = getattr(exports, "Workload")
    except (ImportError, AttributeError) as error:
        if "ACTR_WORKLOAD_WORLD_MODULE" in os.environ:
            raise RuntimeError(
                "componentize-py runtime exports are unavailable; regenerate bindings "
                f"for world module `{_world_module()}`"
            ) from error
        return object

    if not isinstance(base, type):
        return object
    return base


class Workload(_generated_workload_base()):
    """Base class converting raw WIT invocation contexts to ActrContext."""

    _CONTEXT_METHODS: ClassVar[set[str]] = {
        "dispatch",
        "on_start",
        "on_ready",
        "on_stop",
        "on_error",
        "on_signaling_connecting",
        "on_signaling_connected",
        "on_signaling_disconnected",
        "on_websocket_connecting",
        "on_websocket_connected",
        "on_websocket_disconnected",
        "on_webrtc_connecting",
        "on_webrtc_connected",
        "on_webrtc_disconnected",
        "on_credential_renewed",
        "on_credential_expiring",
        "on_mailbox_backpressure",
        "on_data_chunk",
    }

    def __init_subclass__(cls, **kwargs: Any) -> None:
        super().__init_subclass__(**kwargs)
        for name in cls._CONTEXT_METHODS:
            method = cls.__dict__.get(name)
            if method is not None:
                setattr(cls, name, _bind_context(method))
