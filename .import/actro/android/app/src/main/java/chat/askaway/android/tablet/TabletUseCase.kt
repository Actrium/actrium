package chat.askaway.android.tablet

import android.os.Bundle
import com.askaway.android.model.ActiveModel

data class TabletUseCase(
    val appList: List<ActiveModel> = emptyList(),
    val currentApp: Bundle? = null
)