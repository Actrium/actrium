package io.actrium.app.tablet

import android.content.Context
import android.os.Bundle
import android.util.Log
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import io.actrium.app.data.database.AppDatabase
import io.actrium.app.data.repository.ActiveRepository
import io.actrium.app.data.repository.BrowserDataRepository
import io.actrium.app.data.sync.BrowserSyncStack
import io.actrium.app.utils.TabBundleUtils
import io.actrium.app.core.ReactNativeBundle
import io.actrium.app.event.NativeModuleEvent
import io.actrium.app.event.ReactModuleEvent
import io.actrium.app.global.Params
import io.actrium.app.model.ActiveModel
import io.actrium.app.model.AppModel
import io.actrium.app.model.RuntimeOpenTabPlan
import io.actrium.app.model.RuntimeTabBookmarkStatus
import io.actrium.app.utils.RuntimeBridgeParams
import io.actrium.app.utils.addMap
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import org.greenrobot.eventbus.EventBus
import org.greenrobot.eventbus.Subscribe
import org.greenrobot.eventbus.ThreadMode
import java.util.UUID


class TabletViewModel(val applicationContext: Context) : ViewModel() {

    private val _tabletUseCase = MutableStateFlow<TabletUseCase?>(null)
    val tabletUseCase: StateFlow<TabletUseCase?> get() = _tabletUseCase
    private val appList = ArrayList<AppModel>()

    var containerHeight = 800
    var containerWidth = 1000
    var currentDisplayModel = PageMode.TAB
    var isFromDeepLink = false
    val pageBundle = Bundle()
    private var activeRepository: ActiveRepository? = null
    // Shared app-level sync stack; the scheduler is deliberately not bound to
    // viewModelScope so pending sync work survives this view-model's destruction.
    private val browserDataRepository: BrowserDataRepository =
        BrowserSyncStack.getInstance(applicationContext).repository

    init {
        _tabletUseCase.value = TabletUseCase()
        if (!EventBus.getDefault().isRegistered(this)) {
            EventBus.getDefault().register(this)
        }
        val database = AppDatabase.getDatabase(applicationContext)
        activeRepository = ActiveRepository(database.activeDao())
        loadActiveListFromDatabase()
    }

    private fun loadActiveListFromDatabase() {
        activeRepository?.let { repository ->
            viewModelScope.launch {
                delay(300)
                try {
                    val savedList = repository.loadActiveList()
                    if (savedList.isNotEmpty()) {
                        if (isFromDeepLink) {
                            // deeplink Enter: Merge saved lists, but keep deeplink Created currentApp No change.
                            val currentAppList = _tabletUseCase.value?.appList ?: emptyList()
                            val mergedList = savedList.filter { saved ->
                                currentAppList.none { it.id == saved.id }
                            } + currentAppList
                            _tabletUseCase.value = _tabletUseCase.value?.copy(appList = mergedList)
                        } else {
                            val bundle = TabBundleUtils.genBundle(savedList[0], savedList, containerHeight, containerWidth)
                            _tabletUseCase.value = _tabletUseCase.value?.copy(appList = savedList, currentApp = bundle)
                        }
                        persistBrowserPageData()
                        Log.d("MainViewModel", "Loaded ${savedList.size} items from database")
                    } else if (!isFromDeepLink && !restoreSharedBrowserSessionIfNeeded(repository)) {
                        _tabletUseCase.value = _tabletUseCase.value?.copy(autoCreate = true)
                    }
                } catch (e: Exception) {
                    Log.e("MainViewModel", "Error loading active list from database", e)
                }
            }
        }
    }

    private suspend fun restoreSharedBrowserSessionIfNeeded(repository: ActiveRepository): Boolean {
        val restoredSession = browserDataRepository.loadSessionActiveList() ?: return false
        val restoredList = restoredSession.activeList
        if (restoredList.isEmpty()) return false

        val currentModel = restoredList.firstOrNull { it.id == restoredSession.currentTabId }
            ?: restoredList.first()
        val bundle = TabBundleUtils.genBundle(currentModel, restoredList, containerHeight, containerWidth)
        _tabletUseCase.value = _tabletUseCase.value?.copy(
            appList = restoredList,
            currentApp = bundle,
            autoCreate = false
        )
        repository.saveActiveList(restoredList)
        persistBrowserPageData(restoredSession.currentTabId)
        Log.d("MainViewModel", "Restored ${restoredList.size} items from shared browser session")
        return true
    }

