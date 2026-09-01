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

        // item Fixed width = (Available width - Column Spacing) / Number of columns
        val itemWidth = (parentWidth - spacingPx * (spanCount - 1)) / spanCount

        // Intercept area height = itemWidth / aspectRatioPlus llOption Regional height
        val itemHeight = (itemWidth / aspectRatio).toInt() + optionHeightPx

        // Settings item Fixed width and height
        val layoutParams = view.layoutParams
        if (layoutParams.width != itemWidth || layoutParams.height != itemHeight) {
            layoutParams.width = itemWidth
            layoutParams.height = itemHeight
            view.layoutParams = layoutParams
        }

        // Column spacing: Left column without left space, right column without right space, and middle column on each side spacingPx/2
        val position = parent.getChildAdapterPosition(view)
        val column = position % spanCount

        outRect.left = if (column == 0) 0 else spacingPx / 2
        outRect.right = if (column == spanCount - 1) 0 else spacingPx / 2
        outRect.top = 0
        outRect.bottom = 0
    }
}