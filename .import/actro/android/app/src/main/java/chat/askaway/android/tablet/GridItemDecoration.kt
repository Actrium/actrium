package chat.askaway.android.tablet

import android.graphics.Rect
import android.view.View
import android.view.ViewGroup
import androidx.recyclerview.widget.GridLayoutManager
import androidx.recyclerview.widget.RecyclerView

class GridItemDecoration(
    private val aspectRatio: Float
) : RecyclerView.ItemDecoration() {

    override fun getItemOffsets(
        outRect: Rect,
        view: View,
        parent: RecyclerView,
        state: RecyclerView.State
    ) {
        super.getItemOffsets(outRect, view, parent, state)

        val layoutManager = parent.layoutManager as? GridLayoutManager ?: return
        val spanCount = layoutManager.spanCount

        // 计算 item 的宽度
        val parentWidth = parent.width - parent.paddingLeft - parent.paddingRight
        val itemWidth = parentWidth / spanCount

        // 根据宽高比计算高度
        val itemHeight = (itemWidth / aspectRatio).toInt()

        // 设置 item 的高度
        val layoutParams = view.layoutParams
        if (layoutParams.height != itemHeight) {
            layoutParams.width = ViewGroup.LayoutParams.MATCH_PARENT
            layoutParams.height = itemHeight
            view.layoutParams = layoutParams
        }
    }
}