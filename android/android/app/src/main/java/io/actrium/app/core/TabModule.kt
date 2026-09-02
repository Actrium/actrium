package io.actrium.app.core

import android.content.Context
import android.view.inputmethod.InputMethodManager
import android.util.Log
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.ReactContextBaseJavaModule
import com.facebook.react.bridge.ReactMethod
import com.facebook.react.bridge.ReadableMap
import org.greenrobot.eventbus.EventBus
import io.actrium.app.event.ReactModuleEvent
import io.actrium.app.event.NativeModuleEvent
import io.actrium.app.global.Params
import com.facebook.react.bridge.Promise
import org.greenrobot.eventbus.Subscribe
import org.greenrobot.eventbus.ThreadMode
import com.facebook.react.modules.core.DeviceEventManagerModule
import com.facebook.react.bridge.Arguments
import com.facebook.react.bridge.WritableMap
import io.actrium.app.MainActivity
import io.actrium.app.utils.RuntimeBridgeParams
import java.util.UUID

class TabModule(reactContext: ReactApplicationContext) : ReactContextBaseJavaModule(reactContext) {
    override fun getName(): String {
        return "TabModule"
    }

    override fun initialize() {
        super.initialize()
        if (!EventBus.getDefault().isRegistered(this)) {
            EventBus.getDefault().register(this)
        }
    }

    override fun invalidate() {
        super.invalidate()
        if (EventBus.getDefault().isRegistered(this)) {
            EventBus.getDefault().unregister(this)
        }
    }

    @Subscribe(threadMode = ThreadMode.MAIN)
    fun onReactModuleEvent(event: NativeModuleEvent) {
        Log.e("TabModule", "Received NativeModuleEvent: ${event.methodName} with params: ${event.params}")
        val params = Arguments.createMap()
        event.params.forEach { (key, value) ->
            putValue(params, key, value)
        }
        sendEvent(event.methodName, params)
    }

    private fun putValue(map: com.facebook.react.bridge.WritableMap, key: String, value: Any?) {
        when (value) {
            is Boolean -> map.putBoolean(key, value)
            is String -> map.putString(key, value)
            is Int -> map.putInt(key, value)
            is Double -> map.putDouble(key, value)
            is Long -> map.putLong(key, value)
            is Map<*, *> -> {
                val nestedMap = Arguments.createMap()
                @Suppress("UNCHECKED_CAST")
                (value as Map<String, Any?>).forEach { (k, v) -> putValue(nestedMap, k, v) }
                map.putMap(key, nestedMap)
            }
        }
    }

    private fun sendEvent(eventName: String, params: WritableMap?) {
        Log.e("TabModule", "Sending event to JS: $eventName with params: $params")
        if (reactApplicationContext.hasActiveReactInstance()) {
            Log.e("TabModule", "ReactContext is active, emitting event: $eventName")
            reactApplicationContext
                .getJSModule(DeviceEventManagerModule.RCTDeviceEventEmitter::class.java)
                .emit(eventName, params)
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

    @ReactMethod
    fun exitFullscreen() {
        EventBus.getDefault().post(ReactModuleEvent(ReactModuleEvent.EVENT_EXIT_FULLSCREEN))
        if (reactApplicationContext.hasCurrentActivity()) {
            reactApplicationContext.currentActivity?.runOnUiThread {
                reactApplicationContext.currentActivity?.onBackPressed()
            }
        }
    }

    @ReactMethod
    fun exitSearch() {
        EventBus.getDefault().post(ReactModuleEvent(ReactModuleEvent.EVENT_EXIT_SEARCH))
        val activity = currentActivity
        if (activity != null && activity is MainActivity) {
            activity.runOnUiThread {
                try {
                    val imm = activity.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
                    val view = activity.currentFocus ?: activity.window.decorView
                    imm.hideSoftInputFromWindow(view.windowToken, 0)
                } catch (e: Exception) {
                    Log.e("TabModule", "Failed to hide keyboard", e)
                }
                activity.onBackPressed()
            }
        }
    }

    @ReactMethod
    fun setTabUnreadStatus(tabId: String, unread: Boolean) {
        EventBus.getDefault().post(
            ReactModuleEvent(
                ReactModuleEvent.EVENT_SET_TAB_UNREAD_STATUS,
                mapOf(
                    RuntimeBridgeParams.tabIdKey to tabId,
                    RuntimeBridgeParams.unreadKey to unread,
                )
            )
        )
    }

    @ReactMethod
    fun updateTabInfo(dict: ReadableMap) {
        Log.e("TabModule", "updateTabInfo called with: $dict")
        EventBus.getDefault().post(
            ReactModuleEvent(
                ReactModuleEvent.EVENT_UPDATE_TAB_INFO,
                dict.toHashMap()
            )
        )
    }

    @ReactMethod
    fun getCurrentFullscreenTab(promise: Promise) {
        Log.e("TabModule", "getCurrentFullscreenTab called, currentTabId: ${Params.currentTabId}")
        promise.resolve(Params.currentTabId)
    }

    @ReactMethod
    fun openTab(dict: ReadableMap, promise: Promise) {
        Log.e("TabModule", "openTab called with: $dict")
        val hashMap = dict.toHashMap()
        if (dict.hasKey(ReactNativeBundle.REACT_NATIVE_VIEW_TAB_TYPE)) {
            val tabId = UUID.randomUUID().toString()
            hashMap.put(RuntimeBridgeParams.openTabTabIdKey, tabId)
            promise.resolve(tabId)
        }
        EventBus.getDefault().post(
            ReactModuleEvent(ReactModuleEvent.EVENT_OPEN_TAB, hashMap)
        )
    }

    @ReactMethod
    fun setTabBookmarkStatus(tabId: String, marked: Boolean) {
        Log.e("TabModule", "setTabBookmarkStatus called with tabId: $tabId, marked: $marked")
        EventBus.getDefault().post(
            ReactModuleEvent(
                ReactModuleEvent.EVENT_TOGGLE_MARK_STATUS,
                mapOf(
                    RuntimeBridgeParams.tabIdKey to tabId,
                    RuntimeBridgeParams.markedKey to marked,
                )
            )
        )
    }

    @ReactMethod
    fun getAllTabs(promise: Promise){
        Log.e("TabModule", "getAllTabs")
        EventBus.getDefault().post(
            ReactModuleEvent(
                ReactModuleEvent.EVENT_GET_ALL_TABS, emptyMap(), promise)
        )
    }
}
