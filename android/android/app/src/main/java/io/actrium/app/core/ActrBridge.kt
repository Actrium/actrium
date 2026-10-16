package io.actrium.app.core

import android.util.Log
import com.facebook.react.bridge.Promise
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.ReactContextBaseJavaModule
import com.facebook.react.bridge.ReactMethod
import com.facebook.react.bridge.ReadableMap
import com.facebook.react.bridge.WritableMap
import com.facebook.react.bridge.Arguments
import com.facebook.react.modules.core.DeviceEventManagerModule
import io.actrium.app.rust.ActriumCore
import io.actrium.app.rust.CoreException
import kotlinx.coroutines.GlobalScope
import kotlinx.coroutines.launch
import io.actrium.app.core.ActrManager

class ActrBridge(val reactContext: ReactApplicationContext) : ReactContextBaseJavaModule(reactContext) {

    companion object {
        private var instance: ActrBridge? = null

        fun getInstance(): ActrBridge? {
            return instance
        }
    }

    private val actrManager = ActrManager.getInstance()
    private val contract = ActriumCore.actrBridgeContract()

    init {
        instance = this
    }

    fun sendStreamEventToRN(body: WritableMap?) {
        if (reactApplicationContext.hasActiveReactInstance()) {
            reactApplicationContext
                .getJSModule(DeviceEventManagerModule.RCTDeviceEventEmitter::class.java)
                .emit(contract.streamEventName, body)
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
        return contract.moduleName
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
        val tabId = dict.getString(contract.keyTabId)
        val actorType = dict.getString(contract.keyActorType)
        val path = dict.getString(contract.keyPath)
        if (tabId == null || actorType == null || path == null) {
            promise.reject(contract.errorInvalidInput, "Missing tabId, actorType or path in JSON config")
            return
        }
        val actorTypeInfo = try {
            ActriumCore.parseActrActorType(actorType)
        } catch (e: CoreException) {
            promise.reject(contract.errorInvalidInput, e.localizedMessage, e)
            return
        }
        GlobalScope.launch {
            try {
                actrManager.initialize(reactContext, tabId, actorTypeInfo, path)
                val result: WritableMap = Arguments.createMap()
                result.putString(contract.keyStatus, contract.statusConnected)
                promise.resolve(result)
            } catch (e: Exception) {
                promise.reject(contract.errorUnknown, e.localizedMessage, e)
            }
        }
    }

    @ReactMethod
    fun sendTextMessage(dict: ReadableMap, promise: Promise) {
        val tabId = dict.getString(contract.keyTabId)
        Log.e("ActrBridge", "sendTextMessage called with: $dict")

        if (tabId == null) {
            promise.reject(contract.errorInvalidInput, "Missing tabId")
            return
        }

        try {
            val messageId = dict.getString(contract.keyMessageId) ?: ""
            val sessionId = dict.getString(contract.keySessionId) ?: ""
            val text = dict.getString(contract.keyText) ?: ""
            val streamId = dict.getString(contract.keyStreamId) ?: ""
            val textResponseStreamId = dict.getString(contract.keyTextResponseStreamId) ?: ""
            val voiceResponseStreamId = dict.getString(contract.keyVoiceResponseStreamId) ?: ""

            val attachmentIds = mutableListOf<String>()
            if (dict.hasKey(contract.keyAttachmentIds) && !dict.isNull(contract.keyAttachmentIds)) {
                val attachments = dict.getArray(contract.keyAttachmentIds)
                if (attachments != null) {
                    for (i in 0 until attachments.size()) {
                        val id = attachments.getString(i)
                        if (id != null) attachmentIds.add(id)
                    }
                }
            }

            var location: ActrManager.LocationData? = null
            if (dict.hasKey(contract.keyLocation) && !dict.isNull(contract.keyLocation)) {
                val locationDict = dict.getMap(contract.keyLocation)
                if (locationDict != null) {
                    location = ActrManager.LocationData(
                        latitude = locationDict.getDouble(contract.keyLatitude),
                        longitude = locationDict.getDouble(contract.keyLongitude),
                        address = locationDict.getString(contract.keyAddress) ?: "",
                        placeName = locationDict.getString(contract.keyPlaceName) ?: ""
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
                        promise.reject(contract.errorPromptRequest, e.localizedMessage, e)
                    } catch (ex: Exception) {
                        Log.e("ActrBridge", "sendTextMessage promise.reject failed", ex)
                    }
                }
            }
        } catch (e: Exception) {
            promise.reject(contract.errorPromptRequest, e.localizedMessage, e)
        }
    }

    @ReactMethod
    fun uploadAttachment(dict: ReadableMap, promise: Promise) {
        val tabId = dict.getString(contract.keyTabId)
        if (tabId == null || !actrManager.hasClient(tabId)) {
            promise.reject(contract.errorNotConnected, "ActrBridge not initialized for tabId/Client instance lost")
            return
        }

        GlobalScope.launch {
            try {
                val fileData = dict.getString(contract.keyFileData)
                if (fileData == null) {
                    promise.reject(contract.errorInvalidInput, "Invalid fileData (null or missing)")
                    return@launch
                }

                val id = if (dict.hasKey(contract.keyId)) dict.getString(contract.keyId) else null
                val filename = if (dict.hasKey(contract.keyFilename)) dict.getString(contract.keyFilename) else null
                val type = if (dict.hasKey(contract.keyType)) dict.getInt(contract.keyType) else 0

                val result = actrManager.uploadAttachment(tabId, id, filename, type, fileData)

                val resultMap = Arguments.createMap()
                resultMap.putString(contract.keyId, result.id)
                resultMap.putInt(contract.keyStatusCode, result.statusCode)
                resultMap.putString(contract.keyErrorMessage, result.errorMessage)
                promise.resolve(resultMap)
            } catch (e: Exception) {
                try {
                    promise.reject(contract.errorActr, e.localizedMessage, e)
                } catch (ex: Exception) {
                    Log.e("ActrBridge", "uploadAttachment promise.reject failed", ex)
                }
            }
        }
    }

    @ReactMethod
    fun unregisterStream(dict: ReadableMap, promise: Promise) {
        val tabId = dict.getString(contract.keyTabId)
        val streamId = dict.getString(contract.keyStreamId)

        if (tabId == null || streamId == null || !actrManager.hasHandler(tabId)) {
            promise.reject(contract.errorInvalidInput, "Missing tabId, streamId or instance not found")
            return
        }

        GlobalScope.launch {
            try {
                actrManager.unregisterStream(tabId, streamId)
                promise.resolve(true)
            } catch (e: Exception) {
                promise.reject(contract.errorGeneric, e.localizedMessage, e)
            }
        }
    }

    @ReactMethod
    fun startVoice(dict: ReadableMap, promise: Promise) {
        val tabId = dict.getString(contract.keyTabId)
        if (tabId == null || !actrManager.hasClient(tabId)) {
            promise.reject(contract.errorNotConnected, "ActrBridge not initialized for tabId")
            return
        }

        GlobalScope.launch {
            try {
                val streamId = dict.getString(contract.keyStreamId) ?: ""
                val messageId = dict.getString(contract.keyMessageId) ?: ""
                val sessionId = dict.getString(contract.keySessionId) ?: ""
                val textResponseStreamId = dict.getString(contract.keyTextResponseStreamId) ?: ""
                val voiceResponseStreamId = dict.getString(contract.keyVoiceResponseStreamId) ?: ""

                val attachmentIds = mutableListOf<String>()
                if (dict.hasKey(contract.keyAttachmentIds) && !dict.isNull(contract.keyAttachmentIds)) {
                    val attachments = dict.getArray(contract.keyAttachmentIds)
                    if (attachments != null) {
                        for (i in 0 until attachments.size()) {
                            val id = attachments.getString(i)
                            if (id != null) attachmentIds.add(id)
                        }
                    }
                }

                var location: ActrManager.LocationData? = null
                if (dict.hasKey(contract.keyLocation)) {
                    val locationMap = dict.getMap(contract.keyLocation)
                    if (locationMap != null) {
                        location = ActrManager.LocationData(
                            latitude = if (locationMap.hasKey(contract.keyLatitude)) locationMap.getDouble(contract.keyLatitude) else 0.0,
                            longitude = if (locationMap.hasKey(contract.keyLongitude)) locationMap.getDouble(contract.keyLongitude) else 0.0,
                            address = locationMap.getString(contract.keyAddress) ?: "",
                            placeName = locationMap.getString(contract.keyPlaceName) ?: ""
                        )
                    }
                }

                val result = actrManager.startVoice(
                    tabId, streamId, messageId, sessionId,
                    textResponseStreamId, voiceResponseStreamId,
                    attachmentIds.takeIf { it.isNotEmpty() }, location
                )

                val resultMap = Arguments.createMap()
                resultMap.putString(contract.keySessionId, result.sessionId)
                resultMap.putString(contract.keyQuestionId, result.questionId)
                resultMap.putString(contract.keyText, result.text)
                resultMap.putInt(contract.keyStatusCode, result.statusCode)
                resultMap.putString(contract.keyErrorMessage, result.errorMessage)
                promise.resolve(resultMap)

            } catch (e: Exception) {
                promise.reject(contract.errorActr, e.localizedMessage, e)
            }
        }
    }

    @ReactMethod
    fun sendVoiceChunk(dict: ReadableMap, audioData: String, promise: Promise) {
        val tabId = dict.getString(contract.keyTabId)
        if (tabId == null || !actrManager.hasClient(tabId)) {
            promise.reject(contract.errorNotConnected, "ActrBridge not initialized for tabId")
            return
        }

        GlobalScope.launch {
            try {
                val streamId = dict.getString(contract.keyStreamId)
                if (streamId == null) {
                    promise.reject(contract.errorInvalidInput, "Missing streamId")
                    return@launch
                }

                val sessionId = dict.getString(contract.keySessionId) ?: ""
                val messageId = dict.getString(contract.keyMessageId) ?: ""
                val chunkId = dict.getString(contract.keyChunkId) ?: ""

                val metadata = mutableMapOf<String, String>()
                if (dict.hasKey(contract.keyMetadata)) {
                    val metadataArray = dict.getArray(contract.keyMetadata)
                    if (metadataArray != null) {
                        for (i in 0 until metadataArray.size()) {
                            val entry = metadataArray.getMap(i)
                            if (entry != null && entry.hasKey(contract.keyMetadataKey) && entry.hasKey(contract.keyMetadataValue)) {
                                val key = entry.getString(contract.keyMetadataKey)
                                val value = entry.getString(contract.keyMetadataValue)
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
                resultMap.putString(contract.keyStreamIdUpper, result.streamId)
                resultMap.putInt(contract.keyStatus, result.status)
                promise.resolve(resultMap)
            } catch (e: Exception) {
                promise.reject(contract.errorActr, e.localizedMessage, e)
            }
        }
    }

    @ReactMethod
    fun shutdownService(dict: ReadableMap, promise: Promise) {
        val tabId = dict.getString(contract.keyTabId)
        if (tabId == null || !actrManager.hasClient(tabId)) {
            promise.reject(contract.errorNotConnected, "ActrBridge not initialized for tabId")
            return
        }
        actrManager.shutdown(tabId)
        promise.resolve(null)
    }
}
