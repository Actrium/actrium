"""
Code generation functions for the unified-dispatch Python protoc plugin.

All dispatch glue (per-RPC classes, per-service handlers, the unified
``actr_dispatch`` function, and ``actr-gen-meta.json``) is driven solely by
the DispatchSpec JSON written by the CLI.  The plugin never reads the manifest
lock, never classifies files by path, and never embeds ActrType strings.
"""

from __future__ import annotations

import hashlib
import json
from importlib.metadata import PackageNotFoundError, version

# ---------------------------------------------------------------------------
# DispatchSpec hash validation
# ---------------------------------------------------------------------------


def validate_spec_hash(spec: dict, expected_hash: str) -> None:
    """Recompute the JCS+SHA-256 hash of *spec* (minus ``spec_hash``).

    Mirrors the canonicalisation in
    ``cli/src/commands/codegen/dispatch_spec.rs``: sorted-keys
    ``json.dumps`` with compact separators, then SHA-256 hex digest.
    """
    if not isinstance(spec, dict):
        raise ValueError("DispatchSpec must be a JSON object")
    allowed_spec_fields = {"schema_version", "routes", "spec_hash"}
    unknown_spec_fields = set(spec) - allowed_spec_fields
    if unknown_spec_fields:
        raise ValueError(
            f"DispatchSpec contains unknown field(s): {sorted(unknown_spec_fields)}"
        )
    if spec.get("schema_version") != "v1":
        raise ValueError(
            f"Unexpected DispatchSpec schema_version: {spec.get('schema_version')!r}"
        )
    routes = spec.get("routes")
    if not isinstance(routes, list):
        raise ValueError("DispatchSpec routes must be a list")
    route_keys: set[str] = set()
    service_aliases: dict[str, set[str | None]] = {}
    required_route_fields = {
        "route_key",
        "proto_file",
        "proto_service",
        "method",
        "input_type",
        "output_type",
        "payload_type",
    }
    allowed_route_fields = required_route_fields | {"dependency_alias"}
    for route in routes:
        if not isinstance(route, dict):
            raise ValueError("DispatchSpec routes must contain JSON objects")
        missing_fields = required_route_fields - set(route)
        unknown_fields = set(route) - allowed_route_fields
        if missing_fields:
            raise ValueError(
                f"DispatchSpec route is missing field(s): {sorted(missing_fields)}"
            )
        if unknown_fields:
            raise ValueError(
                f"DispatchSpec route contains unknown field(s): {sorted(unknown_fields)}"
            )
        route_key = route.get("route_key")
        service = route.get("proto_service")
        if not isinstance(route_key, str) or not route_key:
            raise ValueError("DispatchSpec route_key must be a non-empty string")
        if route_key in route_keys:
            raise ValueError(f"DispatchSpec contains duplicate route_key {route_key}")
        route_keys.add(route_key)
        if not isinstance(service, str) or not service:
            raise ValueError(
                f"DispatchSpec route {route_key} has no proto_service"
            )
        for field in required_route_fields - {"route_key", "proto_service"}:
            if not isinstance(route[field], str) or not route[field]:
                raise ValueError(
                    f"DispatchSpec route {route_key} field {field} must be a non-empty string"
                )
        alias = route.get("dependency_alias")
        if alias is not None and (not isinstance(alias, str) or not alias.strip()):
            raise ValueError(
                f"DispatchSpec remote route {route_key} has an empty dependency_alias"
            )
        service_aliases.setdefault(service, set()).add(alias)
    for service, aliases in service_aliases.items():
        if len(aliases) != 1:
            raise ValueError(
                f"DispatchSpec mixes local/remote routes or dependency aliases for service {service}"
            )
    embedded_hash = spec.get("spec_hash")
    if embedded_hash is not None and not isinstance(embedded_hash, str):
        raise ValueError("DispatchSpec spec_hash must be a string when present")
    if not expected_hash:
        raise ValueError("DispatchSpec hash option is required")
    if embedded_hash is not None and embedded_hash != expected_hash:
        raise ValueError(
            f"DispatchSpec hash mismatch: option {expected_hash}, file {embedded_hash}"
        )
    spec_without_hash = {k: v for k, v in spec.items() if k != "spec_hash"}
    canonical = json.dumps(
        spec_without_hash,
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=False,
        allow_nan=False,
    )
    actual = hashlib.sha256(canonical.encode("utf-8")).hexdigest()
    if actual != expected_hash:
        raise ValueError(
            f"DispatchSpec hash mismatch: expected {expected_hash}, got {actual}"
        )


