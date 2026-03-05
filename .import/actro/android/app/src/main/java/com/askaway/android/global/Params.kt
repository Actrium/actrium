package com.askaway.android.global

import com.askaway.android.model.AppModel
import kotlinx.coroutines.flow.MutableStateFlow

object Params {
    val appList = MutableStateFlow<List<AppModel>>(emptyList())

    var currentTabId: String? = null

}