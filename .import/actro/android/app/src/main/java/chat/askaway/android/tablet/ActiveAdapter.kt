package chat.askaway.android.tablet

import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.ImageView
import android.widget.TextView
import androidx.recyclerview.widget.RecyclerView
import com.askaway.android.R
import com.askaway.android.model.ActiveModel

class ActiveAdapter: RecyclerView.Adapter<ActiveAdapter.ActiveViewHolder>() {

    private val items = mutableListOf<ActiveModel>()
    var onItemClick: ((ActiveModel, View) -> Unit)? = null
    var onDeleteClick: ((ActiveModel) -> Unit)? = null

    fun updateData(newList: List<ActiveModel>) {
        items.clear()
        items.addAll(newList)
        notifyDataSetChanged()
    }

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): ActiveViewHolder {
        val view = LayoutInflater.from(parent.context).inflate(R.layout.item_tablet_app, parent, false)
        return ActiveViewHolder(view)
    }

    override fun onBindViewHolder(holder: ActiveViewHolder, position: Int) {
        val item = items[position]
        holder.tvAppName.text = item.name
        // 加载icon图片资源，建议使用Glide/Picasso等库
        // 例如：Glide.with(holder.ivIcon).load(item.icon).into(holder.ivIcon)
        // 删除按钮可绑定点击事件
        // holder.ivDel.setOnClickListener { ... }
    }

    override fun getItemCount(): Int = items.size

    class ActiveViewHolder(itemView: View) : RecyclerView.ViewHolder(itemView) {
        val ivIcon: ImageView = itemView.findViewById(R.id.ivIcon)
        val tvAppName: TextView = itemView.findViewById(R.id.tvAppName)
        val ivDel: ImageView = itemView.findViewById(R.id.ivDel)
    }
}