package com.askaway.android.core

import android.util.Log
import com.facebook.react.bridge.Promise
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.ReactContextBaseJavaModule
import com.facebook.react.bridge.ReactMethod
import com.facebook.react.bridge.ReadableMap
import com.facebook.react.bridge.WritableMap
import com.facebook.react.bridge.Arguments
import com.facebook.soloader.LogUtil
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

class ActrBridge(reactContext: ReactApplicationContext) : ReactContextBaseJavaModule(reactContext) {

    private val clients = mutableMapOf<String, ActrRefWrapper>() // Map of tabId to Actr client instances

    override fun getName(): String {
        return "ActrBridge"
    }

    @ReactMethod
    fun initializeService(dict: ReadableMap, promise: Promise) {
        Log.e("ActrBridge", "initializeService called with config: $dict")
//        val tabId = dict.getString("tabId")
        val tabId = "e6c6fefa-51c1-4ac0-baea-4f4ad00fff72"
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
        //val tabId = dict.getString("tabId")
        val tabId = "e6c6fefa-51c1-4ac0-baea-4f4ad00fff72"
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
            // TODO: send request to backend or further processing
            GlobalScope.launch {
                clients[tabId]?.call(
                    "data_stream_app.AskLocalService.SendText",
                    PayloadType.RPC_RELIABLE,
                    request.toByteArray(),
                    30000L
                )
                promise.resolve(null)
            }
        } catch (e: Exception) {
            promise.reject("USR_PROMPT_REQUEST_ERROR", e.localizedMessage, e)
        }
    }

    suspend fun rtcInit(tabId: String, actorType: String, path: String){
        LogUtil.e("ActrBridge", "RTC initialized")
        val clientSystem = createActrSystem(path)
        val handler = MyUnifiedHandler()
        val clientWorkload = UnifiedWorkload(handler)
        val clientNode = clientSystem.attach(clientWorkload)
        val clientRef = clientNode.start()
        val parts = actorType.split("+")
        val manufacturer = parts[0]
        val name = parts[1]
        val serverType = ActrType(manufacturer, name)
        val serverActorIds = clientRef.discover(serverType, 1u)
        handler.setServerActId(serverActorIds[0])
        clients[tabId] = clientRef
    }
}