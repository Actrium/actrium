/**
 * DispatchSpec v1 — JSON descriptor consumed by the Kotlin protoc plugin to
 * generate the unified `actr_dispatch.kt` dispatcher.
 *
 * Mirrors the shape defined in `crates/actr-cli/src/commands/codegen/dispatch_spec.rs`.
 * The hash is recomputed locally using the same JCS + SHA-256 algorithm to
 * detect stale specs.
 */
package io.actrium.codegen

import java.io.File
import java.security.MessageDigest
import org.json.JSONArray
import org.json.JSONObject

data class DispatchRoute(
    val routeKey: String,
    val protoFile: String,
    val protoService: String,
    val method: String,
    val inputType: String,
    val outputType: String,
    val payloadType: String,
    val replyMode: String = "return",
    val dependencyAlias: String? = null,
    /** Preserve whether v1 JSON carried this later-added optional field. */
    val replyModePresent: Boolean = true,
)

data class DispatchSpec(
    val schemaVersion: String,
    val routes: List<DispatchRoute>,
    val specHash: String?,
) {
    companion object {
        /** Load a `DispatchSpec` from a JSON file at `path`. */
        fun fromPath(path: String): DispatchSpec {
            val content = File(path).readText()
            return fromJson(content)
        }

        /** Parse a `DispatchSpec` from a JSON string. */
        fun fromJson(content: String): DispatchSpec {
            val json = JSONObject(content)
            val allowedSpecFields = setOf("schema_version", "routes", "spec_hash")
            val unknownSpecFields = json.keySet() - allowedSpecFields
            require(unknownSpecFields.isEmpty()) {
                "dispatch spec contains unknown fields: ${unknownSpecFields.sorted()}"
            }
            require(json.has("schema_version") && json.has("routes")) {
                "dispatch spec must contain schema_version and routes"
            }
            val schemaVersion = json.getString("schema_version")
            require(schemaVersion == "v1") {
                "unsupported dispatch spec schema_version `$schemaVersion`; expected `v1`"
            }
            val routesArr = json.getJSONArray("routes")
            val routes = mutableListOf<DispatchRoute>()
            val requiredRouteFields =
                setOf(
                    "route_key",
                    "proto_file",
                    "proto_service",
                    "method",
                    "input_type",
                    "output_type",
                    "payload_type",
                )
            val allowedRouteFields = requiredRouteFields + setOf("dependency_alias", "reply_mode")
            for (i in 0 until routesArr.length()) {
                val route = routesArr.getJSONObject(i)
                val missingRouteFields = requiredRouteFields - route.keySet()
                val unknownRouteFields = route.keySet() - allowedRouteFields
                require(missingRouteFields.isEmpty()) {
                    "dispatch route #$i is missing fields: ${missingRouteFields.sorted()}"
                }
                require(unknownRouteFields.isEmpty()) {
                    "dispatch route #$i contains unknown fields: ${unknownRouteFields.sorted()}"
                }
                routes.add(
                    DispatchRoute(
                        routeKey = route.getString("route_key"),
                        protoFile = route.getString("proto_file"),
                        protoService = route.getString("proto_service"),
                        method = route.getString("method"),
                        inputType = route.getString("input_type"),
                        outputType = route.getString("output_type"),
                        payloadType = route.getString("payload_type"),
                        replyMode = route.optString("reply_mode", "return").also {
                            require(it == "return" || it == "explicit") {
                                "dispatch route #$i has unsupported reply_mode `$it`"
                            }
                        },
                        dependencyAlias =
                            if (route.has("dependency_alias") && !route.isNull("dependency_alias")) {
                                route.getString("dependency_alias")
                            } else {
                                null
                            },
                        replyModePresent = route.has("reply_mode"),
                    )
                )
            }
            val specHash =
                if (json.has("spec_hash") && !json.isNull("spec_hash")) {
                    json.getString("spec_hash")
                } else {
                    null
                }
            return DispatchSpec(schemaVersion, routes, specHash).also { it.validateRoutes() }
        }
    }

    /** Local routes have `dependencyAlias == null`. */
    fun localRoutes(): List<DispatchRoute> = routes.filter { it.dependencyAlias == null }

    /** Remote routes have `dependencyAlias != null`. */
    fun remoteRoutes(): List<DispatchRoute> = routes.filter { it.dependencyAlias != null }

    private fun validateRoutes() {
        val duplicateRoute =
            routes.groupingBy { it.routeKey }.eachCount().entries.firstOrNull { it.value > 1 }
        require(duplicateRoute == null) {
            "dispatch spec contains duplicate route_key `${duplicateRoute?.key}`"
        }

        for ((service, serviceRoutes) in routes.groupBy { it.protoService }) {
            val aliases = serviceRoutes.map { it.dependencyAlias }.toSet()
            require(aliases.size == 1) {
                "dispatch spec mixes local/remote routes or dependency aliases for service `$service`"
            }
            val alias = aliases.single()
            require(alias == null || alias.isNotBlank()) {
                "dispatch spec remote service `$service` has an empty dependency_alias"
            }
        }
        for (route in routes) {
            require(route.routeKey.isNotEmpty()) { "dispatch spec route_key must be non-empty" }
            val requiredValues =
                listOf(
                    route.protoFile,
                    route.protoService,
                    route.method,
                    route.inputType,
                    route.outputType,
                    route.payloadType,
                    route.replyMode,
                )
            require(requiredValues.all { it.isNotEmpty() }) {
                "dispatch route `${route.routeKey}` contains an empty required field"
            }
        }
    }

    /**
     * Verify the embedded `spec_hash` matches a recomputed JCS + SHA-256 hash
     * of the spec (without the `spec_hash` field), and that `expectedHash`
     * matches the same value.
     */
    fun verifyHash(expectedHash: String) {
        val recomputed = recomputeHash()

        if (specHash != null && specHash != recomputed) {
            throw IllegalArgumentException(
                "dispatch spec hash mismatch: embedded `$specHash` but recomputed " +
                    "`$recomputed`; the spec on disk is stale or corrupted"
            )
        }

        if (expectedHash.isNotEmpty() && expectedHash != recomputed) {
            throw IllegalArgumentException(
                "dispatch spec hash mismatch: CLI passed `$expectedHash` but the spec " +
                    "on disk hashes to `$recomputed`; rerun `actr gen` to refresh the spec"
            )
        }
    }

    /** Recompute the canonical JCS + SHA-256 hash without the `spec_hash` field. */
    fun recomputeHash(): String {
        val specWithoutHash = DispatchSpec(schemaVersion, routes, null)
        val canonical = canonicalizeValue(specToJsonValue(specWithoutHash))
        val canonicalStr = canonicalToCompactString(canonical)
        return computeSha256Hex(canonicalStr.toByteArray(Charsets.UTF_8))
    }

    /** Serialize this spec to a canonical JSON value (JCS-sorted). */
    private fun specToJsonValue(spec: DispatchSpec): Any {
        val routesArr = JSONArray()
        for (route in spec.routes) {
            val routeObj = JSONObject()
            routeObj.put("route_key", route.routeKey)
            routeObj.put("proto_file", route.protoFile)
            routeObj.put("proto_service", route.protoService)
            routeObj.put("method", route.method)
            routeObj.put("input_type", route.inputType)
            routeObj.put("output_type", route.outputType)
            routeObj.put("payload_type", route.payloadType)
            if (route.replyModePresent) {
                routeObj.put("reply_mode", route.replyMode)
            }
            if (route.dependencyAlias != null) {
                routeObj.put("dependency_alias", route.dependencyAlias)
            }
            routesArr.put(routeObj)
        }
        val specObj = JSONObject()
        specObj.put("schema_version", spec.schemaVersion)
        specObj.put("routes", routesArr)
        if (spec.specHash != null) {
            specObj.put("spec_hash", spec.specHash)
        }
        return specObj
    }

    /** Canonicalize a JSON value per JCS (RFC 8785): sorted keys, no whitespace. */
    @Suppress("UNCHECKED_CAST")
    private fun canonicalizeValue(value: Any): Any {
        return when (value) {
            is JSONObject -> {
                val keys = value.keySet().sorted()
                val sorted = JSONObject()
                for (key in keys) {
                    sorted.put(key, canonicalizeValue(value.get(key)))
                }
                sorted
            }
            is JSONArray -> {
                val arr = JSONArray()
                for (i in 0 until value.length()) {
                    arr.put(canonicalizeValue(value.get(i)))
                }
                arr
            }
            else -> value
        }
    }

    /** Produce a compact JSON string (no whitespace) from a canonical value. */
    private fun canonicalToCompactString(value: Any): String {
        return when (value) {
            is JSONObject -> {
                val keys = value.keySet().sorted()
                val parts = keys.map { key ->
                    val v = value.get(key)
                    "${jcsStringLiteral(key)}:${canonicalToCompactString(v)}"
                }
                parts.joinToString(",", "{", "}")
            }
            is JSONArray -> {
                val parts = (0 until value.length()).map { canonicalToCompactString(value.get(it)) }
                parts.joinToString(",", "[", "]")
            }
            is String -> jcsStringLiteral(value)
            else -> value.toString()
        }
    }

    /** Encode one JSON string using the RFC 8785 escaping rules. */
    private fun jcsStringLiteral(value: String): String {
        val result = StringBuilder("\"")
        var index = 0
        while (index < value.length) {
            val codePoint = value.codePointAt(index)
            require(codePoint !in 0xD800..0xDFFF) {
                "dispatch spec contains an unpaired UTF-16 surrogate"
            }
            when (codePoint) {
                0x08 -> result.append("\\b")
                0x09 -> result.append("\\t")
                0x0A -> result.append("\\n")
                0x0C -> result.append("\\f")
                0x0D -> result.append("\\r")
                0x22 -> result.append("\\\"")
                0x5C -> result.append("\\\\")
                in 0x00..0x1F ->
                    result.append("\\u").append(codePoint.toString(16).padStart(4, '0'))
                else -> result.appendCodePoint(codePoint)
            }
            index += Character.charCount(codePoint)
        }
        return result.append('"').toString()
    }

    /** Compute a SHA-256 hex digest. */
    private fun computeSha256Hex(data: ByteArray): String {
        val digest = MessageDigest.getInstance("SHA-256")
        val result = digest.digest(data)
        return result.joinToString("") { "%02x".format(it) }
    }
}
