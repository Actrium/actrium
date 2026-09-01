package io.actrium.actro.core.asr

import android.content.Context
import io.actrium.actro.rust.AsrModelPlanInfo
import java.io.File
import java.io.FileOutputStream

object ModelLoader {
    private val modelCache = mutableMapOf<String, String>()

    /**
     * Will assets . The model directory is copied to an internal storage and returns the absolute path of the model in the file system.
     * If a copy is made (the tag file exists), skip the copy and return directly to the path.
     */
    fun getModelPath(context: Context, plan: AsrModelPlanInfo): String {
        modelCache[plan.languageDir]?.let { return it }

        val targetDir = File(context.filesDir, plan.storageRelativePath)
        val markerFile = File(targetDir, plan.readyMarker)

        if (!markerFile.exists()) {
            copyAssetDir(context, plan.assetPath, targetDir)
            markerFile.createNewFile()
        }

        val absolutePath = targetDir.absolutePath
        modelCache[plan.languageDir] = absolutePath
        return absolutePath
    }

    /**
     * Recursive Copy assets Directory to file system.
     */
    private fun copyAssetDir(context: Context, assetPath: String, targetDir: File) {
        val assetList = context.assets.list(assetPath) ?: return

        if (assetList.isEmpty()) {
            // It's a file, copy it
            copyAssetFile(context, assetPath, targetDir)
        } else {
            // It's a directory, recurse
            if (!targetDir.exists()) {
                targetDir.mkdirs()
            }
            for (child in assetList) {
                copyAssetDir(context, "$assetPath/$child", File(targetDir, child))
            }
        }
    }

    private fun copyAssetFile(context: Context, assetPath: String, targetFile: File) {
        context.assets.open(assetPath).use { input ->
            FileOutputStream(targetFile).use { output ->
                input.copyTo(output)
            }
        }
    }
}
