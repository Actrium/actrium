package com.askaway.android

import android.os.Bundle
import android.util.Log
import androidx.lifecycle.LiveData
import androidx.lifecycle.MutableLiveData
import androidx.lifecycle.ViewModel
import com.askaway.android.event.ReactModuleEvent
import com.askaway.android.global.Params
import com.askaway.android.model.ActiveModel
import com.askaway.android.model.AppModel
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow
import org.greenrobot.eventbus.EventBus
import org.greenrobot.eventbus.Subscribe
import org.greenrobot.eventbus.ThreadMode
import java.util.UUID

class MainViewModel : ViewModel() {
    private val _tabIndex = MutableLiveData(0)
    val tabIndex: LiveData<Int> = _tabIndex
    var containerHeight = 1000
    var containerWidth = 720
    private val _currentReactNativeStatus = MutableSharedFlow<Bundle>(replay = 0, extraBufferCapacity = 1)
    val currentReactNativeStatus: SharedFlow<Bundle> = _currentReactNativeStatus

    // ActiveModel 集合
    private val _activeList = MutableSharedFlow<List<ActiveModel>>(replay = 1, extraBufferCapacity = 1)
    val activeList: SharedFlow<List<ActiveModel>> = _activeList

    private val _activeModelList = mutableListOf<ActiveModel>()

    init {
        // 注册 EventBus
        if (!EventBus.getDefault().isRegistered(this)) {
            EventBus.getDefault().register(this)
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

            ReactModuleEvent.EVENT_SET_TAB_UNREAD_STATUS -> {
                val tabId = event.params["tabId"] as? String
                val unread = event.params["unread"] as? Boolean
                Log.e("MainViewModel", "${_activeModelList[0].id}")
                if (tabId != null && unread != null) {
                    val index = _activeModelList.indexOfFirst { it.id == tabId }
                    if (index != -1) {
                        val oldItem = _activeModelList[index]
                        if (oldItem.hasUnread != unread) {
                            _activeModelList[index] = oldItem.copy(
                                hasUnread = unread
                            )
                            _activeList.tryEmit(_activeModelList.toList())
                        }
                    }
                }
            }
        }
    }

    fun setTabIndex(index: Int) {
        _tabIndex.value = index
    }

    fun addApp() {
        val tabModel = AppModel(id = UUID.randomUUID().toString())
        Params.appList.value += tabModel
        val activeModel = ActiveModel(
            id = tabModel.id,
            name = "Active App ${System.currentTimeMillis()}",
            icon = "",
            screenshot = ""
        )
        addActiveModel(activeModel)
        _currentReactNativeStatus.tryEmit(
            tabModel.genReactNativeBundle(
                Params.appList.value,
                containerHeight,
                containerWidth
            )
        )
    }

    fun genReactNativeTabModel(): AppModel {
        return AppModel(id = UUID.randomUUID().toString())
    }

    /**
     * 添加 ActiveModel 到列表
     */
    fun addActiveModel(activeModel: ActiveModel) {
        _activeModelList.add(activeModel)
        _activeList.tryEmit(_activeModelList.toList())
    }

    /**
     * 批量添加 ActiveModel 到列表
     */
    fun addActiveModels(models: List<ActiveModel>) {
        _activeModelList.addAll(models)
        _activeList.tryEmit(_activeModelList.toList())
    }

    /**
     * 清除所有 ActiveModel
     */
    fun clearActiveModels() {
        _activeModelList.clear()
        _activeList.tryEmit(_activeModelList.toList())
    }

    /**
     * 移除指定 ActiveModel
     */
    fun removeActiveModel(id: String) {
        _activeModelList.removeAll { it.id == id }
        _activeList.tryEmit(_activeModelList.toList())
    }
}
