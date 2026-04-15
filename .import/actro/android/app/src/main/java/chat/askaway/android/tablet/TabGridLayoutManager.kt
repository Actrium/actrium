package chat.askaway.android.tablet

import android.content.Context
import androidx.recyclerview.widget.GridLayoutManager
import kotlin.math.ceil
import kotlin.math.sqrt

class TabGridLayoutManager(
    context: Context,
    private var itemAspectRatio: Float = 16f / 9f
) : GridLayoutManager(context, 2) {

    fun updateAspectRatio(newAspectRatio: Float) {
        if (itemAspectRatio != newAspectRatio) {
            itemAspectRatio = newAspectRatio
            requestLayout()
        }
    }

    override fun onLayoutChildren(recycler: androidx.recyclerview.widget.RecyclerView.Recycler?, state: androidx.recyclerview.widget.RecyclerView.State?) {
        // 计算最佳列数
        val itemCount = itemCount
        if (itemCount > 0) {
            val availableWidth = width - paddingLeft - paddingRight
            val availableHeight = height - paddingTop - paddingBottom
            val columnCount = calculateColumnCount(itemCount, availableWidth, availableHeight)
            spanCount = columnCount
        }
        super.onLayoutChildren(recycler, state)
    }

    private fun calculateColumnCount(itemCount: Int, availableWidth: Int, availableHeight: Int): Int {
        if (itemCount == 0 || availableWidth <= 0 || availableHeight <= 0) return 2

        // 从平方根开始尝试
        val sqrtCount = sqrt(itemCount.toDouble()).toInt().coerceAtLeast(2)
        var bestColumnCount = 2

        // 尝试不同的列数，找到最合适的
        for (cols in sqrtCount..itemCount.coerceAtMost(6)) {
            val rows = ceil(itemCount.toFloat() / cols).toInt()

            // 计算每个 item 的宽度（考虑 margin）
            val marginPerItem = 16 // 8dp margin on each side = 16dp total
            val itemWidth = (availableWidth / cols) - marginPerItem

            // 根据宽高比计算高度
            val itemHeight = (itemWidth / itemAspectRatio).toInt()
            val totalHeight = rows * (itemHeight + marginPerItem)

            if (totalHeight <= availableHeight) {
                bestColumnCount = cols
                break
            }
        }

        return bestColumnCount.coerceAtLeast(2)
    }
}