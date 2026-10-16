package io.actrium.app.utils

import android.animation.AnimatorListenerAdapter
import android.animation.AnimatorSet
import android.animation.ObjectAnimator
import android.animation.ValueAnimator
import android.graphics.Outline
import android.graphics.Rect
import android.util.Log
import android.view.View
import android.view.ViewOutlineProvider
import android.view.animation.DecelerateInterpolator

object ViewAnimationHelper {

    /**
     * Execute animated display (support for shared elements roll-up and bottom slide)
     *
     * @param targetView View to display (e. g. flReactNative)
     * @param sourceView Source view (for sharing element animations, for null)
     * @param onRetry Request to try again when view is not ready
     */
    fun animateShow(targetView: View, sourceView: View?, onRetry: () -> Unit) {
        if (sourceView != null) {
            // Reset all transformation properties to ensure accurate measurements
            targetView.translationX = 0f
            targetView.translationY = 0f
            targetView.scaleX = 1f
            targetView.scaleY = 1f
            targetView.pivotX = 0f
            targetView.pivotY = 0f
            targetView.clipBounds = null
            targetView.clipToOutline = false

            // Ensure visibility INVISIBLE"not GONE) getLocationInWindow Normal work
            if (targetView.visibility == View.GONE) {
                targetView.visibility = View.INVISIBLE
            }

            // Make sure the view has a valid size or try again after the layout
            if ((targetView.width == 0 || targetView.height == 0)) {
                targetView.post { onRetry() }
                return
            }

            // --- Execute top-matched animations (Container Transform) ---

            val sourceLoc = IntArray(2)
            sourceView.getLocationInWindow(sourceLoc)

            val targetLoc = IntArray(2)
            targetView.getLocationInWindow(targetLoc)

            // 1. Unified scaling based on width
            val scale = sourceView.width.toFloat() / targetView.width

            // 2. Calculates the level shift to align the upper left corner
            val startX = sourceLoc[0] - targetLoc[0].toFloat()
            val startY = sourceLoc[1] - targetLoc[1].toFloat()

            // 3. Set Initial Properties (Pivot Top Left)
            targetView.pivotX = 0f
            targetView.pivotY = 0f
            targetView.translationX = startX
            targetView.translationY = startY
            targetView.scaleX = scale
            targetView.scaleY = scale

            // 4. Use ViewOutlineProvider Set rounded angles (under rounded angles)
            val startVisibleHeight = sourceView.height.toFloat() / scale
            val cornerRadiusPx = 38 * targetView.resources.displayMetrics.density

            val animOutlineProvider = object : ViewOutlineProvider() {
                var clipHeight = startVisibleHeight.toInt()
                override fun getOutline(view: View, outline: Outline) {
                    outline.setRoundRect(0, 0, view.width, clipHeight, cornerRadiusPx)
                }
            }
            targetView.outlineProvider = animOutlineProvider
            targetView.clipToOutline = true

            targetView.visibility = View.VISIBLE

            // Enable hardware layer to ensure full resolution cache of content and avoid deterioration during magnification
            targetView.setLayerType(View.LAYER_TYPE_HARDWARE, null)

            val set = AnimatorSet()

            // Animated border cut (with rounded angles)
            val clipAnimator = ValueAnimator.ofInt(startVisibleHeight.toInt(), targetView.height)
            clipAnimator.addUpdateListener { animation ->
                val valHeight = animation.animatedValue as Int
                animOutlineProvider.clipHeight = valHeight
                targetView.invalidateOutline()
            }

            set.playTogether(
                ObjectAnimator.ofFloat(targetView, "translationX", startX, 0f),
                ObjectAnimator.ofFloat(targetView, "translationY", startY, 0f),
                ObjectAnimator.ofFloat(targetView, "scaleX", scale, 1f),
                ObjectAnimator.ofFloat(targetView, "scaleY", scale, 1f),
                clipAnimator
            )
            set.duration = 250
            set.interpolator = DecelerateInterpolator()
            set.addListener(object : AnimatorListenerAdapter() {
                override fun onAnimationEnd(animation: android.animation.Animator) {
                    targetView.clipToOutline = false
                    targetView.outlineProvider = ViewOutlineProvider.BOUNDS
                    targetView.invalidateOutline()
                    // Restore default layer type after animation
                    targetView.setLayerType(View.LAYER_TYPE_NONE, null)
                }
            })
            set.start()
        } else {
            // --- Default bottom slide into animation ---
            val startY = targetView.height.toFloat()

            // Make sure the starting position is at the bottom.
            targetView.translationY = startY
            targetView.visibility = View.VISIBLE

            // Create animated animations that slide from the bottom
            ObjectAnimator.ofFloat(targetView, "translationY", startY, 0f).apply {
                duration = 300
                interpolator = DecelerateInterpolator()
                start()
            }
        }
    }