    private fun currentTabId(): String? {
        return _tabletUseCase.value?.currentApp
            ?.getString(RuntimeBridgeParams.rootCurrentTabIdKey)
    }

    private fun persistBrowserPageData(currentTabId: String? = currentTabId()) {
        val activeSnapshot = _tabletUseCase.value?.appList.orEmpty()
        viewModelScope.launch {
            browserDataRepository.saveActiveSnapshot(activeSnapshot, currentTabId)
        }
    }

    private fun recordBrowserVisit(activeModel: ActiveModel) {
        viewModelScope.launch {
            browserDataRepository.recordVisit(activeModel)
        }
    }

    fun cancelCreate() {
        _tabletUseCase.value = _tabletUseCase.value?.copy(autoCreate = false)
    }

    fun saveActiveListToDatabase() {
        activeRepository?.let { repository ->
            viewModelScope.launch {
                try {
                    repository.saveActiveList(_tabletUseCase.value!!.appList.toList())
                    persistBrowserPageData()
                    Log.d("MainViewModel", "Saved items to database")
                } catch (e: Exception) {
                    Log.e("MainViewModel", "Error saving active list to database", e)
                }
            }
        }
    }

    /**
     * Copy that.ReactNativeCall the original notification.
     */
    @Subscribe(threadMode = ThreadMode.MAIN)
    fun onReactNativeCall(event: ReactModuleEvent) {
        when (event.methodName) {
            ReactModuleEvent.EVENT_UPDATE_TAB_INFO -> updateAppInfo(event.params)
            ReactModuleEvent.EVENT_EXIT_SEARCH -> hideSearch()
            ReactModuleEvent.EVENT_TOGGLE_MARK_STATUS -> {
                runtimeTabBookmarkStatusFromParams(event.params)?.let { updateBookmarkStatus(it) }
            }

            ReactModuleEvent.EVENT_OPEN_TAB -> openTabByReactNative(event.params)
        }
    }


    private fun updateAppInfo(params: Map<String, Any?>) {
        val update = runtimeTabUpdateFromParams(params)
        if (update != null) {
            _tabletUseCase.value?.let { useCase ->
                val appList = useCase.appList.toMutableList()
                val index = appList.indexOfFirst { it.id == update.tabId }
                var updatedModel: ActiveModel? = null
                if (index != -1) {
                    val oldItem = appList[index]
                    appList[index] = oldItem.applyRuntimeUpdate(update).also { updatedModel = it }
                    Log.e("TabletViewModel", "${appList[index]}")
                }
                _tabletUseCase.value = _tabletUseCase.value?.copy(appList = appList)
                updatedModel?.let { recordBrowserVisit(it) }
                persistBrowserPageData()
            }
        }
    }

    private fun runtimeTabUpdateFromParams(params: Map<String, Any?>) =
        RuntimeBridgeParams.tabUpdateFrom(params)

    private fun runtimeTabBookmarkStatusFromParams(params: Map<String, Any?>) =
        RuntimeBridgeParams.tabBookmarkStatusFrom(params)

