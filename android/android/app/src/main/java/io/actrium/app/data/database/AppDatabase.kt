package io.actrium.app.data.database

import android.content.Context
import androidx.room.Database
import androidx.room.Room
import androidx.room.RoomDatabase
import io.actrium.app.data.dao.ActiveDao
import io.actrium.app.data.entity.ActiveEntity
import io.actrium.app.generated.ActriumAndroidPersistenceContract

@Database(
    entities = [ActiveEntity::class],
    version = ActriumAndroidPersistenceContract.DATABASE_VERSION,
    exportSchema = false
)
abstract class AppDatabase : RoomDatabase() {
    abstract fun activeDao(): ActiveDao

    companion object {
        @Volatile
        private var INSTANCE: AppDatabase? = null

        fun getDatabase(context: Context): AppDatabase {
            return INSTANCE ?: synchronized(this) {
                // Continue using the existing file, including its SQLite sidecars.
                val legacyName = "actro_database"
                val databaseName = if (context.applicationContext.getDatabasePath(legacyName).exists()) {
                    legacyName
                } else {
                    ActriumAndroidPersistenceContract.DATABASE_NAME
                }
                val instance = Room.databaseBuilder(
                    context.applicationContext,
                    AppDatabase::class.java,
                    databaseName
                ).build()
                INSTANCE = instance
                instance
            }
        }
    }
}
