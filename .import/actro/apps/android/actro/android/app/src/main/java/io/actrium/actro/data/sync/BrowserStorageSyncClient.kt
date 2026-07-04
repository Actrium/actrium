package io.actrium.actro.data.sync

import io.actrium.actro.data.database.BrowserStorageDatabase
import io.actrium.actro.rust.ActroCore
import java.net.HttpURLConnection
import java.net.URL
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject

data class BrowserStorageSyncHttpRequest(
    val method: String,
    val url: String,
    val headers: Map<String, String>,
    val bodyJson: String
)

data class BrowserStorageSyncHttpResponse(
    val statusCode: Int,
    val bodyJson: String,
    val errorMessage: String? = null
)

interface BrowserStorageSyncTransport {
    suspend fun send(request: BrowserStorageSyncHttpRequest): BrowserStorageSyncHttpResponse
}

class HttpUrlConnectionBrowserStorageSyncTransport : BrowserStorageSyncTransport {
    override suspend fun send(request: BrowserStorageSyncHttpRequest): BrowserStorageSyncHttpResponse =
        withContext(Dispatchers.IO) {
            try {
                val connection = URL(request.url).openConnection() as HttpURLConnection
                connection.requestMethod = request.method
                request.headers.forEach { (name, value) ->
                    connection.setRequestProperty(name, value)
                }
                connection.doOutput = request.bodyJson.isNotEmpty()
                if (request.bodyJson.isNotEmpty()) {
                    connection.outputStream.use { stream ->
                        stream.write(request.bodyJson.toByteArray(Charsets.UTF_8))
                    }
                }

                val statusCode = connection.responseCode
                val stream = if (statusCode in 200..399) {
                    connection.inputStream
                } else {
                    connection.errorStream
                }
                BrowserStorageSyncHttpResponse(
                    statusCode = statusCode,
                    bodyJson = stream?.bufferedReader(Charsets.UTF_8)?.use { it.readText() }.orEmpty(),
                    errorMessage = null,
                )
            } catch (error: Exception) {
                BrowserStorageSyncHttpResponse(
                    statusCode = 0,
                    bodyJson = "",
                    errorMessage = error.message ?: error.javaClass.simpleName,
                )
            }
        }
}

