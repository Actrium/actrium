package io.actrium.datastream

import io.actrium.actr.ActrId
import io.actrium.actr.DataChunk
import io.actrium.actr.DataChunkCallback
import kotlinx.coroutines.delay

// 1:1 port of SessionAckCollector.swift.
//
// Swift `actor SessionAckCollector: DataChunkCallback` serializes all access
// via actor isolation. Kotlin has no language-level actors; a plain class
// guarded by a synchronized monitor reproduces the same exclusive-access
// invariant (the onStream callback and the waitForCompletion poller never
// touch the chunks map concurrently).
class SessionAckCollector(
    private val streamId: String,
    private val expectedCount: Int,
) : DataChunkCallback {
    private val lock = Any()
    private val chunks = HashMap<ULong, DataChunk>()

    override suspend fun onStream(chunk: DataChunk, sender: ActrId) {
        if (chunk.streamId != streamId) return
        synchronized(lock) { chunks[chunk.sequence] = chunk }
    }

    val receivedCount: Int
        get() = synchronized(lock) { chunks.size }

    /// Poll until all expected chunks are received or the timeout elapses.
    /// Returns the received chunks sorted by sequence (matching Swift's
    /// `.values.sorted { $0.sequence < $1.sequence }`).
    suspend fun waitForCompletion(timeoutMs: Long = 30_000L): List<DataChunk> {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            synchronized(lock) {
                if (chunks.size >= expectedCount) {
                    return chunks.values.sortedBy { it.sequence }
                }
            }
            delay(100) // 100ms, matching Swift's Task.sleep(100_000_000)
        }
        synchronized(lock) {
            if (chunks.size >= expectedCount) {
                return chunks.values.sortedBy { it.sequence }
            }
        }
        val got = synchronized(lock) { chunks.size }
        throw ProbeError.Timeout(
            "SessionAckCollector: received $got/$expectedCount chunks on stream $streamId",
        )
    }

    /// Wait a short window and verify no new chunks arrived.
    suspend fun assertNoNewChunks(afterMs: Long = 3_000L): Boolean {
        val before = synchronized(lock) { chunks.size }
        delay(afterMs)
        return synchronized(lock) { chunks.size == before }
    }
}
