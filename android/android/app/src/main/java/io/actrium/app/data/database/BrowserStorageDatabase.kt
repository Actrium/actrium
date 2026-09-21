package io.actrium.app.data.database

import android.content.Context
import android.database.Cursor
import android.database.SQLException
import android.database.sqlite.SQLiteDatabase
import android.database.sqlite.SQLiteStatement
import io.actrium.app.rust.ActriumCore
import io.actrium.app.rust.BrowserStorageRepositorySqlInfo
import io.actrium.app.rust.BrowserStorageSchemaInfo
import io.actrium.app.rust.BrowserStorageSyncSqlInfo
import java.io.File
import java.util.UUID
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject

data class BrowserHistoryItem(
    val title: String,
    val address: String,
    val visitedAtUnixSeconds: Long,
    val visitCount: Int = 1,
    val favicon: String? = null
)

data class BrowserBookmarkItem(
    val id: String = UUID.randomUUID().toString(),
    val folderId: String? = null,
    val title: String,
    val address: String,
    val createdAtUnixSeconds: Long,
    val updatedAtUnixSeconds: Long = createdAtUnixSeconds,
    val sortOrder: Int = 0,
    val favicon: String? = null,
    val ownerKind: String = "user",
    val ownerId: String = ""
)

data class BrowserSessionTab(
    val id: String = UUID.randomUUID().toString(),
    val title: String,
    val address: String,
    val createdAtUnixSeconds: Long,
    val updatedAtUnixSeconds: Long = createdAtUnixSeconds
)

data class BrowserSessionSnapshot(
    val activeTabIndex: Int,
    val tabs: List<BrowserSessionTab>,
    val ownerKind: String = "device",
    val ownerId: String = "",
    val surfaceKind: String = "browser",
    val surfaceId: String = "default"
)

