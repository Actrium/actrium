package io.actrium.app.data.repository

import io.actrium.app.data.dao.ActiveDao
import io.actrium.app.data.entity.ActiveEntity
import io.actrium.app.model.ActiveModel
import io.actrium.app.utils.ActiveTabCoreAdapter
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

class ActiveRepository(private val activeDao: ActiveDao) {

    suspend fun loadActiveList(): List<ActiveModel> = withContext(Dispatchers.IO) {
        activeDao.getAllActiveList().map { it.toActiveModel() }
    }

    suspend fun saveActiveList(activeList: List<ActiveModel>) = withContext(Dispatchers.IO) {
        val entities = ActiveTabCoreAdapter.rowsFromActiveModels(activeList)
            .map { row -> ActiveEntity.fromRowPayload(row) }
        activeDao.deleteAll()
        activeDao.insertAll(entities)
    }

    suspend fun getActiveListCount(): Int = withContext(Dispatchers.IO) {
        activeDao.getCount()
    }
}
