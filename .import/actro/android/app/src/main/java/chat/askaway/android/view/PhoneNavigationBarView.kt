package chat.askaway.android.view

import android.content.Context
import android.util.AttributeSet
import android.util.Log
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView
import com.askaway.android.R

/**
 * 自定义标题栏视图，包含标题文字和添加按钮
 */
class PhoneNavigationBarView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
    defStyleAttr: Int = 0
) : LinearLayout(context, attrs, defStyleAttr) {

    val titleTextView :TextView

    init {
        orientation = VERTICAL
        // 只保留view_header作为内容
        val header = View.inflate(context, R.layout.view_header, null)
        val container = header.findViewById<FrameLayout>(R.id.headerContainer)
        // 设置marginTop为状态栏高度
        val statusBarHeight = getStatusBarHeight()
        val lp = header.layoutParams as? MarginLayoutParams ?: MarginLayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
        container.setPadding(0,statusBarHeight, 0,0)
//        lp.topMargin = statusBarHeight
        header.layoutParams = lp
        titleTextView = header.findViewById(R.id.tvTitle)
        addView(header)
    }

    fun setTitle(title: String) {
        titleTextView.text = title
        titleTextView.textSize = 34f
    }

    fun setSubtitle(subtitle: String?) {
        titleTextView.text = subtitle
        titleTextView.textSize = 28f
    }

    fun setShowLargeTitle(show: Boolean) {
        titleTextView.textSize = 34f
    }

    /**
     * 获取状态栏高度
     */
    private fun getStatusBarHeight(): Int {
        var result = 0
        val resourceId = resources.getIdentifier("status_bar_height", "dimen", "android")
        if (resourceId > 0) {
            result = resources.getDimensionPixelSize(resourceId)
        }
        return result
    }
}