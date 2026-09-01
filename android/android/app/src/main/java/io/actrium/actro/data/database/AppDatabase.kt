package io.actrium.actro.data.database

import android.content.Context
import androidx.room.Database
import androidx.room.Room
import androidx.room.RoomDatabase
import io.actrium.actro.data.dao.ActiveDao
import io.actrium.actro.data.entity.ActiveEntity
import io.actrium.actro.generated.ActroAndroidPersistenceContract

@Database(
    entities = [ActiveEntity::class],
    version = ActroAndroidPersistenceContract.DATABASE_VERSION,
    exportSchema = false
)
abstract class AppDatabase : RoomDatabase() {
    abstract fun activeDao(): ActiveDao

    companion object {
        @Volatile
        private var INSTANCE: AppDatabase? = null

        fun getDatabase(context: Context): AppDatabase {
            return INSTANCE ?: synchronized(this) {
                val instance = Room.databaseBuilder(
                    context.applicationContext,
                    AppDatabase::class.java,
                    ActroAndroidPersistenceContract.DATABASE_NAME
                ).build()
                INSTANCE = instance
                instance
            }
        }
    }
}
