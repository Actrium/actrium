package chat.askaway.android.core

import android.util.Base64
import android.util.Log
import com.facebook.soloader.LogUtil
import io.actor_rtc.actr.ContextBridge
import io.actor_rtc.actr.RpcEnvelopeBridge
import io.actor_rtc.actr.WorkloadBridge
import io.actr.datastreamapp.MyUnifiedHandler
import io.actr.datastreamapp.UnifiedWorkload
import io.actor_rtc.actr.dsl.createActrSystem
import io.actor_rtc.actr.ActrRefWrapper
import io.actor_rtc.actr.PayloadType
import ask.Ask.UsrPromptRequest
import data_stream_app.Local.TextMessage
import io.actor_rtc.actr.ActrType
import java.util.UUID
import com.google.protobuf.ByteString
import ask.Ask.AttachRequest
import ask.Ask.AttachResponse
import ask.Ask.AttachmentType
import ask.Ask.Location
import data_stream_app.Local.StartVoiceMessage
import data_stream_app.Local.StartVoiceMessageResponse
import data_stream_app.Local.VoiceChunkMessage
import data_stream_app.Local.VoiceChunkMessageResponse

class ActrManager {

    companion object {
        @Volatile
        private var instance: ActrManager? = null

        fun getInstance(): ActrManager {
            return instance ?: synchronized(this) {
                instance ?: ActrManager().also { instance = it }
            }
        }
    }

    private val clients = mutableMapOf<String, ActrRefWrapper>()
    private val handles = mutableMapOf<String, MyUnifiedHandler>()

    suspend fun initialize(tabId: String, actorType: String, path: String) {
        LogUtil.e("ActrManager", "RTC initialized__${path}")
        val clientSystem = createActrSystem(path)
        Log.e("ActrManager", "init_____1")
        val handler = MyUnifiedHandler()
        Log.e("ActrManager", "init_____2")
        val clientWorkload = UnifiedWorkload(handler)
        Log.e("ActrManager", "init_____3")
        val clientNode = clientSystem.attach(clientWorkload)
        Log.e("ActrManager", "init_____4")
        val clientRef = clientNode.start()
        Log.e("ActrManager", "init_____5")
        val parts = actorType.split("+")
        Log.e("ActrManager", "init_____6")
        val manufacturer = parts[0]
        Log.e("ActrManager", "init_____7")
        val name = parts[1]
        Log.e("ActrManager", "manufacturer:${manufacturer} name:${name}")
        val serverType = ActrType(manufacturer, name)
        Log.e("ActrManager", "init_____8")
        val serverActorIds = clientRef.discover(serverType, 1u)
        handler.setServerActId(serverActorIds[0])
        clients[tabId] = clientRef
        handles[tabId] = handler
    }

    suspend fun sendTextMessage(
        tabId: String,
        messageId: String,
        sessionId: String,
        text: String,
        streamId: String,
        textResponseStreamId: String,
        voiceResponseStreamId: String,
        attachmentIds: List<String>?,
        location: LocationData?
    ) {
        val builder = TextMessage.newBuilder()
        builder.messageId = messageId
        builder.sessionId = sessionId
        builder.text = text
        builder.streamId = streamId
        builder.textResponseStreamId = textResponseStreamId
        builder.voiceResponseStreamId = voiceResponseStreamId

        attachmentIds?.forEach { id ->
            builder.addAttachmentIds(id)
        }

        location?.let {
            val locationBuilder = Location.newBuilder()
            locationBuilder.latitude = it.latitude
            locationBuilder.longitude = it.longitude
            locationBuilder.address = it.address
            locationBuilder.placeName = it.placeName
            builder.location = locationBuilder.build()
        }

        val request = builder.build()
        clients[tabId]?.call(
            "data_stream_app.AskLocalService.SendText",
            PayloadType.RPC_RELIABLE,
            request.toByteArray(),
            20000L
        )
    }