class BrowserStorageSyncClient(
    private val database: BrowserStorageDatabase,
    private val transport: BrowserStorageSyncTransport = HttpUrlConnectionBrowserStorageSyncTransport()
) {
    suspend fun runSyncCycle(
        baseUrl: String,
        accessToken: String,
        scope: String = "default",
        deviceId: String,
        generatedAtUnixSeconds: Long = currentUnixSeconds(),
        limitPerCollection: Int = 200
    ): String {
        val requestPlanJson = database.exportSyncCycleRequestPlan(
            baseUrl = baseUrl,
            accessToken = accessToken,
            scope = scope,
            deviceId = deviceId,
            generatedAtUnixSeconds = generatedAtUnixSeconds,
            limitPerCollection = limitPerCollection,
        )
        val requestPlan = BrowserStorageSyncCycleRequestPlan.fromJson(requestPlanJson)
        if (requestPlan == null) {
            return syncClientResultJson(
                requestPlan = requestPlanJson.toJsonObjectOrNull(),
                transportResult = JSONObject()
                    .put("classification", "clientPlanUnavailable")
                    .put("shouldRetry", false)
                    .put("clientAction", "skipCycle"),
                applyResult = JSONObject.NULL,
                applied = false,
            )
        }

        val response = transport.send(requestPlan.toHttpRequest())
        val transportResultJson = ActroCore.browserStorageSyncCycleTransportResultJson(
            statusCode = response.statusCode,
            responseJson = response.bodyJson,
            errorMessage = response.errorMessage.orEmpty(),
        )
        val transportResult = transportResultJson.toJsonObjectOrNull() ?: JSONObject()
        var applyResult: Any = JSONObject.NULL
        var applied = false

        if (transportResult.optString("classification") == "success") {
            val responseJson = transportResult.optJSONObject("response")?.toString() ?: response.bodyJson
            val applyResultJson = database.applySyncCycleResponse(responseJson)
            val applyObject = applyResultJson.toJsonObjectOrNull()
            if (applyObject != null) {
                applyResult = applyObject
                applied = true
            }
        }

        return syncClientResultJson(
            requestPlan = requestPlanJson.toJsonObjectOrNull(),
            transportResult = transportResult,
            applyResult = applyResult,
            applied = applied,
        )
    }

    suspend fun runLocalFolderSyncCycle(
        rootDir: String,
        scope: String = "default",
        deviceId: String,
        generatedAtUnixSeconds: Long = currentUnixSeconds(),
        limitPerCollection: Int = 200,
        retryAttempt: Int = 0,
        observedAtUnixSeconds: Long = generatedAtUnixSeconds,
        maxAttempts: Int = 3
    ): String {
        val capabilityPreflight = localFolderCapabilityPreflight(observedAtUnixSeconds)
        if (capabilityPreflight?.optBoolean("shouldBlockSync", false) == true) {
            val providerDiagnostics = cloudProviderDiagnostics(
                capabilityPreflight = capabilityPreflight,
                compatibilityPreflight = null,
                statusPolicy = null,
                scheduleState = null,
                observedAtUnixSeconds = observedAtUnixSeconds,
            )
            val observabilityEvent = cloudObservabilityEvent(
                providerDiagnostics = providerDiagnostics,
                attemptPlan = null,
                loopPlan = null,
                observedAtUnixSeconds = observedAtUnixSeconds,
            )
            return localFolderSyncClientResultJson(
                rootDir = rootDir,
                pushRequest = null,
                capabilityPreflight = capabilityPreflight,
                compatibilityPreflight = null,
                schemaRolloutPlan = null,
                schemaMigrationPlaybook = null,
                recoverySnapshotRound = null,
                cycleRound = null,
                statusPolicy = null,
                scheduleState = null,
                attemptPlan = null,
                loopPlan = null,
                applyResult = null,
                compactionRound = null,
                cleanupRound = null,
                providerDiagnostics = providerDiagnostics,
                observabilityEvent = observabilityEvent,
                classification = capabilityPreflight.optString("classification"),
                shouldRetry = false,
                clientAction = capabilityPreflight.optString("clientAction"),
                applied = false,
            )
        }
        val compatibilityPreflight = localFolderCompatibilityPreflight(
            rootDir = rootDir,
            observedAtUnixSeconds = observedAtUnixSeconds,
        )
        if (compatibilityPreflight?.optBoolean("shouldBlockSync", false) == true) {
            val schemaRolloutPlan = cloudSchemaRolloutPlan(
                compatibilityPreflight = compatibilityPreflight,
                observedAtUnixSeconds = observedAtUnixSeconds,
            )
            val schemaMigrationPlaybook = cloudSchemaMigrationPlaybook(
                schemaRolloutPlan = schemaRolloutPlan,
                observedAtUnixSeconds = observedAtUnixSeconds,
            )
            val recoverySnapshotRound = localFolderRecoverySnapshotRound(
                rootDir = rootDir,
                schemaMigrationPlaybook = schemaMigrationPlaybook,
                compatibilityPreflight = compatibilityPreflight,
                observedAtUnixSeconds = observedAtUnixSeconds,
            )
            val providerDiagnostics = cloudProviderDiagnostics(
                capabilityPreflight = capabilityPreflight,
                compatibilityPreflight = compatibilityPreflight,
                statusPolicy = null,
                scheduleState = null,
                observedAtUnixSeconds = observedAtUnixSeconds,
            )
            val observabilityEvent = cloudObservabilityEvent(
                providerDiagnostics = providerDiagnostics,
                attemptPlan = null,
                loopPlan = null,
                observedAtUnixSeconds = observedAtUnixSeconds,
            )
            return localFolderSyncClientResultJson(
                rootDir = rootDir,
                pushRequest = null,
                capabilityPreflight = capabilityPreflight,
                compatibilityPreflight = compatibilityPreflight,
                schemaRolloutPlan = schemaRolloutPlan,
                schemaMigrationPlaybook = schemaMigrationPlaybook,
                recoverySnapshotRound = recoverySnapshotRound,
                cycleRound = null,
                statusPolicy = null,
                scheduleState = null,
                attemptPlan = null,
                loopPlan = null,
                applyResult = null,
                compactionRound = null,
                cleanupRound = null,
                providerDiagnostics = providerDiagnostics,
                observabilityEvent = observabilityEvent,
                classification = compatibilityPreflight.optString("classification"),
                shouldRetry = false,
                clientAction = compatibilityPreflight.optString("clientAction"),
                applied = false,
            )
        }
        val pushRequestJson = database.exportDirtySyncPushRequest(
            scope = scope,
            deviceId = deviceId,
            generatedAtUnixSeconds = generatedAtUnixSeconds,
            limitPerCollection = limitPerCollection,
        )
        if (pushRequestJson == "null") {
            val providerDiagnostics = cloudProviderDiagnostics(
                capabilityPreflight = capabilityPreflight,
                compatibilityPreflight = compatibilityPreflight,
                statusPolicy = null,
                scheduleState = null,
                observedAtUnixSeconds = observedAtUnixSeconds,
            )
            val observabilityEvent = cloudObservabilityEvent(
                providerDiagnostics = providerDiagnostics,
                attemptPlan = null,
                loopPlan = null,
                observedAtUnixSeconds = observedAtUnixSeconds,
            )
            return localFolderSyncClientResultJson(
                rootDir = rootDir,
                pushRequest = pushRequestJson.toJsonObjectOrNull(),
                capabilityPreflight = capabilityPreflight,
                compatibilityPreflight = compatibilityPreflight,
                schemaRolloutPlan = null,
                schemaMigrationPlaybook = null,
                recoverySnapshotRound = null,
                cycleRound = null,
                statusPolicy = null,
                scheduleState = null,
                attemptPlan = null,
                loopPlan = null,
                applyResult = null,
                compactionRound = null,
                cleanupRound = null,
                providerDiagnostics = providerDiagnostics,
                observabilityEvent = observabilityEvent,
                classification = "clientPlanUnavailable",
                shouldRetry = false,
                clientAction = "skipCloudCycle",
                applied = false,
            )
        }

        val cycleRoundJson = ActroCore.browserStorageSyncCloudLocalFolderCycleRoundJson(
            rootDir = rootDir,
            pushRequestJson = pushRequestJson,
            cursorsJson = database.syncCursorsJson(scope),
            deviceId = deviceId,
            generatedAtUnixSeconds = generatedAtUnixSeconds,
            maxLogObjectsPerCollection = limitPerCollection.coerceIn(1, 1_000),
        )
        val cycleRound = cycleRoundJson.toJsonObjectOrNull()
        if (cycleRound == null) {
            val providerDiagnostics = cloudProviderDiagnostics(
                capabilityPreflight = capabilityPreflight,
                compatibilityPreflight = compatibilityPreflight,
                statusPolicy = null,
                scheduleState = null,
                observedAtUnixSeconds = observedAtUnixSeconds,
            )
            val observabilityEvent = cloudObservabilityEvent(
                providerDiagnostics = providerDiagnostics,
                attemptPlan = null,
                loopPlan = null,
                observedAtUnixSeconds = observedAtUnixSeconds,
            )
            return localFolderSyncClientResultJson(
                rootDir = rootDir,
                pushRequest = pushRequestJson.toJsonObjectOrNull(),
                capabilityPreflight = capabilityPreflight,
                compatibilityPreflight = compatibilityPreflight,
                schemaRolloutPlan = null,
                schemaMigrationPlaybook = null,
                recoverySnapshotRound = null,
                cycleRound = null,
                statusPolicy = null,
                scheduleState = null,
                attemptPlan = null,
                loopPlan = null,
                applyResult = null,
                compactionRound = null,
                cleanupRound = null,
                providerDiagnostics = providerDiagnostics,
                observabilityEvent = observabilityEvent,
                classification = "clientPlanUnavailable",
                shouldRetry = false,
                clientAction = "skipCloudCycle",
                applied = false,
            )
        }

        val statusPolicy = ActroCore.browserStorageSyncCloudCycleStatusPolicyJson(
            cycleRoundJson = cycleRoundJson,
            retryAttempt = retryAttempt,
        ).toJsonObjectOrNull()
        val scheduleState = ActroCore.browserStorageSyncCloudCycleScheduleStateJson(
            statusPolicyJson = statusPolicy?.toString().orEmpty(),
            observedAtUnixSeconds = observedAtUnixSeconds,
        ).toJsonObjectOrNull()
        val attemptPlan = ActroCore.browserStorageSyncCloudCycleAttemptPlanJson(
            cycleRoundJson = cycleRoundJson,
            retryAttempt = retryAttempt,
            observedAtUnixSeconds = observedAtUnixSeconds,
        ).toJsonObjectOrNull()
        val loopPlan = ActroCore.browserStorageSyncCloudCycleLoopPlanJson(
            cycleRoundsJson = JSONArray().put(cycleRound).toString(),
            retryAttempt = retryAttempt,
            observedAtUnixSeconds = observedAtUnixSeconds,
            maxAttempts = maxAttempts,
        ).toJsonObjectOrNull()
        val applyResult = database.applyCloudCycleClientPlan(cycleRoundJson).toJsonObjectOrNull()
        val compactionRound = ActroCore.browserStorageSyncCloudLocalFolderCompactionRoundJson(
            rootDir = rootDir,
            deviceId = deviceId,
            generatedAtUnixSeconds = generatedAtUnixSeconds,
        ).toJsonObjectOrNull()
        val cleanupRound = ActroCore.browserStorageSyncCloudLocalFolderCleanupRoundJson(
            rootDir = rootDir,
            cursorsJson = database.syncCursorsJson(scope),
            deviceId = deviceId,
            generatedAtUnixSeconds = generatedAtUnixSeconds,
        ).toJsonObjectOrNull()
        val providerErrorClassification = localFolderProviderErrorClassification(
            cycleRound = cycleRound,
            compactionRound = compactionRound,
            cleanupRound = cleanupRound,
            retryAttempt = retryAttempt,
            observedAtUnixSeconds = observedAtUnixSeconds,
        )
        val providerDiagnostics = cloudProviderDiagnostics(
            capabilityPreflight = capabilityPreflight,
            compatibilityPreflight = compatibilityPreflight,
            providerErrorClassification = providerErrorClassification,
            statusPolicy = statusPolicy,
            scheduleState = scheduleState,
            observedAtUnixSeconds = observedAtUnixSeconds,
        )
        val observabilityEvent = cloudObservabilityEvent(
            providerDiagnostics = providerDiagnostics,
            attemptPlan = attemptPlan,
            loopPlan = loopPlan,
            observedAtUnixSeconds = observedAtUnixSeconds,
        )
        val statusClientAction = statusPolicy?.optString("clientAction").orEmpty()
        val clientAction = statusClientAction.ifEmpty { cycleRound.optString("clientAction") }

        return localFolderSyncClientResultJson(
            rootDir = rootDir,
            pushRequest = pushRequestJson.toJsonObjectOrNull(),
            capabilityPreflight = capabilityPreflight,
            compatibilityPreflight = compatibilityPreflight,
            schemaRolloutPlan = null,
            schemaMigrationPlaybook = null,
            recoverySnapshotRound = null,
            cycleRound = cycleRound,
            statusPolicy = statusPolicy,
            scheduleState = scheduleState,
            attemptPlan = attemptPlan,
            loopPlan = loopPlan,
            applyResult = applyResult,
            compactionRound = compactionRound,
            cleanupRound = cleanupRound,
            providerDiagnostics = providerDiagnostics,
            observabilityEvent = observabilityEvent,
            classification = statusPolicy?.optString("classification").orEmpty(),
            shouldRetry = statusPolicy?.optBoolean("shouldRetry", false) ?: false,
            clientAction = clientAction,
            applied = applyResult?.optBoolean("applied", false) ?: false,
        )
    }

    suspend fun runLocalFolderMembershipRound(
        rootDir: String,
        requestJson: String,
        observedAtUnixSeconds: Long = currentUnixSeconds()
    ): String {
        val capabilityPreflight = localFolderCapabilityPreflight(observedAtUnixSeconds)
        if (capabilityPreflight?.optBoolean("shouldBlockSync", false) == true) {
            val providerDiagnostics = cloudProviderDiagnostics(
                capabilityPreflight = capabilityPreflight,
                compatibilityPreflight = null,
                statusPolicy = null,
                scheduleState = null,
                observedAtUnixSeconds = observedAtUnixSeconds,
            )
            val observabilityEvent = cloudObservabilityEvent(
                providerDiagnostics = providerDiagnostics,
                attemptPlan = null,
                loopPlan = null,
                observedAtUnixSeconds = observedAtUnixSeconds,
            )
            return localFolderMembershipClientResultJson(
                rootDir = rootDir,
                membershipRequest = requestJson.toJsonObjectOrNull(),
                capabilityPreflight = capabilityPreflight,
                compatibilityPreflight = null,
                schemaRolloutPlan = null,
                schemaMigrationPlaybook = null,
                recoverySnapshotRound = null,
                membershipRound = null,
                statusPolicy = null,
                scheduleState = null,
                attemptPlan = null,
                loopPlan = null,
                providerDiagnostics = providerDiagnostics,
                observabilityEvent = observabilityEvent,
                classification = capabilityPreflight.optString("classification"),
                completed = false,
                shouldReread = false,
                shouldRetry = false,
                clientAction = capabilityPreflight.optString("clientAction"),
            )
        }

        val membershipRoundJson = ActroCore.browserStorageSyncCloudLocalFolderMembershipRoundJson(
            rootDir = rootDir,
            requestJson = requestJson,
        )
        val membershipRound = membershipRoundJson.toJsonObjectOrNull()
        if (membershipRound == null) {
            val providerDiagnostics = cloudProviderDiagnostics(
                capabilityPreflight = capabilityPreflight,
                compatibilityPreflight = null,
                statusPolicy = null,
                scheduleState = null,
                observedAtUnixSeconds = observedAtUnixSeconds,
            )
            val observabilityEvent = cloudObservabilityEvent(
                providerDiagnostics = providerDiagnostics,
                attemptPlan = null,
                loopPlan = null,
                observedAtUnixSeconds = observedAtUnixSeconds,
            )
            return localFolderMembershipClientResultJson(
                rootDir = rootDir,
                membershipRequest = requestJson.toJsonObjectOrNull(),
                capabilityPreflight = capabilityPreflight,
                compatibilityPreflight = null,
                schemaRolloutPlan = null,
                schemaMigrationPlaybook = null,
                recoverySnapshotRound = null,
                membershipRound = null,
                statusPolicy = null,
                scheduleState = null,
                attemptPlan = null,
                loopPlan = null,
                providerDiagnostics = providerDiagnostics,
                observabilityEvent = observabilityEvent,
                classification = "clientPlanUnavailable",
                completed = false,
                shouldReread = false,
                shouldRetry = false,
                clientAction = "skipCloudMembership",
            )
        }

        val completed = membershipRound.optBoolean("completed", false)
        val planningErrorKind = membershipRound.optString("planningErrorKind")
        val classification = if (completed) {
            "success"
        } else {
            planningErrorKind.ifEmpty { "membershipRoundIncomplete" }
        }
        val statusPolicy = cloudMembershipStatusPolicy(
            membershipRound = membershipRound,
            retryAttempt = 0,
        )
        val scheduleState = cloudMembershipScheduleState(
            statusPolicy = statusPolicy,
            observedAtUnixSeconds = observedAtUnixSeconds,
        )
        val attemptPlan = cloudMembershipAttemptPlan(
            membershipRound = membershipRound,
            retryAttempt = 0,
            observedAtUnixSeconds = observedAtUnixSeconds,
        )
        val loopPlan = cloudMembershipLoopPlan(
            membershipRound = membershipRound,
            retryAttempt = 0,
            observedAtUnixSeconds = observedAtUnixSeconds,
            maxAttempts = 3,
        )
        val compatibilityPreflight = membershipRound.optJSONObject("compatibilityPreflight")
        val schemaRolloutPlan: JSONObject?
        val schemaMigrationPlaybook: JSONObject?
        val recoverySnapshotRound: JSONObject?
        if (compatibilityPreflight?.optBoolean("shouldBlockSync", false) == true) {
            schemaRolloutPlan = cloudSchemaRolloutPlan(
                compatibilityPreflight = compatibilityPreflight,
                observedAtUnixSeconds = observedAtUnixSeconds,
            )
            schemaMigrationPlaybook = cloudSchemaMigrationPlaybook(
                schemaRolloutPlan = schemaRolloutPlan,
                observedAtUnixSeconds = observedAtUnixSeconds,
            )
            recoverySnapshotRound = localFolderRecoverySnapshotRound(
                rootDir = rootDir,
                schemaMigrationPlaybook = schemaMigrationPlaybook,
                compatibilityPreflight = compatibilityPreflight,
                observedAtUnixSeconds = observedAtUnixSeconds,
            )
        } else {
            schemaRolloutPlan = null
            schemaMigrationPlaybook = null
            recoverySnapshotRound = null
        }
        val providerErrorClassification = localFolderProviderErrorClassification(
            membershipRound = membershipRound,
            recoverySnapshotRound = recoverySnapshotRound,
            retryAttempt = 0,
            observedAtUnixSeconds = observedAtUnixSeconds,
        )
        val providerDiagnostics = cloudProviderDiagnostics(
            capabilityPreflight = capabilityPreflight,
            compatibilityPreflight = compatibilityPreflight,
            providerErrorClassification = providerErrorClassification,
            statusPolicy = statusPolicy,
            scheduleState = scheduleState,
            observedAtUnixSeconds = observedAtUnixSeconds,
        )
        val observabilityEvent = cloudObservabilityEvent(
            providerDiagnostics = providerDiagnostics,
            attemptPlan = attemptPlan,
            loopPlan = loopPlan,
            observedAtUnixSeconds = observedAtUnixSeconds,
        )

        return localFolderMembershipClientResultJson(
            rootDir = rootDir,
            membershipRequest = requestJson.toJsonObjectOrNull(),
            capabilityPreflight = capabilityPreflight,
            compatibilityPreflight = compatibilityPreflight,
            schemaRolloutPlan = schemaRolloutPlan,
            schemaMigrationPlaybook = schemaMigrationPlaybook,
            recoverySnapshotRound = recoverySnapshotRound,
            membershipRound = membershipRound,
            statusPolicy = statusPolicy,
            scheduleState = scheduleState,
            attemptPlan = attemptPlan,
            loopPlan = loopPlan,
            providerDiagnostics = providerDiagnostics,
            observabilityEvent = observabilityEvent,
            classification = statusPolicy?.optString("classification")?.takeIf { it.isNotEmpty() }
                ?: classification,
            completed = statusPolicy?.optBoolean("completed", completed) ?: completed,
            shouldReread = statusPolicy?.optBoolean(
                "shouldReread",
                membershipRound.optBoolean("shouldReread", false),
            ) ?: membershipRound.optBoolean("shouldReread", false),
            shouldRetry = statusPolicy?.optBoolean(
                "shouldRetry",
                membershipRound.optBoolean("shouldRetry", false),
            ) ?: membershipRound.optBoolean("shouldRetry", false),
            clientAction = statusPolicy?.optString("clientAction")?.takeIf { it.isNotEmpty() }
                ?: membershipRound.optString("clientAction"),
        )
    }
}

