package io.actrium.app.tablet

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import android.util.AttributeSet
import android.view.View
import io.actrium.app.R

/**
 * Customises the given direction inverted angles (deep corners) View.
 *
 * Pass. reverseDirection Specifies the direction of the straight angle, with the diagonal angle automatically generated.
 * Dimple radius = min(width, height), no manual settings are required.
 *
 * Direction statement (direct angle position) → Dimple angle position:
 * - topLeft:     Top Left of the Straight Angle → Bottom Right of the Dimple
 * - topRight:    Top right of the straight corner → Bottom Left of the Dimple
 * - bottomLeft:  Bottom Left of the Straight Angle → Top Right of the Dimple
 * - bottomRight: Right-hand corner bottom → Top Left of the Dimple
 *
 * Example used:
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

    /** The angle is in the direction of the angle, and the diagonal angle is automatically dented. */
    var direction: Direction = Direction.BOTTOM_RIGHT
        set(value) { field = value; rebuildPath() }

    /** Fill Colour */
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

        // Decimal radius to take the width to the minimum
        val r = minOf(w, h)

        path.reset()

        when (direction) {
            // Right corner down right. (w,h)Zoomed corner above left (0,0)
            Direction.BOTTOM_RIGHT -> {
                path.moveTo(w, h)
                path.lineTo(w, 0f)
                path.lineTo(r, 0f)
                path.arcTo(RectF(-r, -r, r, r), 0f, 90f, false)
                path.lineTo(0f, h)
                path.close()
            }

            // Straight angle down left. (0,h)Zoom in the top right. (w,0)
            Direction.BOTTOM_LEFT -> {
                path.moveTo(0f, h)
                path.lineTo(0f, 0f)
                path.lineTo(w - r, 0f)
                path.arcTo(RectF(w - r, -r, w + r, r), 180f, -90f, false)
                path.lineTo(w, h)
                path.close()
            }

            // Straight corner top left (0,0)The dent is down right. (w,h)
            Direction.TOP_LEFT -> {
                path.moveTo(0f, 0f)
                path.lineTo(0f, h)
                path.lineTo(w - r, h)
                path.arcTo(RectF(w - r, h - r, w + r, h + r), 180f, -90f, false)
                path.lineTo(w, 0f)
                path.close()
            }

            // Right corner top right. (w,0)Zoomed corner down to the left (0,h)
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