    suspend fun uploadAttachment(
        tabId: String,
        id: String?,
        filename: String?,
        type: Int,
        fileData: String
    ): AttachmentResult {
        val client = clients[tabId] ?: throw Exception("Client instance not found")

        val binaryData = Base64.decode(fileData, Base64.DEFAULT)

        val builder = AttachRequest.newBuilder()
        builder.id = id ?: UUID.randomUUID().toString()
        builder.filename = filename ?: "file"
        builder.setTypeValue(type)
        builder.data = ByteString.copyFrom(binaryData)

        val request = builder.build()
        val responseBytes = client.call(
            "ask.AskService.Attach",
            PayloadType.RPC_RELIABLE,
            request.toByteArray(),
            30000L
        )

        val response = AttachResponse.parseFrom(responseBytes)
        return AttachmentResult(
            id = response.id,
            statusCode = response.statusCode,
            errorMessage = response.errorMessage
        )
    }

    suspend fun unregisterStream(tabId: String, streamId: String) {
        val handler = handles[tabId] ?: throw Exception("Handler not found")
        val contextBridge = handler.streamInfos?.remove(streamId)
        contextBridge?.unregisterStream(streamId)
    }

    suspend fun startVoice(
        tabId: String,
        streamId: String,
        messageId: String,
        sessionId: String,
        textResponseStreamId: String,
        voiceResponseStreamId: String,
        attachmentIds: List<String>?,
        location: LocationData?
    ): VoiceStartResult {
        val client = clients[tabId] ?: throw Exception("Client not found")

        val builder = StartVoiceMessage.newBuilder()
        builder.streamId = streamId
        builder.messageId = messageId
        builder.sessionId = sessionId
        builder.textResponseStreamId = textResponseStreamId
        builder.voiceResponseStreamId = voiceResponseStreamId

        attachmentIds?.forEach { id ->
            builder.addAttachmentIds(id)
        }

        location?.let {
            val locationBuilder = Location.newBuilder()
            locationBuilder.latitude = it.latitude
            locationBuilder.longitude = it.longitude
            locationBuilder.address = it.address
            locationBuilder.placeName = it.placeName
            builder.location = locationBuilder.build()
        }

        if (builder.streamId.isEmpty()) {
            throw Exception("Missing required field (streamId)")
        }

        val request = builder.build()
        val responseBytes = client.call(
            "data_stream_app.AskLocalService.StartSendVoice",
            PayloadType.RPC_RELIABLE,
            request.toByteArray(),
            20000L
        )

        val response = StartVoiceMessageResponse.parseFrom(responseBytes)
        return VoiceStartResult(
            sessionId = response.sessionId,
            questionId = response.questionId,
            text = response.text,
            statusCode = response.statusCode,
            errorMessage = response.errorMessage
        )
    }

    suspend fun sendVoiceChunk(
        tabId: String,
        streamId: String,
        sessionId: String,
        messageId: String,
        chunkId: String,
        metadata: Map<String, String>?,
        audioData: String
    ): VoiceChunkResult {
        val client = clients[tabId] ?: throw Exception("Client not found")

        val builder = VoiceChunkMessage.newBuilder()
        builder.streamId = streamId
        builder.sessionId = sessionId
        builder.messageId = messageId
        builder.chunkId = chunkId

        metadata?.forEach { (key, value) ->
            builder.putMetadata(key, value)
        }

        val bytes = Base64.decode(audioData, Base64.DEFAULT)
        builder.chunkData = ByteString.copyFrom(bytes)

        val request = builder.build()
        val responseBytes = client.call(
            "data_stream_app.AskLocalService.SendVoiceChunk",
            PayloadType.RPC_RELIABLE,
            request.toByteArray(),
            20000L
        )

        val response = VoiceChunkMessageResponse.parseFrom(responseBytes)
        return VoiceChunkResult(
            streamId = response.streamId,
            status = response.status
        )
    }

    fun shutdown(tabId: String) {
        clients[tabId]?.shutdown()
        clients.remove(tabId)
        handles.remove(tabId)
    }

    fun hasClient(tabId: String): Boolean {
        return clients.containsKey(tabId)
    }

    fun hasHandler(tabId: String): Boolean {
        return handles.containsKey(tabId)
    }

    data class LocationData(
        val latitude: Double,
        val longitude: Double,
        val address: String,
        val placeName: String
    )

    data class AttachmentResult(
        val id: String,
        val statusCode: Int,
        val errorMessage: String
    )

    data class VoiceStartResult(
        val sessionId: String,
        val questionId: String,
        val text: String,
        val statusCode: Int,
        val errorMessage: String
    )

    data class VoiceChunkResult(
        val streamId: String,
        val status: Int
    )
}