private data class BrowserStorageSyncCycleRequestPlan(
    val method: String,
    val url: String,
    val headers: Map<String, String>,
    val bodyJson: String
) {
    fun toHttpRequest(): BrowserStorageSyncHttpRequest =
        BrowserStorageSyncHttpRequest(
            method = method,
            url = url,
            headers = headers,
            bodyJson = bodyJson,
        )

    companion object {
        fun fromJson(json: String): BrowserStorageSyncCycleRequestPlan? {
            return try {
                if (json == "null") return null
                val root = JSONObject(json)
                val headersJson = root.optJSONArray("headers") ?: JSONArray()
                val headers = mutableMapOf<String, String>()
                for (index in 0 until headersJson.length()) {
                    val header = headersJson.getJSONObject(index)
                    headers[header.getString("name")] = header.getString("value")
                }
                BrowserStorageSyncCycleRequestPlan(
                    method = root.getString("method"),
                    url = root.getString("url"),
                    headers = headers,
                    bodyJson = root.getString("bodyJson"),
                )
            } catch (_: JSONException) {
                null
            }
        }
    }
}

private fun syncClientResultJson(
    requestPlan: JSONObject?,
    transportResult: JSONObject,
    applyResult: Any,
    applied: Boolean
): String =
    JSONObject()
        .put("version", 1)
        .put("storageSchemaVersion", ActroCore.browserStorageSchemaVersion().toLong())
        .put("endpointType", "syncCycle")
        .put("requestPlan", requestPlan ?: JSONObject.NULL)
        .put("transportResult", transportResult)
        .put("applyResult", applyResult)
        .put("applied", applied)
        .put("classification", transportResult.optString("classification"))
        .put("shouldRetry", transportResult.optBoolean("shouldRetry", false))
        .put("clientAction", transportResult.optString("clientAction"))
        .toString()

