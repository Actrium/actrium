package io.actrium.app.utils

import android.content.Context
import kotlin.math.ceil
import kotlin.math.roundToInt

object GridViewColumnCalculator {
    private const val ITEM_WIDTH = 260f
    private const val ITEM_HEIGHT = 160f
    private const val ITEM_SPACING_DP = 20f
    private val HEIGHT_OVER_WIDTH = ITEM_HEIGHT / ITEM_WIDTH

    fun calculateNumColumns(
        context: Context,
        gridWidthPx: Int,
        gridHeightPx: Int,
        itemCount: Int,
        ratio: Float = 1.0f
    ): Int {
        if (gridWidthPx <= 0 || gridHeightPx <= 0 || itemCount <= 0) return 1
        val spacingPx = dpToPx(context, ITEM_SPACING_DP)
        var fallbackColumns = 1

        for (columns in 1..itemCount) {
            val availableWidth = gridWidthPx - spacingPx * (columns - 1)
            if (availableWidth <= 0f) break

            val itemWidth = availableWidth / columns
            val itemHeight = itemWidth / ratio

            val rows = ceil(itemCount / columns.toDouble()).toInt()
            val requiredHeight = rows * itemHeight + spacingPx * (rows - 1)

            fallbackColumns = columns
            if (requiredHeight <= gridHeightPx) {
                return columns
            }
        }
        return fallbackColumns
    }

    private fun dpToPx(context: Context, dp: Float): Float {
        val density = context.resources.displayMetrics.density
        return (dp * density).roundToInt().toFloat()
    }
}
