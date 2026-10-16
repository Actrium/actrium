package io.actrium.app.tablet

import android.content.Context
import android.graphics.Canvas
import android.util.AttributeSet
import android.widget.FrameLayout

/**
 * Scalable packagings for packaging ReactRootView
 */
class ScalableContainer @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
    defStyleAttr: Int = 0
) : FrameLayout(context, attrs, defStyleAttr) {

    var targetWidth: Int = 0
    var targetHeight: Int = 0
    var originalWidth: Int = 0
    var originalHeight: Int = 0

    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        if (targetWidth > 0 && targetHeight > 0 && originalWidth > 0 && originalHeight > 0) {
            // Measurements View As Original Size
            val childWidthSpec = MeasureSpec.makeMeasureSpec(originalWidth, MeasureSpec.EXACTLY)
            val childHeightSpec = MeasureSpec.makeMeasureSpec(originalHeight, MeasureSpec.EXACTLY)

            for (i in 0 until childCount) {
                getChildAt(i).measure(childWidthSpec, childHeightSpec)
            }

            // Target size for your own size settings
            setMeasuredDimension(targetWidth, targetHeight)
        } else {
            super.onMeasure(widthMeasureSpec, heightMeasureSpec)
        }
    }

    override fun onLayout(changed: Boolean, left: Int, top: Int, right: Int, bottom: Int) {
        if (targetWidth > 0 && targetHeight > 0 && originalWidth > 0 && originalHeight > 0) {
            // Son View Layout by Original Size
            for (i in 0 until childCount) {
                val child = getChildAt(i)
                child.layout(0, 0, originalWidth, originalHeight)
            }
        } else {
            super.onLayout(changed, left, top, right, bottom)
        }
    }

    override fun dispatchDraw(canvas: Canvas) {
        if (targetWidth > 0 && targetHeight > 0 && originalWidth > 0 && originalHeight > 0) {
            // Calculates the width scale, which is applied separately to fill the container
            val scaleX = targetWidth.toFloat() / originalWidth
            val scaleY = targetHeight.toFloat() / originalHeight

            canvas.save()
            canvas.scale(scaleX, scaleY, 0f, 0f)
            super.dispatchDraw(canvas)
            canvas.restore()
        } else {
            super.dispatchDraw(canvas)
        }
    }

    fun setScale(targetW: Int, targetH: Int, originalW: Int, originalH: Int) {
        if (this.targetWidth == targetW && this.targetHeight == targetH &&
            this.originalWidth == originalW && this.originalHeight == originalH) {
            // Parameters have not changed, no re-programming is required
            return
        }
        this.targetWidth = targetW
        this.targetHeight = targetH
        this.originalWidth = originalW
        this.originalHeight = originalH
        requestLayout()
        invalidate()
    }

    fun clearScale() {
        if (this.targetWidth == 0 && this.targetHeight == 0 &&
            this.originalWidth == 0 && this.originalHeight == 0) {
            // Cleared, no duplicates.
            return
        }
        this.targetWidth = 0
        this.targetHeight = 0
        this.originalWidth = 0
        this.originalHeight = 0
        requestLayout()
        invalidate()
    }
}