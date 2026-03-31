package chat.askaway.android.tablet

import android.content.Context
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.PagerSnapHelper
import androidx.recyclerview.widget.RecyclerView

class TabPagerLayoutManager(context: Context) : LinearLayoutManager(context, HORIZONTAL, false) {

    private var pagerSnapHelper: PagerSnapHelper? = null

    override fun onAttachedToWindow(view: RecyclerView?) {
        super.onAttachedToWindow(view)
        if (pagerSnapHelper == null && view != null) {
            pagerSnapHelper = PagerSnapHelper()
            pagerSnapHelper?.attachToRecyclerView(view)
        }
    }

    override fun canScrollHorizontally(): Boolean {
        return false
    }

    override fun canScrollVertically(): Boolean {
        return false
    }
}
