package com.example

import android.util.Log
import com.example.generated.ActrDispatchHandler
import io.actrium.actr.DataChunk
import io.actrium.actr.DataChunkStreamBridge
import io.actrium.actr.PayloadType
import io.actrium.actr.dsl.ActrContext
import io.actrium.actr.dsl.asFlow
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * Implementation of ActrDispatchHandler (StreamClientHandler)
 *
 * Implements the DuplexStreamService client protocol:
 * 1. Call StartDuplexStream RPC on the server
 * 2. Send DataChunk chunks to the server
 * 3. Optionally receive return stream from the server
 */
class MyUnifiedHandler : ActrDispatchHandler {
    companion object {
        private const val TAG = "MyUnifiedHandler"
    }

    override suspend fun start_stream(
        request: local.StreamClientOuterClass.ClientStartStreamRequest,
        ctx: ActrContext,
    ): local.StreamClientOuterClass.ClientStartStreamResponse {
        val clientId = request.clientId
        val sessionId = request.sessionId
        val messageCount = request.messageCount

        Log.i(
            TAG,
            "start_stream: client_id=$clientId, session_id=$sessionId, message_count=$messageCount",
        )

        try {
            val serverType = ctx.resolveDependency("stream")
            Log.i(
                TAG,
                "🌐 discovering server type: ${serverType.publisher}/${serverType.name}",
            )
            val serverId = ctx.discover(serverType)
            Log.i(TAG, "🎯 discovered server: ${serverId.serialNumber}")

            // Step 1: Call StartDuplexStream RPC on the server
            val clientStreamId = "client-$sessionId"
            val startReq =
                local.DataStreamPeer.StartDuplexStreamRequest
                    .newBuilder()
                    .setSessionId(sessionId)
                    .setClientToServiceStreamId(clientStreamId)
                    .setClientChunkCount(messageCount.toInt())
                    .setPayloadMode(local.DataStreamPeer.StreamPayloadMode.STREAM_RELIABLE)
                    .setNote("Android duplex stream test")
                    .build()

            // RpcReliable Call already owns its transport retry and readiness
            // deadline. Do not wrap this ordinary call in a second SDK retry.
            val startRespPayload =
                ctx.callRaw(
                    serverId,
                    "local.DuplexStreamService.StartDuplexStream",
                    PayloadType.RPC_RELIABLE,
                    startReq.toByteArray(),
                    30000L,
                )
            val startResp = local.DataStreamPeer.StartDuplexStreamResponse.parseFrom(startRespPayload)
            Log.i(
                TAG,
                "StartDuplexStream response: session=${startResp.sessionId}, " +
                    "accepted_stream=${startResp.acceptedClientToServiceStreamId}, " +
                    "return_stream=${startResp.serviceToClientStreamId}, status=${startResp.status}",
            )

            // Step 2: Subscribe to the server's return stream (if any);
            // the collector coroutine closes the subscription when cancelled.
            val serverStreamId = startResp.serviceToClientStreamId
            var serverStream: DataChunkStreamBridge? = null
            if (serverStreamId.isNotBlank()) {
                val stream = ctx.dataChunks(serverStreamId, null)
                serverStream = stream
                CoroutineScope(Dispatchers.IO).launch {
                    stream.asFlow().collect { event ->
                        val text = String(event.chunk.payload, Charsets.UTF_8)
                        Log.i(
                            TAG,
                            "client received ${event.chunk.sequence} from ${event.sender.serialNumber}: $text",
                        )
                    }
                }
                Log.i(TAG, "✅ Subscribed to server return stream: $serverStreamId")
            }

            // Step 3: Send DataChunk chunks to the server (synchronous)
            val sink = ctx.dataChunkSink(serverId, clientStreamId, PayloadType.STREAM_RELIABLE)
            try {
                for (i in 1..messageCount) {
                    val message = "[client $clientId] message $i"
                    val dataChunk =
                        DataChunk(
                            streamId = clientStreamId,
                            sequence = i.toULong(),
                            payload = message.toByteArray(Charsets.UTF_8),
                            metadata = emptyList(),
                            timestampMs = System.currentTimeMillis(),
                        )

                    Log.i(TAG, "client sending $i/$messageCount: $message")
                    try {
                        sink.send(dataChunk)
                    } catch (e: Exception) {
                        Log.e(TAG, "client sink send error: ${e.message}")
                    }
                    delay(1000)
                }
            } finally {
                sink.closeAsync()
            }

            // Step 4: Call FinishDuplexStream
            try {
                val finishReq =
                    local.DataStreamPeer.FinishDuplexStreamRequest
                        .newBuilder()
                        .setSessionId(sessionId)
                        .setClientToServiceStreamId(clientStreamId)
                        .setServiceToClientStreamId(serverStreamId)
                        .build()
                val finishRespPayload =
                    ctx.callRaw(
                        serverId,
                        "local.DuplexStreamService.FinishDuplexStream",
                        PayloadType.RPC_RELIABLE,
                        finishReq.toByteArray(),
                        30000L,
                    )
                val finishResp =
                    local.DataStreamPeer.FinishDuplexStreamResponse.parseFrom(finishRespPayload)
                Log.i(
                    TAG,
                    "FinishDuplexStream: recv=${finishResp.clientChunksReceived}, " +
                        "sent=${finishResp.serviceChunksSent}, status=${finishResp.status}",
                )
            } catch (e: Exception) {
                Log.e(TAG, "FinishDuplexStream error: ${e.message}")
            }

            // Step 5: Close the return stream subscription
            serverStream?.let { stream ->
                try {
                    stream.closeAsync()
                    Log.i(TAG, "✅ Closed stream subscription: $serverStreamId")
                } catch (e: Exception) {
                    Log.e(TAG, "stream close error: ${e.message}")
                }
            }

            return local.StreamClientOuterClass.ClientStartStreamResponse
                .newBuilder()
                .setAccepted(true)
                .setMessage("started duplex stream: $sessionId")
                .build()
        } catch (e: Exception) {
            Log.e(TAG, "❌ start_stream failed: ${e.message}", e)
            return local.StreamClientOuterClass.ClientStartStreamResponse
                .newBuilder()
                .setAccepted(false)
                .setMessage("Failed to start stream: ${e.message}")
                .build()
        }
    }
}
