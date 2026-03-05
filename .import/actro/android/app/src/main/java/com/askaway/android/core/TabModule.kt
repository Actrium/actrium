package com.askaway.android.core

import android.util.Log
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.ReactContextBaseJavaModule
import com.facebook.react.bridge.ReactMethod
import com.facebook.react.bridge.ReadableMap
import org.greenrobot.eventbus.EventBus
import com.askaway.android.event.ReactModuleEvent
import com.askaway.android.event.NativeModuleEvent
import com.askaway.android.global.Params
import com.facebook.react.bridge.Promise
import org.greenrobot.eventbus.Subscribe
import org.greenrobot.eventbus.ThreadMode
import com.facebook.react.modules.core.DeviceEventManagerModule
import com.facebook.react.bridge.Arguments
import com.facebook.react.bridge.WritableMap

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
        if (event.methodName == NativeModuleEvent.EVENT_EXIT_BOOKMARK_CHANGED) {
            val params = Arguments.createMap()
            event.params.forEach { (key, value) ->
                when (value) {
                    is Boolean -> params.putBoolean(key, value)
                    is String -> params.putString(key, value)
                    is Int -> params.putInt(key, value)
                    is Double -> params.putDouble(key, value)
                }
            }
            sendEvent("onTabBookmarkChanged", params)
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
        if (reactApplicationContext.hasCurrentActivity()) {
            reactApplicationContext.currentActivity?.runOnUiThread {
                reactApplicationContext.currentActivity?.onBackPressed()
            }
        }
    }

    @ReactMethod
    fun setTabUnreadStatus(tabId: String, unread: Boolean) {
        EventBus.getDefault().post(
            ReactModuleEvent(
                ReactModuleEvent.EVENT_SET_TAB_UNREAD_STATUS,
                mapOf("tabId" to tabId, "unread" to unread)
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
    fun openTab(dict: ReadableMap) {
        Log.e("TabModule", "openTab called with: $dict")
        EventBus.getDefault().post(
            ReactModuleEvent(ReactModuleEvent.EVENT_OPEN_TAB, dict.toHashMap())
        )
    }

    @ReactMethod
    fun setTabBookmarkStatus(tabId: String, marked: Boolean) {
        Log.e("TabModule", "setTabBookmarkStatus called with tabId: $tabId, marked: $marked")
        EventBus.getDefault().post(
            ReactModuleEvent(
                ReactModuleEvent.EVENT_TOGGLE_MARK_STATUS,
                mapOf("tabId" to tabId, "marked" to marked)
            )
        )
    }
}