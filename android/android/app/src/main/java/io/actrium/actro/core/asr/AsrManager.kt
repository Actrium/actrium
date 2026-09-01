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
     * Loading Models by Language and Creating Recognizer.
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
     * Receive PCM Audio clips, current recognition and return of the current result.
     * acceptWaveForm Back true Means you've detected a break, take it. getResult;
     * Back false The blogger says he is still talking, and he takes it. getPartialResult.
     * Just right. final result(Closed test results) Post-treatment (terms and postpoints),partial result Don't process.
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
     * Can not get folder: %s: %s
     * Use audioBuffer Full Data Reconstruction Recognizer Re-identify to ensure the most accurate results.
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
     * Clear audio buffer, reset Recognizer(new round of identification)
     */
    fun reset() {
        audioBuffer = ByteArrayOutputStream()
        recognizer?.close()
        recognizer = model?.let { Recognizer(it, SAMPLE_RATE) }
    }

    /**
     * Release resources.
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
     * Reprocessing of the final result of the Chinese language only (terms and postpoints), and skipping of English or other languages.
     */
    private fun postProcessIfNeeded(text: String): String {
        if (text.isBlank()) return text
        val langDir = currentLangDir ?: return text
        return if (langDir == "zh") TextPostProcessor.process(text) else text
    }

    /**
     * Parsing getResult / getFinalResult Returned JSON, format:{"text": "Hello."}
     */
    private fun parseResultText(json: String): String {
        return ActroCore.parseVoskResultText(json)
    }

    /**
     * Parsing getPartialResult Returned JSON, format:{"partial": "You."}
     */
    private fun parsePartialText(json: String): String {
        return ActroCore.parseVoskPartialText(json)
    }
}
