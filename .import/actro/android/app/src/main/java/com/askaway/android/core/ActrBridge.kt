package com.askaway.android.core

import android.util.Base64
import android.util.Log
import com.facebook.react.bridge.Promise
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.ReactContextBaseJavaModule
import com.facebook.react.bridge.ReactMethod
import com.facebook.react.bridge.ReadableMap
import com.facebook.react.bridge.WritableMap
import com.facebook.react.bridge.Arguments
import com.facebook.soloader.LogUtil
import com.facebook.react.modules.core.DeviceEventManagerModule
import io.actor_rtc.actr.ContextBridge
import io.actor_rtc.actr.RpcEnvelopeBridge
import io.actor_rtc.actr.WorkloadBridge
import io.actr.datastreamapp.MyUnifiedHandler
import io.actr.datastreamapp.UnifiedWorkload
import io.actor_rtc.actr.dsl.createActrSystem
import kotlinx.coroutines.GlobalScope
import kotlinx.coroutines.launch
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
import com.askaway.android.global.Params
import ask.Ask.Location
import data_stream_app.Local.StartVoiceMessage
import data_stream_app.Local.StartVoiceMessageResponse
import data_stream_app.Local.VoiceChunkMessage
import data_stream_app.Local.VoiceChunkMessageResponse

class ActrBridge(reactContext: ReactApplicationContext) : ReactContextBaseJavaModule(reactContext) {

    companion object {
        private var instance: ActrBridge? = null

        fun getInstance(): ActrBridge? {
            return instance
        }
    }

    private val clients = mutableMapOf<String, ActrRefWrapper>() // Map of tabId to Actr client instances
    private val handles = mutableMapOf<String, MyUnifiedHandler>() // Map of tabId to UnifiedHandler instances

    init {
        instance = this
    }

    fun sendStreamEventToRN(body: WritableMap?) {
        if (reactApplicationContext.hasActiveReactInstance()) {
            reactApplicationContext
                .getJSModule(DeviceEventManagerModule.RCTDeviceEventEmitter::class.java)
                .emit("ActrStreamMessage", body)
        }
    }

    @ReactMethod
    fun addListener(eventName: String) {
        // Required for RN built-in Event Emitter Calls.
    }

    @ReactMethod
    fun removeListeners(count: Int) {
        // Required for RN built-in Event Emitter Calls.
    }

    override fun getName(): String {
        return "ActrBridge"
    }

    override fun invalidate() {
        super.invalidate()
        if (instance == this) {
            instance = null
        }
    }

    @ReactMethod
    fun initializeService(dict: ReadableMap, promise: Promise) {
        Log.e("ActrBridge", "initializeService called with config: $dict")
        val tabId = dict.getString("tabId")
        val actorType = dict.getString("actorType")
        val path = dict.getString("path")
        if (tabId == null || actorType == null || path == null) {
            promise.reject("INVALID_INPUT", "Missing tabId, actorType or path in JSON config")
            return
        }
        GlobalScope.launch {
            try {
                rtcInit(tabId, actorType, path)
                val result: WritableMap = Arguments.createMap()
                result.putString("status", "connected")
                promise.resolve(result)
            } catch (e: Exception) {
                promise.reject("UNKNOWN_ERROR", e.localizedMessage, e)
            }
        }
    }

    @ReactMethod
    fun sendTextMessage(dict: ReadableMap, promise: Promise) {
        val tabId = dict.getString("tabId")
        Log.e("ActrBridge", "sendTextMessage called with: $dict")
        try {
            val builder = TextMessage.newBuilder()
            // Required fields from dict
            builder.messageId = dict.getString("messageId") ?: ""
            builder.sessionId = dict.getString("sessionId") ?: ""
            builder.text = dict.getString("text") ?: ""
            builder.streamId = dict.getString("streamId") ?: ""
            builder.textResponseStreamId = dict.getString("textResponseStreamId") ?: ""
            builder.voiceResponseStreamId = dict.getString("voiceResponseStreamId") ?: ""
            // attachmentIds is an array
            if (dict.hasKey("attachmentIds") && !dict.isNull("attachmentIds")) {
                val attachments = dict.getArray("attachmentIds")
                if (attachments != null) {
                    for (i in 0 until attachments.size()) {
                        val id = attachments.getString(i)
                        if (id != null) builder.addAttachmentIds(id)
                    }
                }
            }
            val request = builder.build()
            // send request to backend or further processing
            GlobalScope.launch {
                try {
                    clients[tabId]?.call(
                        "data_stream_app.AskLocalService.SendText",
                        PayloadType.RPC_RELIABLE,
                        request.toByteArray(),
                        20000L
                    )
                    promise.resolve(null)
                } catch (e: Exception) {
                    try {
                        promise.reject("USR_PROMPT_REQUEST_ERROR", e.localizedMessage, e)
                    } catch (ex: Exception) {
                        Log.e("ActrBridge", "sendTextMessage promise.reject failed", ex)
                    }
                }
            }
        } catch (e: Exception) {
            promise.reject("USR_PROMPT_REQUEST_ERROR", e.localizedMessage, e)
        }
    }

