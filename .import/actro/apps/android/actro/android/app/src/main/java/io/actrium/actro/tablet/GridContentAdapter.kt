package io.actrium.actro.tablet

import android.graphics.Bitmap
import android.os.VibrationEffect
import android.os.Vibrator
import android.util.Log
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import androidx.cardview.widget.CardView
import androidx.core.content.ContextCompat
import androidx.core.view.isVisible
import androidx.recyclerview.widget.RecyclerView
import io.actrium.actro.R
import io.actrium.actro.model.ActiveModel
import io.actrium.actro.model.AppStatus

class GridContentAdapter : RecyclerView.Adapter<RecyclerView.ViewHolder>() {

    init {
        setHasStableIds(true)
    }

    companion object {
        private const val VIEW_TYPE_CONTENT = 0
        private const val VIEW_TYPE_ADD_BUTTON = 1
    }

    private val items = mutableListOf<ActiveModel>()
    var onItemClick: ((Int, View) -> Unit)? = null
    var onAddClick: (() -> Unit)? = null
    var onDeleteClick: ((String) -> Unit)? = null
    var onMarkClick: ((ActiveModel) -> Unit)? = null
    var onItemLongClick: ((ActiveModel, Int, Int) -> Unit)? = null
    var containerWidth = 0
    var containerHeight = 0
    var spanCount = 3
    var spacingPx = 0
    var marginHorizontalPx = 0

    private var recyclerView: RecyclerView? = null

    // 正在播放删除动画的 item id 集合，动画期间 updateItems 跳过 notifyDataSetChanged
    private val animatingDeleteIds = mutableSetOf<String>()

    override fun onAttachedToRecyclerView(recyclerView: RecyclerView) {
        super.onAttachedToRecyclerView(recyclerView)
        this.recyclerView = recyclerView
    }

    override fun onDetachedFromRecyclerView(recyclerView: RecyclerView) {
        super.onDetachedFromRecyclerView(recyclerView)
        this.recyclerView = null
    }

    fun updateItems(newItems: List<ActiveModel>) {
        val isAnimating = animatingDeleteIds.isNotEmpty()
        items.clear()
        items.addAll(newItems)
        if (isAnimating) {
            // 动画期间只更新数据，不刷新视图
            return
        }
        notifyDataSetChanged()
    }

    override fun getItemId(position: Int): Long {
        return if (position < items.size) items[position].id.hashCode().toLong() else RecyclerView.NO_ID
    }