private fun localFolderSyncClientResultJson(
    rootDir: String,
    pushRequest: JSONObject?,
    capabilityPreflight: JSONObject?,
    compatibilityPreflight: JSONObject?,
    schemaRolloutPlan: JSONObject?,
    schemaMigrationPlaybook: JSONObject?,
    recoverySnapshotRound: JSONObject?,
    cycleRound: JSONObject?,
    statusPolicy: JSONObject?,
    scheduleState: JSONObject?,
    attemptPlan: JSONObject?,
    loopPlan: JSONObject?,
    applyResult: JSONObject?,
    compactionRound: JSONObject?,
    cleanupRound: JSONObject?,
    providerDiagnostics: JSONObject?,
    observabilityEvent: JSONObject?,
    classification: String,
    shouldRetry: Boolean,
    clientAction: String,
    applied: Boolean
): String =
    JSONObject()
        .put("version", 1)
        .put("storageSchemaVersion", ActroCore.browserStorageSchemaVersion().toLong())
        .put("endpointType", "cloudLocalFolderCycle")
        .put("providerKind", "localFolder")
        .put("rootDir", rootDir)
        .put("pushRequest", pushRequest ?: JSONObject.NULL)
        .put("capabilityPreflight", capabilityPreflight ?: JSONObject.NULL)
        .put("compatibilityPreflight", compatibilityPreflight ?: JSONObject.NULL)
        .put("schemaRolloutPlan", schemaRolloutPlan ?: JSONObject.NULL)
        .put("schemaMigrationPlaybook", schemaMigrationPlaybook ?: JSONObject.NULL)
        .put("recoverySnapshotRound", recoverySnapshotRound ?: JSONObject.NULL)
        .put("cycleRound", cycleRound ?: JSONObject.NULL)
        .put("statusPolicy", statusPolicy ?: JSONObject.NULL)
        .put("scheduleState", scheduleState ?: JSONObject.NULL)
        .put("attemptPlan", attemptPlan ?: JSONObject.NULL)
        .put("loopPlan", loopPlan ?: JSONObject.NULL)
        .put("applyResult", applyResult ?: JSONObject.NULL)
        .put("compactionRound", compactionRound ?: JSONObject.NULL)
        .put("cleanupRound", cleanupRound ?: JSONObject.NULL)
        .put("providerDiagnostics", providerDiagnostics ?: JSONObject.NULL)
        .put("observabilityEvent", observabilityEvent ?: JSONObject.NULL)
        .put("applied", applied)
        .put("classification", classification)
        .put("shouldRetry", shouldRetry)
        .put("clientAction", clientAction)
        .toString()

