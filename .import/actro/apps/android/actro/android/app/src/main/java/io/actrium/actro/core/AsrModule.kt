package io.actrium.actro.core

import android.util.Base64
import android.util.Log
import io.actrium.actro.core.asr.AsrManager
import io.actrium.actro.core.asr.AsrResult
import com.facebook.react.bridge.Arguments
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.ReactContextBaseJavaModule
import com.facebook.react.bridge.ReactMethod
import com.facebook.react.bridge.Promise
import com.facebook.react.modules.core.DeviceEventManagerModule
import io.actrium.actro.rust.ActroCore

class AsrModule(reactContext: ReactApplicationContext) : ReactContextBaseJavaModule(reactContext) {

    companion object {
        private const val TAG = "AsrModule"
    }

    private val contract = ActroCore.asrBridgeContract()

    override fun getName(): String = contract.moduleName

    /**
     * 设置语音语种并初始化 ASR 引擎。
     * @param language 语种代码，如 "zh-CN"、"en-US"
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
     * 传输音频数据（Base64 编码字符串）。
     * 解码后送入 ASR 引擎，通过事件发送识别结果。
     * @param audioData Base64 编码的 PCM 音频数据
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
     * 停止录音，获取最终识别文本。
     * 通过事件发送最终结果，并重置 Recognizer。
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