    /**
     * 删除 item 并播放动画：后方 item 平移到新位置
     * 策略：先移除数据并刷新布局（无动画），再将受影响的 item 偏移回原位并动画滑入
     */
    fun removeItemWithAnimation(id: String, onAnimationEnd: (() -> Unit)? = null) {
        val position = items.indexOfFirst { it.id == id }
        if (position == -1) {
            onAnimationEnd?.invoke()
            return
        }

        animatingDeleteIds.add(id)

        val viewHolder = recyclerView?.findViewHolderForAdapterPosition(position)
        val itemView = viewHolder?.itemView

        if (itemView == null) {
            items.removeAt(position)
            animatingDeleteIds.remove(id)
            notifyDataSetChanged()
            onAnimationEnd?.invoke()
            return
        }

        // 删除前：记录每个可见 item 的位置（通过 itemId 映射，布局后 ViewHolder 可能复用）
        val layoutManager = recyclerView?.layoutManager as? androidx.recyclerview.widget.GridLayoutManager
        val firstVisible = layoutManager?.findFirstVisibleItemPosition() ?: 0
        val lastVisible = layoutManager?.findLastVisibleItemPosition() ?: -1

        // 记录删除位置之后每个 item 的原始位置，key = itemId
        val oldPositions = mutableMapOf<Long, IntArray>()  // itemId -> [left, top]
        for (i in position..lastVisible.coerceAtLeast(0)) {
            val vh = recyclerView?.findViewHolderForAdapterPosition(i) ?: continue
            oldPositions[vh.itemId] = intArrayOf(vh.itemView.left, vh.itemView.top)
        }

        // 禁用默认 ItemAnimator
        val originalAnimator = recyclerView?.itemAnimator
        recyclerView?.itemAnimator = null

        // 先移除数据并刷新，RecyclerView 会把后方 item 直接排到正确位置
        items.removeAt(position)
        notifyDataSetChanged()

        // 等布局完成后，对受影响的 item 做平移动画
        recyclerView?.post {
            val newLayoutManager = recyclerView?.layoutManager ?: return@post
            val newFirstVisible = (newLayoutManager as? androidx.recyclerview.widget.GridLayoutManager)
                ?.findFirstVisibleItemPosition() ?: 0
            val newLastVisible = (newLayoutManager as? androidx.recyclerview.widget.GridLayoutManager)
                ?.findLastVisibleItemPosition() ?: -1

            // 先找到第一个有位移记录的 item，获取参考位移量（用于屏幕外新进入的 item）
            var refDx = 0f
            var refDy = 0f
            for (i in maxOf(position, newFirstVisible)..newLastVisible.coerceAtLeast(0)) {
                val vh = recyclerView?.findViewHolderForAdapterPosition(i) ?: continue
                val oldPos = oldPositions[vh.itemId]
                if (oldPos != null) {
                    val dx = oldPos[0] - vh.itemView.left
                    val dy = oldPos[1] - vh.itemView.top
                    if (dx != 0 || dy != 0) {
                        refDx = dx.toFloat()
                        refDy = dy.toFloat()
                        break
                    }
                }
            }

            for (i in maxOf(position, newFirstVisible)..newLastVisible.coerceAtLeast(0)) {
                val vh = recyclerView?.findViewHolderForAdapterPosition(i) ?: continue
                val child = vh.itemView
                val oldPos = oldPositions[vh.itemId]
                val dx: Float
                val dy: Float
                if (oldPos != null) {
                    // 删除前有记录，用精确偏移
                    dx = (oldPos[0] - child.left).toFloat()
                    dy = (oldPos[1] - child.top).toFloat()
                } else {
                    // 屏幕外新进入的 item，用参考位移量
                    dx = refDx
                    dy = refDy
                }
                if (dx != 0f || dy != 0f) {
                    child.translationX = dx
                    child.translationY = dy
                    child.animate()
                        .translationX(0f)
                        .translationY(0f)
                        .setDuration(250L)
                        .start()
                }
            }

            // 动画结束后恢复 ItemAnimator
            recyclerView?.postDelayed({
                animatingDeleteIds.remove(id)
                recyclerView?.itemAnimator = originalAnimator
            }, 260L)

            onAnimationEnd?.invoke()
        }
    }

    override fun getItemCount(): Int = items.size + 1

    override fun getItemViewType(position: Int): Int {
        return if (position == items.size) {
            VIEW_TYPE_ADD_BUTTON
        } else {
            VIEW_TYPE_CONTENT
        }
    }

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder {
        return when (viewType) {
            VIEW_TYPE_ADD_BUTTON -> {
                val view = LayoutInflater.from(parent.context)
                    .inflate(R.layout.item_safari_add, parent, false)
                AddButtonViewHolder(view)
            }

            else -> {
                val view = LayoutInflater.from(parent.context)
                    .inflate(R.layout.item_grid_content, parent, false)
                GridItemViewHolder(view)
            }
        }
    }

