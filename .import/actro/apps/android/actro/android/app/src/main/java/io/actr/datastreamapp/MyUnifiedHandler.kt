/**
 * Unified Handler Implementation
 *
 * This file provides the implementation for all local service handlers.
 * Implement your business logic in this class.
 */
package io.actr.datastreamapp

import android.util.Log
import ask.Ask
import data_stream_app.Local
import data_stream_app.Local.StartVoiceMessage
import data_stream_app.Local.StartVoiceMessageResponse
import data_stream_app.Local.TextMessage
import data_stream_app.Local.TextMessageResponse
import data_stream_app.Local.VoiceChunkMessage
import data_stream_app.Local.VoiceChunkMessageResponse
import io.actor_rtc.actr.ActrException
import io.actor_rtc.actr.ActrId
import io.actor_rtc.actr.ContextBridge
import io.actor_rtc.actr.DataStream
import io.actor_rtc.actr.MetadataEntry
import io.actor_rtc.actr.PayloadType
import io.actr.datastreamapp.generated.UnifiedHandler


/**
 * Implementation of UnifiedHandler
 *
 * This class handles all local service requests.
 * Remote service requests are automatically forwarded by the UnifiedDispatcher.
 */
class MyUnifiedHandler : UnifiedHandler {

    private var serverActId: ActrId? = null
    val streamInfos = mutableMapOf<String, ContextBridge>() // Map of streamId to StreamInfo

    companion object {
        private const val TAG = "MyUnifiedHandler"
        private const val PROMPT_ROUTE_KEY = "ask.AskService.Prompt"
        private const val PROMPT_TIMEOUT_MS = 300_000L
    }

    fun setServerActId(actId: ActrId) {
        serverActId = actId
    }

    /**
     * Handle send_text request for AskLocalService service
     *
     * @param request The TextMessage request message
     * @param ctx Context bridge for actor operations
     * @return TextMessageResponse response message
     */
    override suspend fun send_text(request: TextMessage, ctx: ContextBridge): TextMessageResponse {
        Log.e("send_text", "send_text__${serverActId}")
        val target = serverActId
            ?: throw ActrException.StateException("Server Act ID is not set. Cannot forward request.")
        Log.e("send_text", "send_text___1")
        return try {
            val usrPromptRequest = Ask.UsrPromptRequest.newBuilder().apply {
                text = request.text
                sessionId = request.sessionId
                questionId = request.messageId
                addAllAttachmentIds(request.attachmentIdsList)
                textResponseStreamId = request.textResponseStreamId
                voiceResponseStreamId = request.voiceResponseStreamId
                if (request.hasLocation()) {
                    location = request.location
                }
            }.build()
            Log.e("send_text", "send_text___2")


            // Register streams AFTER the prompt is accepted (mirrors iOS handler behavior).
            if (request.textResponseStreamId.isNotEmpty()) {
                val streamId = request.textResponseStreamId
                streamInfos[streamId] = ctx
                Log.i(TAG, "Registering text response stream: $streamId")
                val callback = AskAwayDataStreamCallback(streamId, StreamType.TEXT)
                ctx.registerStream(streamId, callback)
            }

            if (request.voiceResponseStreamId.isNotEmpty()) {
                val streamId = request.voiceResponseStreamId
                streamInfos[streamId] = ctx
                Log.i(TAG, "Registering voice response stream: $streamId")
                val callback = AskAwayDataStreamCallback(streamId, StreamType.VOICE)
                ctx.registerStream(streamId, callback)
            }
            val responseBytes = ctx.callRaw(
                target,
                PROMPT_ROUTE_KEY,
                PayloadType.RPC_RELIABLE,
                usrPromptRequest.toByteArray(),
                PROMPT_TIMEOUT_MS
            )
            Log.e("send_text", "send_text___3")
            val reply = Ask.AssistantReply.parseFrom(responseBytes)
            Local.TextMessageResponse.newBuilder().apply {
                questionId = reply.questionId
                sessionId = reply.sessionId
                text = reply.text
                statusCode = reply.statusCode
                errorMessage = reply.errorMessage
            }.build()
        } catch (e: ActrException) {
            Log.e(TAG, "send_text ActrException: ${e.message}", e)
            Local.TextMessageResponse.newBuilder().apply {
                questionId = request.messageId
                sessionId = request.sessionId
                statusCode = -1
                errorMessage = e.message ?: "ActrException"
            }.build()
        } catch (e: Exception) {
            Log.e(TAG, "send_text Exception: ${e.message}", e)
            Local.TextMessageResponse.newBuilder().apply {
                questionId = request.messageId
                sessionId = request.sessionId
                statusCode = -1
                errorMessage = e.message ?: "Unknown error"
            }.build()
        }
    }