# ---------------------------------------------------------------------------
# Per-service module generation
# ---------------------------------------------------------------------------


def generate_service_module(
    service_key: str,
    routes: list[dict],
    type_to_owner: dict[str, tuple[str, str, tuple[str, ...]]],
) -> dict:
    """Generate a Python module for one local service.

    Emits per-RPC ``<Package><Service><Method>Rpc`` classes, the service
    ``Handler`` Protocol, and the ``ServiceDispatcher`` class.
    """
    proto_file = routes[0]["proto_file"]
    proto_module = proto_module_name(proto_file)
    proto_alias = pb2_alias(proto_file)

    imports: dict[str, str] = {}
    imports[proto_alias] = import_line_for(proto_file, proto_module, proto_alias)
    type_refs: dict[str, tuple[str, str]] = {}

    for route in routes:
        for type_key in (route["input_type"], route["output_type"]):
            cleaned = type_key.lstrip(".")
            if cleaned in type_refs:
                continue
            owner = type_to_owner.get(cleaned)
            if owner is None:
                type_refs[cleaned] = (proto_alias, extract_message_type(cleaned))
                continue
            _owner_package, owner_proto_name, owner_type_path = owner
            alias = pb2_alias(owner_proto_name)
            if alias not in imports:
                imports[alias] = import_line_for(
                    owner_proto_name, proto_module_name(owner_proto_name), alias
                )
            type_refs[cleaned] = (alias, ".".join(owner_type_path))

    if "." in service_key:
        package_name, service_name = service_key.rsplit(".", 1)
    else:
        package_name, service_name = "", service_key

    sections: list[str] = [generate_preamble(list(imports.values()))]

    for route in routes:
        sections.append(generate_rpc_class(route, type_refs, proto_alias, package_name))

    sections.append(
        generate_handler_protocol(service_name, routes, type_refs, proto_alias)
    )
    sections.append(
        generate_service_dispatcher(
            service_name, package_name, routes, type_refs, proto_alias
        )
    )

    content = "\n\n".join(section for section in sections if section)
    file_name = workload_module_name(package_name, service_name)
    return {"name": file_name, "content": content}


def generate_preamble(import_lines: list[str]) -> str:
    """Generate module preamble with imports."""
    return "\n".join(
        [
            "# DO NOT EDIT.",
            "# Generated by protoc-gen-actrframework-python",
            "from __future__ import annotations",
            "",
            "from typing import Any, Protocol",
            "",
            *import_lines,
        ]
    )


def generate_rpc_class(
    route: dict,
    type_refs: dict[str, tuple[str, str]],
    default_alias: str,
    package_name: str,
) -> str:
    """Generate a per-RPC class implementing the ``RpcRequest`` Protocol."""
    method_name = route["method"]
    service_name = route["proto_service"].rsplit(".", 1)[-1]
    input_cleaned = route["input_type"].lstrip(".")
    output_cleaned = route["output_type"].lstrip(".")

    input_alias, input_type = type_refs.get(
        input_cleaned, (default_alias, extract_message_type(input_cleaned))
    )
    output_alias, output_type = type_refs.get(
        output_cleaned, (default_alias, extract_message_type(output_cleaned))
    )

    class_name = f"{to_pascal_case(package_name)}{service_name}{method_name}Rpc"
    route_key = route["route_key"]
    payload_type = route.get("payload_type", "rpcReliable")

    lines: list[str] = [
        f"class {class_name}:",
        f'    """RPC request class for {route_key}."""',
        "",
        f"    Response = {output_alias}.{output_type}",
        "",
        f"    def __init__(self, request: {input_alias}.{input_type}) -> None:",
        "        self._request = request",
        "",
        "    @property",
        "    def route_key(self) -> str:",
        f'        return "{route_key}"',
        "",
        "    @property",
        "    def payload_type(self) -> Any:",
        f'        return "{payload_type}"',
        "",
        "    def encode_request(self) -> bytes:",
        "        return self._request.SerializeToString()",
        "",
        f"    def decode_response(self, data: bytes) -> {output_alias}.{output_type}:",
        f"        return {output_alias}.{output_type}.FromString(data)",
    ]
    return "\n".join(lines)