    /**
     * Execute hidden animations (support shared elements recovery and bottom slides out)
     *
     * @param targetView A hidden view (e. g.) flReactNative)
     * @param sourceView Source view (for sharing element animations, for null)
     * @param onAnimationEnd The echo after the animation
     */
    fun animateHide(targetView: View, sourceView: View?, onAnimationEnd: () -> Unit) {
        Log.e("ViewAnimationHelper", "animateHide ${sourceView}  ${sourceView?.isAttachedToWindow}")
        if (sourceView != null && sourceView.isAttachedToWindow) {
            val sourceLoc = IntArray(2)
            sourceView.getLocationInWindow(sourceLoc)

            val targetLoc = IntArray(2)
            targetView.getLocationInWindow(targetLoc)

            val width = targetView.width
            val height = targetView.height
            Log.e("ViewAnimationHelper", "animateHide: targetView width=$width, sourceView width=${sourceView.width}")
            Log.e("ViewAnimationHelper", "animateHide: targetView height=$height, sourceView height=${sourceView.height}")
            val scale = sourceView.width.toFloat() / width

            // Calculate End Clip Height (matches source aspect ratio)
            val endVisibleHeight = sourceView.height.toFloat() / scale

            // Calculate vertical center inset to clip symmetrically from top and bottom
            val verticalInset = (height - endVisibleHeight) / 2f

            val baseEndY = (sourceLoc[1] - targetLoc[1]).toFloat()
            val endX = (sourceLoc[0] - targetLoc[0]).toFloat()
            val endY = baseEndY - verticalInset * scale

            val cornerRadiusPx = 38 * targetView.resources.displayMetrics.density

            val animOutlineProvider = object : ViewOutlineProvider() {
                var clipTop = 0
                var clipBottom = height
                override fun getOutline(view: View, outline: Outline) {
                    outline.setRoundRect(0, clipTop, view.width, clipBottom, cornerRadiusPx)
                }
            }
            targetView.outlineProvider = animOutlineProvider
            targetView.clipToOutline = true
            targetView.clipBounds = null

            // Enable hardware layer to ensure that the contents remain clear during the downsizing of animation
            targetView.setLayerType(View.LAYER_TYPE_HARDWARE, null)

            val set = AnimatorSet()

            val clipAnimator = ValueAnimator.ofFloat(0f, verticalInset)
            clipAnimator.addUpdateListener { animation ->
                val inset = animation.animatedValue as Float
                animOutlineProvider.clipTop = inset.toInt()
                animOutlineProvider.clipBottom = (height - inset).toInt()
                targetView.invalidateOutline()
            }
            Log.e("ViewAnimationHelper", "animateHide: endX=$endX, endY=$endY, scale=$scale, verticalInset=$verticalInset")
            set.playTogether(
                ObjectAnimator.ofFloat(targetView, "translationX", 0f, endX),
                ObjectAnimator.ofFloat(targetView, "translationY", 0f, endY),
                ObjectAnimator.ofFloat(targetView, "scaleX", 1f, scale),
                ObjectAnimator.ofFloat(targetView, "scaleY", 1f, scale),
                clipAnimator
            )
            set.duration = 180
            set.interpolator = DecelerateInterpolator()
            set.addListener(object : AnimatorListenerAdapter() {
                override fun onAnimationEnd(animation: android.animation.Animator) {
                    onAnimationEnd()
                    // Reset properties
                    targetView.translationX = 0f
                    targetView.translationY = 0f
                    targetView.scaleX = 1f
                    targetView.scaleY = 1f
                    targetView.clipBounds = null
                    targetView.clipToOutline = false
                    targetView.outlineProvider = ViewOutlineProvider.BOUNDS
                    targetView.invalidateOutline()
                    targetView.pivotX = 0f
                    targetView.pivotY = 0f
                    // Restore default layer type after animation
                    targetView.setLayerType(View.LAYER_TYPE_NONE, null)
                }
            })

            // Ensure pivot is set to 0,0 for Top-Left based transformations
            targetView.pivotX = 0f
            targetView.pivotY = 0f
            set.start()
        } else {
            // Fallback: Scale down vertically to center (Adjust both top and bottom)
            targetView.pivotX = targetView.width / 2f
            targetView.pivotY = targetView.height / 2f

            val set = AnimatorSet()
            set.playTogether(
                ObjectAnimator.ofFloat(targetView, "scaleY", 1f, 0f),
                ObjectAnimator.ofFloat(targetView, "alpha", 1f, 0f)
            )
            set.duration = 300
            set.interpolator = DecelerateInterpolator()
            set.addListener(object : AnimatorListenerAdapter() {
                override fun onAnimationEnd(animation: android.animation.Animator) {
                    onAnimationEnd()
                    // Reset
                    targetView.scaleY = 1f
                    targetView.alpha = 1f
                    targetView.pivotX = 0f
                    targetView.pivotY = 0f
                }
            })
            set.start()
        }
    }

