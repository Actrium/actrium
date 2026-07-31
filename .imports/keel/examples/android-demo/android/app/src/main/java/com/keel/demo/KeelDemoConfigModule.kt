package com.keel.demo

import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.ReactContextBaseJavaModule
import com.facebook.react.module.annotations.ReactModule

@ReactModule(name = KeelDemoConfigModule.NAME)
class KeelDemoConfigModule(reactContext: ReactApplicationContext) :
    ReactContextBaseJavaModule(reactContext) {

    companion object {
        const val NAME = "KeelDemoConfig"
    }

    override fun getName(): String = NAME

    override fun getConstants(): Map<String, Any> = mapOf(
        "sandboxEngine" to BuildConfig.SANDBOX_ENGINE
    )
}
