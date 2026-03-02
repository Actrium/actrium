package com.askaway.android.core

import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.ReactContextBaseJavaModule
import com.facebook.react.bridge.ReactMethod

class TabModule(reactContext: ReactApplicationContext) : ReactContextBaseJavaModule(reactContext) {
    override fun getName(): String {
        return "TabModule"
    }

    @ReactMethod
    fun exitFullscreen(){
        if(reactApplicationContext.hasCurrentActivity()){
            reactApplicationContext.currentActivity?.runOnUiThread {
                reactApplicationContext.currentActivity?.onBackPressed()
            }
        }
    }
}