private fun localFolderMembershipClientResultJson(
    rootDir: String,
    membershipRequest: JSONObject?,
    capabilityPreflight: JSONObject?,
    compatibilityPreflight: JSONObject?,
    schemaRolloutPlan: JSONObject?,
    schemaMigrationPlaybook: JSONObject?,
    recoverySnapshotRound: JSONObject?,
    membershipRound: JSONObject?,
    statusPolicy: JSONObject?,
    scheduleState: JSONObject?,
    attemptPlan: JSONObject?,
    loopPlan: JSONObject?,
    providerDiagnostics: JSONObject?,
    observabilityEvent: JSONObject?,
    classification: String,
    completed: Boolean,
    shouldReread: Boolean,
    shouldRetry: Boolean,
    clientAction: String
): String =
    JSONObject()
        .put("version", 1)
        .put("storageSchemaVersion", ActroCore.browserStorageSchemaVersion().toLong())
        .put("endpointType", "cloudLocalFolderMembership")
        .put("providerKind", "localFolder")
        .put("rootDir", rootDir)
        .put("membershipRequest", membershipRequest ?: JSONObject.NULL)
        .put("capabilityPreflight", capabilityPreflight ?: JSONObject.NULL)
        .put("compatibilityPreflight", compatibilityPreflight ?: JSONObject.NULL)
        .put("schemaRolloutPlan", schemaRolloutPlan ?: JSONObject.NULL)
        .put("schemaMigrationPlaybook", schemaMigrationPlaybook ?: JSONObject.NULL)
        .put("recoverySnapshotRound", recoverySnapshotRound ?: JSONObject.NULL)
        .put("membershipRound", membershipRound ?: JSONObject.NULL)
        .put("statusPolicy", statusPolicy ?: JSONObject.NULL)
        .put("scheduleState", scheduleState ?: JSONObject.NULL)
        .put("attemptPlan", attemptPlan ?: JSONObject.NULL)
        .put("loopPlan", loopPlan ?: JSONObject.NULL)
        .put("providerDiagnostics", providerDiagnostics ?: JSONObject.NULL)
        .put("observabilityEvent", observabilityEvent ?: JSONObject.NULL)
        .put("completed", completed)
        .put("shouldReread", shouldReread)
        .put("shouldRetry", shouldRetry)
        .put("classification", classification)
        .put("clientAction", clientAction)
        .toString()

