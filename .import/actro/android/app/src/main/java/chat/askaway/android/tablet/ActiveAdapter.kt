package chat.askaway.android.tablet

import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import androidx.recyclerview.widget.RecyclerView
import coil.load
import com.askaway.android.R
import com.askaway.android.databinding.ItemTabletAppBinding
import com.askaway.android.model.ActiveModel

class ActiveAdapter : RecyclerView.Adapter<ActiveAdapter.ActiveViewHolder>() {

    private val items = mutableListOf<ActiveModel>()
    var onItemClick: ((ActiveModel, View) -> Unit)? = null
    var onDeleteClick: ((ActiveModel) -> Unit)? = null

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
        holder.bind(items[position], onItemClick, onDeleteClick)
    }

    override fun getItemCount(): Int = items.size

    class ActiveViewHolder(private val binding: ItemTabletAppBinding) : RecyclerView.ViewHolder(binding.root) {
        fun bind(
            item: ActiveModel,
            onItemClick: ((ActiveModel, View) -> Unit)?,
            onDeleteClick: ((ActiveModel) -> Unit)?
        ) {
            binding.tvAppName.text = item.name
            if (item.icon.isEmpty()) {
                binding.ivIcon.setImageResource(R.drawable.app_default)
            } else {
                binding.ivIcon.load(item.icon)
            }
            binding.ivDel.setOnClickListener { onDeleteClick?.invoke(item) }
        }
    }
}