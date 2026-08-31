package io.actrium.actr.dsl

import io.actrium.actr.ActrException

/**
 * Movable response handle for generated EXPLICIT RPC handlers.
 *
 * The handle may outlive its handler. If it is never sent, the host expires
 * the pending reply at the RFC-0323 in-flight TTL.
 */
class Reply<T>(
    private val context: ActrContext,
    private val encode: (T) -> ByteArray,
) {
    /** Deliver one response through the current invocation context. */
    suspend fun send(response: T) {
        context.replyRaw(encode(response))
    }

    /** Deliver one ActrError through the current invocation context. */
    suspend fun sendError(error: ActrException) {
        context.replyError(error)
    }
}
