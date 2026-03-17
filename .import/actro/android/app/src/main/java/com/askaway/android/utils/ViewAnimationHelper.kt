package com.askaway.android.utils

import android.animation.AnimatorSet
import android.animation.ObjectAnimator
import android.animation.ValueAnimator
import android.graphics.Rect
import android.util.Log
import android.view.View
import android.view.animation.DecelerateInterpolator

object ViewAnimationHelper {

    /**
     * 执行显示动画（支持共享元素展开和底部滑入）
     *
     * @param targetView 需要显示的视图（如 flReactNative）
     * @param sourceView 来源视图（用于共享元素动画，可为 null）
     * @param onRetry 当视图宽高未准备好时，请求重试的回调
     */
    fun animateShow(targetView: View, sourceView: View?, onRetry: () -> Unit) {
        if (sourceView != null) {
            // 重置所有变换属性，确保测量准确
            targetView.translationX = 0f
            targetView.translationY = 0f
            targetView.scaleX = 1f
            targetView.scaleY = 1f
            targetView.pivotX = 0f
            targetView.pivotY = 0f
            targetView.clipBounds = null

            // 确保可见性为 INVISIBLE（而非 GONE），以便 getLocationInWindow 正常工作
            if (targetView.visibility == View.GONE) {
                targetView.visibility = View.INVISIBLE
            }

            // 确保视图有有效的尺寸，否则在布局后重试
            if ((targetView.width == 0 || targetView.height == 0)) {
                targetView.post { onRetry() }
                return
            }

            // --- 执行顶部对齐的展开动画 (Container Transform) ---

            val sourceLoc = IntArray(2)
            sourceView.getLocationInWindow(sourceLoc)

            val targetLoc = IntArray(2)
            targetView.getLocationInWindow(targetLoc)

            // 1. 根据宽度计算统一缩放比例
            val scale = sourceView.width.toFloat() / targetView.width

            // 2. 计算平移量以对齐左上角
            val startX = sourceLoc[0] - targetLoc[0].toFloat()
            val startY = sourceLoc[1] - targetLoc[1].toFloat()

            // 3. 设置初始属性 (Pivot 左上角)
            targetView.pivotX = 0f
            targetView.pivotY = 0f
            targetView.translationX = startX
            targetView.translationY = startY
            targetView.scaleX = scale
            targetView.scaleY = scale

            // 4. 设置裁剪 (显示底部)
            val startVisibleHeight = sourceView.height.toFloat() / scale
            val startRect = Rect(0, 0, targetView.width, startVisibleHeight.toInt())
            targetView.clipBounds = startRect

            targetView.visibility = View.VISIBLE

            val set = AnimatorSet()

            // 动画裁剪边界
            val clipAnimator = ValueAnimator.ofInt(startRect.bottom, targetView.height)
            clipAnimator.addUpdateListener { animation ->
                val valHeight = animation.animatedValue as Int
                if (valHeight >= targetView.height - 1) {
                    targetView.clipBounds = null
                } else {
                    targetView.clipBounds = Rect(0, 0, targetView.width, valHeight)
                }
            }

            set.playTogether(
                ObjectAnimator.ofFloat(targetView, "translationX", startX, 0f),
                ObjectAnimator.ofFloat(targetView, "translationY", startY, 0f),
                ObjectAnimator.ofFloat(targetView, "scaleX", scale, 1f),
                ObjectAnimator.ofFloat(targetView, "scaleY", scale, 1f),
                clipAnimator
            )
            set.duration = 350
            set.interpolator = DecelerateInterpolator()
            set.start()
        } else {
            // --- 默认底部滑入动画 ---
            val startY = targetView.height.toFloat()

            // 确保起始位置在底部
            targetView.translationY = startY
            targetView.visibility = View.VISIBLE

            // 创建从底部滑入的动画
            ObjectAnimator.ofFloat(targetView, "translationY", startY, 0f).apply {
                duration = 300
                interpolator = DecelerateInterpolator()
                start()
            }
        }
    }

    /**
     * 执行隐藏动画（支持共享元素收回和底部滑出）
     *
     * @param targetView 需要隐藏的视图（如 flReactNative）
     * @param sourceView 来源视图（用于共享元素动画，可为 null）
     * @param onAnimationEnd 动画结束后的回调
     */
    fun animateHide(targetView: View, sourceView: View?, onAnimationEnd: () -> Unit) {
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

            val set = AnimatorSet()

            val clipAnimator = ValueAnimator.ofFloat(0f, verticalInset)
            clipAnimator.addUpdateListener { animation ->
                val inset = animation.animatedValue as Float
                // Clip from both top and bottom
                val top = inset.toInt()
                val bottom = (height - inset).toInt()
                targetView.clipBounds = Rect(1, top, width, bottom)
            }
            Log.e("ViewAnimationHelper", "animateHide: endX=$endX, endY=$endY, scale=$scale, verticalInset=$verticalInset")
            set.playTogether(
                ObjectAnimator.ofFloat(targetView, "translationX", 0f, endX),
                ObjectAnimator.ofFloat(targetView, "translationY", 0f, endY),
                ObjectAnimator.ofFloat(targetView, "scaleX", 1f, scale),
                ObjectAnimator.ofFloat(targetView, "scaleY", 1f, scale),
                clipAnimator
            )
            set.duration = 250
            set.interpolator = DecelerateInterpolator()
            set.addListener(object : android.animation.AnimatorListenerAdapter() {
                override fun onAnimationEnd(animation: android.animation.Animator) {
                    onAnimationEnd()
                    // Reset properties
                    targetView.translationX = 0f
                    targetView.translationY = 0f
                    targetView.scaleX = 1f
                    targetView.scaleY = 1f
                    targetView.clipBounds = null
                    targetView.pivotX = 0f
                    targetView.pivotY = 0f
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
            set.addListener(object : android.animation.AnimatorListenerAdapter() {
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
}

