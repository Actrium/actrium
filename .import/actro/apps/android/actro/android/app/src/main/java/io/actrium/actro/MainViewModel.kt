package io.actrium.actro

import android.os.Bundle
import android.util.Log
import androidx.lifecycle.LiveData
import androidx.lifecycle.MutableLiveData
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import io.actrium.actro.data.repository.ActiveRepository
import io.actrium.actro.data.repository.BrowserDataRepository
import io.actrium.actro.utils.TabBundleUtils
import io.actrium.actro.event.NativeModuleEvent
import io.actrium.actro.event.ReactModuleEvent
import io.actrium.actro.global.Params
import io.actrium.actro.model.ActiveModel
import io.actrium.actro.model.AppModel
import io.actrium.actro.model.RuntimeOpenTabPlan
import io.actrium.actro.model.RuntimeTabBookmarkStatus
import io.actrium.actro.model.RuntimeTabUnread
import io.actrium.actro.utils.RuntimeBridgeParams
import io.actrium.actro.utils.addMap
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import org.greenrobot.eventbus.EventBus
import org.greenrobot.eventbus.Subscribe
import org.greenrobot.eventbus.ThreadMode
import java.util.UUID

class MainViewModel(
    private val activeRepository: ActiveRepository? = null,
    private val browserDataRepository: BrowserDataRepository? = null
) : ViewModel() {
    private val _tabIndex = MutableLiveData(0)
    val tabIndex: LiveData<Int> = _tabIndex
    var containerHeight = 1000
    var containerWidth = 720
    private val _currentReactNativeStatus = MutableSharedFlow<Bundle>(replay = 0, extraBufferCapacity = 1)
    val currentReactNativeStatus: SharedFlow<Bundle> = _currentReactNativeStatus

    // ActiveModel 集合
    private val _activeList = MutableStateFlow<List<ActiveModel>>(emptyList())
    val activeList: StateFlow<List<ActiveModel>> = _activeList

    private val _showPageType = MutableSharedFlow<String>(replay = 0, extraBufferCapacity = 1)
    val showPageType: SharedFlow<String> = _showPageType

    val pageBundle = Bundle()

    init {
        // 注册 EventBus
        if (!EventBus.getDefault().isRegistered(this)) {
            EventBus.getDefault().register(this)
        }

        // 从数据库加载数据
        loadActiveListFromDatabase()
    }

    private fun loadActiveListFromDatabase() {
        activeRepository?.let { repository ->
            viewModelScope.launch {
                try {
                    val savedList = repository.loadActiveList()
                    if (savedList.isNotEmpty()) {
                        _activeList.update { current ->
                             savedList.filter { saved ->
                                current.none { it.id == saved.id }
                            } + current
                        }
                        persistBrowserPageData()
                        Log.d("MainViewModel", "Loaded ${savedList.size} items from database")
                    } else {
                        restoreSharedBrowserSessionIfNeeded(repository)
                    }
                } catch (e: Exception) {
                    Log.e("MainViewModel", "Error loading active list from database", e)
                }
            }
        }
    }

    private suspend fun restoreSharedBrowserSessionIfNeeded(repository: ActiveRepository) {
        val restoredSession = browserDataRepository?.loadSessionActiveList() ?: return
        if (restoredSession.activeList.isEmpty()) return

        _activeList.update { current ->
            restoredSession.activeList.filter { restored ->
                current.none { it.id == restored.id }
            } + current
        }
        Params.currentTabId = restoredSession.currentTabId
        repository.saveActiveList(_activeList.value)
        persistBrowserPageData(restoredSession.currentTabId)
        Log.d("MainViewModel", "Restored ${restoredSession.activeList.size} items from shared browser session")
    }

    private fun persistBrowserPageData(currentTabId: String? = Params.currentTabId) {
        val repository = browserDataRepository ?: return
        val activeSnapshot = _activeList.value
        viewModelScope.launch {
            repository.saveActiveSnapshot(activeSnapshot, currentTabId)
        }
    }

    private fun recordBrowserVisit(activeModel: ActiveModel) {
        val repository = browserDataRepository ?: return
        viewModelScope.launch {
            repository.recordVisit(activeModel)
        }
    }

    fun saveActiveListToDatabase() {
        activeRepository?.let { repository ->
            viewModelScope.launch {
                try {
                    repository.saveActiveList(_activeList.value)
                    persistBrowserPageData()
                    Log.d("MainViewModel", "Saved ${_activeList.value.size} items to database")
                } catch (e: Exception) {
                    Log.e("MainViewModel", "Error saving active list to database", e)
                }
            }
        }
    }

    override fun onCleared() {
        super.onCleared()
        // 反注册 EventBus
        if (EventBus.getDefault().isRegistered(this)) {
            EventBus.getDefault().unregister(this)
        }
    }

    @Subscribe(threadMode = ThreadMode.MAIN)
    fun onReactModuleEvent(event: ReactModuleEvent) {
        when (event.methodName) {
            ReactModuleEvent.EVENT_EXIT_FULLSCREEN -> {
                // Handle exit fullscreen
            }

            ReactModuleEvent.EVENT_EXIT_SEARCH -> {
                // Handle exit search
            }

            ReactModuleEvent.EVENT_OPEN_TAB -> {
                handleOpenTab(event.params)
            }

            ReactModuleEvent.EVENT_SET_TAB_UNREAD_STATUS -> {
                runtimeTabUnreadFromParams(event.params)?.let { updateUnread(it) }
            }

            ReactModuleEvent.EVENT_UPDATE_TAB_INFO -> {
                val update = runtimeTabUpdateFromParams(event.params)
                if (update != null) {
                    var updatedModel: ActiveModel? = null
                    _activeList.update { list ->
                        list.map { item ->
                            if (item.id == update.tabId) {
                                item.applyRuntimeUpdate(update).also { updatedModel = it }
                            } else {
                                item
                            }
                        }
                    }
                    updatedModel?.let { recordBrowserVisit(it) }
                    persistBrowserPageData()
                }
            }

            ReactModuleEvent.EVENT_TOGGLE_MARK_STATUS -> {
                runtimeTabBookmarkStatusFromParams(event.params)?.let { updateBookmarkStatus(it) }
            }

            ReactModuleEvent.EVENT_GET_ALL_TABS -> {
                val tabs = com.facebook.react.bridge.Arguments.createArray()
                _activeList.value.forEach { model ->
                    val map = com.facebook.react.bridge.Arguments.createMap()
                    map.putString(RuntimeBridgeParams.tabIdKey, model.id)
                    map.putString(RuntimeBridgeParams.titleKey, model.name)
                    map.putString(RuntimeBridgeParams.iconKey, model.icon)
                    map.putString(RuntimeBridgeParams.appUrlKey, model.url)
                    tabs.pushMap(map)
                }
                event.promise?.resolve(tabs)
            }
        }
    }

    private fun runtimeTabUpdateFromParams(params: Map<String, Any?>) =
        RuntimeBridgeParams.tabUpdateFrom(params)

    private fun runtimeTabUnreadFromParams(params: Map<String, Any?>) =
        RuntimeBridgeParams.tabUnreadFrom(params)

    private fun runtimeTabBookmarkStatusFromParams(params: Map<String, Any?>) =
        RuntimeBridgeParams.tabBookmarkStatusFrom(params)

    private fun handleOpenTab(params: Map<String, Any?>) {
        val plan = runtimeOpenTabPlanFromParams(params)
        when (plan.kind) {
            RuntimeBridgeParams.openTabKindSystemPage -> openSystemPage(params, plan)
            else -> openAppTab(params, plan)
        }
    }

    private fun openAppTab(params: Map<String, Any?>, plan: RuntimeOpenTabPlan) {
        val tabId = plan.tabId
        Log.e("MainViewModel", "Received EVENT_OPEN_TAB for tabId: $tabId")
        val hasActive = tabId != null && _activeList.value.any { it.id == tabId }
        if (hasActive) {
            val appModel = _activeList.value.find { it.id == tabId }
            Log.e("MainViewModel", "Received EVENT_OPEN_TAB appModel: $appModel")
            appModel?.let {
                _currentReactNativeStatus.tryEmit(
                    TabBundleUtils.genBundle(it, _activeList.value, containerHeight, containerWidth)
                )
                persistBrowserPageData(it.id)
            }
        } else {
            addApp(filteredOpenTabParams(params, plan))
        }
    }

    private fun openSystemPage(params: Map<String, Any?>, plan: RuntimeOpenTabPlan) {
        val systemAppType = plan.systemAppType ?: return
        Log.e(
            "MainViewModel",
            "Received EVENT_OPEN_TAB for systemAppType: $systemAppType with params: $params"
        )
        pageBundle.clear()
        val bundle = Bundle()
        bundle.addMap(params)
        pageBundle.putBundle(RuntimeBridgeParams.rootTabKey, bundle)
        pageBundle.putBoolean(RuntimeBridgeParams.rootSingleTabModeKey, true)
        pageBundle.addMap(filteredOpenTabParams(params, plan))
        _showPageType.tryEmit(systemAppType)
    }

    private fun runtimeOpenTabPlanFromParams(params: Map<String, Any?>) =
        RuntimeBridgeParams.openTabPlanFrom(
            params,
            preferTabletRoutes = false,
            systemKeyPresenceMatters = false,
        )

    fun setTabIndex(index: Int) {
        _tabIndex.value = index
    }

    fun addApp(params: Map<String, Any?>? = null, appId: String? = null) {
        val tabModel = AppModel(id = UUID.randomUUID().toString())
        Params.appList.add(tabModel)
        val activeModel = ActiveModel(
            id = tabModel.id,
            name = "",
            icon = "",
            screenshot = ""
        )
        addActiveModel(activeModel)
        val bundle = tabModel.genReactNativeBundle(Params.appList, containerHeight, containerWidth)
        bundle.addMap(params)
        if (appId != null) {
            bundle.putString("appId", appId)
        }
        _currentReactNativeStatus.tryEmit(bundle)
    }

    fun genBundle(model: ActiveModel): Bundle {
        return TabBundleUtils.genBundle(model, _activeList.value, containerHeight, containerWidth)
    }

    /**
     * 添加 ActiveModel 到列表
     */
    fun addActiveModel(activeModel: ActiveModel) {
        _activeList.update { it + activeModel }
        persistBrowserPageData()
    }

    /**
     * 批量添加 ActiveModel 到列表
     */
    fun addActiveModels(models: List<ActiveModel>) {
        _activeList.update { it + models }
        persistBrowserPageData()
    }

    /**
     * 清除所有 ActiveModel
     */
    fun clearActiveModels() {
        _activeList.update { emptyList() }
        persistBrowserPageData()
    }

    /**
     * 移除指定 ActiveModel
     */
    fun removeActiveModel(id: String) {
        _activeList.update { it.filterNot { model -> model.id == id } }
        Params.appList.removeAll { it.id == id }
        persistBrowserPageData()
    }


    private fun updateUnread(payload: RuntimeTabUnread) {
        val index = _activeList.value.indexOfFirst { it.id == payload.tabId }
        if (index != -1) {
            val oldItem = _activeList.value[index]
            if (oldItem.hasUnread != payload.unread) {
                _activeList.update { list ->
                    list.mapIndexed { i, item ->
                        if (i == index) item.withUnreadStatus(payload) else item
                    }
                }
                persistBrowserPageData()
            }
        }
    }

    private fun updateBookmarkStatus(payload: RuntimeTabBookmarkStatus) {
        val index = _activeList.value.indexOfFirst { it.id == payload.tabId }
        if (index != -1) {
            _activeList.update { list ->
                list.mapIndexed { i, item ->
                    if (i == index) item.withBookmarkStatus(payload) else item
                }
            }
            persistBrowserPageData()
        }
    }

    fun toggleMarkStatus(activeModel: ActiveModel) {
        EventBus.getDefault().post(
            NativeModuleEvent(
                NativeModuleEvent.EVENT_TAB_BOOKMARK_CHANGED,
                tabBookmarkEventParams(activeModel.id, !activeModel.isMarked)
            )
        )
    }

    fun canCaptureScreenshot(tabId: String): Boolean {
        return _activeList.value.find { it.id == tabId }?.name?.isNotEmpty() == true
    }

    fun updateScreenshot(tabId: String, screenshotPath: String) {
        val index = _activeList.value.indexOfFirst { it.id == tabId }
        if (index != -1) {
            _activeList.update { list ->
                list.mapIndexed { i, item ->
                    if (i == index) item.copy(screenshot = screenshotPath) else item
                }
            }
        }
    }

    fun enterFullScreen(tabId: String?) {
        if (tabId == null) return
        EventBus.getDefault().post(
            NativeModuleEvent(
                NativeModuleEvent.EVENT_TAB_ENTERED_FULLSCREEN,
                tabFullscreenEventParams(tabId)
            )
        )
        persistBrowserPageData(tabId)
    }

    fun exitFullScreen(tabId: String?) {
        if (tabId == null) return
        EventBus.getDefault().post(
            NativeModuleEvent(
                NativeModuleEvent.EVENT_TAB_EXITED_FULLSCREEN,
                tabFullscreenEventParams(tabId)
            )
        )
    }

    fun isActiveListEmpty(): Boolean = _activeList.value.isEmpty()

    fun changeInputText(text: String) {
        EventBus.getDefault().post(
            NativeModuleEvent(
                NativeModuleEvent.EVENT_SEARCH_INPUT_CHANGED,
                mapOf("inputText" to text, "timestamp" to System.currentTimeMillis())
            )
        )
    }

    fun updateBookmark(bundle: Bundle) {
        EventBus.getDefault().post(
            NativeModuleEvent(NativeModuleEvent.EVENT_UPDATE_BOOKMARK, bundleToMap(bundle))
        )
    }

    fun updateSet(bundle: Bundle) {
        EventBus.getDefault().post(
            NativeModuleEvent(NativeModuleEvent.EVENT_UPDATE_SET, bundleToMap(bundle))
        )
    }

    @Suppress("DEPRECATION")
    private fun bundleToMap(bundle: Bundle): Map<String, Any?> {
        val map = mutableMapOf<String, Any?>()
        for (key in bundle.keySet()) {
            val value = bundle.get(key)
            when (value) {
                is Bundle -> map[key] = bundleToMap(value)
                else -> map[key] = value
            }
        }
        return map
    }

    private fun filteredOpenTabParams(
        params: Map<String, Any?>,
        plan: RuntimeOpenTabPlan,
    ): Map<String, Any?> = RuntimeBridgeParams.filteredOpenTabParams(params, plan)

    private fun tabBookmarkEventParams(tabId: String, isBookmarked: Boolean): Map<String, Any?> {
        return RuntimeBridgeParams.tabBookmarkEventParams(tabId, isBookmarked)
    }

    private fun tabFullscreenEventParams(tabId: String): Map<String, Any?> {
        return RuntimeBridgeParams.tabFullscreenEventParams(tabId)
    }
}
