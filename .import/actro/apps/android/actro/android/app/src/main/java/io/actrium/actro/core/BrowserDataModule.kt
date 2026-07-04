package io.actrium.actro.core

import com.facebook.react.bridge.Arguments
import com.facebook.react.bridge.Promise
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.ReactContextBaseJavaModule
import com.facebook.react.bridge.ReactMethod
import com.facebook.react.bridge.WritableArray
import io.actrium.actro.data.database.BrowserBookmarkItem
import io.actrium.actro.data.database.BrowserHistoryItem
import io.actrium.actro.data.repository.BrowserDataRepository
import io.actrium.actro.data.sync.BrowserStorageSyncCredentialStore
import io.actrium.actro.data.sync.BrowserStorageSyncScheduler
import io.actrium.actro.data.sync.BrowserSyncStack
import java.nio.charset.StandardCharsets
import java.util.UUID
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

class BrowserDataModule(reactContext: ReactApplicationContext) :
    ReactContextBaseJavaModule(reactContext) {
    // Only bridges @ReactMethod promises; the shared scheduler runs in the
    // singleton's own application scope and is not tied to this module's lifecycle.
    private val moduleScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val syncStack = BrowserSyncStack.getInstance(reactContext.applicationContext)
    private val syncCredentialStore: BrowserStorageSyncCredentialStore = syncStack.credentialStore
    private val syncScheduler: BrowserStorageSyncScheduler = syncStack.scheduler
    private val repository: BrowserDataRepository = syncStack.repository

    override fun getName(): String = "BrowserDataModule"

    override fun invalidate() {
        // Cancels only the promise-bridging coroutines above; pending sync work
        // in the shared scheduler is unaffected because it runs in the
        // BrowserSyncStack application scope.
        moduleScope.cancel()
        super.invalidate()
    }

    @ReactMethod
    fun getRecentHistory(limit: Int, promise: Promise) {
        moduleScope.launch {
            runCatching {
                historyItemsToArray(repository.loadRecentHistory(limit))
            }.fold(
                onSuccess = promise::resolve,
                onFailure = { error -> promise.reject("BROWSER_HISTORY_READ_FAILED", error) }
            )
        }
    }

    @ReactMethod
    fun deleteHistoryItem(address: String, promise: Promise) {
        moduleScope.launch {
            runCatching {
                repository.deleteHistory(address)
            }.fold(
                onSuccess = promise::resolve,
                onFailure = { error -> promise.reject("BROWSER_HISTORY_DELETE_FAILED", error) }
            )
        }
    }

    @ReactMethod
    fun clearHistory(promise: Promise) {
        moduleScope.launch {
            runCatching {
                repository.clearHistory()
            }.fold(
                onSuccess = promise::resolve,
                onFailure = { error -> promise.reject("BROWSER_HISTORY_CLEAR_FAILED", error) }
            )
        }
    }

    @ReactMethod
    fun getBookmarks(limit: Int, promise: Promise) {
        moduleScope.launch {
            runCatching {
                bookmarkItemsToArray(repository.loadBookmarks(limit))
            }.fold(
                onSuccess = promise::resolve,
                onFailure = { error -> promise.reject("BROWSER_BOOKMARK_READ_FAILED", error) }
            )
        }
    }

    @ReactMethod
    fun deleteBookmark(address: String, promise: Promise) {
        moduleScope.launch {
            runCatching {
                repository.deleteBookmark(address)
            }.fold(
                onSuccess = promise::resolve,
                onFailure = { error -> promise.reject("BROWSER_BOOKMARK_DELETE_FAILED", error) }
            )
        }
    }

    @ReactMethod
    fun clearBookmarks(promise: Promise) {
        moduleScope.launch {
            runCatching {
                repository.clearBookmarks()
            }.fold(
                onSuccess = promise::resolve,
                onFailure = { error -> promise.reject("BROWSER_BOOKMARK_CLEAR_FAILED", error) }
            )
        }
    }

    @ReactMethod
    fun saveSyncSession(environment: String, token: String, baseUrl: String, promise: Promise) {
        moduleScope.launch {
            runCatching {
                syncCredentialStore.saveSession(
                    environment = environment,
                    accessToken = token,
                    baseUrl = baseUrl,
                )
                syncScheduler.scheduleAfterLocalChange(delaySeconds = 0)
                true
            }.fold(
                onSuccess = promise::resolve,
                onFailure = { error -> promise.reject("BROWSER_SYNC_SESSION_SAVE_FAILED", error) }
            )
        }
    }

    @ReactMethod
    fun clearSyncSession(environment: String, promise: Promise) {
        moduleScope.launch {
            runCatching {
                syncCredentialStore.clearSession(environment)
                syncScheduler.cancelPendingSync()
                true
            }.fold(
                onSuccess = promise::resolve,
                onFailure = { error -> promise.reject("BROWSER_SYNC_SESSION_CLEAR_FAILED", error) }
            )
        }
    }

    @ReactMethod
    fun syncNow(promise: Promise) {
        moduleScope.launch {
            runCatching {
                syncScheduler.runNow()
            }.fold(
                onSuccess = promise::resolve,
                onFailure = { error -> promise.reject("BROWSER_SYNC_FAILED", error) }
            )
        }
    }

    @ReactMethod
    fun getSyncStatus(promise: Promise) {
        moduleScope.launch {
            runCatching {
                syncScheduler.statusJson()
            }.fold(
                onSuccess = promise::resolve,
                onFailure = { error -> promise.reject("BROWSER_SYNC_STATUS_FAILED", error) }
            )
        }
    }

    private fun historyItemsToArray(items: List<BrowserHistoryItem>): WritableArray {
        val array = Arguments.createArray()
        items.forEach { item ->
            val map = Arguments.createMap()
            map.putString("id", stableId("history", item.address))
            map.putString("title", item.title)
            map.putString("appURL", item.address)
            map.putDouble("visitTime", item.visitedAtUnixSeconds.toDouble() * 1000.0)
            array.pushMap(map)
        }
        return array
    }

    private fun bookmarkItemsToArray(items: List<BrowserBookmarkItem>): WritableArray {
        val array = Arguments.createArray()
        items.forEach { item ->
            val map = Arguments.createMap()
            map.putString("id", item.id)
            map.putString("title", item.title)
            map.putString("appURL", item.address)
            map.putDouble("createdAt", item.createdAtUnixSeconds.toDouble() * 1000.0)
            map.putDouble("lastUsed", item.updatedAtUnixSeconds.toDouble() * 1000.0)
            array.pushMap(map)
        }
        return array
    }

    private fun stableId(prefix: String, value: String): String {
        return UUID.nameUUIDFromBytes("$prefix:$value".toByteArray(StandardCharsets.UTF_8)).toString()
    }
}
