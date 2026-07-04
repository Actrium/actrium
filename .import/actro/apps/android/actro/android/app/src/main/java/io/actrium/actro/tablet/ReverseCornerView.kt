package io.actrium.actro.tablet

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import android.util.AttributeSet
import android.view.View
import io.actrium.actro.R

/**
 * 支持指定方向反圆角（凹圆角）的自定义 View。
 *
 * 通过 reverseDirection 指定直角所在方向，对角自动生成凹圆角。
 * 凹圆角半径 = min(width, height)，无需手动设置。
 *
 * 方向说明（直角位置 → 凹圆角位置）：
 * - topLeft:     直角左上 → 凹圆角右下
 * - topRight:    直角右上 → 凹圆角左下
 * - bottomLeft:  直角左下 → 凹圆角右上
 * - bottomRight: 直角右下 → 凹圆角左上
 *
 * 使用示例：
 * ```
 * <ReverseCornerView
 *     android:layout_width="16dp"
 *     android:layout_height="16dp"
 *     android:layout_gravity="bottom|start"
 *     app:reverseDirection="bottomLeft"
 *     app:fillColor="#D1D1D6" />
 * ```
 */
class ReverseCornerView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
    defStyleAttr: Int = 0
) : View(context, attrs, defStyleAttr) {

    enum class Direction(val value: Int) {
        TOP_LEFT(0),
        TOP_RIGHT(1),
        BOTTOM_LEFT(2),
        BOTTOM_RIGHT(3);

        companion object {
            fun fromValue(value: Int): Direction =
                entries.find { it.value == value } ?: BOTTOM_RIGHT
        }
    }

    /** 直角所在方向，对角自动为凹圆角 */
    var direction: Direction = Direction.BOTTOM_RIGHT
        set(value) { field = value; rebuildPath() }

    /** 填充颜色 */
    var fillColor: Int = Color.WHITE
        set(value) { field = value; paint.color = value; invalidate() }

    private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.FILL
    }

    private val path = Path()

    init {
        attrs?.let {
            val ta = context.obtainStyledAttributes(it, R.styleable.ReverseCornerView)
            val dirValue = ta.getInt(R.styleable.ReverseCornerView_reverseDirection, Direction.BOTTOM_RIGHT.value)
            direction = Direction.fromValue(dirValue)
            fillColor = ta.getColor(R.styleable.ReverseCornerView_fillColor, Color.WHITE)
            ta.recycle()
        }
        paint.color = fillColor
    }

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        super.onSizeChanged(w, h, oldw, oldh)
        rebuildPath()
    }

    private fun rebuildPath() {
        val w = width.toFloat()
        val h = height.toFloat()
        if (w <= 0f || h <= 0f) return

        // 凹圆角半径取宽高最小值
        val r = minOf(w, h)

        path.reset()

        when (direction) {
            // 直角在右下 (w,h)，凹圆角在左上 (0,0)
            Direction.BOTTOM_RIGHT -> {
                path.moveTo(w, h)
                path.lineTo(w, 0f)
                path.lineTo(r, 0f)
                path.arcTo(RectF(-r, -r, r, r), 0f, 90f, false)
                path.lineTo(0f, h)
                path.close()
            }

            // 直角在左下 (0,h)，凹圆角在右上 (w,0)
            Direction.BOTTOM_LEFT -> {
                path.moveTo(0f, h)
                path.lineTo(0f, 0f)
                path.lineTo(w - r, 0f)
                path.arcTo(RectF(w - r, -r, w + r, r), 180f, -90f, false)
                path.lineTo(w, h)
                path.close()
            }

            // 直角在左上 (0,0)，凹圆角在右下 (w,h)
            Direction.TOP_LEFT -> {
                path.moveTo(0f, 0f)
                path.lineTo(0f, h)
                path.lineTo(w - r, h)
                path.arcTo(RectF(w - r, h - r, w + r, h + r), 180f, -90f, false)
                path.lineTo(w, 0f)
                path.close()
            }

            // 直角在右上 (w,0)，凹圆角在左下 (0,h)
            Direction.TOP_RIGHT -> {
                path.moveTo(w, 0f)
                path.lineTo(w, h)
                path.lineTo(r, h)
                path.arcTo(RectF(-r, h - r, r, h + r), 0f, -90f, false)
                path.lineTo(0f, 0f)
                path.close()
            }
        }

        invalidate()
    }

    override fun draw(canvas: Canvas) {
        super.draw(canvas)
        if (width > 0 && height > 0) {
            canvas.drawPath(path, paint)
        }
    }
}