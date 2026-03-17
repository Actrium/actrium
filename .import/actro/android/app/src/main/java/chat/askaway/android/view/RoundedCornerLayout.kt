package chat.askaway.android.view

import android.content.Context
import android.graphics.Canvas
import android.graphics.Path
import android.graphics.RectF
import android.util.AttributeSet
import android.util.TypedValue
import android.widget.FrameLayout
import com.askaway.android.R
import kotlin.math.max
import kotlin.math.min

/**
 * FrameLayout with independently configurable corner radii.
 * It clips child drawing to rounded bounds.
 */
class RoundedCornerLayout @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
    defStyleAttr: Int = 0
) : FrameLayout(context, attrs, defStyleAttr) {

    private val clipPath = Path()
    private val rect = RectF()

    private var topLeftRadiusPx = 0f
    private var topRightRadiusPx = 0f
    private var bottomRightRadiusPx = 0f
    private var bottomLeftRadiusPx = 0f

    init {
        setWillNotDraw(false)

        if (attrs != null) {
            val ta = context.obtainStyledAttributes(attrs, R.styleable.RoundedCornerLayout)
            try {
                val all = ta.getDimension(R.styleable.RoundedCornerLayout_cornerRadius, -1f)
                if (all >= 0f) {
                    topLeftRadiusPx = all
                    topRightRadiusPx = all
                    bottomRightRadiusPx = all
                    bottomLeftRadiusPx = all
                }

                val tl = ta.getDimension(R.styleable.RoundedCornerLayout_cornerTopLeftRadius, -1f)
                val tr = ta.getDimension(R.styleable.RoundedCornerLayout_cornerTopRightRadius, -1f)
                val br = ta.getDimension(R.styleable.RoundedCornerLayout_cornerBottomRightRadius, -1f)
                val bl = ta.getDimension(R.styleable.RoundedCornerLayout_cornerBottomLeftRadius, -1f)

                if (tl >= 0f) topLeftRadiusPx = tl
                if (tr >= 0f) topRightRadiusPx = tr
                if (br >= 0f) bottomRightRadiusPx = br
                if (bl >= 0f) bottomLeftRadiusPx = bl
            } finally {
                ta.recycle()
            }
        }
    }

    fun setAllCornerRadiusDp(radiusDp: Float) {
        val r = dp(max(0f, radiusDp))
        setCornerRadiusPx(r, r, r, r)
    }

    fun setAllCornerRadiusPx(radiusPx: Float) {
        val r = max(0f, radiusPx)
        setCornerRadiusPx(r, r, r, r)
    }

    fun setCornerRadiusDp(
        topLeftDp: Float,
        topRightDp: Float,
        bottomRightDp: Float,
        bottomLeftDp: Float
    ) {
        setCornerRadiusPx(
            dp(max(0f, topLeftDp)),
            dp(max(0f, topRightDp)),
            dp(max(0f, bottomRightDp)),
            dp(max(0f, bottomLeftDp))
        )
    }

    fun setCornerRadiusPx(
        topLeftPx: Float,
        topRightPx: Float,
        bottomRightPx: Float,
        bottomLeftPx: Float
    ) {
        topLeftRadiusPx = max(0f, topLeftPx)
        topRightRadiusPx = max(0f, topRightPx)
        bottomRightRadiusPx = max(0f, bottomRightPx)
        bottomLeftRadiusPx = max(0f, bottomLeftPx)
        invalidate()
    }

    override fun dispatchDraw(canvas: Canvas) {
        val saveCount = canvas.save()
        buildClipPath(width.toFloat(), height.toFloat())
        canvas.clipPath(clipPath)
        super.dispatchDraw(canvas)
        canvas.restoreToCount(saveCount)
    }

    private fun buildClipPath(w: Float, h: Float) {
        clipPath.reset()
        if (w <= 0f || h <= 0f) return

        rect.set(0f, 0f, w, h)
        val maxR = min(w, h) / 2f

        val tl = min(topLeftRadiusPx, maxR)
        val tr = min(topRightRadiusPx, maxR)
        val br = min(bottomRightRadiusPx, maxR)
        val bl = min(bottomLeftRadiusPx, maxR)

        val radii = floatArrayOf(
            tl, tl,
            tr, tr,
            br, br,
            bl, bl
        )

        clipPath.addRoundRect(rect, radii, Path.Direction.CW)
        clipPath.close()
    }

    private fun dp(value: Float): Float {
        return TypedValue.applyDimension(
            TypedValue.COMPLEX_UNIT_DIP,
            value,
            resources.displayMetrics
        )
    }
}
