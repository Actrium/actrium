package chat.askaway.android.tablet

import android.os.Bundle
import android.util.Log
import androidx.lifecycle.ViewModel
import com.askaway.android.core.ReactNativeBundle
import com.askaway.android.event.NativeModuleEvent
import com.askaway.android.event.ReactModuleEvent
import com.askaway.android.global.Params
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
            ReactModuleEvent.EVENT_EXIT_SEARCH -> hideSearch()
            ReactModuleEvent.EVENT_TOGGLE_MARK_STATUS -> {
                val tabId = event.params["tabId"] as? String
                val marked = event.params["marked"] as? Boolean
                if (tabId != null && marked != null) {
                    _tabletUseCase.value?.let { useCase ->
                        val index = useCase.appList.indexOfFirst { it.id == tabId }
                        if (index != -1) {
                            val appList = useCase.appList.toMutableList()
                            val oldItem = appList[index]
                            appList[index] = oldItem.copy(isMarked = marked)
                            _tabletUseCase.value = _tabletUseCase.value?.copy(appList = appList)
                        }
                    }
                }
            }

            ReactModuleEvent.EVENT_OPEN_TAB -> openTabByReactNative(event.params)
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
                    Log.e("TabletViewModel", "${appList[index]}")
                }
                _tabletUseCase.value = _tabletUseCase.value?.copy(appList = appList)
            }
        }
    }

    fun addApp(params: Map<String, Any?>? = null) {
        Log.e("TabletViewModel", "addApp:${params}")
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

    fun changeActiveModel(id: String) {
        Log.e("TabletViewModel", "changeActiveModel")
        val index = appList.indexOfFirst { it.id == id }
        if (index != -1) {
            val activeModel = appList[index]
            val bundle = activeModel.genReactNativeBundle(appList, containerHeight, containerWidth)
            _tabletUseCase.value = _tabletUseCase.value?.copy(
                currentApp = bundle
            )
        }
    }

    fun toggleMarkStatus(activeModel: ActiveModel) {
        EventBus.getDefault().post(
            NativeModuleEvent(
                NativeModuleEvent.EVENT_EXIT_BOOKMARK_CHANGED,
                mapOf("tabId" to activeModel.id, "isBookmarked" to !activeModel.isMarked)
            )
        )
    }

    fun removeActiveModel(id: String) {
        val index = appList.indexOfFirst { it.id == id }
        if (index != -1) {
            appList.removeAt(index)
            if (appList.isNotEmpty()) {
                val newIndex = maxOf(0, index - 1)
                val activeModel = appList[newIndex]
                val bundle = activeModel.genReactNativeBundle(appList, containerHeight, containerWidth)
                _tabletUseCase.value = _tabletUseCase.value?.copy(
                    appList = _tabletUseCase.value!!.appList.filter { it.id != id },
                    currentApp = bundle
                )
            } else {
                _tabletUseCase.value = _tabletUseCase.value?.copy(
                    appList = _tabletUseCase.value!!.appList.filter { it.id != id },
                    currentApp = null
                )
            }
        }

        Log.e("TabletViewModel", "removeActiveModel ${id}")
    }

    fun changeDisplayMode(mode: PageMode) {
        _tabletUseCase.value = _tabletUseCase.value?.copy(pageMode = mode)
    }

    fun toggleMenu() {
        _tabletUseCase.value?.let {
            it.currentApp?.let { bundle ->
                val tabId = bundle.getString("currentTabId")
                val currentApp = _tabletUseCase.value?.appList?.find { it.id == tabId }
                currentApp?.let { app ->
                    EventBus.getDefault().post(
                        NativeModuleEvent(
                            NativeModuleEvent.EVENT_EXTENSION_PANEL_VISIBLE_CHANGE,
                            mapOf("tabId" to tabId, "visible" to app.panelVisible)
                        )
                    )
                    currentApp.panelVisible = !currentApp.panelVisible
                }
            }
        }
    }

    fun showSet() {
        _tabletUseCase.value = _tabletUseCase.value?.copy(showPageType = ReactNativeBundle.REACT_NATIVE_VIEW_TYPE_SETTINGS)
    }

    fun hideSet() {
        if (_tabletUseCase.value?.showPageType == ReactNativeBundle.REACT_NATIVE_VIEW_TYPE_SETTINGS) {
            _tabletUseCase.value = _tabletUseCase.value?.copy(showPageType = "")
        }
    }

    fun showSearch() {
        _tabletUseCase.value = _tabletUseCase.value?.copy(showPageType = ReactNativeBundle.REACT_NATIVE_VIEW_TYPE_SEARCH)
    }

    fun hideSearch() {
        if (_tabletUseCase.value?.showPageType == ReactNativeBundle.REACT_NATIVE_VIEW_TYPE_SEARCH) {
            _tabletUseCase.value = _tabletUseCase.value?.copy(showPageType = "")
        }
    }

    fun showBookmark() {
        _tabletUseCase.value = _tabletUseCase.value?.copy(showPageType = ReactNativeBundle.REACT_NATIVE_VIEW_TYPE_BOOKMARK)
    }

    fun hideBookmark() {
        if (_tabletUseCase.value?.showPageType == ReactNativeBundle.REACT_NATIVE_VIEW_TYPE_BOOKMARK) {
            _tabletUseCase.value = _tabletUseCase.value?.copy(showPageType = "")
        }
    }

    /**
     * 为指定的 app 生成 ReactNative Bundle
     */
    fun genBundleForApp(appId: String): Bundle? {
        val index = appList.indexOfFirst { it.id == appId }
        return if (index != -1) {
            appList[index].genReactNativeBundle(appList, containerHeight, containerWidth)
        } else {
            null
        }
    }

    /**
     * 通过reactNative调用的方式打开Tab
     */
    private fun openTabByReactNative(params: Map<String, Any?>) {
        Log.e("TabletViewModel", "openTabByReactNative:${params}")
        if (params.keys.contains(ReactNativeBundle.REACT_NATIVE_VIEW_TYPE)) {
            openSystemPage(params)
        } else if (params.keys.contains(ReactNativeBundle.REACT_NATIVE_VIEW_TAB_TYPE)) {
            openSystemTab(params)
        } else {
            openTab(params)
        }
    }

    private fun openTab(params: Map<String, Any?>) {
        val tabId = params["tabId"] as? String
        Log.e("MainViewModel", "Received EVENT_OPEN_TAB for tabId: $tabId")
        val hasActive = tabId != null && appList.any { it.id == tabId }
        if (hasActive) {
            val appModel = appList.find { it.id == tabId }
            appModel?.let {
                _tabletUseCase.value = _tabletUseCase.value?.copy(
                    currentApp = it.genReactNativeBundle(
                        Params.appList,
                        containerHeight,
                        containerWidth
                    )
                )
            }
        } else {
            Log.e("TabletViewModel", "no hasActive")
            val filteredParams = params.filterKeys { it != "tabId" }
            addApp(filteredParams)
        }
    }

    private fun openSystemTab(params: Map<String, Any?>) {
        val tabId = if (params.keys.contains("tabId")) params["tabId"] as String else UUID.randomUUID().toString()
        val tabModel = AppModel(id = tabId)
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

    private fun openSystemPage(params: Map<String, Any?>) {
        pageBundle.clear()
        val systemAppType = params["systemAppType"] as? String
        pageBundle.clear()
        val bundle = Bundle()
        bundle.addMap(params)
        val filteredParams = params.filterKeys { it != "systemAppType" }
        pageBundle.putBundle("tab", bundle)
        pageBundle.putBoolean("singleTabMode", true)
        pageBundle.addMap(filteredParams)
        systemAppType?.let {
            _tabletUseCase.value = _tabletUseCase.value?.copy(showPageType = it)
        }
    }


    override fun onCleared() {
        super.onCleared()
        // 反注册 EventBus
        if (EventBus.getDefault().isRegistered(this)) {
            EventBus.getDefault().unregister(this)
        }
    }
}