def generate_handler_protocol(
    service_name: str,
    routes: list[dict],
    type_refs: dict[str, tuple[str, str]],
    default_alias: str,
) -> str:
    """Generate a typed Handler Protocol for a local service."""
    lines: list[str] = [f"class {service_name}Handler(Protocol):"]

    if not routes:
        lines.append("    pass")
        return "\n".join(lines)

    for route in routes:
        method_name = to_snake_case(route["method"])
        input_cleaned = route["input_type"].lstrip(".")
        output_cleaned = route["output_type"].lstrip(".")
        input_alias, input_type = type_refs.get(
            input_cleaned, (default_alias, extract_message_type(input_cleaned))
        )
        output_alias, output_type = type_refs.get(
            output_cleaned, (default_alias, extract_message_type(output_cleaned))
        )
        lines.append(
            f"    def {method_name}(self, req: {input_alias}.{input_type}) -> "
            f"{output_alias}.{output_type}: ..."
        )

    return "\n".join(lines)


def generate_service_dispatcher(
    service_name: str,
    package_name: str,
    routes: list[dict],
    type_refs: dict[str, tuple[str, str]],
    default_alias: str,
) -> str:
    """Generate a typed route-key dispatcher for a local service."""
    lines: list[str] = [
        f"class {service_name}Dispatcher:",
        f"    def __init__(self, handler: {service_name}Handler) -> None:",
        "        self._handler = handler",
        "",
        "    def dispatch(self, route_key: str, payload: bytes) -> bytes:",
        "",
    ]

    for route in routes:
        route_key = route["route_key"]
        method_name = to_snake_case(route["method"])
        input_cleaned = route["input_type"].lstrip(".")
        output_cleaned = route["output_type"].lstrip(".")
        input_alias, input_type = type_refs.get(
            input_cleaned, (default_alias, extract_message_type(input_cleaned))
        )
        output_alias, output_type = type_refs.get(
            output_cleaned, (default_alias, extract_message_type(output_cleaned))
        )
        lines.extend(
            [
                f'        if route_key == "{route_key}":',
                f"            req = {input_alias}.{input_type}.FromString(payload)",
                f"            resp = self._handler.{method_name}(req)",
                f"            if not isinstance(resp, {output_alias}.{output_type}):",
                f'                raise TypeError("{method_name} must return {output_type}")',
                "            return resp.SerializeToString()",
                "",
            ]
        )

    lines.append('        raise ValueError(f"Unknown route_key: {route_key}")')
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# Unified dispatcher (actr_dispatch.py)
# ---------------------------------------------------------------------------


