package chat.askaway.android.tablet

import android.os.Bundle
import com.askaway.android.model.ActiveModel

data class TabletUseCase(
    val appList: List<ActiveModel> = emptyList(),
    val currentApp: Bundle? = null,
    val pageMode: PageMode = PageMode.TAB,
    val showPageType: String = "",
    val autoCreate:Boolean = false,
)

/**
 * pad中应用显示模式，TAB为页签形式，GRID为网格形式
 */
enum class PageMode {
    TAB,
    GRID
}