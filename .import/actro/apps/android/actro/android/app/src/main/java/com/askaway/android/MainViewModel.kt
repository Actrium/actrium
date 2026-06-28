package com.askaway.android

import android.os.Bundle
import android.util.Log
import androidx.lifecycle.LiveData
import androidx.lifecycle.MutableLiveData
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import chat.askaway.android.data.repository.ActiveRepository
import chat.askaway.android.utils.TabBundleUtils
import com.askaway.android.event.NativeModuleEvent
import com.askaway.android.event.ReactModuleEvent
import com.askaway.android.global.Params
import com.askaway.android.model.ActiveModel
import com.askaway.android.model.AppModel
import com.askaway.android.model.AppStatus
import com.askaway.android.utils.addMap
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
    private val activeRepository: ActiveRepository? = null
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
                        Log.d("MainViewModel", "Loaded ${savedList.size} items from database")
                    }
                } catch (e: Exception) {
                    Log.e("MainViewModel", "Error loading active list from database", e)
                }
            }
        }
    }

    fun saveActiveListToDatabase() {
        activeRepository?.let { repository ->
            viewModelScope.launch {
                try {
                    repository.saveActiveList(_activeList.value)
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
                val systemAppType = event.params["systemAppType"] as? String
                if (systemAppType == null) { // 打开应用
                    val tabId = event.params["tabId"] as? String
                    Log.e("MainViewModel", "Received EVENT_OPEN_TAB for tabId: $tabId")
                    val hasActive = tabId != null && _activeList.value.any { it.id == tabId }
                    if (hasActive) {
                        val appModel = _activeList.value.find { it.id == tabId }
                        Log.e("MainViewModel", "Received EVENT_OPEN_TAB appModel: $appModel")
                        appModel?.let {
                            _currentReactNativeStatus.tryEmit(
                                TabBundleUtils.genBundle(it, _activeList.value, containerHeight, containerWidth)
                            )
                        }
                    } else {
                        val filteredParams = event.params.filterKeys { it != "tabId" }
                        addApp(filteredParams)
                    }
                } else {  // 打开其他页面
                    Log.e(
                        "MainViewModel",
                        "Received EVENT_OPEN_TAB for systemAppType: $systemAppType with params: ${event.params}"
                    )
                    pageBundle.clear()
                    val bundle = Bundle()
                    bundle.addMap(event.params)
                    val filteredParams = event.params.filterKeys { it != "systemAppType" }
                    pageBundle.putBundle("tab", bundle)
                    pageBundle.putBoolean("singleTabMode", true)
                    pageBundle.addMap(filteredParams)
                    _showPageType.tryEmit(systemAppType)
                }
            }

            ReactModuleEvent.EVENT_SET_TAB_UNREAD_STATUS -> {
                val tabId = event.params["tabId"] as? String
                val unread = event.params["unread"] as? Boolean
                updateUnread(tabId, unread)
            }

            ReactModuleEvent.EVENT_UPDATE_TAB_INFO -> {
                val tabId = event.params["tabId"] as? String
                val title = event.params["title"] as? String
                val icon = event.params["icon"] as? String
                val url = event.params["appUrl"] as? String
                val status = event.params["status"] as? String
                if (tabId != null) {
                    val index = _activeList.value.indexOfFirst { it.id == tabId }
                    if (index != -1) {
                        val oldItem = _activeList.value[index]
                        _activeList.update { list ->
                            list.mapIndexed { i, item ->
                                if (i == index) oldItem.copy(
                                    name = title ?: oldItem.name,
                                    icon = icon ?: oldItem.icon,
                                    url = url ?: oldItem.url,
                                    status = status?.let { AppStatus.fromValue(it) } ?: oldItem.status
                                ) else item
                            }
                        }
                    }
                }
            }

            ReactModuleEvent.EVENT_TOGGLE_MARK_STATUS -> {
                val tabId = event.params["tabId"] as? String
                val marked = event.params["marked"] as? Boolean
                if (tabId != null && marked != null) {
                    val index = _activeList.value.indexOfFirst { it.id == tabId }
                    if (index != -1) {
                        _activeList.update { list ->
                            list.mapIndexed { i, item ->
                                if (i == index) item.copy(isMarked = marked) else item
                            }
                        }
                    }
                }
            }

            ReactModuleEvent.EVENT_GET_ALL_TABS -> {
                val tabs = com.facebook.react.bridge.Arguments.createArray()
                _activeList.value.forEach { model ->
                    val map = com.facebook.react.bridge.Arguments.createMap()
                    map.putString("tabId", model.id)
                    map.putString("title", model.name)
                    map.putString("icon", model.icon)
                    map.putString("appUrl", model.url)
                    tabs.pushMap(map)
                }
                event.promise?.resolve(tabs)
            }
        }
    }

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


    fun genReactNativeTabModel(): AppModel {
        return AppModel(id = UUID.randomUUID().toString())
    }

    /**
     * 添加 ActiveModel 到列表
     */
    fun addActiveModel(activeModel: ActiveModel) {
        _activeList.update { it + activeModel }
    }

    /**
     * 批量添加 ActiveModel 到列表
     */
    fun addActiveModels(models: List<ActiveModel>) {
        _activeList.update { it + models }
    }

    /**
     * 清除所有 ActiveModel
     */
    fun clearActiveModels() {
        _activeList.update { emptyList() }
    }

    /**
     * 移除指定 ActiveModel
     */
    fun removeActiveModel(id: String) {
        _activeList.update { it.filterNot { model -> model.id == id } }
        Params.appList.removeAll { it.id == id }
    }


    private fun updateUnread(tabId: String?, unread: Boolean?) {
        if (tabId != null && unread != null) {
            val index = _activeList.value.indexOfFirst { it.id == tabId }
            if (index != -1) {
                val oldItem = _activeList.value[index]
                if (oldItem.hasUnread != unread) {
                    _activeList.update { list ->
                        list.mapIndexed { i, item ->
                            if (i == index) item.copy(hasUnread = unread) else item
                        }
                    }
                }
            }
        }
    }

    fun toggleMarkStatus(activeModel: ActiveModel) {
        EventBus.getDefault().post(
            NativeModuleEvent(
                NativeModuleEvent.EVENT_TAB_BOOKMARK_CHANGED,
                mapOf("tabId" to activeModel.id, "isBookmarked" to !activeModel.isMarked)
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
                mapOf("tabId" to tabId)
            )
        )
    }

    fun exitFullScreen(tabId: String?) {
        if (tabId == null) return
        EventBus.getDefault().post(
            NativeModuleEvent(
                NativeModuleEvent.EVENT_TAB_EXITED_FULLSCREEN,
                mapOf("tabId" to tabId)
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
}
