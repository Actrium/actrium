package io.actrium.actro.utils

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

            // When the keyboard pops up, flSearch Adds the subview below padding
            if (imeHeight > 0) {
                setPadding(0, 0, 0, imeHeight)
            } else {
                setPadding(0, 0, 0, 0)
            }
            // Consumption insets To Prevent Default Processing
            WindowInsets.CONSUMED
        }
    } else {
        // Android 5.0-10 Compatibility treatment
        viewTreeObserver.addOnGlobalLayoutListener {
            val rect = Rect()
            rootView.getWindowVisibleDisplayFrame(rect)
            val heightDiff = rootView.height - rect.bottom
            if (heightDiff > 100) { // Keyboard height greater than 100px
                setPadding(0, 0, 0, heightDiff)
            } else {
                setPadding(0, 0, 0, 0)
            }
        }
    }
}

/**
 * Intercept View It's... Bitmap , and save to cache file.
 *
 * @param fileName  Cache File Name
 * @param scaleFactor Scale factor, default 0.5f(Small heights down by half, pixels down to 1/4) , suitable Grid Thumbnails and Transition Animations
 * @param quality    JPEG Compressed Mass 0-100Default 70
 */
fun View.screenshot(fileName: String, scaleFactor: Float = 0.5f, quality: Int = 70): String? {
    try {
        if (width <= 0 || height <= 0) return null
        val bitmapWidth = (width * scaleFactor).toInt().coerceAtLeast(1)
        val bitmapHeight = (height * scaleFactor).toInt().coerceAtLeast(1)
        val bitmap = createBitmap(bitmapWidth, bitmapHeight, Bitmap.Config.RGB_565)
        val canvas = Canvas(bitmap)
        if (scaleFactor != 1f) {
            canvas.scale(scaleFactor, scaleFactor)
        }
        draw(canvas)
        val cacheDir = context.cacheDir
        val file = File(cacheDir, fileName)

        FileOutputStream(file).use { out ->
            bitmap.compress(Bitmap.CompressFormat.JPEG, quality, out)
        }
        return file.absolutePath
    } catch (e: Exception) {
        e.printStackTrace()
        return null
    }
}