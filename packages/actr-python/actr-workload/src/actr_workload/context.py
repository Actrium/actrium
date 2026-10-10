from __future__ import annotations

import functools
import importlib
import os
import sys
from collections.abc import Awaitable, Callable
from pathlib import Path
from typing import Any, ClassVar, Generic, Protocol, TypeVar

from .constants import DEFAULT_WORLD_MODULE, ENV_WORLD_MODULE

ResponseT = TypeVar("ResponseT", covariant=True)
ReplyT = TypeVar("ReplyT")

_discovered_world_module: str | None = None


class RpcRequest(Protocol[ResponseT]):
    """Typed request metadata and codec implemented by generated RPC classes."""

    route_key: str
    payload_type: Any

    def encode_request(self) -> bytes: ...

    def decode_response(self, data: bytes) -> ResponseT: ...


def _componentize_py_preinit() -> bool:
    """Detect componentize-py's pre-init sandbox.

    The sandbox environment is closed: it only receives PYTHONHOME,
    PYTHONPATH and PYTHONUNBUFFERED, so ``ENV_WORLD_MODULE`` never reaches
    the guest during the snapshot run. ``PYTHONHOME=/python`` is the
    embedded-stdlib mount componentize-py 0.25 uses and is not set by
    normal interpreters.
    """

    return os.environ.get("PYTHONHOME", "") == "/python"


def _scan_python_path_for_world_package() -> str | None:
    """Find a generated world package on ``sys.path`` by layout.

    componentize-py mounts the runtime world bindings at ``/world``, ahead
    of every caller-supplied path, so the first package shaped like
    ``<name>/imports/host.py`` + ``<name>/exports/`` is the bindings the
    component will run against. Used only inside the pre-init sandbox,
    where the configured module name is otherwise unknowable.
    """

    for entry in sys.path:
        try:
            root = Path(entry) if entry else Path.cwd()
            if not root.is_dir():
                continue
            children = sorted(root.iterdir())
        except OSError:
            continue
        for child in children:
            if not (child / "__init__.py").is_file():
                continue
            if not (child / "imports" / "host.py").is_file():
                continue
            if not (child / "exports").is_dir():
                continue
            return child.name
    return None


def _prime_world_bindings() -> None:
    """Import the world host/types bindings eagerly (issue #638).

    componentize-py bundles a guest by snapshotting the interpreter after
    running the workload module's top level (pre-init). Resolving the host
    bindings lazily at dispatch time left them out of the snapshot, so the
    first ``ctx.call`` / ``ctx.tell`` / ``ctx.log`` trapped with
    ``ModuleNotFoundError``. Importing here runs during pre-init, keeping
    the modules in the snapshot, and records the world module name — the
    only channel that survives into the component, since the sandbox
    environment cannot convey it.

    Failures are tolerated so authoring environments without generated
    bindings (tests, type checks) keep importing this package.
    """

    global _discovered_world_module

    candidates = [os.environ.get(ENV_WORLD_MODULE, DEFAULT_WORLD_MODULE)]
    if _componentize_py_preinit():
        scanned = _scan_python_path_for_world_package()
        if scanned is not None and scanned not in candidates:
            candidates.append(scanned)

    for name in candidates:
        try:
            importlib.import_module(f"{name}.imports.host")
            importlib.import_module(f"{name}.imports.types")
        except ImportError:
            continue
        _discovered_world_module = name
        return


_prime_world_bindings()


class Reply(Generic[ReplyT]):
    """Explicit reply handle for generated EXPLICIT RPC handlers.

    The handle may outlive its handler. If it is never sent, the host expires
    the pending reply at the RFC-0323 in-flight TTL.
    """

    def __init__(
        self,
        ctx: "ActrContext",
        encoder: Callable[[ReplyT], bytes],
    ) -> None:
        self._ctx = ctx
        self._encoder = encoder

    async def send(self, response: ReplyT) -> None:
        """Deliver one encoded response through the current invocation."""
        if not self._ctx.reply_enabled:
            return
        await self._ctx.reply_raw(self._encoder(response))

    async def send_error(self, error: Any) -> None:
        """Deliver one ActrError through the current invocation."""
        if not self._ctx.reply_enabled:
            return
        await self._ctx.reply_error(error)


def _world_module() -> str:
    """Return the componentize-py world module selected for this workload.

    ``DEFAULT_WORLD_MODULE`` is the normal package name.  The environment
    override is used by the build helper when callers choose a custom
    ``world_module``; componentize-py imports the workload in a subprocess so
    this is the only configuration that needs to cross that boundary.  Inside
    a component the name recorded by ``_prime_world_bindings`` during pre-init
    wins, because the sandbox environment does not propagate the override.
    """

    if _discovered_world_module is not None:
        return _discovered_world_module
    return os.environ.get(ENV_WORLD_MODULE, DEFAULT_WORLD_MODULE)


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

    @property
    def reply_enabled(self) -> bool:
        return bool(self._invocation_ctx.reply_enabled)

    async def call(
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
        timeout_ms: int,
        payload_type: str = "rpcReliable",
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

    async def reply_raw(self, payload: bytes) -> None:
        """Deliver a response through the current invocation's reply boundary."""
        host, _ = _bindings()
        await host.reply(self._invocation_ctx.ctx_token, payload)

    async def reply_error(self, error: Any) -> None:
        """Deliver an ActrError through the current invocation's reply boundary."""
        host, _ = _bindings()
        await host.reply_error(self._invocation_ctx.ctx_token, error)

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
        if ENV_WORLD_MODULE in os.environ:
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
        "on_webrtc_state_changed",
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
