package io.actrium.app.tablet

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
 * For managing multiple ReactNative Component GridView Containers
 * Support scaling and moving the picture as it moves
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
        // Disable cropping, allow subviews to go beyond boundaries while scaling
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

        // Calculate GridView Layout Parameters
        val totalHorizontalSpacing = (numColumns - 1) * horizontalSpacing
        itemWidth = (containerWidth - totalHorizontalSpacing) / numColumns
        val aspectRatio = containerWidth.toFloat() / containerHeight.toFloat()
        itemHeight = (itemWidth / aspectRatio).toInt()

        if (isGridMode && !isAnimating) {
            // GridView Mode: Calculate each item Size and Location
            rnViews.forEachIndexed { index, view ->
                val row = index / numColumns
                val col = index % numColumns

                val x = containerPadding + col * (itemWidth + horizontalSpacing)
                val y = containerPadding + row * (itemHeight + verticalSpacing)

                view.layout(x, y, x + itemWidth, y + itemHeight)
                view.visibility = View.VISIBLE

                // Reset Transformation
                view.scaleX = 1f
                view.scaleY = 1f
                view.translationX = 0f
                view.translationY = 0f
            }
        } else if (!isAnimating) {
            // Fullscreen mode: All views stacked together, current view at the top
            rnViews.forEachIndexed { index, view ->
                view.layout(0, 0, width, height)
                view.visibility = if (index == currentIndex) View.VISIBLE else View.GONE

                // Reset Transformation
                view.scaleX = 1f
                view.scaleY = 1f
                view.translationX = 0f
                view.translationY = 0f
            }
        } else {
            // Keep a full screen layout while animating
            rnViews.forEachIndexed { index, view ->
                view.layout(0, 0, width, height)
            }
        }
    }

    /**
     * Zoom out of fullscreen mode to GridView Mode
     */
    fun shrinkToGrid(onComplete: (() -> Unit)? = null) {
        Log.e("RNGridContainer", "shrinkToGrid___1")
        if (isGridMode || rnViews.isEmpty()) {
            Log.e("RNGridContainer", "shrinkToGrid___2")
            onComplete?.invoke()
            return
        }
        Log.e("RNGridContainer", "shrinkToGrid___3")

        // Make sure the containers are visible so that they can be measured.
        if (visibility != View.VISIBLE) {
            visibility = View.VISIBLE
        }

        // Check if the container size is valid and if not, wait until the measurement is completed
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

        // Count first. GridView Layout Parameters
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

            // Ensures that the zoom values are valid
            if (scaleX.isNaN() || scaleY.isNaN() || scaleX <= 0 || scaleY <= 0) {
                Log.e("RNGridContainer", "Invalid scale values")
                isAnimating = false
                onComplete?.invoke()
                return
            }

            // Add background colour for debugging
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
     * From GridView Magnify Mode to Full Screen Mode
     */
    fun expandToFullScreen(index: Int, onComplete: (() -> Unit)? = null) {
        if (!isGridMode || index >= rnViews.size || index < 0) {
            onComplete?.invoke()
            return
        }

        // Make sure the container is visible
        if (visibility != View.VISIBLE) {
            visibility = View.VISIBLE
        }

        // Check if the container size is working.
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
                // Current selected view zooms to fullscreen
                // Calculates the transformation from current position to fullscreen
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
                // Other view fading
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
     * Move to Specified RN Component
     */
    fun scrollToIndex(index: Int, onComplete: (() -> Unit)? = null) {
        if (isGridMode || index >= rnViews.size || index < 0 || index == currentIndex) {
            onComplete?.invoke()
            return
        }

        // Make sure the container is visible
        if (visibility != View.VISIBLE) {
            visibility = View.VISIBLE
        }

        // Check if the container size is working.
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
