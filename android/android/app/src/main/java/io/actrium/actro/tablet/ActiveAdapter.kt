package io.actrium.actro.tablet

import android.os.Build
import android.util.Log
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.view.animation.DecelerateInterpolator
import androidx.recyclerview.widget.RecyclerView
import io.actrium.actro.R
import io.actrium.actro.databinding.ItemTabletAppBinding
import io.actrium.actro.databinding.ItemTabletAppLeftPadBinding
import io.actrium.actro.databinding.ItemTabletAppRightPadBinding
import coil.load
import io.actrium.actro.model.ActiveModel
import io.actrium.actro.model.AppStatus

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
    /** The echo of the selected item when it changes, the parameter is the selected item in the actual data position */
    var onSelectedChanged: ((Int) -> Unit)? = null

    var currentId: String? = ""
        set(value) {
            if (field == value) return
            val oldDataPos = items.indexOfFirst { it.id == field }
            field = value
            val newDataPos = items.indexOfFirst { it.id == value }

            // Collection needs refreshing adapter positionAvoid notifyDataSetChanged It causes a shake.
            val changedPositions = mutableSetOf<Int>()
            // Left-left virtual item
            if (oldDataPos == 0 || newDataPos == 0) changedPositions.add(0)
            if (oldDataPos == items.size - 1 || newDataPos == items.size - 1) {
                if (items.isNotEmpty()) changedPositions.add(items.size + 1)
            }
            // Old selection and its neighbors
            if (oldDataPos >= 0) {
                changedPositions.add(oldDataPos + 1)
                if (oldDataPos > 0) changedPositions.add(oldDataPos)
                if (oldDataPos < items.size - 1) changedPositions.add(oldDataPos + 2)
            }
            // New selection and its neighbours
            if (newDataPos >= 0) {
                changedPositions.add(newDataPos + 1)
                if (newDataPos > 0) changedPositions.add(newDataPos)
                if (newDataPos < items.size - 1) changedPositions.add(newDataPos + 2)
            }
            changedPositions.forEach { pos ->
                if (pos in 0 until itemCount) notifyItemChanged(pos)
            }

            if (newDataPos >= 0) {
                onSelectedChanged?.invoke(newDataPos)
            }
        }

    fun updateData(newList: List<ActiveModel>) {
        val oldIds = items.map { it.id }
        val newIds = newList.map { it.id }

        if (oldIds == newIds) {
            // Data List id The order hasn't changed.
            var contentChanged = false
            for (i in items.indices) {
                if (items[i] != newList[i]) {
                    contentChanged = true
                    break
                }
            }
            items.clear()
            items.addAll(newList)
            if (contentChanged) {
                // Only to refresh changes in content item, avoid full-scale re-engineering leading to shaking
                for (i in newList.indices) {
                    notifyItemChanged(i + 1) // +1 Offset
                }
            }
            return
        }

        // Added or deleted changes, add new entries item Animation for entry
        val existingIds = oldIds.toSet()
        val newItems = newList.filter { it.id !in existingIds }

        items.clear()
        items.addAll(newList)
        notifyDataSetChanged()

        // Add item Recording needs animation. position(converted to adapter position)
        newItems.forEach { newItem ->
            val dataPos = items.indexOfFirst { it.id == newItem.id }
            if (dataPos >= 0) {
                pendingAnimatePositions.add(dataPos + 1) // +1 Offset because there's a front. left pad
            }
        }
    }

    /**
     * Add New item And play animation (sliding from the bottom)
     */
    fun addItemWithAnimation(item: ActiveModel) {
        items.add(item)
        val insertPosition = items.size // adapter position = Data size♪ 'Cause there's a way forward ♪ left pad)
        notifyItemInserted(insertPosition)
        pendingAnimatePositions.add(insertPosition)
    }

    /** You need to play animated entries. position ♪ Gathering ♪ adapter position) */
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
        return items.size + 2 // One virtual at a time. item
    }

    /** Data item Number of persons (excluding virtual) item) */
    val dataItemCount: Int get() = items.size

    private fun bindLeftPad(holder: LeftPadViewHolder, position: Int) {
        val selectedPosition = items.indexOfFirst { it.id == currentId }

        // Select the original first item Time (selectedPosition == 0Left virtual item Show rcvBottomRight
        holder.binding.rcvBottomRight.visibility =
            if (selectedPosition == 0) View.VISIBLE else View.GONE
    }

    private fun bindRightPad(holder: RightPadViewHolder, position: Int) {
        val selectedPosition = items.indexOfFirst { it.id == currentId }

        // Select the last one that has been selected item , Right Virtual item Show rcvBottomLeft
        holder.binding.rcvBottomLeft.visibility =
            if (selectedPosition == items.size - 1) View.VISIBLE else View.GONE
    }

    private fun bindNormal(holder: ActiveViewHolder, position: Int) {
        Log.e("ActiveAdapter", "onBindViewHolder")
        val dataPosition = position - 1 // Less left virtual item Offset
        val item = items[dataPosition]
        holder.bind(item, onItemClick, onDeleteClick, onItemLongClick)
        holder.binding.ivDel.visibility = View.VISIBLE

        // Add item Animation of entrance: slide from bottom
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

        if (item.id == currentId) {
            holder.binding.llContent.setBackgroundResource(R.drawable.tablet_tab_bg_selected)
            holder.binding.rcvBottomLeft.visibility = View.GONE
            holder.binding.rcvBottomRight.visibility = View.GONE
            // Selected: Show BridgesViewFilling with the neighborsitemSpacing between
            // Left-left virtual item It's true. item There's always a neighbor. Bridge. View Always Show
            holder.binding.vBridgeLeft.visibility = View.VISIBLE
            holder.binding.vBridgeRight.visibility = View.VISIBLE
        } else {
            holder.binding.llContent.setBackgroundResource(R.drawable.tablet_tab_bg_normal)
            // PreviousitemShow a dimpled corner on the lower right corner
            if (dataPosition == selectedPosition - 1) {
                holder.binding.rcvBottomRight.visibility = View.VISIBLE
            } else {
                holder.binding.rcvBottomRight.visibility = View.GONE
            }
            // The last one.itemShow dents in the lower left corner
            if (dataPosition == selectedPosition + 1) {
                holder.binding.rcvBottomLeft.visibility = View.VISIBLE
            } else {
                holder.binding.rcvBottomLeft.visibility = View.GONE
            }
            // Do not show bridges for non-selected itemsView
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
                binding.tvAppName.text = if (item.status == AppStatus.DEAD) {
                    "${item.name}(${binding.root.context.getString(R.string.server_closed)})"
                } else {
                    item.name
                }
            }

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
            binding.ivDel.setOnClickListener { onDeleteClick?.invoke(item) }
            binding.root.setOnClickListener { onItemClick?.invoke(item) }
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
        }
    }
}