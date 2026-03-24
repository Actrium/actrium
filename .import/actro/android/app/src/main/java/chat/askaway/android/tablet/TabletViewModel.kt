package chat.askaway.android.tablet

import android.os.Bundle
import android.util.Log
import androidx.lifecycle.ViewModel
import com.askaway.android.core.ReactNativeBundle
import com.askaway.android.event.ReactModuleEvent
import com.askaway.android.model.ActiveModel
import com.askaway.android.model.AppModel
import com.askaway.android.utils.addMap
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import org.greenrobot.eventbus.EventBus
import org.greenrobot.eventbus.Subscribe
import org.greenrobot.eventbus.ThreadMode
import java.util.UUID


class TabletViewModel : ViewModel() {

    private val _tabletUseCase = MutableStateFlow<TabletUseCase?>(null)
    val tabletUseCase: StateFlow<TabletUseCase?> get() = _tabletUseCase
    private val appList = ArrayList<AppModel>()

    var containerHeight = 1000
    var containerWidth = 720
    var currentDisplayModel = PageMode.TAB
    val pageBundle = Bundle()

    init {
        _tabletUseCase.value = TabletUseCase()
        if (!EventBus.getDefault().isRegistered(this)) {
            EventBus.getDefault().register(this)
        }
    }

    /**
     * 收到ReactNative调用原生发生的通知
     */
    @Subscribe(threadMode = ThreadMode.MAIN)
    fun onReactNativeCall(event: ReactModuleEvent) {
        when (event.methodName) {
            ReactModuleEvent.EVENT_UPDATE_TAB_INFO -> updateAppInfo(event.params)
        }
    }

    private fun updateAppInfo(params: Map<String, Any?>) {
        val tabId = params["tabId"] as? String
        val title = params["title"] as? String
        val icon = params["icon"] as? String
        val url = params["url"] as? String
        if (tabId != null) {
            _tabletUseCase.value?.let { useCase ->
                val appList = useCase.appList.toMutableList()
                val index = appList.indexOfFirst { it.id == tabId }
                if (index != -1) {
                    val oldItem = appList[index]
                    appList[index] =
                        oldItem.copy(name = title ?: oldItem.name, icon = icon ?: oldItem.icon, url = url ?: oldItem.url)
                }
                _tabletUseCase.value = _tabletUseCase.value?.copy(appList = appList)
            }
        }
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

    fun removeActiveModel(id: String) {
        appList.removeAll { it.id == id }
        _tabletUseCase.value = _tabletUseCase.value?.copy(appList = _tabletUseCase.value!!.appList.filter { it.id != id })
        Log.e("TabletViewModel", "removeActiveModel ${id}")
    }

    fun changeDisplayMode(mode: PageMode) {
        _tabletUseCase.value = _tabletUseCase.value?.copy(pageMode = mode)
    }

    fun showSet() {
        _tabletUseCase.value = _tabletUseCase.value?.copy(showPageType = ReactNativeBundle.REACT_NATIVE_VIEW_TYPE_SETTINGS)
    }

    fun hideSet(){
        if(_tabletUseCase.value?.showPageType == ReactNativeBundle.REACT_NATIVE_VIEW_TYPE_SETTINGS){
            _tabletUseCase.value = _tabletUseCase.value?.copy(showPageType = "")
        }
    }
}