def generate_unified_dispatch(
    local_routes: list[dict],
    remote_routes: list[dict],
    type_to_owner: dict[str, tuple[str, str, tuple[str, ...]]],
) -> str:
    """Generate the ``actr_dispatch.py`` unified dispatcher module.

    Local routes call the ``ActrDispatchHandler``; remote routes resolve the
    dependency alias, discover the target, and call ``ctx.call_raw``. The
    generated code never inlines ``DEFAULT_RPC_TIMEOUT_MS`` or uses
    ``call_raw_with_timeout`` / ``call_with_timeout``.
    """
    # Build pb2 imports for local route request types.
    imports: dict[str, str] = {}
    type_refs: dict[str, tuple[str, str]] = {}

    for route in local_routes:
        proto_file = route["proto_file"]
        proto_alias = pb2_alias(proto_file)
        if proto_alias not in imports:
            imports[proto_alias] = import_line_for(
                proto_file, proto_module_name(proto_file), proto_alias
            )
        for type_key in (route["input_type"], route["output_type"]):
            cleaned = type_key.lstrip(".")
            if cleaned in type_refs:
                continue
            owner = type_to_owner.get(cleaned)
            if owner is None:
                type_refs[cleaned] = (proto_alias, extract_message_type(cleaned))
                continue
            _owner_package, owner_proto_name, owner_type_path = owner
            alias = pb2_alias(owner_proto_name)
            if alias not in imports:
                imports[alias] = import_line_for(
                    owner_proto_name, proto_module_name(owner_proto_name), alias
                )
            type_refs[cleaned] = (alias, ".".join(owner_type_path))

    # Also resolve types for ActrDispatchHandler method signatures.
    for route in local_routes:
        for type_key in (route["input_type"], route["output_type"]):
            cleaned = type_key.lstrip(".")
            if cleaned not in type_refs:
                owner = type_to_owner.get(cleaned)
                if owner is None:
                    type_refs[cleaned] = (
                        pb2_alias(route["proto_file"]),
                        extract_message_type(cleaned),
                    )
                else:
                    _pkg, owner_proto, owner_path = owner
                    alias = pb2_alias(owner_proto)
                    if alias not in imports:
                        imports[alias] = import_line_for(
                            owner_proto, proto_module_name(owner_proto), alias
                        )
                    type_refs[cleaned] = (alias, ".".join(owner_path))

    sections: list[str] = []

    # Preamble
    preamble_lines = [
        "# DO NOT EDIT.",
        "# Generated by protoc-gen-actrframework-python",
        "from __future__ import annotations",
        "",
        "from typing import TYPE_CHECKING, Any, Protocol",
        "",
        "if TYPE_CHECKING:",
        "    from actr_workload import ActrContext",
        "",
        *list(imports.values()),
    ]
    sections.append("\n".join(preamble_lines))

    # ActrDispatchHandler remains a marker Protocol for remote-only specs so
    # the dispatcher signature is identical for every route mix.
    sections.append(generate_actr_dispatch_handler(local_routes, type_refs))

    # UnknownRoute exception
    sections.append(
        'class UnknownRoute(Exception):\n    def __init__(self, route_key: str) -> None:\n        super().__init__(f"Unknown route key: {route_key}")\n        self.route_key = route_key'
    )

    # actr_dispatch function
    sections.append(
        generate_actr_dispatch_function(local_routes, remote_routes, type_refs)
    )

    return "\n\n".join(sections)


def generate_actr_dispatch_handler(
    local_routes: list[dict],
    type_refs: dict[str, tuple[str, str]],
) -> str:
    """Generate the ``ActrDispatchHandler`` Protocol aggregating all local RPCs."""
    lines: list[str] = ["class ActrDispatchHandler(Protocol):"]

    if not local_routes:
        lines.append("    pass")
        return "\n".join(lines)

    for route in local_routes:
        service_name = route["proto_service"].rsplit(".", 1)[-1]
        method_name = route["method"]
        handler_method = f"{to_snake_case(service_name)}_{to_snake_case(method_name)}"
        input_cleaned = route["input_type"].lstrip(".")
        output_cleaned = route["output_type"].lstrip(".")
        default_alias = pb2_alias(route["proto_file"])
        input_alias, input_type = type_refs.get(
            input_cleaned, (default_alias, extract_message_type(input_cleaned))
        )
        output_alias, output_type = type_refs.get(
            output_cleaned, (default_alias, extract_message_type(output_cleaned))
        )
        lines.append(
            f"    async def {handler_method}("
            f"self, req: {input_alias}.{input_type}, ctx: ActrContext"
            f") -> {output_alias}.{output_type}: ..."
        )

    return "\n".join(lines)