    override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) {
        // 重置动画属性，防止 ViewHolder 复用时残留
        holder.itemView.translationX = 0f
        holder.itemView.translationY = 0f
        holder.itemView.alpha = 1f
        when (holder) {
            is AddButtonViewHolder -> {
                holder.itemView.setOnClickListener {
                    onAddClick?.invoke()
                }
            }

            is GridItemViewHolder -> {
                val item = items[position]
                holder.bind(item)

                // 设置点击事件和长按事件
                var touchX = 0
                var touchY = 0
                holder.ivScreenshot.setOnTouchListener { _, event ->
                    touchX = event.rawX.toInt()
                    touchY = event.rawY.toInt()
                    false
                }
                holder.ivScreenshot.setOnClickListener {
                    onItemClick?.invoke(position, holder.ivScreenshot)
                }
                holder.ivScreenshot.setOnLongClickListener {
                    // 长按震动反馈
                    val vibrator = holder.ivScreenshot.context.getSystemService(android.content.Context.VIBRATOR_SERVICE) as Vibrator
                    vibrator.vibrate(VibrationEffect.createOneShot(30, VibrationEffect.DEFAULT_AMPLITUDE))
                    onItemLongClick?.invoke(item, touchX, touchY)
                    true
                }

                holder.ivMark.setOnClickListener {
                    Log.e("GridContentAdapter", "ivMark.setOnClickListener")
                    onMarkClick?.invoke(item)
                }

                // 设置删除按钮点击事件
                holder.ivDelete.setOnClickListener {
                    onDeleteClick?.invoke(item.id)
                }
            }
        }
    }

    inner class GridItemViewHolder(itemView: View) : RecyclerView.ViewHolder(itemView) {
        private val cardView: CardView = itemView.findViewById(R.id.cardView)
        val ivDelete: ImageView = itemView.findViewById(R.id.ivDelete)
        val ivMark: ImageView = itemView.findViewById(R.id.ivMark)
        val tvName: TextView = itemView.findViewById(R.id.tvName)
        val ivScreenshot: ImageView = itemView.findViewById(R.id.ivScreenshot)

        fun bind(item: ActiveModel) {
            val density = itemView.context.resources.displayMetrics.density

            val cardLayoutParams = cardView.layoutParams as LinearLayout.LayoutParams

            if (item.screenshot.isNotEmpty()) {
                ivScreenshot.visibility = View.VISIBLE
                val bitmap = decodeScreenshot(item.screenshot)
                if (bitmap != null) {
                    ivScreenshot.setImageBitmap(bitmap)
                } else {
                    ivScreenshot.setImageResource(0)
                }
            } else {
                ivScreenshot.visibility = View.GONE
            }

            cardView.radius = 12f * density

            if (item.name.isEmpty()) {
                tvName.setText(R.string.blank_tab)
                ivMark.isVisible = false
            } else {
                ivMark.isVisible = true
                tvName.text = if (item.status == AppStatus.DEAD) {
                    "${item.name}(${itemView.context.getString(R.string.server_closed)})"
                } else {
                    item.name
                }
                if (item.isMarked) {
                    ivMark.setColorFilter(ContextCompat.getColor(ivMark.context, R.color.yellow))
                } else {
                    ivMark.setColorFilter(ContextCompat.getColor(ivMark.context, R.color.icon_1))
                }
            }

            val margin = (8 * density).toInt()
            cardLayoutParams.setMargins(margin, margin, margin, margin)
            cardLayoutParams.width = LinearLayout.LayoutParams.MATCH_PARENT
            cardLayoutParams.height = 0
            cardLayoutParams.weight = 1f

            // ivScreenshot 填满 CardView，用 FIT_CENTER 保持宽高比
            val screenshotParams = ivScreenshot.layoutParams
            screenshotParams.width = FrameLayout.LayoutParams.MATCH_PARENT
            screenshotParams.height = FrameLayout.LayoutParams.MATCH_PARENT
            ivScreenshot.layoutParams = screenshotParams

            cardView.layoutParams = cardLayoutParams
        }

        private fun decodeScreenshot(path: String): Bitmap? {
            return try {
                android.graphics.BitmapFactory.decodeFile(path)
            } catch (e: Exception) {
                Log.e("GridContentAdapter", "Failed to decode screenshot", e)
                null
            }
        }
    }

    class AddButtonViewHolder(itemView: View) : RecyclerView.ViewHolder(itemView)
}