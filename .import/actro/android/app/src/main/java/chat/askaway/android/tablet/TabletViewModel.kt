package chat.askaway.android.tablet

import androidx.lifecycle.ViewModel
import com.askaway.android.model.ActiveModel
import com.askaway.android.model.AppModel
import com.askaway.android.utils.addMap
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import java.util.UUID

class TabletViewModel : ViewModel() {
    // StateFlow 可观察对象，绑定TabletUseCase
    var containerHeight = 1000
    var containerWidth = 720
    private val _tabletUseCase = MutableStateFlow<TabletUseCase?>(null)
    val tabletUseCase: StateFlow<TabletUseCase?> get() = _tabletUseCase
    private val appList = ArrayList<AppModel>()

    init {
        _tabletUseCase.value = TabletUseCase()
    }

    fun addApp(params: Map<String, Any?>? = null) {
        val tabModel = AppModel(id = UUID.randomUUID().toString())
        appList.add(tabModel)
        val activeModel = ActiveModel(
            id = tabModel.id,
            name = "",
            icon = "",
            screenshot = ""
        )
        val bundle = tabModel.genReactNativeBundle(appList, containerHeight, containerWidth)
        bundle.addMap(params)
        _tabletUseCase.value = _tabletUseCase.value!!.copy(
            appList = _tabletUseCase.value!!.appList + activeModel,
            currentApp = bundle
        )
    }
}