class BrowserStorageDatabase private constructor(
    private val databasePath: String,
    private val schema: BrowserStorageSchemaInfo = ActriumCore.browserStorageSchema(),
    private val sql: BrowserStorageRepositorySqlInfo = ActriumCore.browserStorageRepositorySql(),
    private val syncSql: BrowserStorageSyncSqlInfo = ActriumCore.browserStorageSyncSql()
) {
    fun loadRecentHistory(limit: Int = DEFAULT_LIMIT): List<BrowserHistoryItem> {
        return query(sql.selectHistoryRecent, listOf(limit)) { cursor ->
            BrowserHistoryItem(
                title = cursor.stringValue("title"),
                address = cursor.stringValue("address"),
                visitedAtUnixSeconds = cursor.longValue("last_visited_at"),
                visitCount = cursor.intValue("visit_count"),
                favicon = cursor.optionalStringValue("favicon")
            )
        }
    }

    fun saveHistory(items: List<BrowserHistoryItem>): Boolean {
        return withDatabase { database ->
            database.runTransaction {
                execute(sql.deleteAllHistory) &&
                    items.all { item -> upsertHistoryPage(item) }
            }
        }
    }

    fun upsertHistoryItem(item: BrowserHistoryItem): Boolean {
        return withDatabase { database ->
            database.upsertHistoryPage(item)
        }
    }

    fun recordHistoryVisit(
        item: BrowserHistoryItem,
        transition: String = "typed"
    ): Boolean {
        return withDatabase { database ->
            database.runTransaction {
                upsertHistoryPage(item) &&
                    execute(
                        sql.insertHistoryVisit,
                        listOf(
                            UUID.randomUUID().toString(),
                            item.address,
                            item.title,
                            item.visitedAtUnixSeconds,
                            transition
                        )
                    )
            }
        }
    }

    fun deleteHistory(address: String): Boolean {
        return withDatabase { database ->
            database.execute(sql.deleteHistoryByAddress, listOf(address))
        }
    }

    fun clearHistory(): Boolean {
        return withDatabase { database ->
            database.execute(sql.deleteAllHistory)
        }
    }

    fun loadRecentBookmarks(limit: Int = DEFAULT_LIMIT): List<BrowserBookmarkItem> {
        return query(sql.selectBookmarksRecent, listOf(limit)) { cursor ->
            BrowserBookmarkItem(
                id = cursor.stringValue("id"),
                folderId = cursor.optionalStringValue("folder_id"),
                title = cursor.stringValue("title"),
                address = cursor.stringValue("address"),
                createdAtUnixSeconds = cursor.longValue("created_at"),
                updatedAtUnixSeconds = cursor.longValue("updated_at"),
                sortOrder = cursor.intValue("sort_order"),
                favicon = cursor.optionalStringValue("favicon"),
                ownerKind = cursor.optionalStringValue("owner_kind") ?: "user",
                ownerId = cursor.optionalStringValue("owner_id").orEmpty()
            )
        }
    }

    fun saveBookmarks(items: List<BrowserBookmarkItem>): Boolean {
        return withDatabase { database ->
            database.runTransaction {
                execute(sql.deleteAllBookmarks) &&
                    items.withIndex().all { (index, item) ->
                        upsertBookmark(item.copy(sortOrder = index))
                    }
            }
        }
    }

    fun upsertBookmarkItem(item: BrowserBookmarkItem): Boolean {
        return withDatabase { database ->
            database.upsertBookmark(item)
        }
    }

    fun deleteBookmark(address: String): Boolean {
        return withDatabase { database ->
            database.execute(sql.deleteBookmarkByAddress, listOf(address))
        }
    }

    fun deleteBookmarkById(id: String): Boolean {
        return withDatabase { database ->
            (database.executeForChangedRows(sql.deleteBookmarkById, listOf(id)) ?: 0) > 0
        }
    }

    fun clearBookmarks(): Boolean {
        return withDatabase { database ->
            database.execute(sql.deleteAllBookmarks)
        }
    }

    fun loadSession(id: String = DEFAULT_SESSION_ID): BrowserSessionSnapshot? {
        return withDatabase { database ->
            val sessionRows = database.query(sql.selectSession, listOf(id)) { cursor ->
                BrowserSessionSnapshot(
                    activeTabIndex = cursor.intValue("active_tab_index"),
                    tabs = emptyList(),
                    ownerKind = cursor.optionalStringValue("owner_kind") ?: "device",
                    ownerId = cursor.optionalStringValue("owner_id") ?: "",
                    surfaceKind = cursor.optionalStringValue("surface_kind") ?: "browser",
                    surfaceId = cursor.optionalStringValue("surface_id") ?: "default"
                )
            }
            val session = sessionRows.firstOrNull() ?: return@withDatabase null
            val tabRows = database.query(sql.selectSessionTabs, listOf(id)) { cursor ->
                SessionTabRow(
                    tabId = cursor.optionalStringValue("tab_id"),
                    tabIndex = cursor.intValue("tab_index"),
                    title = cursor.stringValue("title"),
                    address = cursor.stringValue("address"),
                    createdAtUnixSeconds = cursor.longValue("created_at"),
                    updatedAtUnixSeconds = cursor.longValue("updated_at")
                )
            }
            val tabIds = sessionTabIdPlanIds(
                tabRows.map { SessionTabIdRequest(it.tabId, it.tabIndex, it.address) }
            )
            val tabs = tabRows.mapIndexed { position, row ->
                BrowserSessionTab(
                    id = tabIds[position],
                    title = row.title,
                    address = row.address,
                    createdAtUnixSeconds = row.createdAtUnixSeconds,
                    updatedAtUnixSeconds = row.updatedAtUnixSeconds
                )
            }
            session.copy(tabs = tabs)
        }
    }

    fun saveSession(
        id: String = DEFAULT_SESSION_ID,
        snapshot: BrowserSessionSnapshot
    ): Boolean {
        val now = currentUnixSeconds()
        // The Rust core owns whole-snapshot session normalization
        // (browser_session_snapshot_normalize_json): canonical addresses,
        // title fallback/truncation, tab cap, active-index clamp, and stable
        // tab ids. Its output is persisted verbatim. When the core resolves
        // the snapshot to no persistable tabs (or the JSON handoff fails),
        // the original snapshot is persisted with plan-resolved tab ids so
        // empty-session saves keep clearing the stored tabs.
        val normalized = coreNormalizedSessionSnapshot(snapshot)
        val persisted = normalized ?: snapshot
        val tabIds = normalized?.tabs?.map { it.id }
            ?: sessionTabIdPlanIds(
                snapshot.tabs.mapIndexed { index, tab -> SessionTabIdRequest(tab.id, index, tab.address) }
            )
        return withDatabase { database ->
            database.runTransaction {
                execute(
                    sql.upsertSession,
                    listOf(
                        id,
                        persisted.activeTabIndex,
                        persisted.ownerKind,
                        persisted.ownerId,
                        persisted.surfaceKind,
                        persisted.surfaceId,
                        now,
                        now
                    )
                ) &&
                    execute(sql.deleteSessionTabs, listOf(id)) &&
                    persisted.tabs.withIndex().all { (index, tab) ->
                        execute(
                            sql.insertSessionTab,
                            listOf(
                                id,
                                index,
                                tabIds[index],
                                tab.title,
                                tab.address,
                                tab.createdAtUnixSeconds,
                                tab.updatedAtUnixSeconds
                            )
                        )
                    }
            }
        }
    }

    fun applyRemoteSyncBatch(batchJson: String): String {
        val planJson = ActriumCore.browserStorageSyncRemoteApplyPlanJson(batchJson)
        if (planJson == "null") return "null"
        return applyRemoteSyncPlan(planJson)
    }

    fun applyRemoteSyncPlan(planJson: String): String {
        val plan = BrowserStorageSyncNativePlan.fromJson(planJson) ?: return "null"
        val resultJson = withDatabase { database ->
            plan.toResultJson(database.executeRemoteApplyPlan(plan))
        }
        return ActriumCore.browserStorageSyncRemoteApplyResultJson(planJson, resultJson)
    }

    fun applySyncAckBatch(ackJson: String): Boolean {
        val planJson = ActriumCore.browserStorageSyncAckPlanJson(ackJson)
        if (planJson == "null") return false
        return applySyncAckPlan(planJson)
    }

    fun applySyncAckPlan(planJson: String): Boolean {
        val plan = BrowserStorageSyncNativePlan.fromJson(planJson) ?: return false
        return withDatabase { database ->
            database.executeSyncAckPlan(plan)
        }
    }

    fun applySyncCycleResponse(responseJson: String): String {
        val cyclePlanJson = ActriumCore.browserStorageSyncClientCyclePlanJson(responseJson)
        if (cyclePlanJson == "null") return "null"

        return try {
            val cyclePlan = JSONObject(cyclePlanJson)
            val remoteApplyPlanJson = cyclePlan.getJSONObject("remoteApplyPlan").toString()
            val remoteApplyPlan = BrowserStorageSyncNativePlan.fromJson(remoteApplyPlanJson) ?: return "null"

            var remoteApplyResultJson = "null"
            if (remoteApplyPlan.operations.isNotEmpty()) {
                remoteApplyResultJson = applyRemoteSyncPlan(remoteApplyPlanJson)
            }

            val ackJson = ActriumCore.browserStorageSyncClientAckBatchAfterRemoteApplyJson(
                responseJson,
                remoteApplyResultJson
            )
            if (ackJson == "null") return "null"

            val ackApplied = applySyncAckBatch(ackJson)
            ActriumCore.browserStorageSyncClientCycleApplyResultJson(
                responseJson,
                remoteApplyResultJson,
                ackApplied
            )
        } catch (_: JSONException) {
            "null"
        }
    }

    fun applyCloudCycleClientPlan(cycleRoundJson: String): String {
        val clientPlanJson = ActriumCore.browserStorageSyncCloudCycleClientPlanJson(cycleRoundJson)
        if (clientPlanJson == "null") return "null"

        return try {
            val clientPlan = JSONObject(clientPlanJson)
            val steps = clientPlan.optJSONArray("steps") ?: JSONArray()
            val stepResults = JSONArray()
            var remoteApplyResultJson = "null"

            for (index in 0 until steps.length()) {
                val step = steps.getJSONObject(index)
                val stepType = step.optString("stepType")
                val required = step.optBoolean("required", true)
                var success = true

                when (stepType) {
                    "applyLocalAck" -> {
                        val ackPlanJson = clientPlan.optJSONObject("ackPlan")?.toString()
                        success = ackPlanJson != null && applySyncAckPlan(ackPlanJson)
                    }
                    "applyRemoteBatch" -> {
                        val remoteApplyPlanJson = clientPlan.optJSONObject("remoteApplyPlan")?.toString()
                        if (remoteApplyPlanJson != null) {
                            remoteApplyResultJson = applyRemoteSyncPlan(remoteApplyPlanJson)
                            success = remoteApplyResultJson != "null"
                        } else {
                            success = false
                        }
                    }
                }

                stepResults.put(
                    JSONObject()
                        .put("stepType", stepType)
                        .put("success", success)
                        .put("error", JSONObject.NULL),
                )

                if (!success && required) {
                    break
                }
            }

            ActriumCore.browserStorageSyncCloudCycleClientApplyResultJson(
                cycleRoundJson,
                stepResults.toString(),
                remoteApplyResultJson
            )
        } catch (_: JSONException) {
            "null"
        }
    }

    fun exportDirtySyncBatch(
        scope: String = "default",
        deviceId: String,
        generatedAtUnixSeconds: Long = currentUnixSeconds(),
        limitPerCollection: Int = 200
    ): String {
        val rowsJson = withDatabase { database ->
            database.dirtySyncRowsJson(limitPerCollection)
        }
        return ActriumCore.browserStorageSyncBatchFromDirtyRowsJson(
            scope,
            deviceId,
            generatedAtUnixSeconds,
            rowsJson
        )
    }

    fun exportDirtySyncPushRequest(
        scope: String = "default",
        deviceId: String,
        generatedAtUnixSeconds: Long = currentUnixSeconds(),
        limitPerCollection: Int = 200
    ): String {
        val rowsJson = withDatabase { database ->
            database.dirtySyncRowsJson(limitPerCollection)
        }
        return ActriumCore.browserStorageSyncPushRequestFromDirtyRowsJson(
            scope,
            deviceId,
            generatedAtUnixSeconds,
            rowsJson
        )
    }

    fun syncCursorsJson(scope: String = "default"): String {
        return withDatabase { database ->
            val cursors = JSONArray()
            database.rawQuery(
                syncSql.selectSyncCursorsForScope,
                arrayOf(scope),
            ).use { cursor ->
                while (cursor.moveToNext()) {
                    val value = JSONObject()
                        .put("collection", cursor.stringValue("collection"))
                        .put("cursor", cursor.stringValue("cursor"))
                    val lastSyncedAtIndex = cursor.getColumnIndex("last_synced_at")
                    if (lastSyncedAtIndex >= 0 && !cursor.isNull(lastSyncedAtIndex)) {
                        value.put("lastSyncedAtUnixSeconds", cursor.getLong(lastSyncedAtIndex))
                    }
                    cursors.put(value)
                }
            }
            cursors.toString()
        }
    }

    fun exportSyncCycleRequestPlan(
        baseUrl: String,
        accessToken: String,
        scope: String = "default",
        deviceId: String,
        generatedAtUnixSeconds: Long = currentUnixSeconds(),
        limitPerCollection: Int = 200
    ): String {
        val pushRequestJson = exportDirtySyncPushRequest(
            scope = scope,
            deviceId = deviceId,
            generatedAtUnixSeconds = generatedAtUnixSeconds,
            limitPerCollection = limitPerCollection,
        )
        if (pushRequestJson == "null") return "null"
        return ActriumCore.browserStorageSyncCycleRequestPlanJson(
            baseUrl,
            accessToken,
            pushRequestJson,
            syncCursorsJson(scope),
        )
    }

    private fun openInitializedDatabase(): SQLiteDatabase {
        val file = File(databasePath)
        file.parentFile?.mkdirs()
        return SQLiteDatabase.openOrCreateDatabase(file, null).also { database ->
            schema.statements.forEach { statement ->
                database.execute(statement)
            }
            database.migrateBrowserStorageSchemaIfNeeded()
        }
    }

    private fun <T> withDatabase(block: (SQLiteDatabase) -> T): T {
        return openInitializedDatabase().use { database ->
            block(database)
        }
    }

    private fun <T> query(
        statement: String,
        args: List<Any?> = emptyList(),
        mapper: (Cursor) -> T
    ): List<T> {
        return withDatabase { database ->
            database.query(statement, args, mapper)
        }
    }

    private fun SQLiteDatabase.upsertHistoryPage(item: BrowserHistoryItem): Boolean {
        return execute(
            sql.upsertHistoryPage,
            listOf(
                item.address,
                item.title,
                item.visitCount.coerceAtLeast(1),
                item.visitedAtUnixSeconds,
                item.visitedAtUnixSeconds,
                item.visitedAtUnixSeconds,
                item.favicon
            )
        )
    }

    private fun SQLiteDatabase.upsertBookmark(item: BrowserBookmarkItem): Boolean {
        return execute(
            sql.upsertBookmark,
            listOf(
                item.id,
                item.folderId,
                item.title,
                item.address,
                item.createdAtUnixSeconds,
                item.updatedAtUnixSeconds,
                item.sortOrder,
                item.favicon,
                item.ownerKind,
                item.ownerId
            )
        )
    }

    private fun SQLiteDatabase.runTransaction(block: SQLiteDatabase.() -> Boolean): Boolean {
        beginTransaction()
        return try {
            val succeeded = block()
            if (succeeded) {
                setTransactionSuccessful()
            }
            succeeded
        } finally {
            endTransaction()
        }
    }

    private fun SQLiteDatabase.execute(statement: String, args: List<Any?> = emptyList()): Boolean {
        return try {
            if (args.isEmpty()) {
                execSQL(statement)
            } else {
                execSQL(statement, args.toTypedArray())
            }
            true
        } catch (_: SQLException) {
            false
        }
    }

    private fun SQLiteDatabase.executeForChangedRows(statement: String, args: List<Any?>): Int? {
        return try {
            compileStatement(statement).use { compiledStatement ->
                compiledStatement.bindArgs(args)
                compiledStatement.executeUpdateDelete()
            }
        } catch (_: SQLException) {
            null
        }
    }

    private fun SQLiteDatabase.executeRemoteApplyPlan(
        plan: BrowserStorageSyncNativePlan
    ): List<JSONObject> {
        try {
            beginTransaction()
        } catch (_: SQLException) {
            return remoteApplyFailureResults(plan, failedOrdinal = null, message = "begin transaction failed")
        }

        var shouldCommit = false
        var results: List<JSONObject>? = null
        val successfulResults = mutableListOf<JSONObject>()
        for ((ordinal, operation) in plan.operations.withIndex()) {
            val statement = remoteApplyStatement(operation.statementKey)
            if (statement == null) {
                results = remoteApplyFailureResults(
                    plan,
                    failedOrdinal = ordinal,
                    message = "unknown statement key: ${operation.statementKey}"
                )
                break
            }

            val rowsAffected = executeForChangedRows(statement, operation.params)
            if (rowsAffected == null) {
                results = remoteApplyFailureResults(
                    plan,
                    failedOrdinal = ordinal,
                    message = "statement execution failed: ${operation.statementKey}"
                )
                break
            }

            successfulResults += JSONObject()
                .put("operationIndex", operation.index)
                .put("rowsAffected", rowsAffected)
        }

        if (results == null) {
            results = try {
                setTransactionSuccessful()
                shouldCommit = true
                successfulResults
            } catch (_: SQLException) {
                remoteApplyFailureResults(
                    plan,
                    failedOrdinal = null,
                    message = "commit transaction failed"
                )
            }
        }

        val finalResults = results ?: successfulResults
        return try {
            endTransaction()
            finalResults
        } catch (_: SQLException) {
            remoteApplyFailureResults(
                plan,
                failedOrdinal = null,
                message = if (shouldCommit) "commit transaction failed" else "rollback transaction failed"
            )
        }
    }

    private fun SQLiteDatabase.executeSyncAckPlan(plan: BrowserStorageSyncNativePlan): Boolean {
        try {
            beginTransaction()
        } catch (_: SQLException) {
            return false
        }

        var succeeded = true
        for (operation in plan.operations) {
            val statement = syncAckStatement(operation.statementKey)
            if (statement == null || executeForChangedRows(statement, operation.params) == null) {
                succeeded = false
                break
            }
        }

        if (succeeded) {
            try {
                setTransactionSuccessful()
            } catch (_: SQLException) {
                succeeded = false
            }
        }

        return try {
            endTransaction()
            succeeded
        } catch (_: SQLException) {
            false
        }
    }

    private fun SQLiteDatabase.dirtySyncRowsJson(limitPerCollection: Int): String {
        val limit = limitPerCollection.coerceIn(1, 1_000)
        val rows = JSONArray()
        val statements = listOf(
            syncSql.selectDirtyHistoryPages,
            syncSql.selectDirtyHistoryVisits,
            syncSql.selectDirtyBookmarkFolders,
            syncSql.selectDirtyBookmarks,
            syncSql.selectDirtySessions,
            syncSql.selectDirtySessionTabs
        )
        for (statement in statements) {
            rawQuery(statement, arrayOf(limit.toString())).use { cursor ->
                while (cursor.moveToNext()) {
                    rows.put(cursor.toJsonObject())
                }
            }
        }
        return rows.toString()
    }

    private fun remoteApplyStatement(statementKey: String): String? {
        return when (statementKey) {
            "applyRemoteHistoryPage" -> syncSql.applyRemoteHistoryPage
            "applyRemoteHistoryVisit" -> syncSql.applyRemoteHistoryVisit
            "applyRemoteBookmarkFolder" -> syncSql.applyRemoteBookmarkFolder
            "applyRemoteBookmark" -> syncSql.applyRemoteBookmark
            "applyRemoteSession" -> syncSql.applyRemoteSession
            "applyRemoteSessionTab" -> syncSql.applyRemoteSessionTab
            else -> null
        }
    }

    private fun syncAckStatement(statementKey: String): String? {
        return when (statementKey) {
            "markHistoryPageSynced" -> syncSql.markHistoryPageSynced
            "markHistoryVisitSynced" -> syncSql.markHistoryVisitSynced
            "markBookmarkFolderSynced" -> syncSql.markBookmarkFolderSynced
            "markBookmarkSynced" -> syncSql.markBookmarkSynced
            "markSessionSynced" -> syncSql.markSessionSynced
            "markSessionTabSynced" -> syncSql.markSessionTabSynced
            "upsertSyncCursor" -> syncSql.upsertSyncCursor
            else -> null
        }
    }

    private fun remoteApplyFailureResults(
        plan: BrowserStorageSyncNativePlan,
        failedOrdinal: Int?,
        message: String
    ): List<JSONObject> {
        return plan.operations.mapIndexed { ordinal, operation ->
            val error = when {
                failedOrdinal == null -> message
                ordinal < failedOrdinal -> "rolled back"
                ordinal > failedOrdinal -> "not executed"
                else -> message
            }
            JSONObject()
                .put("operationIndex", operation.index)
                .put("rowsAffected", 0)
                .put("error", error)
        }
    }

    /**
     * Replays the Rust-owned incremental schema migration plan
     * (browser_storage_schema_migration_plan_json). All DDL literals, their
     * order, and the schema version stamp live in the Rust core so migrated
     * databases stay byte-identical across platforms; this adapter only
     * performs PRAGMA table_info column introspection and statement
     * execution.
     */
    private fun SQLiteDatabase.migrateBrowserStorageSchemaIfNeeded() {
        val plan = try {
            JSONObject(ActriumCore.browserStorageSchemaMigrationPlanJson())
        } catch (ignored: JSONException) {
            return
        }
        val addColumns = plan.optJSONArray("addColumns") ?: JSONArray()
        for (index in 0 until addColumns.length()) {
            val entry = addColumns.optJSONObject(index) ?: continue
            val table = entry.optString("table")
            val column = entry.optString("column")
            val statement = entry.optString("statement")
            if (table.isEmpty() || column.isEmpty() || statement.isEmpty()) {
                continue
            }
            if (!columnExists(table, column)) {
                execute(statement)
            }
        }
        val idempotentStatements = plan.optJSONArray("idempotentStatements") ?: JSONArray()
        for (index in 0 until idempotentStatements.length()) {
            val statement = idempotentStatements.optString(index)
            if (statement.isNotEmpty()) {
                execute(statement)
            }
        }
    }

    private fun SQLiteDatabase.columnExists(table: String, column: String): Boolean {
        return query("PRAGMA table_info($table)") { cursor ->
            cursor.stringValue("name")
        }.any { it == column }
    }

    /**
     * Normalizes a whole session snapshot in one batched Rust call
     * (browser_session_snapshot_normalize_json) so canonical addresses,
     * title rules, the tab cap, active-index clamping, and stable tab ids
     * stay byte-identical across all platforms. Tab timestamps and the
     * owner/surface fields are platform-owned columns outside the Rust
     * contract and are re-attached from the input snapshot. Returns null
     * when the core resolves the snapshot to no persistable tabs or the
     * JSON handoff fails; callers then persist the input snapshot unchanged.
     */
    private fun coreNormalizedSessionSnapshot(snapshot: BrowserSessionSnapshot): BrowserSessionSnapshot? {
        val request = JSONObject()
            // Browser-data snapshot contract version; the core stamps its
            // own version on the normalized output.
            .put("version", 1)
            .put("activeTabIndex", snapshot.activeTabIndex)
            .put(
                "tabs",
                JSONArray().apply {
                    snapshot.tabs.forEach { tab ->
                        put(
                            JSONObject()
                                .put("id", tab.id)
                                .put("title", tab.title)
                                .put("address", tab.address)
                        )
                    }
                }
            )
        val normalizedJson = ActriumCore.browserSessionSnapshotNormalizeJson(request.toString())
        return try {
            val normalized = JSONObject(normalizedJson)
            val tabsJson = normalized.getJSONArray("tabs")
            if (tabsJson.length() == 0) return null

            val timestampsByTabId = HashMap<String, Pair<Long, Long>>()
            snapshot.tabs.forEach { tab ->
                val key = tab.id.trim()
                if (key.isNotEmpty() && !timestampsByTabId.containsKey(key)) {
                    timestampsByTabId[key] = tab.createdAtUnixSeconds to tab.updatedAtUnixSeconds
                }
            }
            // Normalization preserves tab order, so when no tab was dropped
            // the same index maps back to the input tab (covers tabs whose
            // empty id was replaced by a Rust stable id).
            val indexAligned = tabsJson.length() == snapshot.tabs.size
            val now = currentUnixSeconds()
            val tabs = List(tabsJson.length()) { position ->
                val tab = tabsJson.getJSONObject(position)
                val tabId = tab.optString("id")
                val timestamps = timestampsByTabId[tabId]
                    ?: if (indexAligned) {
                        snapshot.tabs[position].createdAtUnixSeconds to
                            snapshot.tabs[position].updatedAtUnixSeconds
                    } else {
                        null
                    }
                BrowserSessionTab(
                    id = tabId,
                    title = tab.optString("title"),
                    address = tab.optString("address"),
                    createdAtUnixSeconds = timestamps?.first ?: now,
                    updatedAtUnixSeconds = timestamps?.second ?: now
                )
            }
            snapshot.copy(
                activeTabIndex = normalized.optInt("activeTabIndex"),
                tabs = tabs
            )
        } catch (error: JSONException) {
            null
        }
    }

    /**
     * Resolves stable session tab ids for a whole session in one batched
     * Rust call (browser_storage_session_tab_id_plan_json), so the id
     * fallback derivation stays byte-identical across all platforms. The
     * returned list is always index-aligned with [tabs]; if the plan is
     * unavailable the raw trimmed ids pass through unchanged.
     */
    private fun sessionTabIdPlanIds(tabs: List<SessionTabIdRequest>): List<String> {
        val fallbackIds = tabs.map { it.id?.trim().orEmpty() }
        if (tabs.isEmpty()) return fallbackIds
        val request = JSONArray()
        tabs.forEach { tab ->
            request.put(
                JSONObject()
                    .put("id", tab.id ?: "")
                    .put("index", tab.index)
                    .put("address", tab.address)
            )
        }
        val planJson = ActriumCore.browserStorageSessionTabIdPlanJson(request.toString())
        return try {
            val tabIdsJson = JSONObject(planJson).getJSONArray("tabIds")
            if (tabIdsJson.length() != tabs.size) return fallbackIds
            List(tabIdsJson.length()) { position -> tabIdsJson.getString(position) }
        } catch (error: JSONException) {
            fallbackIds
        }
    }

    private data class SessionTabIdRequest(
        val id: String?,
        val index: Int,
        val address: String
    )

    private data class SessionTabRow(
        val tabId: String?,
        val tabIndex: Int,
        val title: String,
        val address: String,
        val createdAtUnixSeconds: Long,
        val updatedAtUnixSeconds: Long
    )

    private fun <T> SQLiteDatabase.query(
        statement: String,
        args: List<Any?> = emptyList(),
        mapper: (Cursor) -> T
    ): List<T> {
        return rawQuery(statement, args.map { it?.toString() }.toTypedArray()).use { cursor ->
            buildList {
                while (cursor.moveToNext()) {
                    add(mapper(cursor))
                }
            }
        }
    }

    private data class BrowserStorageSyncNativePlan(
        val version: Int,
        val storageSchemaVersion: Int,
        val scope: String,
        val deviceId: String,
        val generatedAtUnixSeconds: Long,
        val operations: List<BrowserStorageSyncNativeOperation>
    ) {
        fun toResultJson(results: List<JSONObject>): String {
            val resultArray = JSONArray()
            results.forEach { resultArray.put(it) }
            return JSONObject()
                .put("version", version)
                .put("storageSchemaVersion", storageSchemaVersion)
                .put("scope", scope)
                .put("deviceId", deviceId)
                .put("generatedAtUnixSeconds", generatedAtUnixSeconds)
                .put("results", resultArray)
                .toString()
        }

        companion object {
            fun fromJson(json: String): BrowserStorageSyncNativePlan? {
                return try {
                    val root = JSONObject(json)
                    val operationsJson = root.getJSONArray("operations")
                    val operations = buildList {
                        for (index in 0 until operationsJson.length()) {
                            add(
                                BrowserStorageSyncNativeOperation.fromJson(
                                    index,
                                    operationsJson.getJSONObject(index)
                                ) ?: return null
                            )
                        }
                    }

                    BrowserStorageSyncNativePlan(
                        version = root.optInt("version"),
                        storageSchemaVersion = root.optInt("storageSchemaVersion"),
                        scope = root.optString("scope"),
                        deviceId = root.optString("deviceId"),
                        generatedAtUnixSeconds = root.optLong("generatedAtUnixSeconds"),
                        operations = operations
                    )
                } catch (_: JSONException) {
                    null
                }
            }
        }
    }

    private data class BrowserStorageSyncNativeOperation(
        val index: Int,
        val statementKey: String,
        val params: List<Any?>
    ) {
        companion object {
            fun fromJson(index: Int, root: JSONObject): BrowserStorageSyncNativeOperation? {
                val statementKey = root.optString("statementKey")
                if (statementKey.isBlank() || !root.has("params")) {
                    return null
                }

                return BrowserStorageSyncNativeOperation(
                    index = index,
                    statementKey = statementKey,
                    params = root.getJSONArray("params").toSqlParams()
                )
            }
        }
    }

    companion object {
        const val DEFAULT_SESSION_ID = "default"
        private const val DEFAULT_LIMIT = 500
        fun createDefault(context: Context): BrowserStorageDatabase {
            return BrowserStorageDatabase(databasePath(context))
        }

        fun createAtPath(path: String): BrowserStorageDatabase {
            return BrowserStorageDatabase(path)
        }

        fun databasePath(context: Context): String {
            val appContext = context.applicationContext
            // Reuse the old store in place to preserve its WAL and journal files.
            val legacyFile = appContext.getDatabasePath("actro_browser.db")
            return if (legacyFile.exists()) {
                legacyFile.absolutePath
            } else {
                appContext.getDatabasePath(ActriumCore.browserStorageDatabaseName()).absolutePath
            }
        }
    }
}

