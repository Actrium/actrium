package com.askaway.android.utils

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Rect
import android.os.Build
import android.view.View
import android.view.WindowInsets
import androidx.core.graphics.createBitmap
import java.io.File
import java.io.FileOutputStream

fun View.adjustKeyboard() {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
        setOnApplyWindowInsetsListener { v, insets ->
            val imeHeight = insets.getInsets(WindowInsets.Type.ime()).bottom

            // 当键盘弹出时，为 flSearch 的子视图添加下方 padding
            if (imeHeight > 0) {
                setPadding(0, 0, 0, imeHeight)
            } else {
                setPadding(0, 0, 0, 0)
            }
            // 消费 insets 以防止默认处理
            WindowInsets.CONSUMED
        }
    } else {
        // Android 5.0-10 的兼容性处理
        viewTreeObserver.addOnGlobalLayoutListener {
            val rect = Rect()
            rootView.getWindowVisibleDisplayFrame(rect)
            val heightDiff = rootView.height - rect.bottom
            if (heightDiff > 100) { // 键盘高度大于 100px
                setPadding(0, 0, 0, heightDiff)
            } else {
                setPadding(0, 0, 0, 0)
            }
        }
    }
}

fun View.screenshot(fileName: String): String? {
    try {
        if (width <= 0 || height <= 0) return null
        val bitmap = createBitmap(width, height, Bitmap.Config.RGB_565)
        val canvas = Canvas(bitmap)
        draw(canvas)
        val cacheDir = context.cacheDir
        val file = File(cacheDir, fileName)

        FileOutputStream(file).use { out ->
            bitmap.compress(Bitmap.CompressFormat.JPEG, 90, out)
        }
        return file.absolutePath
    } catch (e: Exception) {
        e.printStackTrace()
        return null
    }
}