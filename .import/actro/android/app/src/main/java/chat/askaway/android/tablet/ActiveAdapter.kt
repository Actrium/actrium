package chat.askaway.android.tablet

import android.os.Build
import android.util.Log
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.view.animation.DecelerateInterpolator
import androidx.core.graphics.toColorInt
import androidx.recyclerview.widget.RecyclerView
import com.askaway.android.R
import com.askaway.android.databinding.ItemTabletAppBinding
import com.askaway.android.model.ActiveModel

class ActiveAdapter : RecyclerView.Adapter<ActiveAdapter.ActiveViewHolder>() {

    private val items = mutableListOf<ActiveModel>()
    var onItemClick: ((ActiveModel) -> Unit)? = null
    var onDeleteClick: ((ActiveModel) -> Unit)? = null
    var onItemLongClick: ((ActiveModel, Int, Int) -> Unit)? = null
    var currentId: String? = ""
        set(value) {
            field = value
            notifyDataSetChanged()
        }

    fun updateData(newList: List<ActiveModel>) {
        // 检测新增的 item id
        val existingIds = items.map { it.id }.toSet()
        val newItems = newList.filter { it.id !in existingIds }

        items.clear()
        items.addAll(newList)
        if (newItems.isEmpty()) {
            notifyDataSetChanged()
        } else {
            notifyDataSetChanged()
            // 对新增 item 记录需要动画的 position
            newItems.forEach { newItem ->
                val pos = items.indexOfFirst { it.id == newItem.id }
                if (pos >= 0) {
                    pendingAnimatePositions.add(pos)
                }
            }
        }
    }

    /**
     * 添加新 item 并播放入场动画（从底部滑入）
     */
    fun addItemWithAnimation(item: ActiveModel) {
        items.add(item)
        val insertPosition = items.size - 1
        notifyItemInserted(insertPosition)
        pendingAnimatePositions.add(insertPosition)
    }

    /** 需要播放入场动画的 position 集合 */
    private val pendingAnimatePositions = mutableSetOf<Int>()


    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): ActiveViewHolder {
        val binding = ItemTabletAppBinding.inflate(LayoutInflater.from(parent.context), parent, false)
        return ActiveViewHolder(binding)
    }

    override fun onBindViewHolder(holder: ActiveViewHolder, position: Int) {
        Log.e("ActiveAdapter", "onBindViewHolder")
        val item = items[position]
        holder.bind(item, onItemClick, onDeleteClick, onItemLongClick)
        holder.binding.ivDel.visibility = View.VISIBLE

        // 新增 item 入场动画：从底部滑入
        if (position in pendingAnimatePositions) {
            pendingAnimatePositions.remove(position)
            holder.itemView.translationY = holder.itemView.height.toFloat()
            holder.itemView.alpha = 0f
            holder.itemView.animate()
                .translationY(0f)
                .alpha(1f)
                .setDuration(300)
                .setInterpolator(DecelerateInterpolator())
                .start()
        } else {
            holder.itemView.translationY = 0f
            holder.itemView.alpha = 1f
        }

        val selectedPosition = items.indexOfFirst { it.id == currentId }

        val selectedColor = "#F2F2F6".toColorInt()

        if (item.id == currentId) {
            holder.binding.llContent.setBackgroundResource(R.drawable.tablet_tab_bg_selected)
            holder.binding.vBridgeLeft.fillColor = selectedColor
            holder.binding.vBridgeRight.fillColor = selectedColor
            holder.binding.rcvBottomLeft.visibility = View.GONE
            holder.binding.rcvBottomRight.visibility = View.GONE
            // 选中项：显示桥接View填补与邻居item之间的间距
            holder.binding.vBridgeLeft.visibility =
                if (selectedPosition > 0) View.VISIBLE else View.GONE
            holder.binding.vBridgeRight.visibility =
                if (selectedPosition < itemCount - 1) View.VISIBLE else View.GONE
        } else {
            holder.binding.llContent.setBackgroundResource(R.drawable.tablet_tab_bg_normal)
            // 前一个item的右下角显示凹圆角
            if (position == selectedPosition - 1) {
                holder.binding.rcvBottomRight.fillColor = selectedColor
                holder.binding.rcvBottomRight.visibility = View.VISIBLE
            } else {
                holder.binding.rcvBottomRight.visibility = View.GONE
            }
            // 后一个item的左下角显示凹圆角
            if (position == selectedPosition + 1) {
                holder.binding.rcvBottomLeft.fillColor = selectedColor
                holder.binding.rcvBottomLeft.visibility = View.VISIBLE
            } else {
                holder.binding.rcvBottomLeft.visibility = View.GONE
            }
            // 非选中项不显示桥接View
            holder.binding.vBridgeLeft.visibility = View.GONE
            holder.binding.vBridgeRight.visibility = View.GONE
        }
    }

    override fun getItemCount(): Int = items.size

    class ActiveViewHolder(val binding: ItemTabletAppBinding) : RecyclerView.ViewHolder(binding.root) {
        fun bind(
            item: ActiveModel,
            onItemClick: ((ActiveModel) -> Unit)?,
            onDeleteClick: ((ActiveModel) -> Unit)?,
            onItemLongClick: ((ActiveModel, Int, Int) -> Unit)?
        ) {
            if (item.name.isEmpty()) {
                binding.tvAppName.setText(R.string.blank_tab)
            } else {
                binding.tvAppName.text = item.name
            }
            binding.ivDel.setOnClickListener { onDeleteClick?.invoke(item) }
            binding.root.setOnClickListener { onItemClick?.invoke(item) }
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
        }
    }
}