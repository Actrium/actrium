package com.askaway.android

import androidx.lifecycle.LiveData
import androidx.lifecycle.MutableLiveData
import androidx.lifecycle.ViewModel

class MainViewModel : ViewModel() {
    private val _tabIndex = MutableLiveData(0)
    val tabIndex: LiveData<Int> = _tabIndex

    fun setTabIndex(index: Int) {
        _tabIndex.value = index
    }

    fun genTab(){

    }
}
