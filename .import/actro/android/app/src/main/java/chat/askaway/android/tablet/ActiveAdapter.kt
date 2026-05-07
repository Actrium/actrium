package chat.askaway.android.tablet

import android.util.Log
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import androidx.recyclerview.widget.RecyclerView
import com.askaway.android.R
import com.askaway.android.databinding.ItemTabletAppBinding
import com.askaway.android.model.ActiveModel

class ActiveAdapter : RecyclerView.Adapter<ActiveAdapter.ActiveViewHolder>() {

    private val items = mutableListOf<ActiveModel>()
    var onItemClick: ((ActiveModel) -> Unit)? = null
    var onDeleteClick: ((ActiveModel) -> Unit)? = null
    var currentId: String? = ""
        set(value) {
            field = value
            notifyDataSetChanged()
        }

    fun updateData(newList: List<ActiveModel>) {
        items.clear()
        items.addAll(newList)
        notifyDataSetChanged()
    }


    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): ActiveViewHolder {
        val binding = ItemTabletAppBinding.inflate(LayoutInflater.from(parent.context), parent, false)
        return ActiveViewHolder(binding)
    }

    override fun onBindViewHolder(holder: ActiveViewHolder, position: Int) {
        Log.e("ActiveAdapter", "onBindViewHolder")
        val item = items[position]
        holder.bind(item, onItemClick, onDeleteClick)
        holder.binding.ivDel.visibility = View.VISIBLE

        val selectedPosition = items.indexOfFirst { it.id == currentId }

        if (item.id == currentId) {
            holder.binding.llContent.setBackgroundResource(R.drawable.tablet_tab_bg_selected)
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
            holder.binding.rcvBottomRight.visibility =
                if (position == selectedPosition - 1) View.VISIBLE else View.GONE
            // 后一个item的左下角显示凹圆角
            holder.binding.rcvBottomLeft.visibility =
                if (position == selectedPosition + 1) View.VISIBLE else View.GONE
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
            onDeleteClick: ((ActiveModel) -> Unit)?
        ) {
            if (item.name.isEmpty()) {
                binding.tvAppName.setText(R.string.blank_tab)
            } else {
                binding.tvAppName.text = item.name
            }
            binding.ivDel.setOnClickListener { onDeleteClick?.invoke(item) }
            binding.root.setOnClickListener { onItemClick?.invoke(item) }
        }
    }
}