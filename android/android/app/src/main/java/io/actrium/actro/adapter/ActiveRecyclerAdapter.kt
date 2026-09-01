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
    // The records are already shown. item id, Add item Delay 300ms Show only after
    private val shownIds = mutableSetOf<String>()
    var onItemClick: ((ActiveModel, View) -> Unit)? = null
    var onDeleteClick: ((ActiveModel) -> Unit)? = null
    var onMarkClick: ((ActiveModel) -> Unit)? = null
    var onItemLongClick: ((ActiveModel, Int, Int) -> Unit)? = null

    private var recyclerView: RecyclerView? = null

    // Playing the deleted animation item id Gathering, during the animation. updateData Skip notifyDataSetChanged
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
        // If there is one, item Playing delete animations, updating only data sources without refreshing view, and then refreshing after animation
        val isAnimating = animatingDeleteIds.isNotEmpty()
        val isInitialLoad = items.isEmpty() && shownIds.isEmpty()
        val newItemIds = newList.map { it.id }.toSet() - items.map { it.id }.toSet() - shownIds
        items.clear()
        items.addAll(newList)

        if (isAnimating) {
            // Only update data during animation, not refresh views
            return
        }

        if (isInitialLoad) {
            // Show all direct displays first load
            shownIds.addAll(newList.map { it.id })
            notifyDataSetChanged()
        } else {
            notifyDataSetChanged()
            // Add item Delay 300ms Mark as shown and refreshed after
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
        // Reset animated properties to prevent ViewHolder Residues on reuse
        holder.itemView.translationX = 0f
        holder.itemView.translationY = 0f
        holder.itemView.alpha = 1f
        val isShown = shownIds.contains(items[position].id)
        holder.bind(items[position], onItemClick, onDeleteClick, onMarkClick, onItemLongClick, isShown)
    }

    override fun getItemCount(): Int = items.size

    /**
     * Delete item And play animated: below item Offset Up
     * Policy: Remove data and refresh layout (no animation) before below item Move back to the original position and move animate
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

        // Disable Default ItemAnimator
        val originalAnimator = recyclerView?.itemAnimator
        recyclerView?.itemAnimator = null

        // The first thing is to remove the data and refresh it.RecyclerView It'll take the bottom. item Line straight to the right position.
        // (including new entry on the screen) itemLike the No.6(a)
        items.removeAt(position)
        shownIds.remove(id)
        notifyDataSetChanged()

        // All visible starting with the deleted position after the layout is finished item Make a move-up picture
        recyclerView?.post {
            val layoutManager = recyclerView?.layoutManager as? androidx.recyclerview.widget.LinearLayoutManager
            val firstVisible = layoutManager?.findFirstVisibleItemPosition() ?: 0
            val lastVisible = layoutManager?.findLastVisibleItemPosition() ?: -1

            for (i in maxOf(position, firstVisible)..lastVisible.coerceAtLeast(0)) {
                val vh = recyclerView?.findViewHolderForAdapterPosition(i) ?: continue
                // First push to the old position (one below) item Height) and then animated to final position
                vh.itemView.translationY = deletedHeight.toFloat()
                vh.itemView.animate()
                    .translationY(0f)
                    .setDuration(250L)
                    .start()
            }

            // Restore after animation ItemAnimator
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
            // Add item Hide Before Show Delay
            binding.root.visibility = if (isShown) View.VISIBLE else View.INVISIBLE
            binding.tvAppName.text = if (item.status == AppStatus.DEAD) {
                "${item.name}(${binding.root.context.getString(R.string.server_closed)})"
            } else {
                item.name
            }

            // Reapplies theme-related colours (%2)configChanges Organisation uiMode Time View No automatic color update)
            binding.root.background = ContextCompat.getDrawable(binding.root.context, R.drawable.item_active_bg)
            binding.llAppBottom.background = ContextCompat.getDrawable(binding.root.context, R.drawable.app_bottom_bg)
            binding.tvAppName.setTextColor(ContextCompat.getColor(binding.root.context, R.color.title_text))
            binding.ivDelete.setColorFilter(ContextCompat.getColor(binding.root.context, R.color.icon_1))

            // Unique use Coil Load it to auto-process view reuse
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
            // Record the contact position for long-term back-to-back
            var touchX = 0
            var touchY = 0
            binding.root.setOnTouchListener { _, event ->
                touchX = event.rawX.toInt()
                touchY = event.rawY.toInt()
                false
            }
            binding.root.setOnLongClickListener {
                // Long-pressed vibration feedback
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
