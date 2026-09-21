/** Stream-first DataChunk adapters over the opaque UniFFI bridge handles. */
package io.actrium.actr.dsl

import io.actrium.actr.ActrId
import io.actrium.actr.DataChunk
import io.actrium.actr.DataChunkSinkBridge
import io.actrium.actr.DataChunkStreamBridge
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow

/** One inbound DataChunk together with its sender. */
data class DataChunkEvent(
    val chunk: DataChunk,
    val sender: ActrId,
)

/**
 * Collect inbound chunks of this subscription as a cold [Flow].
 *
 * The flow is single-consumer (the bridge rejects concurrent `next()` calls).
 * Cancelling collection closes the bridge in `finally`, which unregisters the
 * subscription and wakes the pending read.
 *
 * Usage: `ctx.dataChunks(streamId, capacity).asFlow().collect { event -> ... }`.
 * The Flow-returning adapter lives on the bridge (not on `ActrContext`)
 * because the generated `ContextBridge.dataChunks` member shadows any
 * same-named extension.
 */
fun DataChunkStreamBridge.asFlow(): Flow<DataChunkEvent> = flow {
    try {
        while (true) {
            val event = next() ?: break
            emit(DataChunkEvent(event.chunk, event.sender))
        }
    } finally {
        // `closeAsync` is the Kotlin-side name of the bridge's async close;
        // UniFFI's AutoCloseable already claims the `close()` name.
        closeAsync()
    }
}

/**
 * Ergonomic wrapper over the opaque sink bridge. Sends are serialized by the
 * bridge in acceptance order; [close] stops accepting new sends, waits for
 * accepted sends to finish, and is idempotent.
 */
class DataChunkSink internal constructor(
    private val bridge: DataChunkSinkBridge,
) {
    /** Send one chunk; its `streamId` must equal the sink's stream ID. */
    suspend fun send(chunk: DataChunk) = bridge.send(chunk)

    /** Flush accepted sends and close. Idempotent. */
    suspend fun close() = bridge.closeAsync()
}

/** Wrap this opaque sink bridge in the ergonomic [DataChunkSink]. */
fun DataChunkSinkBridge.asSink(): DataChunkSink = DataChunkSink(this)