private fun SQLiteStatement.bindArgs(args: List<Any?>) {
    clearBindings()
    args.forEachIndexed { index, value ->
        val bindIndex = index + 1
        when (value) {
            null -> bindNull(bindIndex)
            is String -> bindString(bindIndex, value)
            is Boolean -> bindLong(bindIndex, if (value) 1L else 0L)
            is Float -> bindDouble(bindIndex, value.toDouble())
            is Double -> bindDouble(bindIndex, value)
            is Number -> bindLong(bindIndex, value.toLong())
            else -> bindString(bindIndex, value.toString())
        }
    }
}

private fun JSONArray.toSqlParams(): List<Any?> {
    // `this` is bound to a local so the reads below cannot resolve against the
    // ArrayList being populated: inside a buildList {} block an unqualified
    // `get(index)` binds to the MutableList builder receiver, not to
    // JSONArray.get, which reads the still-empty builder and throws on the
    // first non-empty params array. Every cloud cycle apply plan that binds a
    // statement parameter hits this, so it must read the JSONArray explicitly.
    val array = this
    val params = ArrayList<Any?>(array.length())
    for (index in 0 until array.length()) {
        params.add(array.get(index).toSqlParam())
    }
    return params
}

private fun Any?.toSqlParam(): Any? {
    return when (this) {
        null, JSONObject.NULL -> null
        is String -> this
        is Boolean -> this
        is Float -> this.toDouble()
        is Double -> this
        is Number -> this.toLong()
        else -> toString()
    }
}

