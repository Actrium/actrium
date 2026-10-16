package io.actrium.app.core

import android.util.Base64
import android.util.Log
import io.actrium.app.core.asr.AsrManager
import io.actrium.app.core.asr.AsrResult
import com.facebook.react.bridge.Arguments
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.ReactContextBaseJavaModule
import com.facebook.react.bridge.ReactMethod
import com.facebook.react.bridge.Promise
import com.facebook.react.modules.core.DeviceEventManagerModule
import io.actrium.app.rust.ActriumCore

class AsrModule(reactContext: ReactApplicationContext) : ReactContextBaseJavaModule(reactContext) {

    companion object {
        private const val TAG = "AsrModule"
    }

    private val contract = ActriumCore.asrBridgeContract()

    override fun getName(): String = contract.moduleName

    /**
     * Set and initialize the voice language ASR Engine.
     * @param language Language code, e.g. "zh-CN", "en-US"
     */
    @ReactMethod
    fun setLanguage(language: String, promise: Promise) {
        try {
            AsrManager.initialize(reactApplicationContext, language)
            promise.resolve(null)
        } catch (e: Exception) {
            Log.e(TAG, "setLanguage failed", e)
            promise.reject(contract.errorInit, e.message, e)
        }
    }

    /**
     * Transfer Audio Data (Base64 Encoding string.
     * Decode and send ASR Engine, send identification results through the event.
     * @param audioData Base64 Encoded PCM Audio Data
     */
    @ReactMethod
    fun feedAudio(audioData: String, promise: Promise) {
        try {
            val pcmData = Base64.decode(audioData, Base64.DEFAULT)
            val result: AsrResult = AsrManager.feedAudio(pcmData)
            sendResultEvent(result)
            promise.resolve(null)
        } catch (e: Exception) {
            Log.e(TAG, "feedAudio failed", e)
            promise.reject(contract.errorFeed, e.message, e)
        }
    }

    /**
     * Stop recording and get the final identification text.
     * Send final results through events and reset Recognizer.
     */
    @ReactMethod
    fun stop(promise: Promise) {
        try {
            val finalText = AsrManager.getResult()
            sendResultEvent(AsrResult(finalText, true))
            AsrManager.reset()
            promise.resolve(finalText)
        } catch (e: Exception) {
            Log.e(TAG, "stop failed", e)
            promise.reject(contract.errorStop, e.message, e)
        }
    }

    private fun sendResultEvent(result: AsrResult) {
        val map = Arguments.createMap().apply {
            putString(contract.keyText, result.text)
            putBoolean(contract.keyIsFinal, result.isFinal)
        }
        reactApplicationContext
            .getJSModule(DeviceEventManagerModule.RCTDeviceEventEmitter::class.java)
            .emit(contract.resultEventName, map)
    }
}