    @ReactMethod
    fun uploadAttachment(dict: ReadableMap, promise: Promise) {
        val tabId = dict.getString("tabId")
        if (tabId == null || !clients.containsKey(tabId)) {
            promise.reject("NOT_CONNECTED", "ActrBridge not initialized for tabId/Client instance lost")
            return
        }

        GlobalScope.launch {
            try {
                val tmpData = dict.getString("fileData")
                if (tmpData == null) {
                    promise.reject("INVALID_INPUT", "Invalid fileData (null or missing)")
                    return@launch
                }

                val binaryData = try {
                    Base64.decode(tmpData, Base64.DEFAULT)
                } catch (e: IllegalArgumentException) {
                    promise.reject("INVALID_INPUT", "Invalid fileData (Base64 decode failed)", e)
                    return@launch
                }

                val builder = AttachRequest.newBuilder()
                builder.id = if (dict.hasKey("id")) dict.getString("id") ?: UUID.randomUUID().toString() else UUID.randomUUID().toString()
                builder.filename = if (dict.hasKey("filename")) dict.getString("filename") ?: "file" else "file"

                val typeInt = if (dict.hasKey("type")) dict.getInt("type") else 0
                builder.setTypeValue(typeInt)
                builder.data = ByteString.copyFrom(binaryData)

                val request = builder.build()

                val client = clients[tabId]
                if (client == null) {
                    promise.reject("NOT_CONNECTED", "Client instance not found")
                    return@launch
                }

                // Assuming "ask.AskService.Attach" is the correct route key consistent with "ask.AskService.Prompt"
                val responseBytes = client.call(
                    "ask.AskService.Attach",
                    PayloadType.RPC_RELIABLE,
                    request.toByteArray(),
                    30000L
                )

                val response = AttachResponse.parseFrom(responseBytes)

                val result = Arguments.createMap()
                result.putString("id", response.id)
                result.putInt("statusCode", response.statusCode)
                result.putString("errorMessage", response.errorMessage)
                promise.resolve(result)
            } catch (e: Exception) {
                try {
                    promise.reject("ACTR_ERROR", e.localizedMessage, e)
                } catch (ex: Exception) {
                    Log.e("ActrBridge", "uploadAttachment promise.reject failed", ex)
                }
            }
        }
    }

    suspend fun rtcInit(tabId: String, actorType: String, path: String){
        LogUtil.e("ActrBridge", "RTC initialized__${path}")
        val clientSystem = createActrSystem(path)
        Log.e("ActrBridge", "init_____1")
        val handler = MyUnifiedHandler()
        Log.e("ActrBridge", "init_____2")
        val clientWorkload = UnifiedWorkload(handler)
        Log.e("ActrBridge", "init_____3")
        val clientNode = clientSystem.attach(clientWorkload)
        Log.e("ActrBridge", "init_____4")
        val clientRef = clientNode.start()
        Log.e("ActrBridge", "init_____5")
        val parts = actorType.split("+")
        Log.e("ActrBridge", "init_____6")
        val manufacturer = parts[0]
        Log.e("ActrBridge", "init_____7")
        val name = parts[1]
        Log.e("ActrBridge", "manufacturer:${manufacturer} name:${name}")
        val serverType = ActrType(manufacturer, name)
        Log.e("ActrBridge", "init_____8")
        val serverActorIds = clientRef.discover(serverType, 1u)
        handler.setServerActId(serverActorIds[0])
        clients[tabId] = clientRef
        handles[tabId] = handler
    }

    @ReactMethod
    fun unregisterStream(dict: ReadableMap, promise: Promise) {
        val tabId = dict.getString("tabId")
        val streamId = dict.getString("streamId")

        if (tabId == null || streamId == null || !handles.containsKey(tabId)) {
            promise.reject("INVALID_INPUT", "Missing tabId, streamId or instance not found")
            return
        }

        GlobalScope.launch {
            try {
                val handler = handles[tabId]
                val contextBridge = handler?.streamInfos?.remove(streamId)
                contextBridge?.unregisterStream(streamId)
                promise.resolve(true)
            } catch (e: Exception) {
                promise.reject("ERROR", e.localizedMessage, e)
            }
        }
    }

