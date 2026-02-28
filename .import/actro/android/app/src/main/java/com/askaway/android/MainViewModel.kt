package com.askaway.android

import android.os.Bundle
import androidx.lifecycle.LiveData
import androidx.lifecycle.MutableLiveData
import androidx.lifecycle.ViewModel
import com.askaway.android.global.Params
import com.askaway.android.model.AppModel
import com.askaway.android.model.ActiveModel
import java.util.UUID
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow

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

    fun setTabIndex(index: Int) {
        _tabIndex.value = index
    }

    fun addApp() {
//        val tabModel = AppModel(id = UUID.randomUUID().toString())
        val tabModel = AppModel(id = "e6c6fefa-51c1-4ac0-baea-4f4ad00fff72")
        Params.appList.value += tabModel
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