def generate_actr_dispatch_function(
    local_routes: list[dict],
    remote_routes: list[dict],
    type_refs: dict[str, tuple[str, str]],
) -> str:
    """Generate the ``actr_dispatch`` async function."""
    sig_params = [
        "ctx: ActrContext",
        "handler: ActrDispatchHandler",
        "route_key: str",
        "payload: bytes",
        "timeout_ms: int",
    ]

    lines: list[str] = [
        "async def actr_dispatch(",
        *[f"    {p}," for p in sig_params],
        ") -> bytes:",
    ]

    if not local_routes and not remote_routes:
        lines.append("    raise UnknownRoute(route_key)")
        return "\n".join(lines)

    # Local routes
    for route in local_routes:
        route_key = python_string_literal(route["route_key"])
        service_name = route["proto_service"].rsplit(".", 1)[-1]
        method_name = route["method"]
        handler_method = f"{to_snake_case(service_name)}_{to_snake_case(method_name)}"
        input_cleaned = route["input_type"].lstrip(".")
        default_alias = pb2_alias(route["proto_file"])
        input_alias, input_type = type_refs.get(
            input_cleaned, (default_alias, extract_message_type(input_cleaned))
        )
        lines.extend(
            [
                f"    if route_key == {route_key}:",
                f"        req = {input_alias}.{input_type}.FromString(payload)",
                f"        resp = await handler.{handler_method}(req, ctx)",
                "        return resp.SerializeToString()",
            ]
        )

    # Remote routes
    for route in remote_routes:
        route_key = python_string_literal(route["route_key"])
        alias = python_string_literal(route["dependency_alias"])
        lines.extend(
            [
                f"    if route_key == {route_key}:",
                f"        actr_type = await ctx.resolve_dependency({alias})",
                "        target = await ctx.discover(actr_type)",
                "        return await ctx.call_raw(target, route_key, payload, timeout_ms)",
            ]
        )

    lines.append("    raise UnknownRoute(route_key)")
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# Metadata (actr-gen-meta.json)
# ---------------------------------------------------------------------------


def generate_metadata(
    spec: dict,
    generated_files: list[str],
    local_routes: list[dict],
    remote_routes: list[dict],
    type_to_owner: dict[str, tuple[str, str, tuple[str, ...]]],
) -> str:
    """Generate ``actr-gen-meta.json`` content."""
    local_services = _build_local_service_metadata(local_routes, type_to_owner)
    remote_services = _build_remote_service_metadata(remote_routes, type_to_owner)

    try:
        plugin_version = version("framework_codegen_python")
    except PackageNotFoundError:
        plugin_version = "unknown"

    metadata = {
        "schema_version": spec.get("schema_version", "v1"),
        "plugin_version": plugin_version,
        "language": "python",
        "dispatch_spec_hash": spec.get("spec_hash", ""),
        "generated_files": sorted([*generated_files, "actr-gen-meta.json"]),
        "generated_local_routes": sorted(r["route_key"] for r in local_routes),
        "generated_remote_routes": sorted(r["route_key"] for r in remote_routes),
        "local_services": local_services,
        "remote_services": remote_services,
    }
    return json.dumps(metadata, indent=2)


def _build_local_service_metadata(
    routes: list[dict],
    type_to_owner: dict[str, tuple[str, str, tuple[str, ...]]],
) -> list[dict]:
    """Build local_services metadata for actr-gen-meta.json."""
    by_service: dict[str, list[dict]] = {}
    for route in routes:
        by_service.setdefault(route["proto_service"], []).append(route)

    services: list[dict] = []
    for service_key, service_routes in by_service.items():
        if "." in service_key:
            package, name = service_key.rsplit(".", 1)
        else:
            package, name = "", service_key
        services.append(
            {
                "name": name,
                "package": package,
                "proto_file": service_routes[0]["proto_file"],
                "handler_interface": f"{name}Handler",
                "workload_type": f"{name}Workload",
                "dispatcher_type": f"{name}Dispatcher",
                "methods": [
                    _build_method_metadata(route, type_to_owner)
                    for route in service_routes
                ],
            }
        )
    return services


