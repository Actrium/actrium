package com.askaway.android.adapter

import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import androidx.recyclerview.widget.RecyclerView
import com.askaway.android.databinding.ItemActiveBinding
import com.askaway.android.model.ActiveModel

class ActiveRecyclerAdapter : RecyclerView.Adapter<ActiveRecyclerAdapter.ActiveViewHolder>() {
    private val items = mutableListOf<ActiveModel>()
    var onItemClick: ((ActiveModel) -> Unit)? = null
    var onDeleteClick: ((ActiveModel) -> Unit)? = null

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
        holder.bind(items[position], onItemClick, onDeleteClick)
    }

    override fun getItemCount(): Int = items.size

    class ActiveViewHolder(private val binding: ItemActiveBinding) : RecyclerView.ViewHolder(binding.root) {
        fun bind(item: ActiveModel, onItemClick: ((ActiveModel) -> Unit)?, onDeleteClick: ((ActiveModel) -> Unit)?) {
            val flActive = binding.root.findViewById<android.widget.FrameLayout>(com.askaway.android.R.id.fl_active)
            // ... 其他绑定逻辑 ...
            binding.root.setOnClickListener { onItemClick?.invoke(item) }
            binding.ivDelete.setOnClickListener { onDeleteClick?.invoke(item) }
            binding.vUnreadBadge.visibility = if (item.hasUnread) View.VISIBLE else View.GONE
        }
    }
}
