package com.askaway.android.core

import android.util.Log
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.ReactContextBaseJavaModule
import com.facebook.react.bridge.ReactMethod
import com.facebook.react.bridge.ReadableMap
import org.greenrobot.eventbus.EventBus
import com.askaway.android.event.ReactModuleEvent

class TabModule(reactContext: ReactApplicationContext) : ReactContextBaseJavaModule(reactContext) {
    override fun getName(): String {
        return "TabModule"
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
}