    fun addApp(params: Map<String, Any?>? = null, appId: String? = null) {
        Log.e("TabletViewModel", "addApp:${params}, ${appId}")
        val tabModel = AppModel(id = UUID.randomUUID().toString())
        var appUrl = ""
        var mode = _tabletUseCase.value?.pageMode ?: PageMode.TAB
        if (params != null && params.keys.contains(RuntimeBridgeParams.appUrlKey)) {
            appUrl = params[RuntimeBridgeParams.appUrlKey] as? String ?: ""
            mode = PageMode.TAB
        }
        appList.add(tabModel)
        val activeModel = ActiveModel(
            id = tabModel.id,
            name = "",
            icon = "",
            screenshot = "",
            url = appUrl
        )
        val bundle = tabModel.genReactNativeBundle(appList, containerHeight, containerWidth)
        bundle.addMap(params)
        if (appId != null) {
            bundle.putString("appId", appId)
        }
        hideBookmark()
        _tabletUseCase.value = _tabletUseCase.value!!.copy(
            appList = _tabletUseCase.value!!.appList + activeModel,
            currentApp = bundle,
            pageMode = mode
        )
        recordBrowserVisit(activeModel)
        persistBrowserPageData(tabModel.id)
    }

    fun changeActiveModel(id: String) {
        Log.e("TabletViewModel", "changeActiveModel:${id}")
        val appList = _tabletUseCase.value?.appList ?: emptyList()
        val index = appList.indexOfFirst { it.id == id }
        if (index != -1) {
            val activeModel = appList[index]
            val bundle = TabBundleUtils.genBundle(activeModel, appList, containerHeight, containerWidth)
            _tabletUseCase.value = _tabletUseCase.value?.copy(
                currentApp = bundle
            )
            persistBrowserPageData(id)
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

    fun removeActiveModel(id: String) {
        val appList = (_tabletUseCase.value?.appList ?: emptyList()).toMutableList()
        val index = appList.indexOfFirst { it.id == id }
        if (index != -1) {
            appList.removeAt(index)
            if (appList.isNotEmpty()) {
                val newIndex = maxOf(0, index - 1)
                val activeModel = appList[newIndex]
                val bundle = TabBundleUtils.genBundle(activeModel, appList, containerHeight, containerWidth)
                _tabletUseCase.value = _tabletUseCase.value?.copy(
                    appList = appList,
                    currentApp = bundle
                )
            } else {
                _tabletUseCase.value = _tabletUseCase.value?.copy(
                    appList = appList,
                    currentApp = null
                )
            }
            persistBrowserPageData()
        }
    }

    fun changeDisplayMode(mode: PageMode) {
        Log.e("TabletViewModel", "changeDisplayMode ${mode}")
        _tabletUseCase.value = _tabletUseCase.value?.copy(pageMode = mode)
    }

    fun toggleMenu() {
        _tabletUseCase.value?.let {
            it.currentApp?.let { bundle ->
                val tabId = bundle.getString(RuntimeBridgeParams.rootCurrentTabIdKey)
                val currentApp = _tabletUseCase.value?.appList?.find { it.id == tabId }
                if (tabId != null && currentApp != null) {
                    EventBus.getDefault().post(
                        NativeModuleEvent(
                            NativeModuleEvent.EVENT_EXTENSION_PANEL_VISIBLE_CHANGE,
                            extensionPanelVisibleEventParams(tabId, currentApp.panelVisible)
                        )
                    )
                    currentApp.panelVisible = !currentApp.panelVisible
                }
            }
        }
    }

    fun changeSize(width: Int, height: Int) {
        EventBus.getDefault().post(
            NativeModuleEvent(
                NativeModuleEvent.EVENT_CONTAINER_SIZE_CHANGE,
                containerSizeChangeEventParams(width, height)
            )
        )
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
     * Assigned app Generate ReactNative Bundle
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
     * Pass.reactNativeOpen with CallTab
     */
    private fun openTabByReactNative(params: Map<String, Any?>) {
        Log.e("TabletViewModel", "openTabByReactNative:${params}")
        val plan = runtimeOpenTabPlanFromParams(params)
        when (plan.kind) {
            RuntimeBridgeParams.openTabKindSystemPage -> openSystemPage(params, plan)
            RuntimeBridgeParams.openTabKindSystemTab -> openSystemTab(params, plan)
            else -> openTab(params, plan)
        }
    }

    private fun openTab(params: Map<String, Any?>, plan: RuntimeOpenTabPlan) {
        val tabId = plan.tabId
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
                persistBrowserPageData(it.id)
            }
        } else {
            Log.e("TabletViewModel", "no hasActive")
            addApp(filteredOpenTabParams(params, plan))
        }
    }

    private fun openSystemTab(params: Map<String, Any?>, plan: RuntimeOpenTabPlan) {
        val tabId = plan.tabId ?: UUID.randomUUID().toString()
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
        persistBrowserPageData(tabModel.id)
    }

    private fun openSystemPage(params: Map<String, Any?>, plan: RuntimeOpenTabPlan) {
        val systemAppType = plan.systemAppType
        pageBundle.clear()
        val bundle = Bundle()
        bundle.addMap(params)
        pageBundle.putBundle(RuntimeBridgeParams.rootTabKey, bundle)
        pageBundle.putBoolean(RuntimeBridgeParams.rootSingleTabModeKey, true)
        pageBundle.addMap(filteredOpenTabParams(params, plan))
        systemAppType?.let {
            _tabletUseCase.value = _tabletUseCase.value?.copy(showPageType = it)
        }
    }

    private fun runtimeOpenTabPlanFromParams(params: Map<String, Any?>) =
        RuntimeBridgeParams.openTabPlanFrom(
            params,
            preferTabletRoutes = true,
            systemKeyPresenceMatters = true,
        )

    private fun filteredOpenTabParams(
        params: Map<String, Any?>,
        plan: RuntimeOpenTabPlan,
    ): Map<String, Any?> = RuntimeBridgeParams.filteredOpenTabParams(params, plan)

    override fun onCleared() {
        super.onCleared()
        // Counter-registration EventBus
        if (EventBus.getDefault().isRegistered(this)) {
            EventBus.getDefault().unregister(this)
        }
    }

    fun enterFullScreen(tabId: String? = null) {
        Log.e("TabletViewModel", "enterFullScreen:${tabId}")
        val id =
            tabId ?: _tabletUseCase.value?.currentApp
                ?.getString(RuntimeBridgeParams.rootCurrentTabIdKey) ?: return
        EventBus.getDefault().post(
            NativeModuleEvent(
                NativeModuleEvent.EVENT_TAB_ENTERED_FULLSCREEN,
                tabFullscreenEventParams(id)
            )
        )
    }

    fun exitFullScreen(tabId: String? = null) {
        Log.e("TabletViewModel", "exitFullScreen:${tabId}")
        val id =
            tabId ?: _tabletUseCase.value?.currentApp
                ?.getString(RuntimeBridgeParams.rootCurrentTabIdKey) ?: return
        EventBus.getDefault().post(
            NativeModuleEvent(
                NativeModuleEvent.EVENT_TAB_EXITED_FULLSCREEN,
                tabFullscreenEventParams(id)
            )
        )
    }

    private fun updateBookmarkStatus(payload: RuntimeTabBookmarkStatus) {
        _tabletUseCase.value?.let { useCase ->
            val index = useCase.appList.indexOfFirst { it.id == payload.tabId }
            if (index != -1) {
                val appList = useCase.appList.toMutableList()
                val oldItem = appList[index]
                appList[index] = oldItem.withBookmarkStatus(payload)
                _tabletUseCase.value = _tabletUseCase.value?.copy(appList = appList)
                persistBrowserPageData()
            }
        }
    }

    private fun tabBookmarkEventParams(tabId: String, isBookmarked: Boolean): Map<String, Any?> {
        return RuntimeBridgeParams.tabBookmarkEventParams(tabId, isBookmarked)
    }

    private fun tabFullscreenEventParams(tabId: String): Map<String, Any?> {
        return RuntimeBridgeParams.tabFullscreenEventParams(tabId)
    }

    private fun extensionPanelVisibleEventParams(tabId: String, visible: Boolean): Map<String, Any?> {
        return RuntimeBridgeParams.extensionPanelVisibleEventParams(tabId, visible)
    }

    private fun containerSizeChangeEventParams(width: Int, height: Int): Map<String, Any?> {
        return RuntimeBridgeParams.containerSizeChangeEventParams(width, height)
    }
}