    /**
     * Handle start_send_voice request for AskLocalService service
     *
     * @param request The StartVoiceMessage request message
     * @param ctx Context bridge for actor operations
     * @return StartVoiceMessageResponse response message
     */
    override suspend fun start_send_voice(request: StartVoiceMessage, ctx: ContextBridge): StartVoiceMessageResponse {
        Log.i(TAG, "Received StartSendVoice request: ${request.streamId}")
        val target = serverActId
            ?: throw ActrException.StateException("Server Act ID is not set. Cannot forward request.")

        return try {
            val usrPromptRequest = Ask.UsrPromptRequest.newBuilder().apply {
                voiceStreamId = request.streamId
                sessionId = request.sessionId
                questionId = request.messageId
                addAllAttachmentIds(request.attachmentIdsList)
                textResponseStreamId = request.textResponseStreamId
                voiceResponseStreamId = request.voiceResponseStreamId
                if (request.hasLocation()) {
                    location = request.location
                }
            }.build()


            // Register streams AFTER the prompt is accepted
            if (request.textResponseStreamId.isNotEmpty()) {
                val streamId = request.textResponseStreamId
                Log.i(TAG, "Registering text response stream: $streamId")
                val callback = AskAwayDataStreamCallback(streamId, StreamType.TEXT)
                ctx.registerStream(streamId, callback)
            }

            if (request.voiceResponseStreamId.isNotEmpty()) {
                val streamId = request.voiceResponseStreamId
                Log.i(TAG, "Registering voice response stream: $streamId")
                val callback = AskAwayDataStreamCallback(streamId, StreamType.VOICE)
                ctx.registerStream(streamId, callback)
            }

            val responseBytes = ctx.callRaw(
                target,
                PROMPT_ROUTE_KEY,
                PayloadType.RPC_RELIABLE,
                usrPromptRequest.toByteArray(),
                PROMPT_TIMEOUT_MS
            )

            val reply = Ask.AssistantReply.parseFrom(responseBytes)

            StartVoiceMessageResponse.newBuilder().apply {
                questionId = reply.questionId
                sessionId = reply.sessionId
                text = reply.text
                statusCode = reply.statusCode
                errorMessage = reply.errorMessage
            }.build()

        } catch (e: ActrException) {
            Log.e(TAG, "start_send_voice ActrException: ${e.message}", e)
            StartVoiceMessageResponse.newBuilder().apply {
                questionId = request.messageId
                sessionId = request.sessionId
                statusCode = -1
                errorMessage = e.message ?: "ActrException"
            }.build()
        } catch (e: Exception) {
            Log.e(TAG, "start_send_voice Exception: ${e.message}", e)
            StartVoiceMessageResponse.newBuilder().apply {
                questionId = request.messageId
                sessionId = request.sessionId
                statusCode = -1
                errorMessage = e.message ?: "Unknown error"
            }.build()
        }
    }

    /**
     * Handle send_voice_chunk request for AskLocalService service
     *
     * @param request The VoiceChunkMessage request message
     * @param ctx Context bridge for actor operations
     * @return VoiceChunkMessageResponse response message
     */
    override suspend fun send_voice_chunk(request: VoiceChunkMessage, ctx: ContextBridge): VoiceChunkMessageResponse {
        Log.i(TAG, "Received sendVoiceChunk: streamId=${request.streamId}, chunkId=${request.chunkId}")
        val target = serverActId
            ?: throw ActrException.StateException("Server Act ID is not set. Cannot forward request.")

        return try {
            val seq = request.chunkId.toULongOrNull() ?: 0UL
            val metadataEntries = request.metadataMap.map { MetadataEntry(it.key, it.value) }

            val chunk = DataStream(
                streamId = request.streamId,
                sequence = seq,
                payload = request.chunkData.toByteArray(),
                metadata = metadataEntries,
                timestampMs = System.currentTimeMillis()
            )

            ctx.sendDataStream(target, chunk)

            VoiceChunkMessageResponse.newBuilder().apply {
                streamId = request.streamId
                status = 0
            }.build()
        } catch (e: Exception) {
            Log.e(TAG, "send_voice_chunk Exception: ${e.message}", e)
            VoiceChunkMessageResponse.newBuilder().apply {
                streamId = request.streamId
                status = -1
            }.build()
        }
    }

    // ===== End of AskLocalService methods =====
}
