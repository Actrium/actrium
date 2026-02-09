package com.askaway.android

import android.os.Bundle
import androidx.lifecycle.LiveData
import androidx.lifecycle.MutableLiveData
import androidx.lifecycle.ViewModel
import com.askaway.android.global.Params
import com.askaway.android.model.AppModel
import java.util.UUID
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow

class MainViewModel : ViewModel() {
    private val _tabIndex = MutableLiveData(0)
    val tabIndex: LiveData<Int> = _tabIndex
    var containerHeight = 600
    var containerWidth = 800
    private val _currentReactNativeStatus = MutableSharedFlow<Bundle>(replay = 0, extraBufferCapacity = 1)
    val currentReactNativeStatus: SharedFlow<Bundle> = _currentReactNativeStatus

    fun setTabIndex(index: Int) {
        _tabIndex.value = index
    }

    fun addApp() {
        val tabModel = AppModel(id = UUID.randomUUID().toString())
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
}
