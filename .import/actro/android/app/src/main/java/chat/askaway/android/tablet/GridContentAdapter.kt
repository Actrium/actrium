package chat.askaway.android.tablet

import android.graphics.Bitmap
import android.util.Log
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import androidx.cardview.widget.CardView
import androidx.core.content.ContextCompat
import androidx.core.view.isVisible
import androidx.recyclerview.widget.RecyclerView
import com.askaway.android.R
import com.askaway.android.model.ActiveModel

class GridContentAdapter : RecyclerView.Adapter<RecyclerView.ViewHolder>() {

    companion object {
        private const val VIEW_TYPE_CONTENT = 0
        private const val VIEW_TYPE_ADD_BUTTON = 1
    }

    private val items = mutableListOf<ActiveModel>()
    var onItemClick: ((Int) -> Unit)? = null
    var onAddClick: (() -> Unit)? = null
    var onDeleteClick: ((String) -> Unit)? = null
    var onMarkClick: ((ActiveModel) -> Unit)? = null
    var containerWidth = 0
    var containerHeight = 0
    var spanCount = 3
    var spacingPx = 0
    var marginHorizontalPx = 0

    fun updateItems(newItems: List<ActiveModel>) {
        items.clear()
        items.addAll(newItems)
        notifyDataSetChanged()
    }

    override fun getItemCount(): Int = items.size + 1

    override fun getItemViewType(position: Int): Int {
        return if (position == items.size) {
            VIEW_TYPE_ADD_BUTTON
        } else {
            VIEW_TYPE_CONTENT
        }
    }

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder {
        return when (viewType) {
            VIEW_TYPE_ADD_BUTTON -> {
                val view = LayoutInflater.from(parent.context)
                    .inflate(R.layout.item_safari_add, parent, false)
                AddButtonViewHolder(view)
            }

            else -> {
                val view = LayoutInflater.from(parent.context)
                    .inflate(R.layout.item_grid_content, parent, false)
                GridItemViewHolder(view)
            }
        }
    }

    override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) {
        when (holder) {
            is AddButtonViewHolder -> {
                holder.itemView.setOnClickListener {
                    onAddClick?.invoke()
                }
            }

            is GridItemViewHolder -> {
                val item = items[position]
                holder.bind(item)

                // 设置点击事件
                holder.ivScreenshot.setOnClickListener {
                    onItemClick?.invoke(position)
                }

                holder.ivMark.setOnClickListener {
                    Log.e("GridContentAdapter", "ivMark.setOnClickListener")
                    onMarkClick?.invoke(item)
                }

                // 设置删除按钮点击事件
                holder.ivDelete.setOnClickListener {
                    onDeleteClick?.invoke(item.id)
                }
            }
        }
    }

    inner class GridItemViewHolder(itemView: View) : RecyclerView.ViewHolder(itemView) {
        private val cardView: CardView = itemView.findViewById(R.id.cardView)
        private val tvTitle: TextView = itemView.findViewById(R.id.tvTitle)
        private val tvUrl: TextView = itemView.findViewById(R.id.tvUrl)
        val ivDelete: ImageView = itemView.findViewById(R.id.ivDelete)
        val ivMark: ImageView = itemView.findViewById(R.id.ivMark)
        val tvName: TextView = itemView.findViewById(R.id.tvName)
        private val llInfo: LinearLayout = itemView.findViewById(R.id.llInfo)
        val ivScreenshot: ImageView = itemView.findViewById(R.id.ivScreenshot)

        fun bind(item: ActiveModel) {
            tvTitle.text = item.name.ifEmpty { "New Tab" }
            tvUrl.text = item.url.ifEmpty { "about:blank" }

            val density = itemView.context.resources.displayMetrics.density

            val cardLayoutParams = cardView.layoutParams as LinearLayout.LayoutParams

            if (item.screenshot.isNotEmpty()) {
                ivScreenshot.visibility = View.VISIBLE
                val bitmap = decodeScreenshot(item.screenshot)
                if (bitmap != null) {
                    ivScreenshot.setImageBitmap(bitmap)
                } else {
                    ivScreenshot.setImageResource(0)
                }
            } else {
                ivScreenshot.visibility = View.GONE
            }

            cardView.radius = 12f * density
            llInfo.visibility = View.VISIBLE

            if (item.name.isEmpty()) {
                tvName.setText(R.string.blank_tab)
                ivMark.isVisible = false
            } else {
                ivMark.isVisible = true
                tvName.text = item.name
                if (item.isMarked) {
                    ivMark.setColorFilter(ContextCompat.getColor(ivMark.context, R.color.yellow))
                } else {
                    ivMark.setColorFilter(ContextCompat.getColor(ivMark.context, R.color.icon_1))
                }
            }

            val margin = (8 * density).toInt()
            cardLayoutParams.setMargins(margin, margin, margin, margin)
            cardLayoutParams.width = LinearLayout.LayoutParams.MATCH_PARENT
            cardLayoutParams.height = 0
            cardLayoutParams.weight = 1f

            // ivScreenshot 填满 CardView，用 FIT_CENTER 保持宽高比
            val screenshotParams = ivScreenshot.layoutParams
            screenshotParams.width = FrameLayout.LayoutParams.MATCH_PARENT
            screenshotParams.height = FrameLayout.LayoutParams.MATCH_PARENT
            ivScreenshot.layoutParams = screenshotParams

            cardView.layoutParams = cardLayoutParams
        }

        private fun decodeScreenshot(data: String): Bitmap? {
            return try {
                val bytes = android.util.Base64.decode(data, android.util.Base64.DEFAULT)
                android.graphics.BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
            } catch (e: Exception) {
                Log.e("GridContentAdapter", "Failed to decode screenshot", e)
                null
            }
        }
    }

    class AddButtonViewHolder(itemView: View) : RecyclerView.ViewHolder(itemView)
}