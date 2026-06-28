package chat.askaway.android.tablet

import android.content.Context
import android.content.res.Configuration
import androidx.recyclerview.widget.GridLayoutManager

class TabGridLayoutManager(
    private val ctx: Context,
    private var itemAspectRatio: Float = 16f / 9f
) : GridLayoutManager(ctx, getSpanCount(ctx)) {

    companion object {
        fun getSpanCount(context: Context): Int {
            return if (context.resources.configuration.orientation == Configuration.ORIENTATION_LANDSCAPE) 4 else 3
        }
    }

    fun updateAspectRatio(newAspectRatio: Float) {
        if (itemAspectRatio != newAspectRatio) {
            itemAspectRatio = newAspectRatio
            requestLayout()
        }
    }

    override fun onLayoutChildren(recycler: androidx.recyclerview.widget.RecyclerView.Recycler?, state: androidx.recyclerview.widget.RecyclerView.State?) {
        spanCount = getSpanCount(ctx)
        super.onLayoutChildren(recycler, state)
    }
}