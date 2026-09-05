package io.actrium.datastream

import java.util.UUID

// 1:1 port of ProbeResult.swift.

data class ProbeResult(
    val id: String = UUID.randomUUID().toString(),
    val name: String,
    val passed: Boolean,
    val durationMs: Long,
    val details: String,
    val logLines: List<String>,
) {
    // Swift icon: "checkmark.circle.fill" / "xmark.circle.fill". Kept for
    // parity; the Compose UI maps this to a Material Icon.
    val icon: String
        get() = if (passed) "checkmark.circle.fill" else "xmark.circle.fill"
}

sealed class ProbeError(message: String) : Exception(message) {
    class Timeout(msg: String) : ProbeError("Timeout: $msg")
    class AssertionFailed(msg: String) : ProbeError("Assertion failed: $msg")
    class RuntimeError(msg: String) : ProbeError("Error: $msg")
}
