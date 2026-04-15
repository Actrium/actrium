package chat.askaway.android.data.dao

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import chat.askaway.android.data.entity.ActiveEntity

@Dao
interface ActiveDao {
    @Query("SELECT * FROM active_list ORDER BY sortOrder ASC")
    suspend fun getAllActiveList(): List<ActiveEntity>

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insertAll(activeList: List<ActiveEntity>)

    @Query("DELETE FROM active_list")
    suspend fun deleteAll()

    @Query("SELECT COUNT(*) FROM active_list")
    suspend fun getCount(): Int
}