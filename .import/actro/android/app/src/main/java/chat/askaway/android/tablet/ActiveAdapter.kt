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
import com.askaway.android.databinding.ItemTabletAppLeftPadBinding
import com.askaway.android.databinding.ItemTabletAppRightPadBinding
import com.askaway.android.model.ActiveModel

class ActiveAdapter : RecyclerView.Adapter<RecyclerView.ViewHolder>() {

    companion object {
        private const val TYPE_LEFT_PAD = 0
        private const val TYPE_NORMAL = 1
        private const val TYPE_RIGHT_PAD = 2
    }

    private val items = mutableListOf<ActiveModel>()
    var onItemClick: ((ActiveModel) -> Unit)? = null
    var onDeleteClick: ((ActiveModel) -> Unit)? = null
    var onItemLongClick: ((ActiveModel, Int, Int) -> Unit)? = null
    /** 选中项变化时的回调，参数为选中项在真实数据中的 position */
    var onSelectedChanged: ((Int) -> Unit)? = null

    var currentId: String? = ""
        set(value) {
            field = value
            notifyDataSetChanged()
            val pos = items.indexOfFirst { it.id == value }
            if (pos >= 0) {
                onSelectedChanged?.invoke(pos)
            }
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
            // 对新增 item 记录需要动画的 position（转换为 adapter position）
            newItems.forEach { newItem ->
                val dataPos = items.indexOfFirst { it.id == newItem.id }
                if (dataPos >= 0) {
                    pendingAnimatePositions.add(dataPos + 1) // +1 偏移，因为前面有 left pad
                }
            }
        }
    }

    /**
     * 添加新 item 并播放入场动画（从底部滑入）
     */
    fun addItemWithAnimation(item: ActiveModel) {
        items.add(item)
        val insertPosition = items.size // adapter position = 数据 size（因为前面有 left pad）
        notifyItemInserted(insertPosition)
        pendingAnimatePositions.add(insertPosition)
    }

    /** 需要播放入场动画的 position 集合（存储的是 adapter position） */
    private val pendingAnimatePositions = mutableSetOf<Int>()

    override fun getItemViewType(position: Int): Int {
        if (items.isEmpty()) return TYPE_NORMAL
        return when (position) {
            0 -> TYPE_LEFT_PAD
            items.size + 1 -> TYPE_RIGHT_PAD
            else -> TYPE_NORMAL
        }
    }

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder {
        val inflater = LayoutInflater.from(parent.context)
        return when (viewType) {
            TYPE_LEFT_PAD -> LeftPadViewHolder(
                ItemTabletAppLeftPadBinding.inflate(inflater, parent, false)
            )
            TYPE_RIGHT_PAD -> RightPadViewHolder(
                ItemTabletAppRightPadBinding.inflate(inflater, parent, false)
            )
            else -> ActiveViewHolder(
                ItemTabletAppBinding.inflate(inflater, parent, false)
            )
        }
    }

    override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) {
        when (holder) {
            is LeftPadViewHolder -> bindLeftPad(holder, position)
            is RightPadViewHolder -> bindRightPad(holder, position)
            is ActiveViewHolder -> bindNormal(holder, position)
        }
    }

    override fun getItemCount(): Int {
        if (items.isEmpty()) return 0
        return items.size + 2 // 左右各一个虚拟 item
    }

    /** 数据 item 的数量（不含虚拟 item） */
    val dataItemCount: Int get() = items.size

    private fun bindLeftPad(holder: LeftPadViewHolder, position: Int) {
        val selectedPosition = items.indexOfFirst { it.id == currentId }
        val selectedColor = "#F2F2F6".toColorInt()

        // 选中原来第一个 item 时（selectedPosition == 0），左侧虚拟 item 显示 rcvBottomRight
        if (selectedPosition == 0) {
            holder.binding.rcvBottomRight.fillColor = selectedColor
            holder.binding.rcvBottomRight.visibility = View.VISIBLE
        } else {
            holder.binding.rcvBottomRight.visibility = View.GONE
        }
    }

    private fun bindRightPad(holder: RightPadViewHolder, position: Int) {
        val selectedPosition = items.indexOfFirst { it.id == currentId }
        val selectedColor = "#F2F2F6".toColorInt()

        // 选中原来最后一个 item 时，右侧虚拟 item 显示 rcvBottomLeft
        if (selectedPosition == items.size - 1) {
            holder.binding.rcvBottomLeft.fillColor = selectedColor
            holder.binding.rcvBottomLeft.visibility = View.VISIBLE
        } else {
            holder.binding.rcvBottomLeft.visibility = View.GONE
        }
    }

    private fun bindNormal(holder: ActiveViewHolder, position: Int) {
        Log.e("ActiveAdapter", "onBindViewHolder")
        val dataPosition = position - 1 // 减去左侧虚拟 item 的偏移
        val item = items[dataPosition]
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
            // 左右虚拟 item 保证真实 item 始终有邻居，桥接 View 始终显示
            holder.binding.vBridgeLeft.visibility = View.VISIBLE
            holder.binding.vBridgeRight.visibility = View.VISIBLE
        } else {
            holder.binding.llContent.setBackgroundResource(R.drawable.tablet_tab_bg_normal)
            // 前一个item的右下角显示凹圆角
            if (dataPosition == selectedPosition - 1) {
                holder.binding.rcvBottomRight.fillColor = selectedColor
                holder.binding.rcvBottomRight.visibility = View.VISIBLE
            } else {
                holder.binding.rcvBottomRight.visibility = View.GONE
            }
            // 后一个item的左下角显示凹圆角
            if (dataPosition == selectedPosition + 1) {
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

    class LeftPadViewHolder(val binding: ItemTabletAppLeftPadBinding) : RecyclerView.ViewHolder(binding.root)

    class RightPadViewHolder(val binding: ItemTabletAppRightPadBinding) : RecyclerView.ViewHolder(binding.root)

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