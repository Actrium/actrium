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
import io.actor_rtc.actr.dsl.createActrSystem
import kotlinx.coroutines.GlobalScope
import kotlinx.coroutines.launch

class ActrBridge(reactContext: ReactApplicationContext) : ReactContextBaseJavaModule(reactContext) {
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

    suspend fun rtcInit(tabId: String, actorType: String, path: String){
        LogUtil.e("ActrBridge", "RTC initialized")
        val clientSystem = createActrSystem(path)
//        val handler = MyUnifiedHandler()
//        val clientWorkload = UnifiedWorkload(handler)
        val clientNode = clientSystem.attach(object : WorkloadBridge {
            override suspend fun onStart(ctx: ContextBridge) {
                Log.e("ActrBridge", "Workload onStart called")
            }

            override suspend fun onStop(ctx: ContextBridge) {
                Log.e("ActrBridge", "Workload onStop called")
            }

            override suspend fun dispatch(
                ctx: ContextBridge,
                envelope: RpcEnvelopeBridge
            ): ByteArray {
                Log.e("ActrBridge", "Workload dispatch called with route: ${envelope.routeKey}")
                return ByteArray(0)
            }

        })
        clientNode.start()
    }
}