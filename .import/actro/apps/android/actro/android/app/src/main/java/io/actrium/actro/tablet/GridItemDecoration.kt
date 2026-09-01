package io.actrium.actro.tablet

import android.graphics.Rect
import android.view.View
import androidx.recyclerview.widget.GridLayoutManager
import androidx.recyclerview.widget.RecyclerView

class GridItemDecoration(
    private val spanCount: Int,
    private val spacingPx: Int,
    private val aspectRatio: Float,
    private val optionHeightPx: Int = 0
) : RecyclerView.ItemDecoration() {

    override fun getItemOffsets(
        outRect: Rect,
        view: View,
        parent: RecyclerView,
        state: RecyclerView.State
    ) {
        super.getItemOffsets(outRect, view, parent, state)

        val layoutManager = parent.layoutManager as? GridLayoutManager ?: return

        val parentWidth = parent.width - parent.paddingLeft - parent.paddingRight

        // item 固定宽度 = (可用宽度 - 列间距) / 列数
        val itemWidth = (parentWidth - spacingPx * (spanCount - 1)) / spanCount

        // 截图区域高度 = itemWidth / aspectRatio，再加上 llOption 区域高度
        val itemHeight = (itemWidth / aspectRatio).toInt() + optionHeightPx

        // 设置 item 的固定宽度和高度
        val layoutParams = view.layoutParams
        if (layoutParams.width != itemWidth || layoutParams.height != itemHeight) {
            layoutParams.width = itemWidth
            layoutParams.height = itemHeight
            view.layoutParams = layoutParams
        }

        // 列间间距：左列无左间距，右列无右间距，中间列两侧各 spacingPx/2
        val position = parent.getChildAdapterPosition(view)
        val column = position % spanCount

        outRect.left = if (column == 0) 0 else spacingPx / 2
        outRect.right = if (column == spanCount - 1) 0 else spacingPx / 2
        outRect.top = 0
        outRect.bottom = 0
    }
}