private fun localFolderCapabilityPreflight(observedAtUnixSeconds: Long): JSONObject? {
    return ActroCore.browserStorageSyncCloudLocalFolderCapabilityPreflightJson(
        observedAtUnixSeconds = observedAtUnixSeconds,
    ).toJsonObjectOrNull()
}

private fun localFolderCompatibilityPreflight(rootDir: String, observedAtUnixSeconds: Long): JSONObject? {
    return ActroCore.browserStorageSyncCloudLocalFolderCompatibilityPreflightJson(
        rootDir = rootDir,
        observedAtUnixSeconds = observedAtUnixSeconds,
    ).toJsonObjectOrNull()
}

private fun cloudSchemaRolloutPlan(
    compatibilityPreflight: JSONObject?,
    observedAtUnixSeconds: Long
): JSONObject? =
    ActroCore.browserStorageSyncCloudSchemaRolloutPlanJson(
        compatibilityPreflightJson = compatibilityPreflight?.toString().orEmpty(),
        observedAtUnixSeconds = observedAtUnixSeconds,
    ).toJsonObjectOrNull()

private fun cloudSchemaMigrationPlaybook(
    schemaRolloutPlan: JSONObject?,
    observedAtUnixSeconds: Long
): JSONObject? =
    ActroCore.browserStorageSyncCloudSchemaMigrationPlaybookJson(
        schemaRolloutPlanJson = schemaRolloutPlan?.toString().orEmpty(),
        observedAtUnixSeconds = observedAtUnixSeconds,
    ).toJsonObjectOrNull()

