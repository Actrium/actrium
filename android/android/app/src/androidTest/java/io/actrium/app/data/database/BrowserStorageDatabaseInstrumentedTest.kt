package io.actrium.app.data.database

import android.content.Context
import android.database.Cursor
import android.database.sqlite.SQLiteDatabase
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.actrium.app.data.sync.BrowserStorageSyncClient
import io.actrium.app.data.sync.BrowserStorageSyncCoordinator
import io.actrium.app.data.sync.BrowserStorageSyncCredentialStore
import io.actrium.app.data.sync.BrowserStorageSyncCredentials
import io.actrium.app.data.sync.BrowserStorageSyncCredentialsProvider
import io.actrium.app.data.sync.BrowserStorageSyncProviderSettingsStore
import io.actrium.app.data.sync.BrowserStorageSyncHttpRequest
import io.actrium.app.data.sync.BrowserStorageSyncHttpResponse
import io.actrium.app.data.sync.BrowserStorageSyncScheduler
import io.actrium.app.data.sync.BrowserStorageSyncTransport
import io.actrium.app.rust.ActriumCore
import java.io.File
import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class BrowserStorageDatabaseInstrumentedTest {
    private val context: Context
        get() = ApplicationProvider.getApplicationContext()

    @After
    fun cleanUp() {
        deleteDatabaseFiles()
    }

    @Test
    fun plansCloudProviderAdapterObjectSpaceThroughRust() {
        val contract = JSONObject(ActriumCore.browserStorageSyncCloudProviderAdapterContractJson())
        assertEquals("cloudProviderAdapterPlan", contract.getString("planType"))
        assertEquals("Actrium/BrowserSync", contract.getJSONObject("objectSpace").getString("rootPath"))
        assertTrue(contract.getString("operationBoundary").contains("coarse rounds"))

        val localFolderCapabilities = ActriumCore.browserStorageSyncCloudLocalFolderCapabilitiesJson()
        val ready = JSONObject(
            ActriumCore.browserStorageSyncCloudProviderAdapterPlanJson(
                adapterSnapshotJson = localFolderCapabilities,
                observedAtUnixSeconds = 1_690,
            ),
        )
        assertEquals("cloudProviderAdapterPlan", ready.getString("planType"))
        assertEquals("ready", ready.getString("classification"))
        assertEquals("localFolder", ready.getString("adapterFamily"))
        assertTrue(ready.getBoolean("productionReady"))
        assertTrue(ready.getBoolean("shouldAllowSync"))
        assertFalse(ready.getBoolean("shouldBlockSync"))
        assertEquals(
            "Actrium/BrowserSync/manifest.json",
            ready.getJSONObject("objectLayout").getString("manifestPath"),
        )
        assertTrue(
            (0 until ready.getJSONArray("requiredPrefixes").length()).any { index ->
                ready.getJSONArray("requiredPrefixes")
                    .getJSONObject(index)
                    .getString("prefix") == "Actrium/BrowserSync/logs/"
            },
        )

        val setup = JSONObject(
            ActriumCore.browserStorageSyncCloudProviderAdapterPlanJson(
                adapterSnapshotJson = JSONObject()
                    .put("providerKind", "localFolder")
                    .put("rootPath", "Wrong/BrowserSync")
                    .put("revisionKind", "contentHash")
                    .put("supportsPrefixList", true)
                    .put("supportsPagedList", true)
                    .put("supportsConditionalWrite", true)
                    .put("supportsConditionalDelete", true)
                    .put("supportsReadIfChanged", true)
                    .put("supportsAtomicRename", false)
                    .put("supportsAppFolder", true)
                    .put("backgroundSync", "platformScheduled")
                    .put("conflictResolution", "optimisticRevision")
                    .put("maxObjectBytes", 8_388_608)
                    .put("maxPageSize", 1_000)
                    .toString(),
                observedAtUnixSeconds = 1_691,
            ),
        )
        assertEquals("providerSetupRequired", setup.getString("classification"))
        assertTrue(setup.getBoolean("shouldBlockSync"))
        assertEquals("configureActriumBrowserSyncRoot", setup.getString("rootPathAction"))
    }

    @Test
    fun classifiesCloudProviderConnectionStatusThroughRust() {
        val contract = JSONObject(ActriumCore.browserStorageSyncCloudProviderConnectionContractJson())
        assertEquals("cloudProviderConnectionStatusPolicy", contract.getString("planType"))
        assertTrue(contract.getString("productConstraint").contains("No Actrium product server"))

        val ready = JSONObject(
            ActriumCore.browserStorageSyncCloudProviderConnectionStatusPolicyJson(
                providerSnapshotJson = JSONObject()
                    .put("providerKind", "localFolder")
                    .put("scope", "profile")
                    .put("deviceId", "device-android")
                    .put("connectionState", "connected")
                    .put("credentialState", "available")
                    .put("membershipStatus", "active")
                    .put("cloudStoreReady", true)
                    .toString(),
                retryAttempt = 0u,
                observedAtUnixSeconds = 1_700,
            ),
        )
        assertEquals("ready", ready.getString("classification"))
        assertTrue(ready.getBoolean("shouldAttemptSync"))
        assertFalse(ready.getBoolean("shouldAttemptMembership"))
        assertEquals("idle", ready.getString("schedulerAction"))

        val membership = JSONObject(
            ActriumCore.browserStorageSyncCloudProviderConnectionStatusPolicyJson(
                providerSnapshotJson = JSONObject()
                    .put("providerKind", "googleDrive")
                    .put("providerAccountId", "user@example.com")
                    .put("scope", "profile")
                    .put("deviceId", "device-android")
                    .put("connectionState", "connected")
                    .put("credentialState", "available")
                    .put("membershipStatus", "missing")
                    .put("cloudStoreReady", true)
                    .toString(),
                retryAttempt = 0u,
                observedAtUnixSeconds = 1_701,
            ),
        )
        assertEquals("membershipRequired", membership.getString("classification"))
        assertTrue(membership.getBoolean("shouldAttemptMembership"))
        assertFalse(membership.getBoolean("shouldAttemptSync"))
        assertEquals("writeCloudMembership", membership.getString("providerAction"))

        val retry = JSONObject(
            ActriumCore.browserStorageSyncCloudProviderConnectionStatusPolicyJson(
                providerSnapshotJson = JSONObject()
                    .put("providerKind", "oneDrive")
                    .put("providerAccountId", "onedrive-user")
                    .put("connectionState", "connecting")
                    .put("credentialState", "available")
                    .put("membershipStatus", "active")
                    .put("cloudStoreReady", true)
                    .put("lastProviderErrorKind", "network")
                    .toString(),
                retryAttempt = 1u,
                observedAtUnixSeconds = 1_702,
            ),
        )
        assertEquals("providerUnavailable", retry.getString("classification"))
        assertTrue(retry.getBoolean("shouldRetry"))
        assertEquals(10, retry.getInt("retryDelaySeconds"))
        assertEquals("scheduleRetry", retry.getString("schedulerAction"))
    }

    @Test
    fun classifiesCloudProviderErrorsThroughRust() {
        val policy = JSONObject(ActriumCore.browserStorageSyncCloudProviderErrorPolicyJson())
        assertEquals("cloudProviderErrorClassification", policy.getString("planType"))
        assertTrue(policy.getString("policyBoundary").contains("coarse native/provider rounds"))

        val reread = JSONObject(
            ActriumCore.browserStorageSyncCloudProviderErrorClassificationJson(
                providerErrorsJson = JSONObject()
                    .put("providerKind", "googleDrive")
                    .put(
                        "errors",
                        org.json.JSONArray()
                            .put(
                                JSONObject()
                                    .put("operationIndex", 1)
                                    .put("operationType", "writeManifest")
                                    .put("path", "Actrium/BrowserSync/manifest.json")
                                    .put("providerStatusCode", 412)
                                    .put("message", "etag mismatch"),
                            ),
                    )
                    .toString(),
                retryAttempt = 0u,
                observedAtUnixSeconds = 1_703,
            ),
        )
        assertEquals("cloudStateRereadRequired", reread.getString("classification"))
        assertTrue(reread.getBoolean("shouldRereadCloud"))
        assertEquals("runCloudReread", reread.getString("schedulerAction"))
        assertEquals(
            "conflict",
            reread.getJSONArray("errors").getJSONObject(0).getString("canonicalErrorKind"),
        )

        val diagnostics = JSONObject(
            ActriumCore.browserStorageSyncCloudProviderDiagnosticsJson(
                capabilityPreflightJson = "",
                compatibilityPreflightJson = "",
                providerErrorClassificationJson = reread.toString(),
                cycleStatusPolicyJson = "",
                scheduleStateJson = "",
                observedAtUnixSeconds = 1_705,
            ),
        )
        assertEquals("cloudConflict", diagnostics.getString("classification"))
        assertEquals(
            "cloudStateRereadRequired",
            diagnostics.getString("providerErrorClassification"),
        )
        assertTrue(diagnostics.getBoolean("pendingCloudReread"))

        val retry = JSONObject(
            ActriumCore.browserStorageSyncCloudProviderErrorClassificationJson(
                providerErrorsJson = JSONObject()
                    .put("providerKind", "oneDrive")
                    .put(
                        "operationResults",
                        org.json.JSONArray()
                            .put(
                                JSONObject()
                                    .put("operationIndex", 2)
                                    .put("operationType", "listLogs")
                                    .put("success", false)
                                    .put("providerStatusCode", 503),
                            ),
                    )
                    .toString(),
                retryAttempt = 1u,
                observedAtUnixSeconds = 1_704,
            ),
        )
        assertEquals("providerUnavailable", retry.getString("classification"))
        assertTrue(retry.getBoolean("shouldRetry"))
        assertEquals(10, retry.getInt("retryDelaySeconds"))
        assertEquals("retryProviderOperation", retry.getString("providerAction"))
    }

    @Test
    fun saveSessionAdoptsCoreNormalizedSnapshot() {
        deleteDatabaseFiles()
        val database = BrowserStorageDatabase.createDefault(context)
        val longTitle = "t".repeat(200)
        val snapshot = BrowserSessionSnapshot(
            activeTabIndex = 9,
            tabs = listOf(
                BrowserSessionTab(
                    id = "",
                    title = longTitle,
                    address = "https://user:secret@example.com:443/private",
                    createdAtUnixSeconds = 1_600,
                ),
                BrowserSessionTab(
                    id = "tab-docs",
                    title = "   ",
                    address = "https://example.com/docs",
                    createdAtUnixSeconds = 1_601,
                ),
            ),
            surfaceKind = "android",
        )
        assertTrue(database.saveSession(snapshot = snapshot))

        val restored = database.loadSession()
        assertNotNull(restored)
        assertEquals(2, restored?.tabs?.size)
        assertEquals(1, restored?.activeTabIndex)
        assertEquals("https://example.com/private", restored?.tabs?.first()?.address)
        assertEquals(160, restored?.tabs?.first()?.title?.length)
        assertEquals(1_600L, restored?.tabs?.first()?.createdAtUnixSeconds)
        assertEquals("tab-docs", restored?.tabs?.last()?.id)
        assertEquals("New Tab", restored?.tabs?.last()?.title)
        assertEquals(1_601L, restored?.tabs?.last()?.createdAtUnixSeconds)
        assertEquals("android", restored?.surfaceKind)

        // Stable tab ids are deterministic: saving the same snapshot again
        // resolves the empty-id tab to the same Rust-derived id.
        val stableId = restored?.tabs?.first()?.id.orEmpty()
        assertTrue(stableId.isNotEmpty())
        assertTrue(database.saveSession(snapshot = snapshot))
        assertEquals(stableId, database.loadSession()?.tabs?.first()?.id)
    }

    @Test
    fun migratesBrowserStorageSQLiteV1V2V3ToV6() {
        for (legacyVersion in 1..3) {
            deleteDatabaseFiles()
            createLegacyBrowserStorageDatabase(legacyVersion)

            val database = BrowserStorageDatabase.createDefault(context)

            val history = database.loadRecentHistory(limit = 10)
            assertEquals(1, history.size)
            assertEquals("https://example.com/history-v$legacyVersion", history.first().address)
            assertEquals(2, history.first().visitCount)

            val bookmarks = database.loadRecentBookmarks(limit = 10)
            assertEquals(1, bookmarks.size)
            assertEquals("https://example.com/bookmark-v$legacyVersion", bookmarks.first().address)
            assertEquals("user", bookmarks.first().ownerKind)
            assertEquals("", bookmarks.first().ownerId)

            val session = database.loadSession()
            assertNotNull(session)
            assertEquals(1, session?.tabs?.size)
            assertEquals("https://example.com/session-v$legacyVersion", session?.tabs?.first()?.address)

            assertBrowserStorageV6Shape(legacyVersion)

            val batch = JSONObject(
                database.exportDirtySyncBatch(
                    scope = "profile",
                    deviceId = "device-android",
                    generatedAtUnixSeconds = 1234,
                    limitPerCollection = 20,
                ),
            )
            assertEquals("profile", batch.getString("scope"))
            assertEquals("device-android", batch.getString("deviceId"))
            assertTrue(batchHasRecord(batch, "historyPage", "https://example.com/history-v$legacyVersion"))
            assertTrue(batchHasRecord(batch, "historyVisit", "visit-v$legacyVersion"))
            assertTrue(batchHasRecord(batch, "bookmark", "bookmark-v$legacyVersion"))
            assertTrue(batchHasBookmarkOwner(batch, "bookmark-v$legacyVersion", "user", ""))
            assertTrue(batchHasRecord(batch, "session", "default"))
            assertTrue(batchHasRecord(batch, "sessionTab"))

            val pushRequest = JSONObject(
                database.exportDirtySyncPushRequest(
                    scope = "profile",
                    deviceId = "device-android",
                    generatedAtUnixSeconds = 1235,
                    limitPerCollection = 20,
                ),
            )
            assertEquals("push", pushRequest.getString("requestType"))
            assertTrue(pushRequest.has("pushBatch"))

            assertTrue(
                database.applySyncAckBatch(
                    JSONObject()
                        .put("version", 1)
                        .put("storageSchemaVersion", 6)
                        .put("scope", "profile")
                        .put("deviceId", "device-android")
                        .put("syncedAtUnixSeconds", 1240)
                        .put(
                            "records",
                            org.json.JSONArray()
                                .put(
                                    JSONObject()
                                        .put("recordType", "historyPage")
                                        .put("recordId", "https://example.com/history-v$legacyVersion")
                                        .put("syncRevision", 999),
                                ),
                        )
                        .put("cursors", org.json.JSONArray())
                        .toString(),
                ),
            )
            assertEquals(
                "synced",
                queryScalarString(
                    "SELECT sync_state FROM browser_history_pages WHERE address = 'https://example.com/history-v$legacyVersion'",
                ),
            )
            assertEquals(
                "999",
                queryScalarString(
                    "SELECT sync_revision FROM browser_history_pages WHERE address = 'https://example.com/history-v$legacyVersion'",
                ),
            )

            val remoteBatchJson = ActriumCore.browserStorageSyncBatchFromDirtyRowsJson(
                scope = "profile",
                deviceId = "device-android",
                generatedAtUnixSeconds = 1250,
                rowsJson = """
                    [
                        {
                            "record_type": "historyPage",
                            "record_id": "https://example.com/remote-v$legacyVersion",
                            "sync_revision": ${1200 + legacyVersion},
                            "sync_state": "remote",
                            "origin_device_id": "device-remote",
                            "updated_at": 1250,
                            "address": "https://example.com/remote-v$legacyVersion",
                            "title": "Remote $legacyVersion",
                            "visit_count": 3,
                            "last_visited_at": 1250,
                            "created_at": 1240
                        }
                    ]
                """.trimIndent(),
            )
            val cycleResponseJson = ActriumCore.browserStorageSyncCycleResponseJson(
                remoteBatchJson,
                JSONObject()
                    .put("version", 1)
                    .put("storageSchemaVersion", 6)
                    .put("scope", "profile")
                    .put("deviceId", "device-android")
                    .put("syncedAtUnixSeconds", 1251)
                    .put("records", org.json.JSONArray())
                    .put("cursors", org.json.JSONArray())
                    .toString(),
            )
            val cycleResult = JSONObject(database.applySyncCycleResponse(cycleResponseJson))
            assertEquals("applyRemoteThenAck", cycleResult.getString("action"))
            assertTrue(cycleResult.getBoolean("ackApplied"))
            assertFalse(cycleResult.getBoolean("cursorAckDeferred"))
            assertEquals(
                "Remote $legacyVersion",
                queryScalarString(
                    "SELECT title FROM browser_history_pages WHERE address = 'https://example.com/remote-v$legacyVersion'",
                ),
            )
            assertEquals(
                "${1200 + legacyVersion}",
                queryScalarString(
                    "SELECT cursor FROM browser_sync_cursors WHERE scope = 'profile' AND collection = 'historyPages'",
                ),
            )

            val cycleRequestPlan = JSONObject(
                database.exportSyncCycleRequestPlan(
                    baseUrl = "http://127.0.0.1:8787/",
                    accessToken = "token-android-123",
                    scope = "profile",
                    deviceId = "device-android",
                    generatedAtUnixSeconds = 1260,
                    limitPerCollection = 20,
                ),
            )
            assertEquals("POST", cycleRequestPlan.getString("method"))
            assertEquals("/v1/browser-storage/sync/cycle", cycleRequestPlan.getString("path"))
            assertEquals(
                "http://127.0.0.1:8787/v1/browser-storage/sync/cycle",
                cycleRequestPlan.getString("url"),
            )
            val headers = cycleRequestPlan.getJSONArray("headers")
            assertTrue(
                (0 until headers.length()).any { index ->
                    val header = headers.getJSONObject(index)
                    header.getString("name") == "Authorization" &&
                        header.getString("value") == "Bearer token-android-123"
                },
            )
            val requestBody = JSONObject(cycleRequestPlan.getString("bodyJson"))
            assertEquals("push", requestBody.getJSONObject("pushRequest").getString("requestType"))
            val requestCursors = requestBody.getJSONArray("cursors")
            assertTrue(
                (0 until requestCursors.length()).any { index ->
                    val cursor = requestCursors.getJSONObject(index)
                    cursor.getString("collection") == "historyPages" &&
                        cursor.getString("cursor") == "${1200 + legacyVersion}"
                },
            )

            assertTrue(database.deleteBookmark("https://example.com/bookmark-v$legacyVersion"))
            assertEquals(
                "1",
                queryScalarString(
                    "SELECT is_deleted FROM browser_bookmarks WHERE id = 'bookmark-v$legacyVersion'",
                ),
            )
            assertFalse(
                queryScalarString(
                    "SELECT deleted_at FROM browser_bookmarks WHERE id = 'bookmark-v$legacyVersion'",
                ).isNullOrBlank(),
            )

            val tombstoneBatch = JSONObject(
                database.exportDirtySyncBatch(
                    scope = "profile",
                    deviceId = "device-android",
                    generatedAtUnixSeconds = 1236,
                    limitPerCollection = 20,
                ),
            )
            assertTrue(batchHasDeletedBookmark(tombstoneBatch, "bookmark-v$legacyVersion"))
        }
    }

    @Test
    fun syncClientExecutesInjectedCycleTransportAndAppliesResponse() = runBlocking {
        deleteDatabaseFiles()
        val database = BrowserStorageDatabase.createDefault(context)
        assertTrue(
            database.upsertHistoryItem(
                BrowserHistoryItem(
                    title = "Local",
                    address = "https://example.com/local",
                    visitedAtUnixSeconds = 1300,
                ),
            ),
        )

        val remoteBatchJson = ActriumCore.browserStorageSyncBatchFromDirtyRowsJson(
            scope = "profile",
            deviceId = "device-android",
            generatedAtUnixSeconds = 1310,
            rowsJson = """
                [
                    {
                        "record_type": "historyPage",
                        "record_id": "https://example.com/sync-client-remote",
                        "sync_revision": 2001,
                        "sync_state": "remote",
                        "origin_device_id": "device-remote",
                        "updated_at": 1310,
                        "address": "https://example.com/sync-client-remote",
                        "title": "Remote From Client",
                        "visit_count": 2,
                        "last_visited_at": 1310,
                        "created_at": 1309
                    }
                ]
            """.trimIndent(),
        )
        val cycleResponseJson = ActriumCore.browserStorageSyncCycleResponseJson(
            remoteBatchJson,
            JSONObject()
                .put("version", 1)
                .put("storageSchemaVersion", 6)
                .put("scope", "profile")
                .put("deviceId", "device-android")
                .put("syncedAtUnixSeconds", 1311)
                .put("records", org.json.JSONArray())
                .put("cursors", org.json.JSONArray())
                .toString(),
        )
        val transport = FakeBrowserStorageSyncTransport(cycleResponseJson)
        val client = BrowserStorageSyncClient(database, transport)

        val result = JSONObject(
            client.runSyncCycle(
                baseUrl = "http://127.0.0.1:8787/",
                accessToken = "token-android-client",
                scope = "profile",
                deviceId = "device-android",
                generatedAtUnixSeconds = 1312,
                limitPerCollection = 20,
            ),
        )

        assertTrue(result.getBoolean("applied"))
        assertEquals("success", result.getString("classification"))
        assertEquals("POST", transport.lastRequest?.method)
        assertEquals(
            "http://127.0.0.1:8787/v1/browser-storage/sync/cycle",
            transport.lastRequest?.url,
        )
        assertEquals("Bearer token-android-client", transport.lastRequest?.headers?.get("Authorization"))
        val requestBody = JSONObject(transport.lastRequest?.bodyJson ?: "{}")
        assertEquals("push", requestBody.getJSONObject("pushRequest").getString("requestType"))
        assertEquals(
            "Remote From Client",
            queryScalarString(
                "SELECT title FROM browser_history_pages WHERE address = 'https://example.com/sync-client-remote'",
            ),
        )
    }

    @Test
    fun syncClientRunsLocalFolderCloudCycleAndAppliesClientPlan() = runBlocking {
        val testRoot = File(context.cacheDir, "actrium-browser-sync-${System.nanoTime()}")
        val remoteDatabaseFile = File(testRoot, "remote.db")
        val localDatabaseFile = File(testRoot, "local.db")
        val cloudRoot = File(testRoot, "cloud")

        try {
            val remoteDatabase = BrowserStorageDatabase.createAtPath(remoteDatabaseFile.absolutePath)
            assertTrue(
                remoteDatabase.recordHistoryVisit(
                    BrowserHistoryItem(
                        title = "Remote Local Folder",
                        address = "https://example.com/android-local-folder-remote",
                        visitedAtUnixSeconds = 1700,
                    ),
                ),
            )

            val remoteClient = BrowserStorageSyncClient(remoteDatabase)
            val seedResult = JSONObject(
                remoteClient.runLocalFolderSyncCycle(
                    rootDir = cloudRoot.absolutePath,
                    scope = "profile",
                    deviceId = "device-remote",
                    generatedAtUnixSeconds = 1710,
                    limitPerCollection = 20,
                    observedAtUnixSeconds = 1710,
                ),
            )

            assertEquals("cloudLocalFolderCycle", seedResult.getString("endpointType"))
            assertEquals("localFolder", seedResult.getString("providerKind"))
            assertEquals(
                "cloudStoreCapabilityPreflight",
                seedResult.getJSONObject("capabilityPreflight").getString("planType"),
            )
            assertEquals(
                "ready",
                seedResult.getJSONObject("capabilityPreflight").getString("classification"),
            )
            assertEquals(
                "localfolder",
                seedResult.getJSONObject("capabilityPreflight")
                    .getJSONObject("capabilities")
                    .getString("providerKind"),
            )
            assertEquals(
                "contenthash",
                seedResult.getJSONObject("capabilityPreflight")
                    .getJSONObject("capabilities")
                    .getString("revisionKind"),
            )
            assertTrue(seedResult.getJSONObject("capabilityPreflight").getBoolean("shouldAllowSync"))
            assertEquals(
                "cloudCompatibilityPreflight",
                seedResult.getJSONObject("compatibilityPreflight").getString("planType"),
            )
            assertEquals(
                "compatible",
                seedResult.getJSONObject("compatibilityPreflight").getString("classification"),
            )
            assertEquals(0, seedResult.getJSONObject("compatibilityPreflight").getInt("documentCount"))
            assertTrue(seedResult.isNull("schemaRolloutPlan"))
            assertTrue(seedResult.isNull("schemaMigrationPlaybook"))
            assertTrue(seedResult.isNull("recoverySnapshotRound"))
            assertEquals("success", seedResult.getString("classification"))
            assertTrue(seedResult.getJSONObject("applyResult").getBoolean("ackApplied"))
            assertEquals(
                "compactionRound",
                seedResult.getJSONObject("compactionRound").getString("planType"),
            )
            assertTrue(seedResult.getJSONObject("compactionRound").getBoolean("completed"))
            assertEquals(
                "nothingToCompact",
                seedResult.getJSONObject("compactionRound").getString("clientAction"),
            )
            assertEquals(
                "cleanupRound",
                seedResult.getJSONObject("cleanupRound").getString("planType"),
            )
            assertTrue(seedResult.getJSONObject("cleanupRound").getBoolean("cleanupCompleted"))
            assertEquals(
                "nothingToCleanup",
                seedResult.getJSONObject("cleanupRound").getString("clientAction"),
            )
            assertEquals(
                "applyLocalAck",
                seedResult.getJSONObject("attemptPlan").getString("clientPlanAction"),
            )
            assertEquals(
                "cloudCycleScheduleState",
                seedResult.getJSONObject("scheduleState").getString("planType"),
            )
            assertEquals(
                "cloudCycleLoopPlan",
                seedResult.getJSONObject("loopPlan").getString("planType"),
            )
            assertEquals(1, seedResult.getJSONObject("loopPlan").getInt("roundCount"))
            assertEquals(
                "applyLocalAck",
                seedResult.getJSONObject("loopPlan").getString("finalAction"),
            )
            assertEquals(
                "cloudProviderDiagnostics",
                seedResult.getJSONObject("providerDiagnostics").getString("planType"),
            )
            assertEquals(
                "success",
                seedResult.getJSONObject("providerDiagnostics").getString("cycleClassification"),
            )
            assertEquals(
                "compatible",
                seedResult.getJSONObject("providerDiagnostics").getString("compatibilityClassification"),
            )
            assertEquals(
                "cloudObservabilityEvent",
                seedResult.getJSONObject("observabilityEvent").getString("planType"),
            )
            assertEquals(
                "success",
                seedResult.getJSONObject("observabilityEvent").getString("classification"),
            )
            assertTrue(seedResult.getJSONObject("observabilityEvent").getBoolean("redactedExportAllowed"))
            assertTrue(File(cloudRoot, "Actrium/BrowserSync/manifest.json").exists())

            val localDatabase = BrowserStorageDatabase.createAtPath(localDatabaseFile.absolutePath)
            val localClient = BrowserStorageSyncClient(localDatabase)
            val pullResult = JSONObject(
                localClient.runLocalFolderSyncCycle(
                    rootDir = cloudRoot.absolutePath,
                    scope = "profile",
                    deviceId = "device-android",
                    generatedAtUnixSeconds = 1720,
                    limitPerCollection = 20,
                    observedAtUnixSeconds = 1720,
                ),
            )

            assertEquals("cloudLocalFolderCycle", pullResult.getString("endpointType"))
            assertEquals(
                "ready",
                pullResult.getJSONObject("capabilityPreflight").getString("classification"),
            )
            assertEquals(
                "cloudCompatibilityPreflight",
                pullResult.getJSONObject("compatibilityPreflight").getString("planType"),
            )
            assertEquals(
                "compatible",
                pullResult.getJSONObject("compatibilityPreflight").getString("classification"),
            )
            assertTrue(pullResult.getJSONObject("compatibilityPreflight").getInt("documentCount") > 0)
            assertTrue(pullResult.isNull("schemaRolloutPlan"))
            assertTrue(pullResult.isNull("schemaMigrationPlaybook"))
            assertTrue(pullResult.isNull("recoverySnapshotRound"))
            assertEquals("success", pullResult.getString("classification"))
            assertTrue(pullResult.getBoolean("applied"))
            assertTrue(pullResult.getJSONObject("applyResult").getBoolean("remoteApplied"))
            assertEquals(
                "compactionRound",
                pullResult.getJSONObject("compactionRound").getString("planType"),
            )
            assertTrue(pullResult.getJSONObject("compactionRound").getBoolean("completed"))
            assertEquals(
                "nothingToCompact",
                pullResult.getJSONObject("compactionRound").getString("clientAction"),
            )
            assertEquals(
                "cleanupRound",
                pullResult.getJSONObject("cleanupRound").getString("planType"),
            )
            assertTrue(pullResult.getJSONObject("cleanupRound").getBoolean("cleanupCompleted"))
            assertEquals(
                "nothingToCleanup",
                pullResult.getJSONObject("cleanupRound").getString("clientAction"),
            )
            assertEquals(
                "applyRemoteBatch",
                pullResult.getJSONObject("attemptPlan").getString("clientPlanAction"),
            )
            assertEquals(
                "cloudCycleScheduleState",
                pullResult.getJSONObject("scheduleState").getString("planType"),
            )
            assertEquals(
                "cloudCycleLoopPlan",
                pullResult.getJSONObject("loopPlan").getString("planType"),
            )
            assertEquals(
                "applyRemoteBatch",
                pullResult.getJSONObject("loopPlan").getString("finalAction"),
            )
            assertEquals(
                "cloudProviderDiagnostics",
                pullResult.getJSONObject("providerDiagnostics").getString("planType"),
            )
            assertEquals(
                "success",
                pullResult.getJSONObject("providerDiagnostics").getString("cycleClassification"),
            )
            assertEquals(
                "compatible",
                pullResult.getJSONObject("providerDiagnostics").getString("compatibilityClassification"),
            )
            assertEquals(
                "cloudObservabilityEvent",
                pullResult.getJSONObject("observabilityEvent").getString("planType"),
            )
            assertEquals(
                "success",
                pullResult.getJSONObject("observabilityEvent").getString("classification"),
            )
            assertTrue(pullResult.getJSONObject("observabilityEvent").getBoolean("redactedExportAllowed"))
            assertEquals(
                "Remote Local Folder",
                queryScalarString(
                    localDatabaseFile,
                    "SELECT title FROM browser_history_pages WHERE address = 'https://example.com/android-local-folder-remote'",
                ),
            )
            assertEquals(
                "1",
                queryScalarString(
                    localDatabaseFile,
                    "SELECT COUNT(*) FROM browser_history_visits WHERE address = 'https://example.com/android-local-folder-remote'",
                ),
            )

            val manifestFile = File(cloudRoot, "Actrium/BrowserSync/manifest.json")
            val manifest = JSONObject(manifestFile.readText())
                .put("storageSchemaVersion", ActriumCore.browserStorageSchemaVersion().toLong() + 1)
            manifestFile.writeText(manifest.toString())

            val blockedResult = JSONObject(
                localClient.runLocalFolderSyncCycle(
                    rootDir = cloudRoot.absolutePath,
                    scope = "profile",
                    deviceId = "device-android",
                    generatedAtUnixSeconds = 1730,
                    limitPerCollection = 20,
                    observedAtUnixSeconds = 1730,
                ),
            )
            assertEquals("clientUpgradeRequired", blockedResult.getString("classification"))
            assertFalse(blockedResult.getBoolean("applied"))
            assertTrue(blockedResult.isNull("cycleRound"))
            assertTrue(blockedResult.isNull("compactionRound"))
            assertTrue(blockedResult.isNull("cleanupRound"))
            assertEquals(
                "cloudCompatibilityPreflight",
                blockedResult.getJSONObject("compatibilityPreflight").getString("planType"),
            )
            assertEquals(
                "clientUpgradeRequired",
                blockedResult.getJSONObject("compatibilityPreflight").getString("classification"),
            )
            assertTrue(blockedResult.getJSONObject("compatibilityPreflight").getBoolean("shouldBlockSync"))
            val schemaRolloutPlan = blockedResult.getJSONObject("schemaRolloutPlan")
            assertEquals("cloudSchemaRolloutPlan", schemaRolloutPlan.getString("planType"))
            assertEquals("clientUpgradeRequired", schemaRolloutPlan.getString("classification"))
            assertTrue(schemaRolloutPlan.getBoolean("shouldBlockSync"))
            val schemaMigrationPlaybook = blockedResult.getJSONObject("schemaMigrationPlaybook")
            assertEquals("cloudSchemaMigrationPlaybook", schemaMigrationPlaybook.getString("planType"))
            assertEquals("clientUpgradeRequired", schemaMigrationPlaybook.getString("classification"))
            assertTrue(schemaMigrationPlaybook.getBoolean("requiresClientUpgrade"))
            assertFalse(schemaMigrationPlaybook.getBoolean("requiresProviderBackup"))
            val recoverySnapshotRound = blockedResult.getJSONObject("recoverySnapshotRound")
            assertEquals("recoverySnapshotRound", recoverySnapshotRound.getString("planType"))
            assertTrue(recoverySnapshotRound.getBoolean("completed"))
            assertEquals("upgradeClient", recoverySnapshotRound.getString("clientAction"))
            assertEquals(
                "cloudRecoverySnapshotResult",
                recoverySnapshotRound.getJSONObject("resultPlan").getString("planType"),
            )
            assertFalse(recoverySnapshotRound.getJSONObject("resultPlan").getBoolean("snapshotRequested"))
            assertEquals(
                "clientUpgradeRequired",
                blockedResult.getJSONObject("providerDiagnostics").getString("classification"),
            )
            assertEquals(
                "clientUpgradeRequired",
                blockedResult.getJSONObject("providerDiagnostics").getString("compatibilityClassification"),
            )
        } finally {
            deleteDatabaseFiles(remoteDatabaseFile)
            deleteDatabaseFiles(localDatabaseFile)
            testRoot.deleteRecursively()
        }
    }

    @Test
    fun syncClientRunsLocalFolderCloudMembershipRound() = runBlocking {
        val testRoot = File(context.cacheDir, "actrium-browser-sync-membership-${System.nanoTime()}")
        val databaseFile = File(testRoot, "browser.db")
        val cloudRoot = File(testRoot, "cloud")

        try {
            val database = BrowserStorageDatabase.createAtPath(databaseFile.absolutePath)
            val client = BrowserStorageSyncClient(database)
            val joinRequestJson = JSONObject()
                .put("scope", "profile")
                .put("actorDeviceId", "device-android")
                .put("memberDeviceId", "device-android")
                .put("displayName", "Actrium Android")
                .put("platform", "Android")
                .put("membershipAction", "connect")
                .put("generatedAtUnixSeconds", 1810)
                .toString()
            val joinResult = JSONObject(
                client.runLocalFolderMembershipRound(
                    rootDir = cloudRoot.absolutePath,
                    requestJson = joinRequestJson,
                    observedAtUnixSeconds = 1810,
                ),
            )

            assertEquals("cloudLocalFolderMembership", joinResult.getString("endpointType"))
            assertEquals("localFolder", joinResult.getString("providerKind"))
            assertEquals(
                "ready",
                joinResult.getJSONObject("capabilityPreflight").getString("classification"),
            )
            assertEquals("success", joinResult.getString("classification"))
            assertEquals(
                "compatible",
                joinResult.getJSONObject("compatibilityPreflight").getString("classification"),
            )
            assertEquals(0, joinResult.getJSONObject("compatibilityPreflight").getInt("documentCount"))
            assertTrue(joinResult.isNull("schemaRolloutPlan"))
            assertTrue(joinResult.isNull("schemaMigrationPlaybook"))
            assertTrue(joinResult.isNull("recoverySnapshotRound"))
            assertEquals(
                "cloudMembershipStatusPolicy",
                joinResult.getJSONObject("statusPolicy").getString("planType"),
            )
            assertEquals("success", joinResult.getJSONObject("statusPolicy").getString("classification"))
            assertEquals(
                "finishCloudDeviceJoined",
                joinResult.getJSONObject("statusPolicy").getString("clientAction"),
            )
            assertEquals(
                "cloudMembershipScheduleState",
                joinResult.getJSONObject("scheduleState").getString("planType"),
            )
            assertEquals("success", joinResult.getJSONObject("scheduleState").getString("classification"))
            assertEquals("idle", joinResult.getJSONObject("scheduleState").getString("schedulerAction"))
            assertEquals(0, joinResult.getJSONObject("scheduleState").getInt("nextRetryAttempt"))
            assertEquals(
                "cloudMembershipAttemptPlan",
                joinResult.getJSONObject("attemptPlan").getString("planType"),
            )
            assertEquals("success", joinResult.getJSONObject("attemptPlan").getString("classification"))
            assertEquals(
                "finishCloudDeviceJoined",
                joinResult.getJSONObject("attemptPlan").getString("attemptAction"),
            )
            assertTrue(joinResult.getJSONObject("attemptPlan").getBoolean("shouldFinishMembership"))
            assertEquals(
                "cloudMembershipLoopPlan",
                joinResult.getJSONObject("loopPlan").getString("planType"),
            )
            assertEquals(1, joinResult.getJSONObject("loopPlan").getInt("roundCount"))
            assertEquals(
                "finishCloudDeviceJoined",
                joinResult.getJSONObject("loopPlan").getString("finalAction"),
            )
            assertEquals(1, joinResult.getJSONObject("loopPlan").getJSONObject("loopSummary").getInt("attemptCount"))
            assertEquals(
                1,
                joinResult.getJSONObject("loopPlan").getJSONObject("loopSummary").getInt("completedAttemptCount"),
            )
            assertEquals(
                1,
                joinResult.getJSONObject("loopPlan").getJSONObject("loopSummary")
                    .getInt("finishMembershipAttemptCount"),
            )
            assertEquals(
                "finishCloudDeviceJoined",
                joinResult.getJSONObject("loopPlan").getJSONObject("loopSummary").getString("finalAction"),
            )
            assertEquals(
                "cloudProviderDiagnostics",
                joinResult.getJSONObject("providerDiagnostics").getString("planType"),
            )
            assertEquals(
                "compatible",
                joinResult.getJSONObject("providerDiagnostics").getString("compatibilityClassification"),
            )
            assertEquals(
                "success",
                joinResult.getJSONObject("providerDiagnostics").getString("scheduleClassification"),
            )
            assertEquals(
                "cloudObservabilityEvent",
                joinResult.getJSONObject("observabilityEvent").getString("planType"),
            )
            assertEquals(
                "finishCloudDeviceJoined",
                joinResult.getJSONObject("observabilityEvent").getString("finalAction"),
            )
            assertTrue(joinResult.getBoolean("completed"))
            assertFalse(joinResult.getBoolean("shouldRetry"))
            assertFalse(joinResult.getBoolean("shouldReread"))
            assertEquals("finishCloudDeviceJoined", joinResult.getString("clientAction"))
            assertEquals(
                "membershipRound",
                joinResult.getJSONObject("membershipRound").getString("planType"),
            )
            assertEquals(
                "join",
                joinResult.getJSONObject("membershipRound").getString("membershipAction"),
            )
            assertTrue(File(cloudRoot, "Actrium/BrowserSync/manifest.json").exists())
            assertTrue(File(cloudRoot, "Actrium/BrowserSync/devices/device-android.json").exists())

            val revokeRequestJson = JSONObject()
                .put("scope", "profile")
                .put("actorDeviceId", "device-android")
                .put("memberDeviceId", "device-android")
                .put("membershipAction", "revoke")
                .put("generatedAtUnixSeconds", 1830)
                .toString()
            val revokeResult = JSONObject(
                client.runLocalFolderMembershipRound(
                    rootDir = cloudRoot.absolutePath,
                    requestJson = revokeRequestJson,
                    observedAtUnixSeconds = 1830,
                ),
            )

            assertEquals("cloudLocalFolderMembership", revokeResult.getString("endpointType"))
            assertEquals("success", revokeResult.getString("classification"))
            assertEquals(
                "compatible",
                revokeResult.getJSONObject("compatibilityPreflight").getString("classification"),
            )
            assertTrue(revokeResult.getJSONObject("compatibilityPreflight").getInt("documentCount") > 0)
            assertEquals(
                "compatible",
                revokeResult.getJSONObject("providerDiagnostics").getString("compatibilityClassification"),
            )
            assertEquals(
                "cloudObservabilityEvent",
                revokeResult.getJSONObject("observabilityEvent").getString("planType"),
            )
            assertTrue(revokeResult.isNull("schemaRolloutPlan"))
            assertTrue(revokeResult.isNull("schemaMigrationPlaybook"))
            assertTrue(revokeResult.isNull("recoverySnapshotRound"))
            assertEquals(
                "cloudMembershipStatusPolicy",
                revokeResult.getJSONObject("statusPolicy").getString("planType"),
            )
            assertEquals("success", revokeResult.getJSONObject("statusPolicy").getString("classification"))
            assertEquals(
                "finishCloudDeviceRevoked",
                revokeResult.getJSONObject("statusPolicy").getString("clientAction"),
            )
            assertEquals(
                "cloudMembershipScheduleState",
                revokeResult.getJSONObject("scheduleState").getString("planType"),
            )
            assertEquals("success", revokeResult.getJSONObject("scheduleState").getString("classification"))
            assertEquals("idle", revokeResult.getJSONObject("scheduleState").getString("schedulerAction"))
            assertEquals(
                "cloudMembershipAttemptPlan",
                revokeResult.getJSONObject("attemptPlan").getString("planType"),
            )
            assertEquals("success", revokeResult.getJSONObject("attemptPlan").getString("classification"))
            assertEquals(
                "finishCloudDeviceRevoked",
                revokeResult.getJSONObject("attemptPlan").getString("attemptAction"),
            )
            assertEquals(
                "cloudMembershipLoopPlan",
                revokeResult.getJSONObject("loopPlan").getString("planType"),
            )
            assertEquals(1, revokeResult.getJSONObject("loopPlan").getInt("roundCount"))
            assertEquals(
                "finishCloudDeviceRevoked",
                revokeResult.getJSONObject("loopPlan").getString("finalAction"),
            )
            assertEquals(1, revokeResult.getJSONObject("loopPlan").getJSONObject("loopSummary").getInt("attemptCount"))
            assertEquals(
                1,
                revokeResult.getJSONObject("loopPlan").getJSONObject("loopSummary")
                    .getInt("finishMembershipAttemptCount"),
            )
            assertEquals(
                "finishCloudDeviceRevoked",
                revokeResult.getJSONObject("loopPlan").getJSONObject("loopSummary").getString("finalAction"),
            )
            assertTrue(revokeResult.getBoolean("completed"))
            assertEquals("finishCloudDeviceRevoked", revokeResult.getString("clientAction"))
            assertEquals(
                "revoke",
                revokeResult.getJSONObject("membershipRound").getString("membershipAction"),
            )
            assertEquals(
                "revoked",
                revokeResult.getJSONObject("membershipRound").getString("memberStatus"),
            )

            val manifest = JSONObject(File(cloudRoot, "Actrium/BrowserSync/manifest.json").readText())
            val devices = manifest.getJSONArray("devices")
            var foundRevokedDevice = false
            for (index in 0 until devices.length()) {
                val device = devices.getJSONObject(index)
                if (
                    device.getString("deviceId") == "device-android" &&
                    device.getString("status") == "revoked"
                ) {
                    foundRevokedDevice = true
                }
            }
            assertTrue(foundRevokedDevice)

            manifest.put("storageSchemaVersion", ActriumCore.browserStorageSchemaVersion().toLong() + 1)
            File(cloudRoot, "Actrium/BrowserSync/manifest.json").writeText(manifest.toString())
            val blockedJoinRequestJson = JSONObject()
                .put("scope", "profile")
                .put("actorDeviceId", "device-android")
                .put("memberDeviceId", "device-android-2")
                .put("displayName", "Actrium Android 2")
                .put("platform", "Android")
                .put("membershipAction", "connect")
                .put("generatedAtUnixSeconds", 1840)
                .toString()
            val blockedJoinResult = JSONObject(
                client.runLocalFolderMembershipRound(
                    rootDir = cloudRoot.absolutePath,
                    requestJson = blockedJoinRequestJson,
                    observedAtUnixSeconds = 1840,
                ),
            )
            assertEquals("clientUpgradeRequired", blockedJoinResult.getString("classification"))
            assertFalse(blockedJoinResult.getBoolean("completed"))
            assertTrue(
                blockedJoinResult.getJSONObject("compatibilityPreflight").getBoolean("shouldBlockSync"),
            )
            assertEquals(
                "clientUpgradeRequired",
                blockedJoinResult.getJSONObject("membershipRound").getString("planningErrorKind"),
            )
            assertEquals(
                "cloudMembershipStatusPolicy",
                blockedJoinResult.getJSONObject("statusPolicy").getString("planType"),
            )
            assertEquals(
                "clientUpgradeRequired",
                blockedJoinResult.getJSONObject("statusPolicy").getString("classification"),
            )
            assertEquals("upgradeClient", blockedJoinResult.getJSONObject("statusPolicy").getString("clientAction"))
            assertEquals(
                "cloudMembershipScheduleState",
                blockedJoinResult.getJSONObject("scheduleState").getString("planType"),
            )
            assertEquals(
                "clientUpgradeRequired",
                blockedJoinResult.getJSONObject("scheduleState").getString("classification"),
            )
            assertTrue(blockedJoinResult.getJSONObject("scheduleState").getBoolean("requiresUserAction"))
            assertEquals("upgradeClient", blockedJoinResult.getJSONObject("scheduleState").getString("userAction"))
            assertEquals(
                "cloudMembershipAttemptPlan",
                blockedJoinResult.getJSONObject("attemptPlan").getString("planType"),
            )
            assertEquals(
                "clientUpgradeRequired",
                blockedJoinResult.getJSONObject("attemptPlan").getString("classification"),
            )
            assertEquals("upgradeClient", blockedJoinResult.getJSONObject("attemptPlan").getString("attemptAction"))
            assertTrue(blockedJoinResult.getJSONObject("attemptPlan").getBoolean("requiresUserAction"))
            assertTrue(blockedJoinResult.getJSONObject("attemptPlan").getBoolean("shouldStopUntilRecovery"))
            assertEquals(
                "cloudMembershipLoopPlan",
                blockedJoinResult.getJSONObject("loopPlan").getString("planType"),
            )
            assertEquals(1, blockedJoinResult.getJSONObject("loopPlan").getInt("roundCount"))
            assertEquals("upgradeClient", blockedJoinResult.getJSONObject("loopPlan").getString("finalAction"))
            assertTrue(blockedJoinResult.getJSONObject("loopPlan").getBoolean("shouldStopUntilRecovery"))
            assertEquals(
                1,
                blockedJoinResult.getJSONObject("loopPlan").getJSONObject("loopSummary").getInt("attemptCount"),
            )
            assertEquals(
                1,
                blockedJoinResult.getJSONObject("loopPlan").getJSONObject("loopSummary")
                    .getInt("stopUntilRecoveryAttemptCount"),
            )
            assertEquals(
                "upgradeClient",
                blockedJoinResult.getJSONObject("loopPlan").getJSONObject("loopSummary").getString("finalAction"),
            )
            assertEquals(
                "clientUpgradeRequired",
                blockedJoinResult.getJSONObject("loopPlan").getJSONObject("loopSummary")
                    .getString("finalClassification"),
            )
            assertEquals(
                "clientUpgradeRequired",
                blockedJoinResult.getJSONObject("providerDiagnostics").getString("classification"),
            )
            assertEquals(
                "clientUpgradeRequired",
                blockedJoinResult.getJSONObject("providerDiagnostics").getString("compatibilityClassification"),
            )
            assertEquals(
                "clientUpgradeRequired",
                blockedJoinResult.getJSONObject("providerDiagnostics").getString("scheduleClassification"),
            )
            assertEquals(
                "cloudObservabilityEvent",
                blockedJoinResult.getJSONObject("observabilityEvent").getString("planType"),
            )
            val blockedMembershipRollout = blockedJoinResult.getJSONObject("schemaRolloutPlan")
            assertEquals("cloudSchemaRolloutPlan", blockedMembershipRollout.getString("planType"))
            assertEquals("clientUpgradeRequired", blockedMembershipRollout.getString("classification"))
            val blockedMembershipPlaybook = blockedJoinResult.getJSONObject("schemaMigrationPlaybook")
            assertEquals("cloudSchemaMigrationPlaybook", blockedMembershipPlaybook.getString("planType"))
            assertEquals("clientUpgradeRequired", blockedMembershipPlaybook.getString("classification"))
            assertFalse(blockedMembershipPlaybook.getBoolean("requiresProviderBackup"))
            val blockedMembershipRecoveryRound = blockedJoinResult.getJSONObject("recoverySnapshotRound")
            assertEquals("recoverySnapshotRound", blockedMembershipRecoveryRound.getString("planType"))
            assertTrue(blockedMembershipRecoveryRound.getBoolean("completed"))
            assertEquals("upgradeClient", blockedMembershipRecoveryRound.getString("clientAction"))
            assertFalse(blockedMembershipRecoveryRound.getJSONObject("resultPlan").getBoolean("snapshotRequested"))
            assertFalse(File(cloudRoot, "Actrium/BrowserSync/devices/device-android-2.json").exists())
        } finally {
            deleteDatabaseFiles(databaseFile)
            testRoot.deleteRecursively()
        }
    }

    @Test
    fun syncCoordinatorSkipsCycleWhenCredentialsUnavailable() = runBlocking {
        deleteDatabaseFiles()
        val database = BrowserStorageDatabase.createDefault(context)
        val transport = FakeBrowserStorageSyncTransport("{}")
        val coordinator = BrowserStorageSyncCoordinator(
            client = BrowserStorageSyncClient(database, transport),
            credentialsProvider = FakeBrowserStorageSyncCredentialsProvider(null),
        )

        val result = JSONObject(
            coordinator.runSyncCycle(
                generatedAtUnixSeconds = 1400,
                limitPerCollection = 20,
                observedAtUnixSeconds = 1500,
            ),
        )

        assertFalse(result.getBoolean("credentialsAvailable"))
        assertEquals("credentialsUnavailable", result.getString("classification"))
        assertFalse(result.getBoolean("shouldRetry"))
        assertEquals("refreshCredentials", result.getString("clientAction"))
        assertEquals("waitForCredentials", result.getJSONObject("statusPolicy").getString("schedulerAction"))
        assertEquals("refreshUserSession", result.getJSONObject("statusPolicy").getString("credentialAction"))
        assertEquals("waitForCredentials", result.getJSONObject("scheduleState").getString("schedulerAction"))
        assertEquals(0, result.getJSONObject("scheduleState").getInt("nextRetryAttempt"))
        assertTrue(result.getJSONObject("scheduleState").isNull("nextRunAtUnixSeconds"))
        assertTrue(result.getJSONObject("scheduleState").getBoolean("waitingForCredentials"))
        assertNull(transport.lastRequest)
    }

    @Test
    fun syncCoordinatorRunsClientWithAvailableCredentials() = runBlocking {
        deleteDatabaseFiles()
        val database = BrowserStorageDatabase.createDefault(context)
        val cycleResponseJson = ActriumCore.browserStorageSyncCycleResponseJson(
            ActriumCore.emptyBrowserStorageSyncBatchJson(
                scope = "profile",
                deviceId = "device-android",
                generatedAtUnixSeconds = 1410,
            ),
            JSONObject()
                .put("version", 1)
                .put("storageSchemaVersion", 6)
                .put("scope", "profile")
                .put("deviceId", "device-android")
                .put("syncedAtUnixSeconds", 1411)
                .put("records", org.json.JSONArray())
                .put("cursors", org.json.JSONArray())
                .toString(),
        )
        val transport = FakeBrowserStorageSyncTransport(cycleResponseJson)
        val coordinator = BrowserStorageSyncCoordinator(
            client = BrowserStorageSyncClient(database, transport),
            credentialsProvider = FakeBrowserStorageSyncCredentialsProvider(
                BrowserStorageSyncCredentials(
                    baseUrl = "http://127.0.0.1:8787/",
                    accessToken = "token-android-coordinator",
                    scope = "profile",
                    deviceId = "device-android",
                ),
            ),
        )

        val result = JSONObject(
            coordinator.runSyncCycle(
                generatedAtUnixSeconds = 1412,
                limitPerCollection = 20,
                retryAttempt = 3,
                observedAtUnixSeconds = 1512,
            ),
        )

        assertTrue(result.getBoolean("credentialsAvailable"))
        assertEquals("success", result.getString("classification"))
        assertEquals("Bearer token-android-coordinator", transport.lastRequest?.headers?.get("Authorization"))
        assertEquals("idle", result.getJSONObject("statusPolicy").getString("schedulerAction"))
        assertTrue(result.getJSONObject("statusPolicy").getBoolean("resetRetryAttempt"))
        assertEquals("idle", result.getJSONObject("scheduleState").getString("schedulerAction"))
        assertEquals(0, result.getJSONObject("scheduleState").getInt("nextRetryAttempt"))
        assertTrue(result.getJSONObject("scheduleState").isNull("nextRunAtUnixSeconds"))
        assertEquals("success", result.getJSONObject("syncResult").getString("classification"))
    }

    @Test
    fun syncCoordinatorSchedulesRetryFromStatusPolicy() = runBlocking {
        deleteDatabaseFiles()
        val database = BrowserStorageDatabase.createDefault(context)
        val transport = FakeBrowserStorageSyncTransport("", statusCode = 503)
        val coordinator = BrowserStorageSyncCoordinator(
            client = BrowserStorageSyncClient(database, transport),
            credentialsProvider = FakeBrowserStorageSyncCredentialsProvider(
                BrowserStorageSyncCredentials(
                    baseUrl = "http://127.0.0.1:8787/",
                    accessToken = "token-android-retry",
                    scope = "profile",
                    deviceId = "device-android",
                ),
            ),
        )

        val result = JSONObject(
            coordinator.runSyncCycle(
                generatedAtUnixSeconds = 1420,
                limitPerCollection = 20,
                retryAttempt = 2,
                observedAtUnixSeconds = 2000,
            ),
        )

        assertEquals("serverUnavailable", result.getString("classification"))
        assertEquals("scheduleRetry", result.getJSONObject("statusPolicy").getString("schedulerAction"))
        assertEquals(3, result.getJSONObject("scheduleState").getInt("nextRetryAttempt"))
        assertEquals(20, result.getJSONObject("scheduleState").getInt("nextRunDelaySeconds"))
        assertEquals(2020, result.getJSONObject("scheduleState").getLong("nextRunAtUnixSeconds"))
    }

    @Test
    fun syncSchedulerConsumesScheduleStateAndResetsAfterSuccess() = runBlocking {
        deleteDatabaseFiles()
        val database = BrowserStorageDatabase.createDefault(context)
        val transport = FakeBrowserStorageSyncTransport("", statusCode = 503)
        val coordinator = BrowserStorageSyncCoordinator(
            client = BrowserStorageSyncClient(database, transport),
            credentialsProvider = FakeBrowserStorageSyncCredentialsProvider(
                BrowserStorageSyncCredentials(
                    baseUrl = "http://127.0.0.1:8787/",
                    accessToken = "token-android-scheduler",
                    scope = "profile",
                    deviceId = "device-android",
                ),
            ),
        )
        val scheduler = BrowserStorageSyncScheduler(coordinator, this)

        scheduler.runNow(
            generatedAtUnixSeconds = 1430,
            limitPerCollection = 20,
            observedAtUnixSeconds = 3000,
        )
        val retrySnapshot = scheduler.snapshot()
        assertEquals("scheduleRetry", retrySnapshot.schedulerAction)
        assertEquals(1, retrySnapshot.retryAttempt)
        assertEquals(5L, retrySnapshot.nextRunDelaySeconds)
        assertEquals(3005L, retrySnapshot.nextRunAtUnixSeconds)
        assertTrue(retrySnapshot.pendingRunScheduled)

        val retryStatus = JSONObject(scheduler.statusJson())
        assertEquals("retryScheduled", retryStatus.getString("status"))
        assertEquals("serverUnavailable", retryStatus.getString("lastClassification"))
        assertTrue(retryStatus.getBoolean("pendingRunScheduled"))

        scheduler.cancelPendingSync()

        transport.statusCode = 200
        transport.responseJson = ActriumCore.browserStorageSyncCycleResponseJson(
            ActriumCore.emptyBrowserStorageSyncBatchJson(
                scope = "profile",
                deviceId = "device-android",
                generatedAtUnixSeconds = 1440,
            ),
            JSONObject()
                .put("version", 1)
                .put("storageSchemaVersion", 6)
                .put("scope", "profile")
                .put("deviceId", "device-android")
                .put("syncedAtUnixSeconds", 1441)
                .put("records", org.json.JSONArray())
                .put("cursors", org.json.JSONArray())
                .toString(),
        )

        scheduler.runNow(
            generatedAtUnixSeconds = 1442,
            limitPerCollection = 20,
            observedAtUnixSeconds = 3010,
        )
        val successSnapshot = scheduler.snapshot()
        assertEquals("idle", successSnapshot.schedulerAction)
        assertEquals(0, successSnapshot.retryAttempt)
        assertNull(successSnapshot.nextRunAtUnixSeconds)
        assertFalse(successSnapshot.pendingRunScheduled)

        val successStatus = JSONObject(scheduler.statusJson())
        assertEquals("idle", successStatus.getString("status"))
        assertEquals(0, successStatus.getInt("retryAttempt"))
        assertFalse(successStatus.getBoolean("pendingRunScheduled"))
    }

    @Test
    fun syncCoordinatorReadsStoredNativeCredentials() = runBlocking {
        deleteDatabaseFiles()
        val database = BrowserStorageDatabase.createDefault(context)
        val cycleResponseJson = ActriumCore.browserStorageSyncCycleResponseJson(
            ActriumCore.emptyBrowserStorageSyncBatchJson(
                scope = "profile",
                deviceId = "device-android",
                generatedAtUnixSeconds = 1420,
            ),
            JSONObject()
                .put("version", 1)
                .put("storageSchemaVersion", 6)
                .put("scope", "profile")
                .put("deviceId", "device-android")
                .put("syncedAtUnixSeconds", 1421)
                .put("records", org.json.JSONArray())
                .put("cursors", org.json.JSONArray())
                .toString(),
        )
        val store = BrowserStorageSyncCredentialStore(
            context,
            preferencesName = "actrium_browser_sync_credentials_test",
        )
        store.clearSession()
        store.saveSession(
            environment = ActriumCore.authEnvironmentProd(),
            accessToken = "token-android-stored",
            baseUrl = "http://127.0.0.1:8787/",
            scope = "profile",
        )
        val transport = FakeBrowserStorageSyncTransport(cycleResponseJson)
        val coordinator = BrowserStorageSyncCoordinator(
            client = BrowserStorageSyncClient(database, transport),
            credentialsProvider = store,
        )

        val result = JSONObject(
            coordinator.runSyncCycle(
                generatedAtUnixSeconds = 1422,
                limitPerCollection = 20,
            ),
        )

        assertTrue(result.getBoolean("credentialsAvailable"))
        assertEquals("success", result.getString("classification"))
        assertEquals("Bearer token-android-stored", transport.lastRequest?.headers?.get("Authorization"))

        store.clearSession()
    }

    @Test
    fun providerSettingsStoreRoundTripsNormalizesAndTracksMembership() {
        val storeFile = File(context.cacheDir, "provider-settings-${System.nanoTime()}.json")
        try {
            val store = BrowserStorageSyncProviderSettingsStore(storeFile) { "adopted-device-id" }

            // First load mints and persists the durable identity, adopting the
            // provided (endpoint) device id, and is not configured yet.
            val initial = store.load()
            assertEquals("adopted-device-id", initial.deviceId)
            assertFalse(initial.isConfigured)
            assertNull(initial.providerKind)
            assertTrue(storeFile.exists())

            // A relative root is rejected: it degrades to not-configured instead
            // of syncing into a guessed working-directory-relative folder.
            val relative = store.save(store.createSnapshot("localFolder", "relative/path"))
            assertFalse(relative.isConfigured)
            assertNull(relative.localFolderRootPath)

            // An unknown provider kind is contract drift and normalizes away.
            assertNull(store.save(store.createSnapshot("dropbox", "/abs/path")).providerKind)

            // An absolute local-folder root configures the provider; the device
            // id is stable across writes.
            val configured = store.configureLocalFolder("/data/local/tmp/actrium-sync")
            assertTrue(configured.isConfigured)
            assertEquals("localFolder", configured.providerKind)
            assertEquals("/data/local/tmp/actrium-sync", configured.localFolderRootPath)
            assertEquals("adopted-device-id", configured.deviceId)
            assertNull(configured.membershipJoinedAtUnixSeconds)

            // The membership marker persists and survives a fresh store instance
            // reading the same file (no re-mint of identity).
            store.saveMembershipJoined(4242)
            val reopened = BrowserStorageSyncProviderSettingsStore(storeFile) { "should-not-be-used" }
            val reloaded = reopened.load()
            assertEquals(4242L, reloaded.membershipJoinedAtUnixSeconds)
            assertEquals("adopted-device-id", reloaded.deviceId)

            // Disconnect stops provider sync and clears the marker but keeps the
            // durable device identity for a later reconnect.
            val disconnected = store.disconnect()
            assertFalse(disconnected.isConfigured)
            assertNull(disconnected.providerKind)
            assertNull(disconnected.membershipJoinedAtUnixSeconds)
            assertEquals("adopted-device-id", disconnected.deviceId)
        } finally {
            storeFile.delete()
        }
    }

    @Test
    fun syncCoordinatorRoutesLocalFolderProviderMembershipThenCycle() = runBlocking {
        val testRoot = File(context.cacheDir, "actrium-provider-coordinator-${System.nanoTime()}")
        val databaseFile = File(testRoot, "browser.db")
        val cloudRoot = File(testRoot, "cloud")
        val storeFile = File(testRoot, "browser-sync-provider.json")
        try {
            testRoot.mkdirs()
            val database = BrowserStorageDatabase.createAtPath(databaseFile.absolutePath)
            assertTrue(
                database.recordHistoryVisit(
                    BrowserHistoryItem(
                        title = "Provider Coordinator",
                        address = "https://example.com/android-provider-coordinator",
                        visitedAtUnixSeconds = 1800,
                    ),
                ),
            )

            val providerStore = BrowserStorageSyncProviderSettingsStore(storeFile) { "device-android-provider" }
            providerStore.configureLocalFolder(cloudRoot.absolutePath)

            // Null endpoint credentials on purpose: if the provider branch did
            // not handle the cycle, the coordinator would fall through and
            // report credentialsUnavailable, and the endpoint transport would be
            // hit. Neither must happen.
            val transport = FakeBrowserStorageSyncTransport("{}")
            val coordinator = BrowserStorageSyncCoordinator(
                client = BrowserStorageSyncClient(database, transport),
                credentialsProvider = FakeBrowserStorageSyncCredentialsProvider(null),
                providerSettingsStore = providerStore,
            )

            // First run: membership is missing, so the coordinator joins then
            // runs the cycle in a single call and returns the cycle envelope.
            val firstResult = JSONObject(
                coordinator.runSyncCycle(
                    generatedAtUnixSeconds = 1810,
                    limitPerCollection = 20,
                    observedAtUnixSeconds = 1810,
                ),
            )
            assertEquals("cloudLocalFolderCycle", firstResult.getString("endpointType"))
            assertEquals("localFolder", firstResult.getString("providerKind"))
            assertTrue(firstResult.getBoolean("credentialsAvailable"))
            assertEquals("success", firstResult.getString("classification"))
            // The lifted scheduleState is scheduler-shaped (carries the field
            // the scheduler reads), not left as an opaque cloud round shape.
            assertTrue(firstResult.getJSONObject("scheduleState").has("schedulerAction"))
            assertNull(transport.lastRequest)

            // The membership marker is now persisted so later runs skip the join.
            assertNotNull(providerStore.load().membershipJoinedAtUnixSeconds)
            assertTrue(File(cloudRoot, "Actrium/BrowserSync/manifest.json").exists())

            // Second run: membership active, routes straight to the cycle round.
            val secondResult = JSONObject(
                coordinator.runSyncCycle(
                    generatedAtUnixSeconds = 1820,
                    limitPerCollection = 20,
                    observedAtUnixSeconds = 1820,
                ),
            )
            assertEquals("cloudLocalFolderCycle", secondResult.getString("endpointType"))
            assertEquals("success", secondResult.getString("classification"))
            assertNull(transport.lastRequest)
        } finally {
            deleteDatabaseFiles(databaseFile)
            testRoot.deleteRecursively()
        }
    }

    private fun createLegacyBrowserStorageDatabase(schemaVersion: Int) {
        val syncColumns = if (schemaVersion >= 3) {
            """
            ,
                sync_state TEXT NOT NULL DEFAULT 'local',
                sync_revision INTEGER NOT NULL DEFAULT 0,
                origin_device_id TEXT NOT NULL DEFAULT '',
                last_synced_at INTEGER,
                deleted_at INTEGER
            """.trimIndent()
        } else {
            ""
        }
        val tabIdColumn = if (schemaVersion >= 2) {
            "tab_id TEXT NOT NULL DEFAULT '',"
        } else {
            ""
        }

        databaseFile().parentFile?.mkdirs()
        SQLiteDatabase.openOrCreateDatabase(databaseFile(), null).use { database ->
            database.execute("PRAGMA foreign_keys = OFF")
            database.execute(
                """
                CREATE TABLE browser_schema_meta (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                )
                """.trimIndent(),
            )
            database.execute(
                "INSERT INTO browser_schema_meta (key, value) VALUES ('browser_storage_schema_version', '$schemaVersion')",
            )

            database.execute(
                """
                CREATE TABLE browser_history_pages (
                    address TEXT PRIMARY KEY,
                    title TEXT NOT NULL,
                    visit_count INTEGER NOT NULL DEFAULT 1,
                    last_visited_at INTEGER NOT NULL,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    favicon TEXT$syncColumns
                )
                """.trimIndent(),
            )
            database.execute(
                """
                CREATE TABLE browser_history_visits (
                    id TEXT PRIMARY KEY,
                    address TEXT NOT NULL,
                    title TEXT NOT NULL,
                    visited_at INTEGER NOT NULL,
                    transition TEXT$syncColumns
                )
                """.trimIndent(),
            )
            database.execute(
                """
                CREATE TABLE browser_bookmark_folders (
                    id TEXT PRIMARY KEY,
                    parent_id TEXT,
                    title TEXT NOT NULL,
                    sort_order INTEGER NOT NULL DEFAULT 0,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL$syncColumns
                )
                """.trimIndent(),
            )
            database.execute(
                """
                CREATE TABLE browser_bookmarks (
                    id TEXT PRIMARY KEY,
                    folder_id TEXT,
                    title TEXT NOT NULL,
                    address TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    sort_order INTEGER NOT NULL DEFAULT 0,
                    favicon TEXT,
                    is_deleted INTEGER NOT NULL DEFAULT 0$syncColumns
                )
                """.trimIndent(),
            )
            database.execute(
                """
                CREATE TABLE browser_sessions (
                    id TEXT PRIMARY KEY,
                    active_tab_index INTEGER NOT NULL DEFAULT 0,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL$syncColumns
                )
                """.trimIndent(),
            )
            database.execute(
                """
                CREATE TABLE browser_session_tabs (
                    session_id TEXT NOT NULL,
                    tab_index INTEGER NOT NULL,
                    $tabIdColumn
                    title TEXT NOT NULL,
                    address TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL$syncColumns,
                    PRIMARY KEY (session_id, tab_index)
                )
                """.trimIndent(),
            )

            database.execute(
                """
                INSERT INTO browser_history_pages (address, title, visit_count, last_visited_at, created_at, updated_at, favicon)
                VALUES ('https://example.com/history-v$schemaVersion', 'History v$schemaVersion', 2, ${100 + schemaVersion}, ${90 + schemaVersion}, ${100 + schemaVersion}, NULL)
                """.trimIndent(),
            )
            database.execute(
                """
                INSERT INTO browser_history_visits (id, address, title, visited_at, transition)
                VALUES ('visit-v$schemaVersion', 'https://example.com/history-v$schemaVersion', 'History v$schemaVersion', ${100 + schemaVersion}, 'typed')
                """.trimIndent(),
            )
            database.execute(
                """
                INSERT INTO browser_bookmarks (id, folder_id, title, address, created_at, updated_at, sort_order, favicon, is_deleted)
                VALUES ('bookmark-v$schemaVersion', NULL, 'Bookmark v$schemaVersion', 'https://example.com/bookmark-v$schemaVersion', ${200 + schemaVersion}, ${200 + schemaVersion}, 0, NULL, 0)
                """.trimIndent(),
            )
            database.execute(
                """
                INSERT INTO browser_sessions (id, active_tab_index, created_at, updated_at)
                VALUES ('default', 0, ${300 + schemaVersion}, ${300 + schemaVersion})
                """.trimIndent(),
            )

            if (schemaVersion >= 2) {
                database.execute(
                    """
                    INSERT INTO browser_session_tabs (session_id, tab_index, tab_id, title, address, created_at, updated_at)
                    VALUES ('default', 0, 'tab-v$schemaVersion', 'Session v$schemaVersion', 'https://example.com/session-v$schemaVersion', ${300 + schemaVersion}, ${300 + schemaVersion})
                    """.trimIndent(),
                )
            } else {
                database.execute(
                    """
                    INSERT INTO browser_session_tabs (session_id, tab_index, title, address, created_at, updated_at)
                    VALUES ('default', 0, 'Session v$schemaVersion', 'https://example.com/session-v$schemaVersion', ${300 + schemaVersion}, ${300 + schemaVersion})
                    """.trimIndent(),
                )
            }
        }
    }

    private fun assertBrowserStorageV6Shape(legacyVersion: Int) {
        assertEquals(
            "6",
            queryScalarString(
                "SELECT value FROM browser_schema_meta WHERE key = 'browser_storage_schema_version'",
            ),
        )
        assertTrue(tableExists("browser_sync_cursors"))
        assertTrue(indexExists("idx_browser_sync_cursors_updated"))
        assertTrue(indexExists("idx_browser_session_tabs_tab_id"))

        listOf(
            "browser_history_pages",
            "browser_history_visits",
            "browser_bookmark_folders",
            "browser_bookmarks",
            "browser_sessions",
            "browser_session_tabs",
        ).forEach { table ->
            assertTrue(columnExists(table, "sync_state"))
            assertTrue(columnExists(table, "sync_revision"))
            assertTrue(columnExists(table, "origin_device_id"))
            assertTrue(columnExists(table, "last_synced_at"))
            assertTrue(columnExists(table, "deleted_at"))
        }

        assertTrue(columnExists("browser_session_tabs", "tab_id"))
        assertTrue(columnExists("browser_bookmarks", "owner_kind"))
        assertTrue(columnExists("browser_bookmarks", "owner_id"))
        assertTrue(columnExists("browser_sessions", "owner_kind"))
        assertTrue(columnExists("browser_sessions", "owner_id"))
        assertTrue(columnExists("browser_sessions", "surface_kind"))
        assertTrue(columnExists("browser_sessions", "surface_id"))
        assertEquals(
            "user",
            queryScalarString("SELECT owner_kind FROM browser_bookmarks WHERE id = 'bookmark-v$legacyVersion'"),
        )
        assertEquals(
            "",
            queryScalarString("SELECT owner_id FROM browser_bookmarks WHERE id = 'bookmark-v$legacyVersion'"),
        )
        assertEquals(
            "device",
            queryScalarString("SELECT owner_kind FROM browser_sessions WHERE id = 'default'"),
        )
        assertEquals(
            "",
            queryScalarString("SELECT owner_id FROM browser_sessions WHERE id = 'default'"),
        )
        assertEquals(
            "browser",
            queryScalarString("SELECT surface_kind FROM browser_sessions WHERE id = 'default'"),
        )
        assertEquals(
            "default",
            queryScalarString("SELECT surface_id FROM browser_sessions WHERE id = 'default'"),
        )
    }

    private fun batchHasRecord(batch: JSONObject, recordType: String, recordId: String? = null): Boolean {
        val records = batch.optJSONArray("records") ?: return false
        for (index in 0 until records.length()) {
            val record = records.getJSONObject(index)
            if (record.optString("recordType") == recordType &&
                (recordId == null || record.optString("recordId") == recordId)
            ) {
                return true
            }
        }
        return false
    }

    private fun batchHasDeletedBookmark(batch: JSONObject, recordId: String): Boolean {
        val records = batch.optJSONArray("records") ?: return false
        for (index in 0 until records.length()) {
            val record = records.getJSONObject(index)
            val payload = record.optJSONObject("payload") ?: continue
            if (record.optString("recordType") == "bookmark" &&
                record.optString("recordId") == recordId &&
                record.has("deletedAtUnixSeconds") &&
                !record.isNull("deletedAtUnixSeconds") &&
                payload.optBoolean("isDeleted", false)
            ) {
                return true
            }
        }
        return false
    }

    private fun batchHasBookmarkOwner(
        batch: JSONObject,
        recordId: String,
        ownerKind: String,
        ownerId: String,
    ): Boolean {
        val records = batch.optJSONArray("records") ?: return false
        for (index in 0 until records.length()) {
            val record = records.getJSONObject(index)
            val payload = record.optJSONObject("payload") ?: continue
            if (record.optString("recordType") == "bookmark" &&
                record.optString("recordId") == recordId &&
                payload.optString("ownerKind") == ownerKind &&
                payload.optString("ownerId") == ownerId
            ) {
                return true
            }
        }
        return false
    }

    private fun tableExists(table: String): Boolean {
        return queryScalarString(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = '$table' LIMIT 1",
        ) == "1"
    }

    private fun indexExists(index: String): Boolean {
        return queryScalarString(
            "SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = '$index' LIMIT 1",
        ) == "1"
    }

    private fun columnExists(table: String, column: String): Boolean {
        return openReadOnlyDatabase().use { database ->
            database.rawQuery("PRAGMA table_info($table)", emptyArray()).use { cursor ->
                cursor.any { it.getString(it.getColumnIndexOrThrow("name")) == column }
            }
        }
    }

    private fun queryScalarString(sql: String): String? {
        return queryScalarString(databaseFile(), sql)
    }

    private fun queryScalarString(databaseFile: File, sql: String): String? {
        return openReadOnlyDatabase(databaseFile).use { database ->
            database.rawQuery(sql, emptyArray()).use { cursor ->
                if (cursor.moveToFirst() && !cursor.isNull(0)) {
                    cursor.getString(0)
                } else {
                    null
                }
            }
        }
    }

    private fun openReadOnlyDatabase(): SQLiteDatabase {
        return openReadOnlyDatabase(databaseFile())
    }

    private fun openReadOnlyDatabase(databaseFile: File): SQLiteDatabase {
        return SQLiteDatabase.openDatabase(
            databaseFile.absolutePath,
            null,
            SQLiteDatabase.OPEN_READONLY,
        )
    }

    private fun deleteDatabaseFiles() {
        context.deleteDatabase(ActriumCore.browserStorageDatabaseName())
        val file = databaseFile()
        listOf(file, File("${file.absolutePath}-shm"), File("${file.absolutePath}-wal")).forEach {
            it.delete()
        }
    }

    private fun deleteDatabaseFiles(file: File) {
        listOf(file, File("${file.absolutePath}-shm"), File("${file.absolutePath}-wal")).forEach {
            it.delete()
        }
    }

    private fun databaseFile(): File {
        return context.getDatabasePath(ActriumCore.browserStorageDatabaseName())
    }

    private fun SQLiteDatabase.execute(sql: String) {
        execSQL(sql)
    }

    private inline fun Cursor.any(predicate: (Cursor) -> Boolean): Boolean {
        while (moveToNext()) {
            if (predicate(this)) return true
        }
        return false
    }
}

private class FakeBrowserStorageSyncTransport(
    var responseJson: String,
    var statusCode: Int = 200
) : BrowserStorageSyncTransport {
    var lastRequest: BrowserStorageSyncHttpRequest? = null

    override suspend fun send(request: BrowserStorageSyncHttpRequest): BrowserStorageSyncHttpResponse {
        lastRequest = request
        return BrowserStorageSyncHttpResponse(statusCode = statusCode, bodyJson = responseJson)
    }
}

private class FakeBrowserStorageSyncCredentialsProvider(
    private val credentials: BrowserStorageSyncCredentials?
) : BrowserStorageSyncCredentialsProvider {
    override suspend fun browserStorageSyncCredentials(): BrowserStorageSyncCredentials? = credentials
}
