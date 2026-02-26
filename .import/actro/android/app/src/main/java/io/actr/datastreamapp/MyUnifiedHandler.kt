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
import io.actr.datastreamapp.generated.UnifiedHandler
import io.actor_rtc.actr.ContextBridge
import data_stream_app.Local.*
import io.actor_rtc.actr.ActrException
import io.actor_rtc.actr.ActrId
import io.actor_rtc.actr.DataStream
import io.actor_rtc.actr.DataStreamCallback
import io.actor_rtc.actr.PayloadType


/**
 * Implementation of UnifiedHandler
 *
 * This class handles all local service requests.
 * Remote service requests are automatically forwarded by the UnifiedDispatcher.
 */
class MyUnifiedHandler : UnifiedHandler {

    private var serverActId: ActrId? = null

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
            val responseBytes = ctx.callRaw(
                target,
                PROMPT_ROUTE_KEY,
                PayloadType.RPC_RELIABLE,
                usrPromptRequest.toByteArray(),
                PROMPT_TIMEOUT_MS
            )
            Log.e("send_text", "send_text___3")
            val reply = Ask.AssistantReply.parseFrom(responseBytes)

            // Register streams AFTER the prompt is accepted (mirrors iOS handler behavior).
            if (request.textResponseStreamId.isNotEmpty()) {
                val streamId = request.textResponseStreamId
                Log.i(TAG, "Registering text response stream: $streamId")
                ctx.registerStream(
                    streamId,
                    object : DataStreamCallback {
                        override suspend fun onStream(chunk: DataStream, sender: ActrId) {
                            val msg = try {
                                String(chunk.payload, Charsets.UTF_8)
                            } catch (e: Exception) {
                                ""
                            }
                            Log.i(
                                TAG,
                                "[text-stream:$streamId] seq=${chunk.sequence} sender=${sender.serialNumber} bytes=${chunk.payload.size} text=${msg.take(200)}"
                            )
                            // NOTE: iOS emits this to RN as an event. Android RN emitter (if needed)
                            // should live in ActrBridge.kt, not in this workload handler.
                        }
                    }
                )
            }

            if (request.voiceResponseStreamId.isNotEmpty()) {
                val streamId = request.voiceResponseStreamId
                Log.i(TAG, "Registering voice response stream: $streamId")
                ctx.registerStream(
                    streamId,
                    object : DataStreamCallback {
                        override suspend fun onStream(chunk: DataStream, sender: ActrId) {
                            Log.i(
                                TAG,
                                "[voice-stream:$streamId] seq=${chunk.sequence} sender=${sender.serialNumber} bytes=${chunk.payload.size}"
                            )
                        }
                    }
                )
            }

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
        TODO("Not yet implemented")
    }

    /**
     * Handle send_voice_chunk request for AskLocalService service
     *
     * @param request The VoiceChunkMessage request message
     * @param ctx Context bridge for actor operations
     * @return VoiceChunkMessageResponse response message
     */
    override suspend fun send_voice_chunk(request: VoiceChunkMessage, ctx: ContextBridge): VoiceChunkMessageResponse {
        TODO("Not yet implemented")
    }

    // ===== End of AskLocalService methods =====

}
