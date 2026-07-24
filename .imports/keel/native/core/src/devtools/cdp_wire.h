/*
 * cdp_wire.h — the pure-string CDP JSON wire helpers, split out of CDPServer so
 * they can be compiled without the networking server. The QuickJS debug wasm
 * embeds the CDP engine (DebuggerAdapter / AdapterDebugTarget / the engine
 * debugger) and needs exactly these helpers; pulling them here keeps the wasm
 * build free of CDPServer.cpp (sockets, threads). CDPServer keeps the HTTP /
 * discovery helpers (buildHttpResponse / parseRequestLine / injectSessionId),
 * which depend on its own types.
 *
 * SessionId is just std::string, so buildEventJSON takes std::optional<std::string>
 * here to avoid depending on CDPServer.h's alias.
 *
 * Licensed under the Apache License, Version 2.0.
 */
#pragma once

#include <optional>
#include <string>

namespace rill::devtools {
namespace cdp {

// Build a CDP event JSON object: {"method":..,"params":..[,"sessionId":..]}.
// params must be a JSON object; invalid input falls back to "{}".
std::string buildEventJSON(const std::string& method, const std::string& params,
                           const std::optional<std::string>& sessionId = std::nullopt);

// Build a CDP response JSON object: {"id":..,"result":..}.
std::string buildResponseJSON(int id, const std::string& result);

// Build a CDP error response JSON object: {"id":..,"error":{..}}.
std::string buildErrorJSON(int id, int code, const std::string& message);

// Escape a string for embedding in JSON (quotes, backslashes, control chars).
std::string escapeJSON(const std::string& str);

// Extract a string value for `key` from a flat JSON object (naive, escape-aware).
// NB: substring-based — it finds the FIRST textual occurrence of the key, even
// inside nested objects. Several call sites rely on exactly that (flat payloads,
// or deliberately reaching into a known nested shape), so its behavior is frozen.
// For keys that MUST be read from the top level only (routing-relevant ones like
// "sessionId"), use parseJSONStringTopLevel below.
std::optional<std::string> parseJSONString(const std::string& json, const std::string& key);

// Extract a string value for `key` from the TOP LEVEL of the outermost JSON
// object only. Nested objects/arrays (e.g. params.sessionId in
// Target.detachFromTarget) are skipped via string-aware depth tracking, and key
// text occurring inside string values is ignored. Returns nullopt when the key
// is absent at the top level or its value is not a string.
std::optional<std::string> parseJSONStringTopLevel(const std::string& json,
                                                   const std::string& key);

// Extract an int value for `key` from a flat JSON object (naive, overflow-safe).
std::optional<int> parseJSONInt(const std::string& json, const std::string& key);

}  // namespace cdp
}  // namespace rill::devtools
