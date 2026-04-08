package com.askaway.android.core

import android.util.Log
import com.facebook.react.bridge.Promise
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.ReactContextBaseJavaModule
import com.facebook.react.bridge.ReactMethod
import com.facebook.react.bridge.ReadableMap
import com.facebook.react.bridge.WritableMap
import com.facebook.react.bridge.Arguments
import com.facebook.react.modules.core.DeviceEventManagerModule
import kotlinx.coroutines.GlobalScope
import kotlinx.coroutines.launch
import chat.askaway.android.core.ActrManager

class ActrBridge(val reactContext: ReactApplicationContext) : ReactContextBaseJavaModule(reactContext) {

    companion object {
        private var instance: ActrBridge? = null

        fun getInstance(): ActrBridge? {
            return instance
        }
    }

    private val actrManager = ActrManager.getInstance()

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
                actrManager.initialize(reactContext, tabId, actorType, path)
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

        if (tabId == null) {
            promise.reject("INVALID_INPUT", "Missing tabId")
            return
        }

        try {
            val messageId = dict.getString("messageId") ?: ""
            val sessionId = dict.getString("sessionId") ?: ""
            val text = dict.getString("text") ?: ""
            val streamId = dict.getString("streamId") ?: ""
            val textResponseStreamId = dict.getString("textResponseStreamId") ?: ""
            val voiceResponseStreamId = dict.getString("voiceResponseStreamId") ?: ""

            val attachmentIds = mutableListOf<String>()
            if (dict.hasKey("attachmentIds") && !dict.isNull("attachmentIds")) {
                val attachments = dict.getArray("attachmentIds")
                if (attachments != null) {
                    for (i in 0 until attachments.size()) {
                        val id = attachments.getString(i)
                        if (id != null) attachmentIds.add(id)
                    }
                }
            }

            var location: ActrManager.LocationData? = null
            if (dict.hasKey("location") && !dict.isNull("location")) {
                val locationDict = dict.getMap("location")
                if (locationDict != null) {
                    location = ActrManager.LocationData(
                        latitude = locationDict.getDouble("latitude"),
                        longitude = locationDict.getDouble("longitude"),
                        address = locationDict.getString("address") ?: "",
                        placeName = locationDict.getString("placeName") ?: ""
                    )
                }
            }

            GlobalScope.launch {
                try {
                    actrManager.sendTextMessage(
                        tabId, messageId, sessionId, text, streamId,
                        textResponseStreamId, voiceResponseStreamId,
                        attachmentIds.takeIf { it.isNotEmpty() }, location
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
        if (tabId == null || !actrManager.hasClient(tabId)) {
            promise.reject("NOT_CONNECTED", "ActrBridge not initialized for tabId/Client instance lost")
            return
        }

        GlobalScope.launch {
            try {
                val fileData = dict.getString("fileData")
                if (fileData == null) {
                    promise.reject("INVALID_INPUT", "Invalid fileData (null or missing)")
                    return@launch
                }

                val id = if (dict.hasKey("id")) dict.getString("id") else null
                val filename = if (dict.hasKey("filename")) dict.getString("filename") else null
                val type = if (dict.hasKey("type")) dict.getInt("type") else 0

                val result = actrManager.uploadAttachment(tabId, id, filename, type, fileData)

                val resultMap = Arguments.createMap()
                resultMap.putString("id", result.id)
                resultMap.putInt("statusCode", result.statusCode)
                resultMap.putString("errorMessage", result.errorMessage)
                promise.resolve(resultMap)
            } catch (e: Exception) {
                try {
                    promise.reject("ACTR_ERROR", e.localizedMessage, e)
                } catch (ex: Exception) {
                    Log.e("ActrBridge", "uploadAttachment promise.reject failed", ex)
                }
            }
        }
    }

    @ReactMethod
    fun unregisterStream(dict: ReadableMap, promise: Promise) {
        val tabId = dict.getString("tabId")
        val streamId = dict.getString("streamId")

        if (tabId == null || streamId == null || !actrManager.hasHandler(tabId)) {
            promise.reject("INVALID_INPUT", "Missing tabId, streamId or instance not found")
            return
        }

        GlobalScope.launch {
            try {
                actrManager.unregisterStream(tabId, streamId)
                promise.resolve(true)
            } catch (e: Exception) {
                promise.reject("ERROR", e.localizedMessage, e)
            }
        }
    }

    @ReactMethod
    fun startVoice(dict: ReadableMap, promise: Promise) {
        val tabId = dict.getString("tabId")
        if (tabId == null || !actrManager.hasClient(tabId)) {
            promise.reject("NOT_CONNECTED", "ActrBridge not initialized for tabId")
            return
        }

        GlobalScope.launch {
            try {
                val streamId = dict.getString("streamId") ?: ""
                val messageId = dict.getString("messageId") ?: ""
                val sessionId = dict.getString("sessionId") ?: ""
                val textResponseStreamId = dict.getString("textResponseStreamId") ?: ""
                val voiceResponseStreamId = dict.getString("voiceResponseStreamId") ?: ""

                val attachmentIds = mutableListOf<String>()
                if (dict.hasKey("attachmentIds") && !dict.isNull("attachmentIds")) {
                    val attachments = dict.getArray("attachmentIds")
                    if (attachments != null) {
                        for (i in 0 until attachments.size()) {
                            val id = attachments.getString(i)
                            if (id != null) attachmentIds.add(id)
                        }
                    }
                }

                var location: ActrManager.LocationData? = null
                if (dict.hasKey("location")) {
                    val locationMap = dict.getMap("location")
                    if (locationMap != null) {
                        location = ActrManager.LocationData(
                            latitude = if (locationMap.hasKey("latitude")) locationMap.getDouble("latitude") else 0.0,
                            longitude = if (locationMap.hasKey("longitude")) locationMap.getDouble("longitude") else 0.0,
                            address = locationMap.getString("address") ?: "",
                            placeName = locationMap.getString("placeName") ?: ""
                        )
                    }
                }

                val result = actrManager.startVoice(
                    tabId, streamId, messageId, sessionId,
                    textResponseStreamId, voiceResponseStreamId,
                    attachmentIds.takeIf { it.isNotEmpty() }, location
                )

                val resultMap = Arguments.createMap()
                resultMap.putString("sessionId", result.sessionId)
                resultMap.putString("questionId", result.questionId)
                resultMap.putString("text", result.text)
                resultMap.putInt("statusCode", result.statusCode)
                resultMap.putString("errorMessage", result.errorMessage)
                promise.resolve(resultMap)

            } catch (e: Exception) {
                promise.reject("ACTR_ERROR", e.localizedMessage, e)
            }
        }
    }

    @ReactMethod
    fun sendVoiceChunk(dict: ReadableMap, audioData: String, promise: Promise) {
        val tabId = dict.getString("tabId")
        if (tabId == null || !actrManager.hasClient(tabId)) {
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

                val sessionId = dict.getString("sessionId") ?: ""
                val messageId = dict.getString("messageId") ?: ""
                val chunkId = dict.getString("chunkId") ?: ""

                val metadata = mutableMapOf<String, String>()
                if (dict.hasKey("metadata")) {
                    val metadataArray = dict.getArray("metadata")
                    if (metadataArray != null) {
                        for (i in 0 until metadataArray.size()) {
                            val entry = metadataArray.getMap(i)
                            if (entry != null && entry.hasKey("key") && entry.hasKey("value")) {
                                val key = entry.getString("key")
                                val value = entry.getString("value")
                                if (key != null && value != null) {
                                    metadata[key] = value
                                }
                            }
                        }
                    }
                }

                val result = actrManager.sendVoiceChunk(
                    tabId, streamId, sessionId, messageId, chunkId,
                    metadata.takeIf { it.isNotEmpty() }, audioData
                )

                val resultMap = Arguments.createMap()
                resultMap.putString("streamID", result.streamId)
                resultMap.putInt("status", result.status)
                promise.resolve(resultMap)
            } catch (e: Exception) {
                promise.reject("ACTR_ERROR", e.localizedMessage, e)
            }
        }
    }

    @ReactMethod
    fun shutdownService(dict: ReadableMap, promise: Promise) {
        val tabId = dict.getString("tabId")
        if (tabId == null || !actrManager.hasClient(tabId)) {
            promise.reject("NOT_CONNECTED", "ActrBridge not initialized for tabId")
            return
        }
        actrManager.shutdown(tabId)
        promise.resolve(null)
    }
}
