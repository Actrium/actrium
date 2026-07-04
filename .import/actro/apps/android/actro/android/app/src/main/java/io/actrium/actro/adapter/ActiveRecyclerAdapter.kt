package io.actrium.actro.adapter

import android.os.Build
import android.view.LayoutInflater
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import androidx.core.content.ContextCompat
import androidx.core.view.isVisible
import androidx.recyclerview.widget.RecyclerView
import coil.load
import io.actrium.actro.R
import io.actrium.actro.databinding.ItemActiveBinding
import io.actrium.actro.model.ActiveModel
import io.actrium.actro.model.AppStatus

class ActiveRecyclerAdapter : RecyclerView.Adapter<ActiveRecyclerAdapter.ActiveViewHolder>() {
    private val items = mutableListOf<ActiveModel>()
    // 记录已经显示过的 item id，新增 item 延时 300ms 后才显示
    private val shownIds = mutableSetOf<String>()
    var onItemClick: ((ActiveModel, View) -> Unit)? = null
    var onDeleteClick: ((ActiveModel) -> Unit)? = null
    var onMarkClick: ((ActiveModel) -> Unit)? = null
    var onItemLongClick: ((ActiveModel, Int, Int) -> Unit)? = null

    private var recyclerView: RecyclerView? = null

    // 正在播放删除动画的 item id 集合，动画期间 updateData 跳过 notifyDataSetChanged
    private val animatingDeleteIds = mutableSetOf<String>()

    override fun onAttachedToRecyclerView(recyclerView: RecyclerView) {
        super.onAttachedToRecyclerView(recyclerView)
        this.recyclerView = recyclerView
    }

    override fun onDetachedFromRecyclerView(recyclerView: RecyclerView) {
        super.onDetachedFromRecyclerView(recyclerView)
        this.recyclerView = null
    }