private fun localFolderRecoverySnapshotRound(
    rootDir: String,
    schemaMigrationPlaybook: JSONObject?,
    compatibilityPreflight: JSONObject?,
    observedAtUnixSeconds: Long
): JSONObject? =
    ActroCore.browserStorageSyncCloudLocalFolderRecoverySnapshotRoundJson(
        rootDir = rootDir,
        schemaMigrationPlaybookJson = schemaMigrationPlaybook?.toString().orEmpty(),
        compatibilityPreflightJson = compatibilityPreflight?.toString().orEmpty(),
        observedAtUnixSeconds = observedAtUnixSeconds,
    ).toJsonObjectOrNull()

private fun cloudMembershipStatusPolicy(
    membershipRound: JSONObject?,
    retryAttempt: Int
): JSONObject? =
    ActroCore.browserStorageSyncCloudMembershipStatusPolicyJson(
        membershipRoundJson = membershipRound?.toString().orEmpty(),
        retryAttempt = retryAttempt,
    ).toJsonObjectOrNull()

private fun cloudMembershipScheduleState(
    statusPolicy: JSONObject?,
    observedAtUnixSeconds: Long
): JSONObject? =
    ActroCore.browserStorageSyncCloudMembershipScheduleStateJson(
        statusPolicyJson = statusPolicy?.toString().orEmpty(),
        observedAtUnixSeconds = observedAtUnixSeconds,
    ).toJsonObjectOrNull()

