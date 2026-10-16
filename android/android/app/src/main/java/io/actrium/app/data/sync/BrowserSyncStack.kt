package io.actrium.app.data.sync

import android.content.Context
import android.util.Log
import io.actrium.app.data.database.BrowserStorageDatabase
import io.actrium.app.data.repository.BrowserDataRepository
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob

/**
 * Application-level singleton that owns the browser-data sync stack.
 *
 * Every entry point (MainActivity, BrowserDataModule, TabletViewModel) must share
 * one scheduler over the same SQLite file: constructing separate stacks per
 * activity / RN module / view-model produces mutually unaware schedulers that
 * race on the database. The scheduler runs in this holder's own
 * application-scoped CoroutineScope instead of an activity or view-model
 * lifecycle scope, so pending sync work survives activity destruction.
 */
class BrowserSyncStack private constructor(context: Context) {

    /** Application-scoped coroutine scope backing the shared scheduler. */
    private val syncScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    val database: BrowserStorageDatabase = BrowserStorageDatabase.createDefault(context)

    val credentialStore: BrowserStorageSyncCredentialStore = BrowserStorageSyncCredentialStore(context)

    // Owner of the durable provider connection settings and device identity;
    // adopts the endpoint credential device id the first time it is written so
    // the same machine never appears as two cloud devices.
    val providerSettingsStore: BrowserStorageSyncProviderSettingsStore =
        BrowserStorageSyncProviderSettingsStore(context) { credentialStore.deviceId() }

    val coordinator: BrowserStorageSyncCoordinator = BrowserStorageSyncCoordinator(
        BrowserStorageSyncClient(database),
        credentialStore,
        providerSettingsStore,
    )

    val scheduler: BrowserStorageSyncScheduler = BrowserStorageSyncScheduler(coordinator, syncScope)

    val repository: BrowserDataRepository = BrowserDataRepository(database) {
        scheduler.scheduleAfterLocalChange()
    }

    companion object {
        private const val TAG = "BrowserSyncStack"

        @Volatile
        private var INSTANCE: BrowserSyncStack? = null

        fun getInstance(context: Context): BrowserSyncStack {
            return INSTANCE ?: synchronized(this) {
                INSTANCE ?: BrowserSyncStack(context.applicationContext).also {
                    INSTANCE = it
                    Log.i(TAG, "Browser sync stack initialized at application scope")
                }
            }
        }
    }
}
