package com.askaway.android.core

import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.ReactContextBaseJavaModule
import com.facebook.react.bridge.ReactMethod
import com.facebook.react.bridge.WritableMap
import com.facebook.react.bridge.Arguments
import com.facebook.react.modules.core.DeviceEventManagerModule
import android.util.Log
import chat.askaway.android.wxapi.WxApiHelper
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.cancel
import java.util.UUID

class UserInfoModule(reactContext: ReactApplicationContext) : ReactContextBaseJavaModule(reactContext) {

    companion object {
        private var currentInstanceId: String? = null
    }

    private val wxApiHelper: WxApiHelper = WxApiHelper.getInstance(reactContext.applicationContext)
    private val googleSignInHelper: GoogleSignInHelper = GoogleSignInHelper.getInstance()
    private val scope = CoroutineScope(Dispatchers.Main + SupervisorJob())
    private val instanceId = UUID.randomUUID().toString().substring(0, 8)

    init {
        currentInstanceId = instanceId
        Log.e("UserInfoModule", "[$instanceId] Initialized (Set as current)")

        scope.launch {
            wxApiHelper.authCode.collect { code ->
                if (instanceId != currentInstanceId) {
                    Log.w("UserInfoModule", "[$instanceId] Ignoring auth code (Not current instance)")
                    return@collect
                }

                Log.e("UserInfoModule", "[$instanceId] Collected auth code: $code")
                if (!code.isNullOrEmpty()) {
                    val params = Arguments.createMap().apply {
                        putString("type", "wechat")
                        putString("code", code)
                    }
                    if (reactContext.hasActiveReactInstance()) {
                         sendEvent("onLoginResult", params)
                    } else {
                         Log.e("UserInfoModule", "[$instanceId] ReactContext is not active, ignoring event")
                    }
                }
            }
        }

        scope.launch {
            wxApiHelper.authErr.collect { err ->
                if (instanceId != currentInstanceId) {
                    Log.w("UserInfoModule", "[$instanceId] Ignoring auth error (Not current instance)")
                    return@collect
                }

                Log.e("UserInfoModule", "[$instanceId] Collected auth err: $err")
                if (!err.isNullOrEmpty()) {
                    val params = Arguments.createMap().apply {
                        putString("type", "wechat")
                        putBoolean("error", true)
                        putString("message", err)
                    }
                    if (reactContext.hasActiveReactInstance()) {
                        sendEvent("onLoginResult", params)
                    } else {
                        Log.e("UserInfoModule", "[$instanceId] ReactContext is not active, ignoring event")
                    }
                }
            }
        }

        // 收集 Google 登录结果
        scope.launch {
            googleSignInHelper.idToken.collect { token ->
                if (instanceId != currentInstanceId) return@collect
                if (!token.isNullOrEmpty()) {
                    val params = Arguments.createMap().apply {
                        putString("type", "google")
                        putString("id_token", token)
                    }
                    if (reactContext.hasActiveReactInstance()) {
                        sendEvent("onLoginResult", params)
                    }
                }
            }
        }

        scope.launch {
            googleSignInHelper.authErr.collect { err ->
                if (instanceId != currentInstanceId) return@collect
                if (!err.isNullOrEmpty()) {
                    val params = Arguments.createMap().apply {
                        putString("type", "google")
                        putBoolean("error", true)
                        putString("message", err)
                    }
                    if (reactContext.hasActiveReactInstance()) {
                        sendEvent("onLoginResult", params)
                    }
                }
            }
        }
    }

    override fun getName(): String {
        return "UserInfoModule"
    }

    override fun invalidate() {
        Log.e("UserInfoModule", "[$instanceId] invalidate called")
        super.invalidate()
        scope.cancel()
    }

    private fun sendEvent(eventName: String, params: WritableMap?) {
        reactApplicationContext
            .getJSModule(DeviceEventManagerModule.RCTDeviceEventEmitter::class.java)
            .emit(eventName, params)
    }

    @ReactMethod
    fun addListener(eventName: String) {
        // Required for RN built-in Event Emitter Calls.
    }

    @ReactMethod
    fun removeListeners(count: Int) {
        // Required for RN built-in Event Emitter Calls.
    }

    @ReactMethod
    fun loginWithType(type: String) {
        Log.e("UserInfoModule", "loginWithType called with: $type")
        when (type) {
            "wechat" -> wxApiHelper.login()
            "google" -> googleSignInHelper.signIn(currentActivity)
        }
    }
}