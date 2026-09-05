package io.actrium.datastream

import io.actrium.actr.ActrId
import io.actrium.actr.DataChunk
import io.actrium.actr.DataChunkCallback
import kotlinx.coroutines.delay

// 1:1 port of StreamEchoCollector.swift.
class StreamEchoCollector(
    private val streamId: String,
    private val expectedCount: Int,
    private val onReceive: (suspend (logLine: String, receivedLine: String) -> Unit)? = null,
) : DataChunkCallback {
    private val lock = Any()
    private val chunks = HashMap<ULong, DataChunk>()

    override suspend fun onStream(chunk: DataChunk, sender: ActrId) {
        if (chunk.streamId != streamId) return
        synchronized(lock) { chunks[chunk.sequence] = chunk }
        val payload = decodePayload(chunk.payload)
        val line = displayLine(payload)
        onReceive?.invoke("$line raw=$payload", line)
    }

    val receivedCount: Int
        get() = synchronized(lock) { chunks.size }

    /// Chunks collected so far, sorted by sequence. Preserves whatever was
    /// received before a failure so the UI isn't wiped on waitForCompletion
    /// timeout (matching Swift's `collectedChunks()`).
    fun collectedChunks(): List<DataChunk> = sortedChunks()

    /// Wait until `expectedCount` chunks arrive (or fall back to the count
    /// fixed at init when expectedCount is null — the two-phase flow passes
    /// the service-reported `client_chunks_received` here).
    suspend fun waitForCompletion(expectedCount: Int? = null, timeoutMs: Long): List<DataChunk> {
        val target = expectedCount ?: this.expectedCount
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            synchronized(lock) {
                if (chunks.size >= target) return sortedChunks()
            }
            delay(100)
        }
        synchronized(lock) {
            if (chunks.size >= target) return sortedChunks()
        }
        val got = synchronized(lock) { chunks.size }
        throw ProbeError.Timeout(
            "StreamEchoCollector: received $got/$target chunks on stream $streamId",
        )
    }

    private fun sortedChunks(): List<DataChunk> =
        synchronized(lock) { chunks.values.sortedBy { it.sequence } }

    companion object {
        // Swift: both branches return "received: \(payload)".
        fun displayLine(payload: String): String = "received: $payload"
    }
}

/// Swift uses `String(data:encoding:.utf8) ?? "<N bytes>"`. All stream
/// payloads here are UTF-8 text, so the lenient decode matches Swift for every
/// real payload; invalid-UTF8 (never occurs) decodes leniently instead of the
/// "<N bytes>" fallback.
private fun decodePayload(bytes: ByteArray): String = String(bytes, Charsets.UTF_8)
