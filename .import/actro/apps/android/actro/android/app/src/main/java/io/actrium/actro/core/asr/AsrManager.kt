package io.actrium.actro.core.asr

import android.content.Context
import io.actrium.actro.rust.ActroCore
import org.vosk.Model
import org.vosk.Recognizer
import java.io.ByteArrayOutputStream

data class AsrResult(val text: String, val isFinal: Boolean)

object AsrManager {

    private const val TAG = "AsrManager"
    private const val SAMPLE_RATE = 16000f

    private var model: Model? = null
    private var recognizer: Recognizer? = null
    private var audioBuffer = ByteArrayOutputStream()
    private var currentLangDir: String? = null

    /**
     * 根据语种加载模型并创建 Recognizer。
     */
    fun initialize(context: Context, language: String) {
        close()
        val modelPlan = ActroCore.asrModelPlan(language)
        currentLangDir = modelPlan.languageDir

        val modelPath = ModelLoader.getModelPath(context.applicationContext, modelPlan)
        model = Model(modelPath)
        reset()
    }

    /**
     * 接收 PCM 音频片段，流式识别并返回当前结果。
     * acceptWaveForm 返回 true 表示检测到断句，取 getResult；
     * 返回 false 表示仍在说话中，取 getPartialResult。
     * 仅对 final result（断句检测结果）做后处理（分词断句加标点），partial result 不处理。
     */
    fun feedAudio(data: ByteArray): AsrResult {
        val rec = recognizer ?: throw IllegalStateException("AsrManager not initialized, call initialize() first")

        audioBuffer.write(data)
        val isEndpoint = rec.acceptWaveForm(data, data.size)

        return if (isEndpoint) {
            val rawText = parseResultText(rec.result)
            val processedText = postProcessIfNeeded(rawText)
            AsrResult(processedText, true)
        } else {
            AsrResult(parsePartialText(rec.partialResult), false)
        }
    }

    /**
     * 获取当前最终识别文本。
     * 用 audioBuffer 全量数据重建 Recognizer 重新识别，确保结果最准确。
     */
    fun getResult(): String {
        val rec = recognizer ?: throw IllegalStateException("AsrManager not initialized, call initialize() first")

        val fullAudio = audioBuffer.toByteArray()
        val rawText = if (fullAudio.isNotEmpty()) {
            val previousRecognizer = recognizer
            val newRec = Recognizer(model!!, SAMPLE_RATE)
            try {
                newRec.acceptWaveForm(fullAudio, fullAudio.size)
                val finalResult = newRec.finalResult
                previousRecognizer?.close()
                recognizer = newRec
                parseResultText(finalResult)
            } catch (e: Exception) {
                newRec.close()
                throw e
            }
        } else {
            parseResultText(rec.finalResult)
        }

        return postProcessIfNeeded(rawText)
    }

    /**
     * 清空音频缓冲区，重置 Recognizer（新一轮识别）。
     */
    fun reset() {
        audioBuffer = ByteArrayOutputStream()
        recognizer?.close()
        recognizer = model?.let { Recognizer(it, SAMPLE_RATE) }
    }

    /**
     * 释放资源。
     */
    fun close() {
        recognizer?.close()
        model?.close()
        recognizer = null
        model = null
        currentLangDir = null
        audioBuffer = ByteArrayOutputStream()
    }

    /**
     * 仅对中文语种的最终结果做后处理（分词断句加标点），英文或其他语种跳过。
     */
    private fun postProcessIfNeeded(text: String): String {
        if (text.isBlank()) return text
        val langDir = currentLangDir ?: return text
        return if (langDir == "zh") TextPostProcessor.process(text) else text
    }

    /**
     * 解析 getResult / getFinalResult 返回的 JSON，格式：{"text": "你好"}
     */
    private fun parseResultText(json: String): String {
        return ActroCore.parseVoskResultText(json)
    }

    /**
     * 解析 getPartialResult 返回的 JSON，格式：{"partial": "你"}
     */
    private fun parsePartialText(json: String): String {
        return ActroCore.parseVoskPartialText(json)
    }
}
