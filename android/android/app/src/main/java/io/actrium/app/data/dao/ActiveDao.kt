package io.actrium.app.data.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import io.actrium.app.data.entity.ActiveEntity
import io.actrium.app.generated.ActriumAndroidPersistenceContract.ActiveList

@Dao
interface ActiveDao {
    @Query(ActiveList.Query.SELECT_ALL_ORDERED_BY_SORT_ORDER)
    suspend fun getAllActiveList(): List<ActiveEntity>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insertAll(activeList: List<ActiveEntity>)

    @Query(ActiveList.Query.DELETE_ALL)
    suspend fun deleteAll()

    @Query(ActiveList.Query.COUNT_ALL)
    suspend fun getCount(): Int
}
