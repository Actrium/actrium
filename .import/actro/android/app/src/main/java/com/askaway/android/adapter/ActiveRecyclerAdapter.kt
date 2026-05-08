package com.askaway.android.adapter

import android.view.LayoutInflater
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import androidx.core.content.ContextCompat
import androidx.core.view.isVisible
import androidx.recyclerview.widget.RecyclerView
import coil.load
import com.askaway.android.R
import com.askaway.android.databinding.ItemActiveBinding
import com.askaway.android.model.ActiveModel

class ActiveRecyclerAdapter : RecyclerView.Adapter<ActiveRecyclerAdapter.ActiveViewHolder>() {
    private val items = mutableListOf<ActiveModel>()
    var onItemClick: ((ActiveModel, View) -> Unit)? = null
    var onDeleteClick: ((ActiveModel) -> Unit)? = null
    var onMarkClick: ((ActiveModel) -> Unit)? = null
    var onItemLongClick: ((ActiveModel, Int, Int) -> Unit)? = null

    fun updateData(newList: List<ActiveModel>) {
        items.clear()
        items.addAll(newList)
        notifyDataSetChanged()
    }

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): ActiveViewHolder {
        val binding = ItemActiveBinding.inflate(LayoutInflater.from(parent.context), parent, false)
        return ActiveViewHolder(binding)
    }

    override fun onBindViewHolder(holder: ActiveViewHolder, position: Int) {
        holder.bind(items[position], onItemClick, onDeleteClick, onMarkClick, onItemLongClick)
    }

    override fun getItemCount(): Int = items.size

    fun findPositionById(id: String): Int = items.indexOfFirst { it.id == id }

    fun getItemAt(position: Int): ActiveModel? = items.getOrNull(position)

    class ActiveViewHolder(private val binding: ItemActiveBinding) : RecyclerView.ViewHolder(binding.root) {
        fun bind(
            item: ActiveModel,
            onItemClick: ((ActiveModel, View) -> Unit)?,
            onDeleteClick: ((ActiveModel) -> Unit)?,
            onMarkClick: ((ActiveModel) -> Unit)?,
            onItemLongClick: ((ActiveModel, Int, Int) -> Unit)?
        ) {
            binding.tvAppName.text = item.name

            // 统一使用 Coil 加载，让它自动处理视图复用
            if (item.icon.isEmpty()) {
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