    @ReactMethod
    fun startVoice(dict: ReadableMap, promise: Promise) {
        val tabId = dict.getString("tabId")
        if (tabId == null || !clients.containsKey(tabId)) {
            promise.reject("NOT_CONNECTED", "ActrBridge not initialized for tabId")
            return
        }

        GlobalScope.launch {
            try {
                val builder = StartVoiceMessage.newBuilder()
                builder.streamId = dict.getString("streamId") ?: ""
                builder.messageId = dict.getString("messageId") ?: ""
                builder.sessionId = dict.getString("sessionId") ?: ""
                builder.textResponseStreamId = dict.getString("textResponseStreamId") ?: ""
                builder.voiceResponseStreamId = dict.getString("voiceResponseStreamId") ?: ""

                if (dict.hasKey("attachmentIds") && !dict.isNull("attachmentIds")) {
                    val attachments = dict.getArray("attachmentIds")
                    if (attachments != null) {
                        for (i in 0 until attachments.size()) {
                            val id = attachments.getString(i)
                            if (id != null) builder.addAttachmentIds(id)
                        }
                    }
                }

                if (dict.hasKey("location")) {
                    val locationMap = dict.getMap("location")
                    if (locationMap != null) {
                        val locationBuilder = Location.newBuilder()
                        locationBuilder.latitude = if (locationMap.hasKey("latitude")) locationMap.getDouble("latitude") else 0.0
                        locationBuilder.longitude = if (locationMap.hasKey("longitude")) locationMap.getDouble("longitude") else 0.0
                        locationBuilder.address = locationMap.getString("address") ?: ""
                        locationBuilder.placeName = locationMap.getString("placeName") ?: ""
                        builder.location = locationBuilder.build()
                    }
                }

                if (builder.streamId.isEmpty()) {
                    promise.reject("INVALID_INPUT", "Missing required field (streamId)")
                    return@launch
                }

                val request = builder.build()
                val client = clients[tabId]
                // Using route key for StartSendVoice as per Swift's usage of "instance.actr.call(request...)" implying it's handled by UnifiedHandler -> MyUnifiedHandler.start_send_voice
                // In MyUnifiedHandler.kt, start_send_voice is implemented.
                // The route key for local service usually follows a pattern. However, for ActrRefWrapper.call, we typically need the route string or relying on typed call if available.
                // Looking at sendTextMessage implementation: "data_stream_app.AskLocalService.SendText"
                // Assuming "data_stream_app.AskLocalService.StartSendVoice" for StartVoiceMessage based on proto package/service names

                val responseBytes = client!!.call(
                    "data_stream_app.AskLocalService.StartSendVoice",
                    PayloadType.RPC_RELIABLE,
                    request.toByteArray(),
                    20000L // 90 seconds timeout
                )

                val response = StartVoiceMessageResponse.parseFrom(responseBytes)

                val result = Arguments.createMap()
                result.putString("sessionId", response.sessionId)
                result.putString("questionId", response.questionId)
                result.putString("text", response.text)
                result.putInt("statusCode", response.statusCode)
                result.putString("errorMessage", response.errorMessage)
                promise.resolve(result)

            } catch (e: Exception) {
                promise.reject("ACTR_ERROR", e.localizedMessage, e)
            }
        }
    }

    @ReactMethod
    fun sendVoiceChunk(dict: ReadableMap, audioData: String, promise: Promise) {
        val tabId = dict.getString("tabId")
        if (tabId == null || !clients.containsKey(tabId)) {
            promise.reject("NOT_CONNECTED", "ActrBridge not initialized for tabId")
            return
        }

        GlobalScope.launch {
            try {
                val streamId = dict.getString("streamId")
                if (streamId == null) {
                    promise.reject("INVALID_INPUT", "Missing streamId")
                    return@launch
                }

                val builder = VoiceChunkMessage.newBuilder()
                builder.streamId = streamId
                builder.sessionId = dict.getString("sessionId") ?: ""
                builder.messageId = dict.getString("messageId") ?: ""
                builder.chunkId = dict.getString("chunkId") ?: ""

                if (dict.hasKey("metadata")) {
                    val metadataArray = dict.getArray("metadata")
                    if (metadataArray != null) {
                        for (i in 0 until metadataArray.size()) {
                            val entry = metadataArray.getMap(i)
                            if (entry != null && entry.hasKey("key") && entry.hasKey("value")) {
                                val key = entry.getString("key")
                                val value = entry.getString("value")
                                if (key != null && value != null) {
                                    builder.putMetadata(key, value)
                                }
                            }
                        }
                    }
                }

                try {
                     val bytes = Base64.decode(audioData, Base64.DEFAULT)
                     builder.chunkData = ByteString.copyFrom(bytes)
                } catch (e: Exception) {
                     promise.reject("INVALID_INPUT", "Invalid audioData base64", e)
                     return@launch
                }

                val request = builder.build()
                val client = clients[tabId]

                val responseBytes = client!!.call(
                    "data_stream_app.AskLocalService.SendVoiceChunk",
                    PayloadType.RPC_RELIABLE,
                    request.toByteArray(),
                    20000L
                )

                val response = VoiceChunkMessageResponse.parseFrom(responseBytes)

                val result = Arguments.createMap()
                result.putString("streamID", response.streamId)
                result.putInt("status", response.status)
                // Assuming message field exists based on Swift usage response.message
                // If compilation fails, check protobuf definition
                // result.putString("message", response.message)

                promise.resolve(result)
            } catch (e: Exception) {
                promise.reject("ACTR_ERROR", e.localizedMessage, e)
            }
        }
    }
}
