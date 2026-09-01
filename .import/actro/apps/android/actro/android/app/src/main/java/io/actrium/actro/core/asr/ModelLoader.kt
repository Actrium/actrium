package io.actrium.actro.core.asr

import android.content.Context
import io.actrium.actro.rust.AsrModelPlanInfo
import java.io.File
import java.io.FileOutputStream

object ModelLoader {
    private val modelCache = mutableMapOf<String, String>()

    /**
     * 将 assets 中的模型目录拷贝到内部存储，返回模型在文件系统中的绝对路径。
     * 如果已拷贝（标记文件存在），则跳过拷贝直接返回路径。
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
     * 递归拷贝 assets 目录到文件系统。
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
