package io.actrium.app.core

import android.content.Context
import android.util.Base64
import android.util.Log
import ask.Ask.AttachRequest
import ask.Ask.AttachResponse
import ask.Ask.Location
import com.facebook.soloader.LogUtil
import com.google.protobuf.ByteString
import actrium.Local.StartVoiceMessage
import actrium.Local.StartVoiceMessageResponse
import actrium.Local.TextMessage
import actrium.Local.VoiceChunkMessage
import actrium.Local.VoiceChunkMessageResponse
import io.actor_rtc.actr.ActrRefWrapper
import io.actor_rtc.actr.ActrType
import io.actor_rtc.actr.PayloadType
import io.actor_rtc.actr.dsl.ActrSystem
import io.actor_rtc.actr.dsl.NetworkMonitor
import io.actor_rtc.actr.dsl.createActrSystem
import io.actrium.app.datastream.MyUnifiedHandler
import io.actrium.app.datastream.UnifiedWorkload
import io.actrium.app.rust.ActrActorTypeInfo
import io.actrium.app.rust.ActriumCore
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import java.util.UUID

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
    private val clientSystems = mutableMapOf<String, ActrSystem>()
    private val networkMonitors = mutableMapOf<String, NetworkMonitor>()
    private val contract = ActriumCore.actrBridgeContract()

    private val networkEventHandles = mutableMapOf<String, io.actor_rtc.actr.dsl.NetworkEventHandle>()

    suspend fun initialize(context: Context, tabId: String, actorType: ActrActorTypeInfo, path: String) {
        LogUtil.e("ActrManager", "RTC initialized__${path}")
        val clientSystem = createActrSystem(path)
        clientSystems[tabId] = clientSystem
        val handler = MyUnifiedHandler()
        val clientWorkload = UnifiedWorkload(handler)
        val clientNode = clientSystem.attach(clientWorkload)
        val networkEventHandle = clientNode.createNetworkEventHandle()
        networkEventHandles[tabId] = networkEventHandle
        val scope = CoroutineScope(Job() + Dispatchers.Main)
        val networkMonitor = NetworkMonitor.createWithHandle(context, scope, { networkEventHandles[tabId] }, { msg ->
            Log.e("ActrManager", "networkMonitor:${msg}")
        })
        val clientRef = clientNode.start()
        Log.e("ActrManager", "publisher:${actorType.publisher} name:${actorType.name}")
        val serverType = ActrType(
            publisher = actorType.publisher,
            name = actorType.name,
            version = actorType.version
        )
        val serverActorIds = clientRef.discover(serverType, 1u)
        handler.setServerActId(serverActorIds[0])
        clients[tabId] = clientRef
        handles[tabId] = handler
        networkMonitors[tabId] = networkMonitor
        networkMonitor.startMonitoring()
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
            contract.askLocalServiceSendTextRouteKey,
            PayloadType.RPC_RELIABLE,
            request.toByteArray(),
            contract.androidAskLocalServiceRpcTimeoutMs.toLong()
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
            contract.askServiceAttachRouteKey,
            PayloadType.RPC_RELIABLE,
            request.toByteArray(),
            contract.androidAskServiceAttachTimeoutMs.toLong()
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
            contract.askLocalServiceStartSendVoiceRouteKey,
            PayloadType.RPC_RELIABLE,
            request.toByteArray(),
            contract.androidAskLocalServiceRpcTimeoutMs.toLong()
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
            contract.askLocalServiceSendVoiceChunkRouteKey,
            PayloadType.RPC_RELIABLE,
            request.toByteArray(),
            contract.androidAskLocalServiceRpcTimeoutMs.toLong()
        )

        val response = VoiceChunkMessageResponse.parseFrom(responseBytes)
        return VoiceChunkResult(
            streamId = response.streamId,
            status = response.status
        )
    }

    fun shutdown(tabId: String) {
        Log.e("ActrManager", "shutdown  ${tabId}")
        networkMonitors[tabId]?.stopMonitoring()
        networkMonitors.remove(tabId)
        networkEventHandles.remove(tabId)
        clientSystems.remove(tabId)
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