    /**
     * Execute hidden animations (based on target) Rect Calculates where to collect)
     *
     * @param targetView A hidden view (e. g.) flReactNative)
     * @param targetRect Target location and dimensions (in window Coordinates System)
     * @param cornerRadiusDp Target round corner (%2)dp)
     * @param onAnimationEnd The echo after the animation
     */
    fun animateHide(targetView: View, targetRect: Rect, cornerRadiusDp: Float, onAnimationEnd: () -> Unit) {
        val targetLoc = IntArray(2)
        targetView.getLocationInWindow(targetLoc)

        val width = targetView.width
        val height = targetView.height
        val scale = targetRect.width().toFloat() / width

        val endVisibleHeight = targetRect.height().toFloat() / scale
        val verticalInset = (height - endVisibleHeight) / 2f

        val baseEndY = (targetRect.top - targetLoc[1]).toFloat()
        val endX = (targetRect.left - targetLoc[0]).toFloat()
        val endY = baseEndY - verticalInset * scale

        val cornerRadiusPx = cornerRadiusDp * targetView.resources.displayMetrics.density

        val animOutlineProvider = object : ViewOutlineProvider() {
            var clipTop = 0
            var clipBottom = height
            override fun getOutline(view: View, outline: Outline) {
                outline.setRoundRect(0, clipTop, view.width, clipBottom, cornerRadiusPx)
            }
        }
        targetView.outlineProvider = animOutlineProvider
        targetView.clipToOutline = true
        targetView.clipBounds = null

        // Enable hardware layer to ensure that the contents remain clear during the downsizing of animation
        targetView.setLayerType(View.LAYER_TYPE_HARDWARE, null)

        val set = AnimatorSet()

        val clipAnimator = ValueAnimator.ofFloat(0f, verticalInset)
        clipAnimator.addUpdateListener { animation ->
            val inset = animation.animatedValue as Float
            animOutlineProvider.clipTop = inset.toInt()
            animOutlineProvider.clipBottom = (height - inset).toInt()
            targetView.invalidateOutline()
        }

        set.playTogether(
            ObjectAnimator.ofFloat(targetView, "translationX", 0f, endX),
            ObjectAnimator.ofFloat(targetView, "translationY", 0f, endY),
            ObjectAnimator.ofFloat(targetView, "scaleX", 1f, scale),
            ObjectAnimator.ofFloat(targetView, "scaleY", 1f, scale),
            clipAnimator
        )
        set.duration = 180
        set.interpolator = DecelerateInterpolator()
        set.addListener(object : AnimatorListenerAdapter() {
            override fun onAnimationEnd(animation: android.animation.Animator) {
                onAnimationEnd()
                targetView.translationX = 0f
                targetView.translationY = 0f
                targetView.scaleX = 1f
                targetView.scaleY = 1f
                targetView.clipBounds = null
                targetView.clipToOutline = false
                targetView.outlineProvider = ViewOutlineProvider.BOUNDS
                targetView.invalidateOutline()
                targetView.pivotX = 0f
                targetView.pivotY = 0f
                // Restore default layer type after animation
                targetView.setLayerType(View.LAYER_TYPE_NONE, null)
            }
        })

        targetView.pivotX = 0f
        targetView.pivotY = 0f
        set.start()
    }
}

