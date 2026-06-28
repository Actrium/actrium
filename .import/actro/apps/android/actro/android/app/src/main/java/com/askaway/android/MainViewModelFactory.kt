package com.askaway.android

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import chat.askaway.android.data.repository.ActiveRepository

class MainViewModelFactory(
    private val activeRepository: ActiveRepository
) : ViewModelProvider.Factory {
    @Suppress("UNCHECKED_CAST")
    override fun <T : ViewModel> create(modelClass: Class<T>): T {
        if (modelClass.isAssignableFrom(MainViewModel::class.java)) {
            return MainViewModel(activeRepository) as T
        }
        throw IllegalArgumentException("Unknown ViewModel class")
    }
}