    fun updateData(newList: List<ActiveModel>) {
        // 如果有 item 正在播放删除动画，仅更新数据源但不刷新视图，等动画结束后再刷新
        val isAnimating = animatingDeleteIds.isNotEmpty()
        val isInitialLoad = items.isEmpty() && shownIds.isEmpty()
        val newItemIds = newList.map { it.id }.toSet() - items.map { it.id }.toSet() - shownIds
        items.clear()
        items.addAll(newList)

        if (isAnimating) {
            // 动画期间只更新数据，不刷新视图
            return
        }

        if (isInitialLoad) {
            // 首次加载直接全部显示
            shownIds.addAll(newList.map { it.id })
            notifyDataSetChanged()
        } else {
            notifyDataSetChanged()
            // 新增 item 延时 300ms 后标记为已显示并刷新
            if (newItemIds.isNotEmpty()) {
                recyclerView?.postDelayed({
                    for (id in newItemIds) {
                        shownIds.add(id)
                        val position = items.indexOfFirst { it.id == id }
                        if (position != -1) {
                            notifyItemChanged(position)
                        }
                    }
                }, 300)
            }
        }
    }

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): ActiveViewHolder {
        val binding = ItemActiveBinding.inflate(LayoutInflater.from(parent.context), parent, false)
        return ActiveViewHolder(binding)
    }

    override fun onBindViewHolder(holder: ActiveViewHolder, position: Int) {
        // 重置动画属性，防止 ViewHolder 复用时残留
        holder.itemView.translationX = 0f
        holder.itemView.translationY = 0f
        holder.itemView.alpha = 1f
        val isShown = shownIds.contains(items[position].id)
        holder.bind(items[position], onItemClick, onDeleteClick, onMarkClick, onItemLongClick, isShown)
    }

    override fun getItemCount(): Int = items.size

    /**
     * 删除 item 并播放动画：下方 item 向上偏移
     * 策略：先移除数据并刷新布局（无动画），再将下方 item 偏移回原位并动画滑入
     */
    fun removeItemWithAnimation(id: String, onAnimationEnd: (() -> Unit)? = null) {
        val position = items.indexOfFirst { it.id == id }
        if (position == -1) {
            onAnimationEnd?.invoke()
            return
        }

        animatingDeleteIds.add(id)

        val viewHolder = recyclerView?.findViewHolderForAdapterPosition(position)
        val deletedHeight = viewHolder?.itemView?.height ?: 0

        if (viewHolder == null || deletedHeight == 0) {
            items.removeAt(position)
            shownIds.remove(id)
            animatingDeleteIds.remove(id)
            notifyDataSetChanged()
            onAnimationEnd?.invoke()
            return
        }

        // 禁用默认 ItemAnimator
        val originalAnimator = recyclerView?.itemAnimator
        recyclerView?.itemAnimator = null

        // 先移除数据并刷新，RecyclerView 会把下方 item 直接排到正确位置
        // （包括屏幕外新进入的 item，比如第6个）
        items.removeAt(position)
        shownIds.remove(id)
        notifyDataSetChanged()

        // 等布局完成后，对所有从删除位置开始的可见 item 做向上平移动画
        recyclerView?.post {
            val layoutManager = recyclerView?.layoutManager as? androidx.recyclerview.widget.LinearLayoutManager
            val firstVisible = layoutManager?.findFirstVisibleItemPosition() ?: 0
            val lastVisible = layoutManager?.findLastVisibleItemPosition() ?: -1

            for (i in maxOf(position, firstVisible)..lastVisible.coerceAtLeast(0)) {
                val vh = recyclerView?.findViewHolderForAdapterPosition(i) ?: continue
                // 先推到旧位置（下方一个 item 高度），再动画滑到最终位置
                vh.itemView.translationY = deletedHeight.toFloat()
                vh.itemView.animate()
                    .translationY(0f)
                    .setDuration(250L)
                    .start()
            }

            // 动画结束后恢复 ItemAnimator
            recyclerView?.postDelayed({
                animatingDeleteIds.remove(id)
                recyclerView?.itemAnimator = originalAnimator
            }, 260L)

            onAnimationEnd?.invoke()
        }
    }

    fun findPositionById(id: String): Int = items.indexOfFirst { it.id == id }

    fun getItemAt(position: Int): ActiveModel? = items.getOrNull(position)

    class ActiveViewHolder(private val binding: ItemActiveBinding) : RecyclerView.ViewHolder(binding.root) {
        fun bind(
            item: ActiveModel,
            onItemClick: ((ActiveModel, View) -> Unit)?,
            onDeleteClick: ((ActiveModel) -> Unit)?,
            onMarkClick: ((ActiveModel) -> Unit)?,
            onItemLongClick: ((ActiveModel, Int, Int) -> Unit)?,
            isShown: Boolean = true
        ) {
            // 新增 item 延时显示前隐藏
            binding.root.visibility = if (isShown) View.VISIBLE else View.INVISIBLE
            binding.tvAppName.text = if (item.status == AppStatus.DEAD) {
                "${item.name}(${binding.root.context.getString(R.string.server_closed)})"
            } else {
                item.name
            }

            // 重新应用主题相关颜色（configChanges 包含 uiMode 时 View 不会自动更新颜色）
            binding.root.background = ContextCompat.getDrawable(binding.root.context, R.drawable.item_active_bg)
            binding.llAppBottom.background = ContextCompat.getDrawable(binding.root.context, R.drawable.app_bottom_bg)
            binding.tvAppName.setTextColor(ContextCompat.getColor(binding.root.context, R.color.title_text))
            binding.ivDelete.setColorFilter(ContextCompat.getColor(binding.root.context, R.color.icon_1))

            // 统一使用 Coil 加载，让它自动处理视图复用
            if (item.status == AppStatus.DEAD) {
                binding.ivAppIcon.load(R.drawable.close_icon)
            } else if (item.icon.isEmpty()) {
                binding.ivAppIcon.load(R.drawable.app_default)
            } else {
                binding.ivAppIcon.load(item.icon) {
                    placeholder(R.drawable.app_default)
                    error(R.drawable.app_default)
                }
            }
            // Load screenshot from local path if available
            if (item.screenshot.isNotEmpty()) {
                binding.ivScreenshot.isVisible = true
                binding.ivScreenshot.load(java.io.File(item.screenshot))
            } else {
                binding.ivScreenshot.isVisible = false
                binding.ivScreenshot.setImageDrawable(null) // Clear if no screenshot
            }
            binding.ivMark.isVisible = item.name.isNotEmpty()

            binding.root.setOnClickListener { onItemClick?.invoke(item, binding.root) }
            // 记录触点位置，供长按回调使用
            var touchX = 0
            var touchY = 0
            binding.root.setOnTouchListener { _, event ->
                touchX = event.rawX.toInt()
                touchY = event.rawY.toInt()
                false
            }
            binding.root.setOnLongClickListener {
                // 长按震动反馈
                val vibrator = binding.root.context.getSystemService(android.content.Context.VIBRATOR_SERVICE) as android.os.Vibrator
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                    vibrator.vibrate(android.os.VibrationEffect.createOneShot(30, android.os.VibrationEffect.DEFAULT_AMPLITUDE))
                } else {
                    @Suppress("DEPRECATION")
                    vibrator.vibrate(30)
                }
                onItemLongClick?.invoke(item, touchX, touchY)
                true
            }
            binding.ivDelete.setOnClickListener { onDeleteClick?.invoke(item) }
            binding.vUnreadBadge.visibility = if (item.hasUnread) View.VISIBLE else View.GONE

            binding.ivMark.setOnClickListener { onMarkClick?.invoke(item) }
            if (item.isMarked) {
                binding.ivMark.setColorFilter(ContextCompat.getColor(binding.root.context, R.color.yellow))
            } else {
                binding.ivMark.setColorFilter(ContextCompat.getColor(binding.root.context, R.color.icon_1))
            }
        }
    }
}
