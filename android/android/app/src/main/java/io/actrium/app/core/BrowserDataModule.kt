package io.actrium.app.core

import com.facebook.react.bridge.Arguments
import com.facebook.react.bridge.Promise
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.ReactContextBaseJavaModule
import com.facebook.react.bridge.ReactMethod
import com.facebook.react.bridge.WritableArray
import io.actrium.app.data.database.BrowserBookmarkItem
import io.actrium.app.data.database.BrowserHistoryItem
import io.actrium.app.data.repository.BrowserDataRepository
import io.actrium.app.data.sync.BrowserStorageSyncConnectOutcome
import io.actrium.app.data.sync.BrowserStorageSyncCredentialStore
import io.actrium.app.data.sync.BrowserStorageSyncLocalFolderConnector
import io.actrium.app.data.sync.BrowserStorageSyncProviderSettingsStore
import io.actrium.app.data.sync.BrowserStorageSyncScheduler
import io.actrium.app.data.sync.BrowserSyncStack
import java.nio.charset.StandardCharsets
import java.util.UUID
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import org.json.JSONObject

class BrowserDataModule(reactContext: ReactApplicationContext) :
    ReactContextBaseJavaModule(reactContext) {
    // Only bridges @ReactMethod promises; the shared scheduler runs in the
    // singleton's own application scope and is not tied to this module's lifecycle.
    private val moduleScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val syncStack = BrowserSyncStack.getInstance(reactContext.applicationContext)
    private val syncCredentialStore: BrowserStorageSyncCredentialStore = syncStack.credentialStore
    private val providerSettingsStore: BrowserStorageSyncProviderSettingsStore = syncStack.providerSettingsStore
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

    // Minimal local-folder provider entry, mirroring the validated Windows
    // engine wiring: the connector persists nothing until the Rust
    // capability / adapter / compatibility preflights pass, and on accept the
    // first scheduled run performs the membership join and cycle round. The
    // folder is the app-scoped external directory (a real absolute path Rust
    // can read/write) with no picker and no runtime permission; a user-visible
    // cross-app shared folder is the product-gated SAF path and out of scope.
    @ReactMethod
    fun connectLocalFolderProvider(promise: Promise) {
        moduleScope.launch {
            runCatching {
                val outcome = BrowserStorageSyncLocalFolderConnector.connect(
                    context = reactApplicationContext,
                    store = providerSettingsStore,
                )
                if (outcome.accepted) {
                    syncScheduler.runNow()
                }
                providerStatusJson(outcome)
            }.fold(
                onSuccess = promise::resolve,
                onFailure = { error -> promise.reject("BROWSER_SYNC_PROVIDER_CONNECT_FAILED", error) }
            )
        }
    }

    @ReactMethod
    fun disconnectProvider(promise: Promise) {
        moduleScope.launch {
            runCatching {
                // Disconnect means "stop browser sync entirely": also clear any
                // leftover endpoint session so the reference-server path does
                // not silently reactivate.
                providerSettingsStore.disconnectAndClearEndpointSession(syncCredentialStore)
                syncScheduler.cancelPendingSync()
                providerStatusJson()
            }.fold(
                onSuccess = promise::resolve,
                onFailure = { error -> promise.reject("BROWSER_SYNC_PROVIDER_DISCONNECT_FAILED", error) }
            )
        }
    }

    @ReactMethod
    fun getProviderStatus(promise: Promise) {
        moduleScope.launch {
            runCatching {
                providerStatusJson()
            }.fold(
                onSuccess = promise::resolve,
                onFailure = { error -> promise.reject("BROWSER_SYNC_PROVIDER_STATUS_FAILED", error) }
            )
        }
    }

    private fun providerStatusJson(connectOutcome: BrowserStorageSyncConnectOutcome? = null): String {
        val settings = connectOutcome?.settings ?: providerSettingsStore.load()
        val json = JSONObject()
            .put("version", 1)
            .put("providerKind", settings.providerKind ?: JSONObject.NULL)
            .put("configured", settings.isConfigured)
            .put("localFolderRootPath", settings.localFolderRootPath ?: JSONObject.NULL)
            .put("membershipActive", settings.membershipJoinedAtUnixSeconds != null)
            .put("scope", settings.scope)
        if (connectOutcome != null) {
            json.put("accepted", connectOutcome.accepted)
            json.put("blockedReason", connectOutcome.blockedReasonKey ?: JSONObject.NULL)
            json.put("blockedDetail", connectOutcome.detailText)
        }
        return json.toString()
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
