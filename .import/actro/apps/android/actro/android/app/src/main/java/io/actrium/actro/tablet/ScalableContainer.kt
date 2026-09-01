package io.actrium.actro.tablet

import android.content.Context
import android.graphics.Canvas
import android.util.AttributeSet
import android.widget.FrameLayout

/**
 * 可缩放的容器，用于包装 ReactRootView
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
            // 测量子 View 为原始尺寸
            val childWidthSpec = MeasureSpec.makeMeasureSpec(originalWidth, MeasureSpec.EXACTLY)
            val childHeightSpec = MeasureSpec.makeMeasureSpec(originalHeight, MeasureSpec.EXACTLY)

            for (i in 0 until childCount) {
                getChildAt(i).measure(childWidthSpec, childHeightSpec)
            }

            // 自己的尺寸设置为目标尺寸
            setMeasuredDimension(targetWidth, targetHeight)
        } else {
            super.onMeasure(widthMeasureSpec, heightMeasureSpec)
        }
    }

    override fun onLayout(changed: Boolean, left: Int, top: Int, right: Int, bottom: Int) {
        if (targetWidth > 0 && targetHeight > 0 && originalWidth > 0 && originalHeight > 0) {
            // 子 View 按原始尺寸布局
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
            // 计算宽高的缩放比例，分别缩放以填充满容器
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
            // 参数没有变化，不需要重新布局
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
            // 已经清除了，不需要重复操作
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