private fun currentUnixSeconds(): Long = System.currentTimeMillis() / 1000

private fun Cursor.stringValue(columnName: String): String {
    return optionalStringValue(columnName).orEmpty()
}

private fun Cursor.optionalStringValue(columnName: String): String? {
    val index = getColumnIndex(columnName)
    if (index < 0 || isNull(index)) {
        return null
    }
    return getString(index)
}

private fun Cursor.longValue(columnName: String): Long {
    val index = getColumnIndex(columnName)
    if (index < 0 || isNull(index)) {
        return 0
    }
    return getLong(index)
}

private fun Cursor.intValue(columnName: String): Int {
    val value = longValue(columnName)
    return value.coerceIn(Int.MIN_VALUE.toLong(), Int.MAX_VALUE.toLong()).toInt()
}

private fun Cursor.toJsonObject(): JSONObject {
    val row = JSONObject()
    for (index in 0 until columnCount) {
        val name = getColumnName(index)
        when (getType(index)) {
            Cursor.FIELD_TYPE_NULL -> row.put(name, JSONObject.NULL)
            Cursor.FIELD_TYPE_INTEGER -> row.put(name, getLong(index))
            Cursor.FIELD_TYPE_FLOAT -> row.put(name, getDouble(index))
            Cursor.FIELD_TYPE_STRING -> row.put(name, getString(index))
            else -> row.put(name, JSONObject.NULL)
        }
    }
    return row
}
