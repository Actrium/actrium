package io.actrium.actro.tablet

import android.os.Bundle
import io.actrium.actro.model.ActiveModel

data class TabletUseCase(
    val appList: List<ActiveModel> = emptyList(),
    val currentApp: Bundle? = null,
    val pageMode: PageMode = PageMode.TAB,
    val showPageType: String = "",
    val autoCreate:Boolean = false,
)

/**
 * pad. To apply display mode,TABSign as page,GRIDAs Grid Form
 */
enum class PageMode {
    TAB,
    GRID
}