private fun cloudMembershipAttemptPlan(
    membershipRound: JSONObject?,
    retryAttempt: Int,
    observedAtUnixSeconds: Long
): JSONObject? =
    ActroCore.browserStorageSyncCloudMembershipAttemptPlanJson(
        membershipRoundJson = membershipRound?.toString().orEmpty(),
        retryAttempt = retryAttempt,
        observedAtUnixSeconds = observedAtUnixSeconds,
    ).toJsonObjectOrNull()

private fun cloudMembershipLoopPlan(
    membershipRound: JSONObject?,
    retryAttempt: Int,
    observedAtUnixSeconds: Long,
    maxAttempts: Int
): JSONObject? =
    ActroCore.browserStorageSyncCloudMembershipLoopPlanJson(
        membershipRoundsJson = JSONArray().put(membershipRound ?: JSONObject.NULL).toString(),
        retryAttempt = retryAttempt,
        observedAtUnixSeconds = observedAtUnixSeconds,
        maxAttempts = maxAttempts,
    ).toJsonObjectOrNull()

private fun localFolderProviderErrorClassification(
    cycleRound: JSONObject? = null,
    membershipRound: JSONObject? = null,
    compactionRound: JSONObject? = null,
    cleanupRound: JSONObject? = null,
    recoverySnapshotRound: JSONObject? = null,
    retryAttempt: Int,
    observedAtUnixSeconds: Long
): JSONObject? {
    val payload = JSONObject()
        .put("planType", "localFolderProviderRoundErrors")
        .put("providerKind", "localFolder")
    if (cycleRound != null) {
        payload.put("cycleRound", cycleRound)
    }
    if (membershipRound != null) {
        payload.put("membershipRound", membershipRound)
    }
    if (compactionRound != null) {
        payload.put("compactionRound", compactionRound)
    }
    if (cleanupRound != null) {
        payload.put("cleanupRound", cleanupRound)
    }
    if (recoverySnapshotRound != null) {
        payload.put("recoverySnapshotRound", recoverySnapshotRound)
    }

    return ActroCore.browserStorageSyncCloudProviderErrorClassificationJson(
        providerErrorsJson = payload.toString(),
        retryAttempt = retryAttempt.coerceAtLeast(0).toUInt(),
        observedAtUnixSeconds = observedAtUnixSeconds,
    ).toJsonObjectOrNull()?.takeIf { it.optInt("failedCount", 0) > 0 }
}

private fun cloudProviderDiagnostics(
    capabilityPreflight: JSONObject?,
    compatibilityPreflight: JSONObject?,
    providerErrorClassification: JSONObject? = null,
    statusPolicy: JSONObject?,
    scheduleState: JSONObject?,
    observedAtUnixSeconds: Long
): JSONObject? =
    ActroCore.browserStorageSyncCloudProviderDiagnosticsJson(
        capabilityPreflightJson = capabilityPreflight?.toString().orEmpty(),
        compatibilityPreflightJson = compatibilityPreflight?.toString().orEmpty(),
        providerErrorClassificationJson = providerErrorClassification?.toString().orEmpty(),
        cycleStatusPolicyJson = statusPolicy?.toString().orEmpty(),
        scheduleStateJson = scheduleState?.toString().orEmpty(),
        observedAtUnixSeconds = observedAtUnixSeconds,
    ).toJsonObjectOrNull()

private fun cloudObservabilityEvent(
    providerDiagnostics: JSONObject?,
    attemptPlan: JSONObject?,
    loopPlan: JSONObject?,
    observedAtUnixSeconds: Long
): JSONObject? =
    ActroCore.browserStorageSyncCloudObservabilityEventJson(
        providerDiagnosticsJson = providerDiagnostics?.toString().orEmpty(),
        attemptPlanJson = attemptPlan?.toString().orEmpty(),
        loopPlanJson = loopPlan?.toString().orEmpty(),
        observedAtUnixSeconds = observedAtUnixSeconds,
    ).toJsonObjectOrNull()

private fun String.toJsonObjectOrNull(): JSONObject? =
    try {
        if (this == "null") null else JSONObject(this)
    } catch (_: JSONException) {
        null
    }

private fun currentUnixSeconds(): Long = System.currentTimeMillis() / 1000