def _build_remote_service_metadata(
    routes: list[dict],
    type_to_owner: dict[str, tuple[str, str, tuple[str, ...]]],
) -> list[dict]:
    """Build remote_services metadata for actr-gen-meta.json."""
    by_service: dict[str, list[dict]] = {}
    for route in routes:
        by_service.setdefault(route["proto_service"], []).append(route)

    services: list[dict] = []
    for service_key, service_routes in by_service.items():
        if "." in service_key:
            package, name = service_key.rsplit(".", 1)
        else:
            package, name = "", service_key
        aliases = {route.get("dependency_alias") for route in service_routes}
        if len(aliases) != 1:
            raise ValueError(
                f"DispatchSpec mixes dependency aliases for service {service_key}"
            )
        dependency_alias = aliases.pop()
        if not isinstance(dependency_alias, str) or not dependency_alias.strip():
            raise ValueError(
                f"DispatchSpec remote service {service_key} has no dependency_alias"
            )
        services.append(
            {
                "name": name,
                "package": package,
                "proto_file": service_routes[0]["proto_file"],
                "dependency_alias": dependency_alias,
                "client_type": f"{name}Client",
                "methods": [
                    _build_method_metadata(route, type_to_owner)
                    for route in service_routes
                ],
            }
        )
    return services


def _build_method_metadata(
    route: dict,
    type_to_owner: dict[str, tuple[str, str, tuple[str, ...]]],
) -> dict:
    """Build method metadata for actr-gen-meta.json."""
    return {
        "name": route["method"],
        "snake_name": to_snake_case(route["method"]),
        "route_key": route["route_key"],
        "input_ref": _build_type_ref(route["input_type"], type_to_owner),
        "output_ref": _build_type_ref(route["output_type"], type_to_owner),
    }


def _build_type_ref(
    full_type: str,
    type_to_owner: dict[str, tuple[str, str, tuple[str, ...]]],
) -> dict:
    """Build a TypeRef dict for actr-gen-meta.json."""
    cleaned = full_type.lstrip(".")
    owner = type_to_owner.get(cleaned)
    if owner is None:
        return {
            "proto_type": cleaned,
            "type_name": extract_message_type(cleaned),
            "proto_package": "",
            "proto_file": "",
        }
    package, proto_file, type_path = owner
    return {
        "proto_type": cleaned,
        "type_name": ".".join(type_path),
        "proto_package": package,
        "proto_file": proto_file,
    }


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def proto_module_name(proto_name: str) -> str:
    """Convert proto file name to module name."""
    filename = proto_name.split("/")[-1]
    base = filename.removesuffix(".proto")
    return f"{base}_pb2"


def pb2_alias(proto_name: str) -> str:
    """pb2 module alias derived from a declaring proto file path."""
    base = (
        proto_name.strip()
        .removesuffix(".proto")
        .replace("/", "_")
        .replace(".", "_")
        .replace("-", "_")
        .lower()
    )
    return f"{base}_pb2" if base else "local_pb2"


def import_line_for(proto_name: str, proto_module: str, proto_alias: str) -> str:
    """Build the ``from .<dir> import <module>_pb2 as <alias>`` import line."""
    proto_dir = proto_name.rsplit("/", 1)[0] if "/" in proto_name else ""
    proto_dir = proto_dir.replace("-", "_")
    if proto_dir:
        return f"from generated.{proto_dir.replace('/', '.')} import {proto_module} as {proto_alias}"
    return f"from generated import {proto_module} as {proto_alias}"


def workload_module_name(package_name: str, service_name: str) -> str:
    """Return the generated workload dispatcher module path."""
    base = to_snake_case(package_name) if package_name else to_snake_case(service_name)
    return f"{base}_workload.py"


def extract_message_type(full_type: str) -> str:
    """Extract message type name from full type path."""
    return full_type.split(".")[-1]


def python_string_literal(value: str) -> str:
    """Return an escaped UTF-8 Python string literal."""
    return json.dumps(value, ensure_ascii=False)


def to_pascal_case(name: str) -> str:
    """Convert snake_case or dotted name to PascalCase, preserving caps."""
    parts = name.replace(".", "_").replace("-", "_").split("_")
    result = ""
    for part in parts:
        if part:
            result += part[0].upper() + part[1:]
    return result


def to_snake_case(name: str) -> str:
    """Convert CamelCase or dotted names to snake_case."""
    import re

    normalized = name.replace(".", "_").replace("-", "_")
    s1 = re.sub("(.)([A-Z][a-z]+)", r"\1_\2", normalized)
    return re.sub("([a-z0-9])([A-Z])", r"\1_\2", s1).lower()
