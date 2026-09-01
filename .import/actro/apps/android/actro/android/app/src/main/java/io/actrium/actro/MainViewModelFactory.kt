package io.actrium.actro

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import io.actrium.actro.data.repository.ActiveRepository
import io.actrium.actro.data.repository.BrowserDataRepository

class MainViewModelFactory(
    private val activeRepository: ActiveRepository,
    private val browserDataRepository: BrowserDataRepository
) : ViewModelProvider.Factory {
    @Suppress("UNCHECKED_CAST")
    override fun <T : ViewModel> create(modelClass: Class<T>): T {
        if (modelClass.isAssignableFrom(MainViewModel::class.java)) {
            return MainViewModel(activeRepository, browserDataRepository) as T
        }
        throw IllegalArgumentException("Unknown ViewModel class")
    }
}
