package chat.askaway.android.tablet

import android.util.Log
import androidx.lifecycle.ViewModel
import com.askaway.android.model.ActiveModel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

class TabletViewModel : ViewModel() {
    // StateFlow 可观察对象，绑定TabletUseCase
    private val _tabletUseCase = MutableStateFlow<TabletUseCase?>(null)
    val tabletUseCase: StateFlow<TabletUseCase?> get() = _tabletUseCase

    init {
        _tabletUseCase.value = TabletUseCase()
    }

    fun addApp() {
        // 这里可以调用 TabletUseCase 的方法来添加新的 ActiveModel
        // 例如：_tabletUseCase.value?.addActive(...)
        _tabletUseCase.value = _tabletUseCase.value!!.copy(
            appList = _tabletUseCase.value!!.appList + ActiveModel(
                "New App",
                "icon_url"
            )
        )
        Log.e("TabletViewModel", "Added new app, total count: ${_tabletUseCase.value?.appList?.size}")
    }
}