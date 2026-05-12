package chat.askaway.android.tablet

import android.animation.Animator
import android.animation.AnimatorListenerAdapter
import android.animation.AnimatorSet
import android.animation.ObjectAnimator
import android.content.Context
import android.util.AttributeSet
import android.util.Log
import android.view.View
import android.widget.FrameLayout
import com.facebook.react.ReactRootView

/**
 * 用于管理多个 ReactNative 组件的 GridView 容器
 * 支持缩放和平移动画效果
 */
class RNGridContainer @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
    defStyleAttr: Int = 0
) : FrameLayout(context, attrs, defStyleAttr) {

    private val rnViews = mutableListOf<ReactRootView>()
    private var numColumns = 2
    private var itemWidth = 0
    private var itemHeight = 0
    private val horizontalSpacing = 20
    private val verticalSpacing = 20
    private val containerPadding = 20
    private var currentIndex = 0
    private var isGridMode = false
    private var isAnimating = false

    init {
        // 禁用裁剪，允许子视图在缩放时超出边界
        clipChildren = false
        clipToPadding = false
    }

    fun setNumColumns(columns: Int) {
        if (numColumns != columns) {
            numColumns = columns
            if (isGridMode && !isAnimating) {
                requestLayout()
            }
        }
    }

    fun addRNView(view: ReactRootView) {
        rnViews.add(view)
        addView(view)
        val s = ""
        if (!isGridMode) {
            view.visibility = if (rnViews.size - 1 == currentIndex) View.VISIBLE else View.GONE
        }
        requestLayout()
    }

    fun removeRNView(view: ReactRootView) {
        val index = rnViews.indexOf(view)
        rnViews.remove(view)
        removeView(view)

        if (index <= currentIndex && currentIndex > 0) {
            currentIndex--
        }
        requestLayout()
    }

    fun getRNViews(): List<ReactRootView> = rnViews

    override fun onLayout(changed: Boolean, left: Int, top: Int, right: Int, bottom: Int) {
        if (rnViews.isEmpty()) return

        val containerWidth = width - 2 * containerPadding
        val containerHeight = height - 2 * containerPadding

        // 计算 GridView 的布局参数
        val totalHorizontalSpacing = (numColumns - 1) * horizontalSpacing
        itemWidth = (containerWidth - totalHorizontalSpacing) / numColumns
        val aspectRatio = containerWidth.toFloat() / containerHeight.toFloat()
        itemHeight = (itemWidth / aspectRatio).toInt()

        if (isGridMode && !isAnimating) {
            // GridView 模式：计算每个 item 的大小和位置
            rnViews.forEachIndexed { index, view ->
                val row = index / numColumns
                val col = index % numColumns

                val x = containerPadding + col * (itemWidth + horizontalSpacing)
                val y = containerPadding + row * (itemHeight + verticalSpacing)

                view.layout(x, y, x + itemWidth, y + itemHeight)
                view.visibility = View.VISIBLE

                // 重置变换
                view.scaleX = 1f
                view.scaleY = 1f
                view.translationX = 0f
                view.translationY = 0f
            }
        } else if (!isAnimating) {
            // 全屏模式：所有视图叠加在一起，当前视图在最前面
            rnViews.forEachIndexed { index, view ->
                view.layout(0, 0, width, height)
                view.visibility = if (index == currentIndex) View.VISIBLE else View.GONE

                // 重置变换
                view.scaleX = 1f
                view.scaleY = 1f
                view.translationX = 0f
                view.translationY = 0f
            }
        } else {
            // 动画过程中，保持全屏布局
            rnViews.forEachIndexed { index, view ->
                view.layout(0, 0, width, height)
            }
        }
    }

    /**
     * 从全屏模式缩小到 GridView 模式
     */
    fun shrinkToGrid(onComplete: (() -> Unit)? = null) {
        Log.e("RNGridContainer", "shrinkToGrid___1")
        if (isGridMode || rnViews.isEmpty()) {
            Log.e("RNGridContainer", "shrinkToGrid___2")
            onComplete?.invoke()
            return
        }
        Log.e("RNGridContainer", "shrinkToGrid___3")

        // 确保容器可见，以便进行测量
        if (visibility != View.VISIBLE) {
            visibility = View.VISIBLE
        }

        // 检查容器尺寸是否有效，如果无效则等待测量完成
        if (width <= 0 || height <= 0) {
            Log.e("RNGridContainer", "shrinkToGrid___4 width=$width height=$height")
            viewTreeObserver.addOnGlobalLayoutListener(object : android.view.ViewTreeObserver.OnGlobalLayoutListener {
                override fun onGlobalLayout() {
                    viewTreeObserver.removeOnGlobalLayoutListener(this)
                    if (width > 0 && height > 0) {
                        shrinkToGrid(onComplete)
                    } else {
                        Log.e("RNGridContainer", "Container still has invalid size after layout")
                        onComplete?.invoke()
                    }
                }
            })
            return
        }

        Log.e("RNGridContainer", "shrinkToGrid___5")
        isAnimating = true

        // 先计算 GridView 的布局参数
        val containerWidth = width - 2 * containerPadding
        val containerHeight = height - 2 * containerPadding
        val totalHorizontalSpacing = (numColumns - 1) * horizontalSpacing
        itemWidth = (containerWidth - totalHorizontalSpacing) / numColumns
        val aspectRatio = containerWidth.toFloat() / containerHeight.toFloat()
        itemHeight = (itemWidth / aspectRatio).toInt()

        Log.e("RNGridContainer", "shrinkToGrid___6 itemWidth=$itemWidth itemHeight=$itemHeight")

        if (itemWidth <= 0 || itemHeight <= 0) {
            Log.e("RNGridContainer", "Invalid item size")
            isAnimating = false
            onComplete?.invoke()
            return
        }

        val animators = mutableListOf<Animator>()

        rnViews.forEachIndexed { index, view ->
            view.visibility = View.VISIBLE
            view.pivotX = 0f
            view.pivotY = 0f

            val row = index / numColumns
            val col = index % numColumns

            val targetX = containerPadding + col * (itemWidth + horizontalSpacing)
            val targetY = containerPadding + row * (itemHeight + verticalSpacing)

            val scaleX = itemWidth.toFloat() / width
            val scaleY = itemHeight.toFloat() / height

            Log.e("RNGridContainer", "shrinkToGrid___7 index=$index targetX=$targetX targetY=$targetY scaleX=$scaleX scaleY=$scaleY width=$width height=$height")

            // 确保缩放值有效
            if (scaleX.isNaN() || scaleY.isNaN() || scaleX <= 0 || scaleY <= 0) {
                Log.e("RNGridContainer", "Invalid scale values")
                isAnimating = false
                onComplete?.invoke()
                return
            }

            // 添加背景色用于调试
            view.setBackgroundColor(android.graphics.Color.argb(50, 255, 0, 0))

            val animatorSet = AnimatorSet()
            animatorSet.playTogether(
                ObjectAnimator.ofFloat(view, "scaleX", 1f, scaleX),
                ObjectAnimator.ofFloat(view, "scaleY", 1f, scaleY),
                ObjectAnimator.ofFloat(view, "translationX", 0f, targetX.toFloat()),
                ObjectAnimator.ofFloat(view, "translationY", 0f, targetY.toFloat())
            )
            animatorSet.duration = 300
            animators.add(animatorSet)
        }

        Log.e("RNGridContainer", "shrinkToGrid___8 animators.size=${animators.size}")

        if (animators.isEmpty()) {
            isAnimating = false
            onComplete?.invoke()
            return
        }

        val finalAnimatorSet = AnimatorSet()
        finalAnimatorSet.playTogether(animators)
        finalAnimatorSet.addListener(object : AnimatorListenerAdapter() {
            override fun onAnimationEnd(animation: Animator) {
                Log.e("RNGridContainer", "shrinkToGrid___9 animation end")
                isGridMode = true
                isAnimating = false
                requestLayout()
                onComplete?.invoke()
            }
        })
        finalAnimatorSet.start()
    }

    /**
     * 从 GridView 模式放大到全屏模式
     */
    fun expandToFullScreen(index: Int, onComplete: (() -> Unit)? = null) {
        if (!isGridMode || index >= rnViews.size || index < 0) {
            onComplete?.invoke()
            return
        }

        // 确保容器可见
        if (visibility != View.VISIBLE) {
            visibility = View.VISIBLE
        }

        // 检查容器尺寸是否有效
        if (width <= 0 || height <= 0) {
            viewTreeObserver.addOnGlobalLayoutListener(object : android.view.ViewTreeObserver.OnGlobalLayoutListener {
                override fun onGlobalLayout() {
                    viewTreeObserver.removeOnGlobalLayoutListener(this)
                    if (width > 0 && height > 0) {
                        expandToFullScreen(index, onComplete)
                    } else {
                        onComplete?.invoke()
                    }
                }
            })
            return
        }

        currentIndex = index
        isAnimating = true

        val animators = mutableListOf<Animator>()

        rnViews.forEachIndexed { i, view ->
            if (i == index) {
                // 当前选中的视图放大到全屏
                // 计算从当前位置到全屏的变换
                val currentLeft = view.left.toFloat()
                val currentTop = view.top.toFloat()
                val currentScaleX = view.scaleX
                val currentScaleY = view.scaleY

                val animatorSet = AnimatorSet()
                animatorSet.playTogether(
                    ObjectAnimator.ofFloat(view, "scaleX", 1f),
                    ObjectAnimator.ofFloat(view, "scaleY", 1f),
                    ObjectAnimator.ofFloat(view, "translationX", -currentLeft),
                    ObjectAnimator.ofFloat(view, "translationY", -currentTop)
                )
                animatorSet.duration = 300
                animators.add(animatorSet)
            } else {
                // 其他视图淡出
                val fadeOut = ObjectAnimator.ofFloat(view, "alpha", 1f, 0f)
                fadeOut.duration = 300
                animators.add(fadeOut)
            }
        }

        if (animators.isEmpty()) {
            isAnimating = false
            onComplete?.invoke()
            return
        }

        val finalAnimatorSet = AnimatorSet()
        finalAnimatorSet.playTogether(animators)
        finalAnimatorSet.addListener(object : AnimatorListenerAdapter() {
            override fun onAnimationEnd(animation: Animator) {
                rnViews.forEachIndexed { i, view ->
                    if (i != index) {
                        view.visibility = View.GONE
                        view.alpha = 1f
                    }
                }
                isGridMode = false
                isAnimating = false
                requestLayout()
                onComplete?.invoke()
            }
        })
        finalAnimatorSet.start()
    }

    /**
     * 平移到指定的 RN 组件
     */
    fun scrollToIndex(index: Int, onComplete: (() -> Unit)? = null) {
        if (isGridMode || index >= rnViews.size || index < 0 || index == currentIndex) {
            onComplete?.invoke()
            return
        }

        // 确保容器可见
        if (visibility != View.VISIBLE) {
            visibility = View.VISIBLE
        }

        // 检查容器尺寸是否有效
        if (width <= 0 || height <= 0) {
            viewTreeObserver.addOnGlobalLayoutListener(object : android.view.ViewTreeObserver.OnGlobalLayoutListener {
                override fun onGlobalLayout() {
                    viewTreeObserver.removeOnGlobalLayoutListener(this)
                    if (width > 0 && height > 0) {
                        scrollToIndex(index, onComplete)
                    } else {
                        onComplete?.invoke()
                    }
                }
            })
            return
        }

        isAnimating = true
        val currentView = rnViews[currentIndex]
        val targetView = rnViews[index]

        currentView.visibility = View.VISIBLE
        targetView.visibility = View.VISIBLE

        val direction = if (index > currentIndex) -1 else 1
        val distance = width.toFloat()

        targetView.translationX = -direction * distance

        val currentAnimator = ObjectAnimator.ofFloat(currentView, "translationX", 0f, direction * distance)
        val targetAnimator = ObjectAnimator.ofFloat(targetView, "translationX", -direction * distance, 0f)

        val animatorSet = AnimatorSet()
        animatorSet.playTogether(currentAnimator, targetAnimator)
        animatorSet.duration = 300
        animatorSet.addListener(object : AnimatorListenerAdapter() {
            override fun onAnimationEnd(animation: Animator) {
                currentView.visibility = View.GONE
                currentView.translationX = 0f
                currentIndex = index
                isAnimating = false
                onComplete?.invoke()
            }
        })
        animatorSet.start()
    }

    fun getCurrentIndex(): Int = currentIndex

    fun isInGridMode(): Boolean = isGridMode
}
