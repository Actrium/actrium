package chat.askaway.android.data.repository

import chat.askaway.android.data.dao.ActiveDao
import chat.askaway.android.data.entity.ActiveEntity
import com.askaway.android.model.ActiveModel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

class ActiveRepository(private val activeDao: ActiveDao) {

    suspend fun loadActiveList(): List<ActiveModel> = withContext(Dispatchers.IO) {
        activeDao.getAllActiveList().map { it.toActiveModel() }
    }

    suspend fun saveActiveList(activeList: List<ActiveModel>) = withContext(Dispatchers.IO) {
        val entities = activeList.mapIndexed { index, model ->
            ActiveEntity.fromActiveModel(model, index)
        }
        activeDao.deleteAll()
        activeDao.insertAll(entities)
    }

    suspend fun getActiveListCount(): Int = withContext(Dispatchers.IO) {
        activeDao.getCount()
    }
}