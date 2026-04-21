package chat.askaway.android.view

import android.content.Context
import android.util.AttributeSet
import android.view.Gravity
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import android.view.View
import com.askaway.android.R

/**
 * 自定义标题栏视图，包含标题文字和添加按钮
 */
class PhoneNavigationBarView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
    defStyleAttr: Int = 0
) : LinearLayout(context, attrs, defStyleAttr) {

    private val titleContainer: LinearLayout
    private val titleTextView: TextView
    private val subtitleTextView: TextView
    private val addButton: ImageView
    var onAddClickListener: (() -> Unit)? = null

    private var showLargeTitle: Boolean = true

    init {
        orientation = HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL

        // 设置背景
        setBackgroundResource(R.drawable.top_gradient)

        // 设置内边距
        val paddingHorizontal = (16 * resources.displayMetrics.density).toInt()
        val paddingTop = (8 * resources.displayMetrics.density).toInt()
        val paddingBottom = (20 * resources.displayMetrics.density).toInt()
        setPadding(paddingHorizontal, paddingTop, paddingHorizontal, paddingBottom)

        // 创建标题容器（垂直布局，用于放置标题和副标题）
        titleContainer = LinearLayout(context).apply {
            layoutParams = LayoutParams(
                LayoutParams.WRAP_CONTENT,
                LayoutParams.WRAP_CONTENT
            )
            orientation = VERTICAL
        }

        // 创建标题 TextView
        titleTextView = TextView(context).apply {
            layoutParams = LayoutParams(
                LayoutParams.WRAP_CONTENT,
                LayoutParams.WRAP_CONTENT
            )
            text = "AskAway"
            setTextColor(context.getColor(android.R.color.white))
            textSize = 34.2f
        }
        titleContainer.addView(titleTextView)

        // 创建副标题 TextView
        subtitleTextView = TextView(context).apply {
            layoutParams = LayoutParams(
                LayoutParams.WRAP_CONTENT,
                LayoutParams.WRAP_CONTENT
            )
            setTextColor(context.getColor(android.R.color.white))
            textSize = 14f
            alpha = 0.7f
            visibility = View.GONE
        }
        titleContainer.addView(subtitleTextView)

        addView(titleContainer)

        // 创建占位 View
        val spacer = View(context).apply {
            layoutParams = LayoutParams(0, 0, 1f)
        }
        addView(spacer)

        // 创建添加按钮
        val buttonSize = (44 * resources.displayMetrics.density).toInt()
        val buttonPadding = (12 * resources.displayMetrics.density).toInt()

        addButton = ImageView(context).apply {
            layoutParams = LayoutParams(buttonSize, buttonSize)
            setBackgroundResource(R.drawable.primary_circle)
            setPadding(buttonPadding, buttonPadding, buttonPadding, buttonPadding)
            setImageResource(R.drawable.ic_add)
            setOnClickListener {
                onAddClickListener?.invoke()
            }
        }
        addView(addButton)
    }

    /**
     * 设置标题文字
     */
    fun setTitle(title: String) {
        titleTextView.text = title
    }

    /**
     * 获取标题文字
     */
    fun getTitle(): String {
        return titleTextView.text.toString()
    }

    /**
     * 设置副标题文字
     */
    fun setSubtitle(subtitle: String?) {
        if (subtitle.isNullOrEmpty()) {
            subtitleTextView.visibility = View.GONE
        } else {
            subtitleTextView.text = subtitle
            subtitleTextView.visibility = View.VISIBLE
        }
    }

    /**
     * 设置是否显示大标题
     */
    fun setShowLargeTitle(show: Boolean) {
        showLargeTitle = show
        if (show) {
            // 大标题模式：34.2sp
            titleTextView.textSize = 34.2f
        } else {
            // 小标题模式：17sp
            titleTextView.textSize = 17f
        }
    }

    /**
     * 设置添加按钮的可见性
     */
    fun setAddButtonVisible(visible: Boolean) {
        addButton.visibility = if (visible) View.VISIBLE else View.GONE
    }
}